use crate::domain::academic::{AcademicElectionBuilder, AcademicOutcome, Slate};

mod domain;
mod error;

fn main() {
    let mut election = match AcademicElectionBuilder::new()
        .add_slate("RED")
        .add_slate("BLU")
        .with_quorum(1)
        .build()
    {
        Ok(e) => {
            println!("Election successfuly created!");
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
    }
    if let Err(e) = election.cast_vote(blu.clone()) {
        eprintln!("WARN: {e}");
    }

    let result = election.close().compute_result();

    match result {
        Ok(AcademicOutcome::Winner(slate)) => {
            println!("{slate} won");
        }
        Ok(AcademicOutcome::Tie(slates)) => {
            for slate in slates {
                println!("{slate}");
            }
            println!("tied");
        }
        Ok(AcademicOutcome::QuorumNotMet) => {
            println!("Election did not meet minimum quorum");
        }
        Err(e) => {
            eprintln!("ERROR: {e}");
        }
    }
}
