use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
};

use crate::{
    domain::{
        election::{Election, Open},
        strategy::TallyStrategy,
    },
    error::{AcademicElectionError, ElectionError},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Slate {
    name: String,
}

impl Display for Slate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl Slate {
    pub fn new(name: &str) -> Result<Self, AcademicElectionError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            Err(AcademicElectionError::InvalidSlateName(
                "Name cannot be empty".into(),
            ))
        } else {
            Ok(Self {
                name: trimmed.to_string(),
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcademicOutcome {
    Winner(Slate),
    Tie(Vec<Slate>),
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
        if votes.is_empty() {
            return Err(ElectionError::TallyFailed("No votes cast".into()));
        }

        if votes.len() < self.minimum_quorum {
            return Ok(AcademicOutcome::QuorumNotMet);
        }

        let mut counts: HashMap<&Slate, usize> = HashMap::new();
        for vote in votes {
            *counts.entry(vote).or_insert(0) += 1;
        }

        let mut max_votes = 0;
        let mut winners = Vec::new();

        for (&slate, &count) in counts.iter() {
            if count > max_votes {
                max_votes = count;
                winners.clear();
                winners.push(slate.clone());
            } else if count == max_votes {
                winners.push(slate.clone());
            }
        }

        if winners.len() == 1 {
            Ok(AcademicOutcome::Winner(winners.into_iter().next().unwrap()))
        } else {
            winners.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(AcademicOutcome::Tie(winners))
        }
    }
}

pub struct AcademicElectionBuilder {
    slate_names: Vec<String>,
    minimum_quorum: usize,
}

impl Default for AcademicElectionBuilder {
    fn default() -> Self {
        Self {
            slate_names: Vec::new(),
            minimum_quorum: 1,
        }
    }
}

impl AcademicElectionBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_slate(mut self, name: &str) -> Self {
        self.slate_names.push(name.to_string());
        self
    }

    pub fn with_quorum(mut self, quorum: usize) -> Self {
        self.minimum_quorum = quorum;
        self
    }

    pub fn build(self) -> Result<Election<AcademicStrategy, Open>, AcademicElectionError> {
        let mut valid_slates = HashSet::new();

        for name in self.slate_names {
            let slate = Slate::new(&name)?;
            valid_slates.insert(slate);
        }

        if valid_slates.is_empty() {
            return Err(AcademicElectionError::NoSlatesRegistered);
        }

        let strategy = AcademicStrategy {
            valid_slates,
            minimum_quorum: self.minimum_quorum,
        };

        Ok(Election::new(strategy))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_pattern_empty_election() {
        let result = AcademicElectionBuilder::new().with_quorum(10).build();
        assert!(matches!(
            result,
            Err(AcademicElectionError::NoSlatesRegistered)
        ));
    }

    #[test]
    fn test_builder_pattern_invalid_slate_name() {
        let result = AcademicElectionBuilder::new()
            .add_slate("Valid Slate")
            .add_slate("   ")
            .build();

        assert!(matches!(
            result,
            Err(AcademicElectionError::InvalidSlateName(_))
        ));
    }

    #[test]
    fn test_academic_election_quorum_not_met() {
        let mut election = AcademicElectionBuilder::new()
            .add_slate("Red")
            .add_slate("Blu")
            .with_quorum(3)
            .build()
            .expect("Failed to build election");

        let red = Slate::new("Red").unwrap();
        assert!(election.cast_vote(red.clone()).is_ok());
        assert!(election.cast_vote(red.clone()).is_ok());

        let nil = Slate::new("Nil").unwrap();
        assert!(election.cast_vote(nil).is_err());

        let result = election.close().compute_result().unwrap();
        assert_eq!(result, AcademicOutcome::QuorumNotMet);
    }

    #[test]
    fn test_academic_election_tie() {
        let mut election = AcademicElectionBuilder::new()
            .add_slate("A")
            .add_slate("B")
            .with_quorum(2)
            .build()
            .expect("Failed to build election");

        let a = Slate::new("A").unwrap();
        let b = Slate::new("B").unwrap();

        election.cast_vote(a.clone()).unwrap();
        election.cast_vote(b.clone()).unwrap();

        let result = election.close().compute_result().unwrap();
        assert_eq!(result, AcademicOutcome::Tie(vec![a, b]));
    }

    #[test]
    fn test_academic_election_winner() {
        let mut election = AcademicElectionBuilder::new()
            .add_slate("Overworld")
            .add_slate("Nether")
            .add_slate("End")
            .with_quorum(1)
            .build()
            .expect("Failed to build election");

        let overworld = Slate::new("Overworld").unwrap();
        let nether = Slate::new("Nether").unwrap();
        let end = Slate::new("End").unwrap();

        election.cast_vote(overworld.clone()).unwrap();
        election.cast_vote(overworld.clone()).unwrap();
        election.cast_vote(overworld.clone()).unwrap();
        election.cast_vote(nether).unwrap();
        election.cast_vote(end).unwrap();

        let result = election.close().compute_result().unwrap();
        assert_eq!(result, AcademicOutcome::Winner(overworld));
    }
}
