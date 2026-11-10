use std::fmt::Display;

use rust_decimal::{Decimal, dec};

use crate::domain::{
    academic::{AcademicElectionBuilder, AcademicOutcome, Slate},
    budget::{BudgetElectionBuilder, BudgetOutcome},
    condo::{CondoElectionBuilder, CondoOutcome, CondoVote},
    storage::{ElectionEvent, EventRecord, EventStore, MemoryEventStore},
};

mod domain;
mod error;

fn print_section(title: &str) {
    println!("\n=== {title} ===");
}

fn show_expected_error<E: Display>(label: &str, result: Result<(), E>) {
    match result {
        Err(error) => println!("  [expected error] {label}: {error}"),
        Ok(()) => panic!("Expected {label}, but the operation succeeded"),
    }
}

fn print_academic_outcome(outcome: &AcademicOutcome) {
    match outcome {
        AcademicOutcome::Winner(slate) => println!("Result: {slate} wins"),
        AcademicOutcome::Tie(slates) => {
            let names = slates
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            println!("Result: tie between {names}");
        }
        AcademicOutcome::QuorumNotMet => println!("Result: quorum not met"),
    }
}

fn print_budget_outcome(outcome: &BudgetOutcome) {
    println!("Selected projects:");
    if outcome.selected_projects.is_empty() {
        println!("  (none)");
    } else {
        for project in &outcome.selected_projects {
            println!("  - {} ({})", project.name, project.cost);
        }
    }
    println!("Total spent: {}", outcome.total_spent);
}

fn print_condo_outcome(outcome: &CondoOutcome) {
    match outcome {
        CondoOutcome::Winner(option) => println!("Result: {option} wins"),
        CondoOutcome::Tie(options) => {
            println!("Result: tie between {}", options.join(", "));
        }
        CondoOutcome::QuorumNotMet => println!("Result: quorum not met"),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    print_section("Academic election");
    println!("Validation examples:");
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
    print_academic_outcome(&academic_outcome);

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
    println!("Audit log:");
    for event in store.get_all()? {
        println!("  #{:<2} {:?}", event.sequence, event.payload);
    }
    println!("Events from sequence 3: {}", store.get_since(3)?.len());

    print_section("Budget election");
    println!("Validation examples:");
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
    let budget_outcome = budget.close().compute_result()?;
    print_budget_outcome(&budget_outcome);

    print_section("Condominium election");
    println!("Validation examples:");
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
    let condo_outcome = condo.close().compute_result()?;
    print_condo_outcome(&condo_outcome);

    Ok(())
}
