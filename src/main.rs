use rust_decimal::{Decimal, dec};
use std::fmt::Display;

use crate::domain::{
    academic::{AcademicElectionBuilder, AcademicOutcome, Slate},
    budget::BudgetElectionBuilder,
    condo::{CondoElectionBuilder, CondoVote},
    storage::{ElectionEvent, EventRecord, EventStore, MemoryEventStore},
};

mod domain;
mod error;

fn show_expected_error<E: Display>(label: &str, result: Result<(), E>) {
    match result {
        Err(error) => println!("Expected {label}: {error}"),
        Ok(()) => panic!("Expected {label}, but the operation succeeded"),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Academic election");
    show_expected_error(
        "no-slate error",
        AcademicElectionBuilder::new().build().map(|_| ()),
    );
    show_expected_error(
        "invalid-slate-name error",
        AcademicElectionBuilder::new()
            .add_slate(" ")
            .build()
            .map(|_| ()),
    );

    let mut academic = AcademicElectionBuilder::new()
        .add_slate("RED")
        .add_slate("BLU")
        .with_quorum(1)
        .build()?;
    let red = Slate::new("RED")?;
    let blu = Slate::new("BLU")?;
    show_expected_error(
        "invalid-vote error",
        academic.cast_vote(Slate::new("GREEN")?),
    );
    academic.cast_vote(red.clone())?;
    academic.cast_vote(red.clone())?;
    academic.cast_vote(blu.clone())?;
    let academic_outcome = academic.close().compute_result()?;
    println!("Academic outcome: {academic_outcome:?}");

    let empty_academic = AcademicElectionBuilder::new().add_slate("RED").build()?;
    show_expected_error(
        "tally-failed error",
        empty_academic.close().compute_result().map(|_| ()),
    );

    let mut store: MemoryEventStore<Slate, AcademicOutcome> = MemoryEventStore::new();
    store.append(EventRecord::new(1, ElectionEvent::ElectionCreated))?;
    store.append(EventRecord::new(2, ElectionEvent::VoteCast(red.clone())))?;
    store.append(EventRecord::new(3, ElectionEvent::VoteCast(blu)))?;
    store.append(EventRecord::new(4, ElectionEvent::ElectionClosed))?;
    store.append(EventRecord::new(
        5,
        ElectionEvent::TallyComputed(academic_outcome),
    ))?;
    show_expected_error(
        "storage sequence-conflict error",
        store.append(EventRecord::new(7, ElectionEvent::ElectionClosed)),
    );
    for event in store.get_all()? {
        println!("[Seq {}] {:?}", event.sequence, event.payload);
    }
    println!("Events since sequence 3: {}", store.get_since(3)?.len());

    println!("\nBudget election");
    show_expected_error(
        "no-projects error",
        BudgetElectionBuilder::new()
            .with_ceiling(dec!(100))
            .build()
            .map(|_| ()),
    );
    show_expected_error(
        "negative-ceiling error",
        BudgetElectionBuilder::new()
            .add_project("Park", dec!(50))
            .with_ceiling(dec!(-1))
            .build()
            .map(|_| ()),
    );
    show_expected_error(
        "invalid-project-name error",
        BudgetElectionBuilder::new()
            .add_project(" ", dec!(50))
            .with_ceiling(dec!(100))
            .build()
            .map(|_| ()),
    );

    let mut budget = BudgetElectionBuilder::new()
        .add_project("Park", dec!(50))
        .add_project("Library", dec!(30))
        .with_ceiling(dec!(60))
        .build()?;
    show_expected_error(
        "invalid budget vote",
        budget.cast_vote(vec!["Unknown".to_owned()]),
    );
    budget.cast_vote(vec!["Park".to_owned(), "Library".to_owned()])?;
    budget.cast_vote(vec!["Library".to_owned(), "Park".to_owned()])?;
    println!("Budget outcome: {:?}", budget.close().compute_result()?);

    println!("\nCondominium election");
    show_expected_error(
        "no-options error",
        CondoElectionBuilder::new().build().map(|_| ()),
    );
    show_expected_error(
        "out-of-range-quorum error",
        CondoElectionBuilder::new()
            .with_quorum(dec!(1.1))
            .add_option("Repair")
            .build()
            .map(|_| ()),
    );
    show_expected_error(
        "invalid-option-name error",
        CondoElectionBuilder::new()
            .add_option(" ")
            .build()
            .map(|_| ()),
    );

    let mut condo = CondoElectionBuilder::new()
        .add_option("Repair")
        .add_option("Renovate")
        .with_quorum(dec!(0.5))
        .build()?;
    show_expected_error(
        "invalid condominium vote",
        condo.cast_vote(CondoVote {
            choice: "Unknown".to_owned(),
            fraction: dec!(0.5),
        }),
    );
    show_expected_error(
        "invalid vote fraction",
        condo.cast_vote(CondoVote {
            choice: "Repair".to_owned(),
            fraction: Decimal::ZERO,
        }),
    );
    condo.cast_vote(CondoVote {
        choice: "Repair".to_owned(),
        fraction: dec!(0.3),
    })?;
    condo.cast_vote(CondoVote {
        choice: "Renovate".to_owned(),
        fraction: dec!(0.4),
    })?;
    println!("Condominium outcome: {:?}", condo.close().compute_result()?);

    Ok(())
}
