use std::collections::{HashMap, HashSet};

use crate::{
    domain::{
        election::{Election, Open},
        strategy::TallyStrategy,
    },
    error::ElectionError,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Slate {
    name: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AcademicOutcome {
    Winner(Slate),
    Tie,
    QuorumNotMet,
}

pub struct AcademicStrategy {
    valid_slates: HashSet<Slate>,
    minimum_quorum: usize,
}

impl TallyStrategy for AcademicStrategy {
    type Vote = Slate;
    type Outcome = AcademicOutcome;

    fn validate(&self, vote: &Self::Vote) -> Result<(), crate::error::ElectionError> {
        if !self.valid_slates.contains(vote) {
            return Err(ElectionError::InvalidVote(format!(
                "Unrecognized slate: {}",
                vote.name
            )));
        }
        Ok(())
    }

    fn tally(&self, votes: &[Self::Vote]) -> Result<Self::Outcome, ElectionError> {
        if votes.len() < self.minimum_quorum {
            return Ok(AcademicOutcome::QuorumNotMet);
        }

        let mut counts: HashMap<&Slate, usize> = HashMap::new();
        for vote in votes {
            *counts.entry(vote).or_insert(0) += 1;
        }

        let mut max_votes = 0;

        let mut winners = Vec::new();
        for (slate, &count) in counts.iter() {
            if count > max_votes {
                max_votes = count;
                winners.clear();
                winners.push((*slate).clone());
            } else if count == max_votes {
                winners.push((*slate).clone());
            }
        }

        if winners.len() == 1 {
            Ok(AcademicOutcome::Winner(winners.remove(0)))
        } else {
            Ok(AcademicOutcome::Tie)
        }
    }
}

pub struct AcademicElectionBuilder {
    valid_slates: HashSet<Slate>,
    minimum_quorum: usize,
}

impl AcademicElectionBuilder {
    pub fn new() -> Self {
        Self {
            valid_slates: HashSet::new(),
            minimum_quorum: 1,
        }
    }

    pub fn add_slate(mut self, name: &str) -> Self {
        self.valid_slates.insert(Slate {
            name: name.to_string(),
        });
        self
    }

    pub fn with_quorum(mut self, quorum: usize) -> Self {
        self.minimum_quorum = quorum;
        self
    }

    pub fn build(self) -> Result<Election<AcademicStrategy, Open>, String> {
        if self.valid_slates.is_empty() {
            return Err("Cannot start an election with zero slates.".into());
        }

        let strategy = AcademicStrategy {
            valid_slates: self.valid_slates,
            minimum_quorum: self.minimum_quorum,
        };

        Ok(Election::new(strategy))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_pattern_empty_academic_election() {
        let builder = AcademicElectionBuilder::new().with_quorum(10);
        assert!(builder.build().is_err());
    }

    #[test]
    fn test_academic_election_quorumnotmet() {
        let mut election = AcademicElectionBuilder::new()
            .add_slate("Red")
            .add_slate("Blu")
            .with_quorum(3)
            .build()
            .unwrap();

        assert!(election.cast_vote(Slate { name: "Red".into() }).is_ok());
        assert!(election.cast_vote(Slate { name: "Red".into() }).is_ok());

        assert!(election.cast_vote(Slate { name: "Nil".into() }).is_err());

        let result = election.close().compute_result().unwrap();
        let expected = AcademicOutcome::QuorumNotMet;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_academic_election_tie() {
        let mut election = AcademicElectionBuilder::new()
            .add_slate("A")
            .add_slate("B")
            .with_quorum(2)
            .build()
            .unwrap();

        election.cast_vote(Slate { name: "A".into() }).unwrap();
        election.cast_vote(Slate { name: "B".into() }).unwrap();

        let result = election.close().compute_result().unwrap();
        let expected = AcademicOutcome::Tie;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_academic_election_winner() {
        let mut election = AcademicElectionBuilder::new()
            .add_slate("Overworld")
            .add_slate("Nether")
            .add_slate("End")
            .with_quorum(1)
            .build()
            .unwrap();

        election
            .cast_vote(Slate {
                name: "Overworld".into(),
            })
            .unwrap();
        election
            .cast_vote(Slate {
                name: "Overworld".into(),
            })
            .unwrap();
        election
            .cast_vote(Slate {
                name: "Overworld".into(),
            })
            .unwrap();
        election
            .cast_vote(Slate {
                name: "Nether".into(),
            })
            .unwrap();
        election.cast_vote(Slate { name: "End".into() }).unwrap();

        let result = election.close().compute_result().unwrap();
        let expected = AcademicOutcome::Winner(Slate {
            name: "Overworld".into(),
        });
        assert_eq!(result, expected);
    }
}
