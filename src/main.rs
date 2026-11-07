use crate::{
    domain::{election::Election, strategy::TallyStrategy},
    error::ElectionError,
};

mod domain;
mod error;

struct MockStrategy;

impl TallyStrategy for MockStrategy {
    type Vote = u32;
    type Outcome = u32;

    fn validate(&self, vote: &Self::Vote) -> Result<(), ElectionError> {
        if *vote == 0 {
            Err(ElectionError::InvalidVote(
                "Vote cannot be 0 in this instance".into(),
            ))
        } else {
            Ok(())
        }
    }

    fn tally(&self, votes: &[Self::Vote]) -> Result<Self::Outcome, ElectionError> {
        let total = votes.iter().sum();

        if total == 0 {
            Err(ElectionError::TallyFailed(
                "Tally cannot be 0 in this instance".into(),
            ))
        } else {
            Ok(total)
        }
    }
}

fn main() {
    let mut election = Election::new(MockStrategy);

    for idx in 1..10 {
        match election.cast_vote(idx) {
            Ok(()) => println!("pililili"),
            Err(failed) => eprintln!("ERROR: {failed}"),
        }
    }

    let closed_election = election.close();

    match closed_election.compute_result() {
        Ok(winner) => println!("{winner} won!"),
        Err(failed) => eprintln!("ERROR: {failed}"),
    }
}
