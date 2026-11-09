use crate::domain::{
    academic::{AcademicElectionBuilder, AcademicOutcome, Slate},
    storage::{ElectionEvent, EventRecord, EventStore, MemoryEventStore},
};

mod domain;
mod error;

fn main() {
    let mut store: MemoryEventStore<Slate, AcademicOutcome> = MemoryEventStore::new();
    let mut sequence = 1;

    let mut election = match AcademicElectionBuilder::new()
        .add_slate("RED")
        .add_slate("BLU")
        .with_quorum(1)
        .build()
    {
        Ok(e) => {
            println!("Election successfuly created!");
            let _ = store.append(EventRecord::new(sequence, ElectionEvent::ElectionCreated));
            sequence += 1;
            e
        }
        Err(e) => {
            eprintln!("Election failed to be created: {e}");
            return;
        }
    };

    let red = Slate::new("RED").expect("RED is valid");
    let blu = Slate::new("BLU").expect("BLU is valid");

    if let Err(e) = election.cast_vote(red.clone()) {
        eprintln!("WARN: {e}");
    } else {
        let _ = store.append(EventRecord::new(
            sequence,
            ElectionEvent::VoteCast(red.clone()),
        ));
        sequence += 1;
    }

    if let Err(e) = election.cast_vote(blu.clone()) {
        eprintln!("WARN: {e}");
    } else {
        let _ = store.append(EventRecord::new(
            sequence,
            ElectionEvent::VoteCast(blu.clone()),
        ));
        sequence += 1;
    }

    let result = election.close().compute_result();

    let _ = store.append(EventRecord::new(sequence, ElectionEvent::ElectionClosed));
    sequence += 1;

    match result {
        Ok(outcome) => {
            let _ = store.append(EventRecord::new(
                sequence,
                ElectionEvent::TallyComputed(outcome.clone()),
            ));

            match outcome {
                AcademicOutcome::Winner(slate) => {
                    println!("{} won!", slate);
                }
                AcademicOutcome::Tie(slates) => {
                    for slate in slates {
                        println!("{}", slate);
                    }
                    println!("tied!");
                }
                AcademicOutcome::QuorumNotMet => {
                    println!("Election did not meet minimum quorum!");
                }
            }
        }
        Err(e) => {
            eprintln!("ERROR: {e}");
        }
    }

    println!("[Audit Log]");
    if let Ok(events) = store.get_all() {
        for event in events {
            println!("[Seq {}] {:?}", event.sequence, event.payload);
        }
    }

    let _recent_events = store.get_since(3);
}
