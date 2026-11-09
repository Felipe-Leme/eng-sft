use rust_decimal::Decimal;
use std::collections::{HashMap, HashSet};

use crate::{domain::strategy::TallyStrategy, error::ElectionError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CondoVote {
    pub choice: String,
    pub fraction: Decimal,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CondoOutcome {
    Winner(String),
    Tie(Vec<String>),
    QuorumNotMet,
}

pub struct CondoStrategy {
    valid_options: HashSet<String>,
    minimum_fraction_quorum: Decimal,
}

impl TallyStrategy for CondoStrategy {
    type Vote = CondoVote;
    type Outcome = CondoOutcome;

    fn validate(&self, vote: &Self::Vote) -> Result<(), crate::error::ElectionError> {
        if !self.valid_options.contains(&vote.choice) {
            return Err(ElectionError::InvalidVote(format!(
                "Option not recognized: {}",
                vote.choice
            )));
        }

        if vote.fraction <= Decimal::ZERO || vote.fraction > Decimal::ONE {
            return Err(ElectionError::InvalidVote(
                "Fraction must be strictly greater than 0 and up to 1.0".into(),
            ));
        }

        Ok(())
    }

    fn tally(&self, votes: &[Self::Vote]) -> Result<Self::Outcome, ElectionError> {
        let total_fraction: Decimal = votes.iter().map(|v| v.fraction).sum();

        if total_fraction < self.minimum_fraction_quorum {
            return Ok(CondoOutcome::QuorumNotMet);
        }

        let mut counts = HashMap::new();
        for vote in votes {
            *counts.entry(&vote.choice).or_insert(Decimal::ZERO) += vote.fraction;
        }

        let mut max_weight = Decimal::ZERO;
        let mut winners = Vec::new();

        for (choice, &weight) in counts.iter() {
            if weight > max_weight {
                max_weight = weight;
                winners.clear();
                winners.push((*choice).clone());
            } else if weight == max_weight {
                winners.push((*choice).clone());
            }
        }

        if winners.len() == 1 {
            Ok(CondoOutcome::Winner(winners.remove(0)))
        } else {
            winners.sort();
            Ok(CondoOutcome::Tie(winners))
        }
    }
}

pub struct CondoElectionBuilder {
    valid_options: HashSet<String>,
    minimum_fraction_quorum: Decimal,
}

impl Default for CondoElectionBuilder {
    fn default() -> Self {
        Self {
            valid_options: HashSet::new(),
            minimum_fraction_quorum: Decimal::ZERO,
        }
    }
}

impl CondoElectionBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_option(mut self, choice: &str) -> Self {
        self.valid_options.insert(choice.to_string());
        self
    }

    pub fn with_quorum(mut self, quorum: Decimal) -> Self {
        self.minimum_fraction_quorum = quorum;
        self
    }

    pub fn build(self) -> Result<CondoStrategy, String> {
        if self.valid_options.is_empty() {
            return Err("Must have at least one valid option".into());
        }

        if self.minimum_fraction_quorum < Decimal::ZERO
            || self.minimum_fraction_quorum > Decimal::ONE
        {
            return Err("Quorum must be between 0.0 and 1.0".into());
        }

        Ok(CondoStrategy {
            valid_options: self.valid_options,
            minimum_fraction_quorum: self.minimum_fraction_quorum,
        })
    }
}
