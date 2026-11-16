use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashSet;

use crate::{
    domain::{
        election::{Election, Open},
        scoreboard::Scoreboard,
        strategy::TallyStrategy,
    },
    error::{CondoElectionError, ElectionError},
    types::{Percentage, Score},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CondoVote {
    pub choice: String,
    #[schemars(with = "String")]
    #[serde(with = "fraction_serde")]
    pub fraction: Percentage,
}

mod fraction_serde {
    use super::*;

    pub fn serialize<S>(fraction: &Percentage, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serde::Serialize::serialize(&(fraction.inner() / Decimal::ONE_HUNDRED), serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Percentage, D::Error>
    where
        D: Deserializer<'de>,
    {
        let fraction = <Decimal as Deserialize>::deserialize(deserializer)?;
        let percentage = fraction
            .checked_mul(Decimal::ONE_HUNDRED)
            .ok_or_else(|| serde::de::Error::custom("Fraction is out of range"))?;
        Percentage::try_from(percentage).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CondoOutcome {
    Winner(String),
    Tie(Vec<String>),
    QuorumNotMet,
}

pub struct CondoStrategy {
    valid_options: HashSet<String>,
    minimum_fraction_quorum: Percentage,
}

impl TallyStrategy for CondoStrategy {
    type Vote = CondoVote;
    type Outcome = CondoOutcome;

    fn validate(&self, vote: &Self::Vote) -> Result<(), ElectionError> {
        if !self.valid_options.contains(&vote.choice) {
            return Err(ElectionError::InvalidVote(format!(
                "Option not recognized: {}",
                vote.choice
            )));
        }

        if vote.fraction <= Percentage::ZERO {
            return Err(ElectionError::InvalidVote(
                "Fraction must be strictly greater than 0 and up to 1.0".into(),
            ));
        }

        Ok(())
    }

    fn tally(&self, votes: &[Self::Vote]) -> Result<Self::Outcome, ElectionError> {
        let total_fraction = votes
            .iter()
            .fold(Score::ZERO, |acc, vote| acc + Score::from(vote.fraction));

        if total_fraction < Score::from(self.minimum_fraction_quorum) {
            return Ok(CondoOutcome::QuorumNotMet);
        }

        let mut scoreboard = Scoreboard::new();
        for vote in votes {
            scoreboard.add_score(vote.choice.clone(), Score::from(vote.fraction));
        }

        let (_, mut winners) = scoreboard.get_winners();

        if winners.len() > 1 {
            Ok(CondoOutcome::Tie(winners))
        } else if winners.len() == 1 {
            Ok(CondoOutcome::Winner(winners.remove(0)))
        } else {
            Ok(CondoOutcome::QuorumNotMet)
        }
    }
}

pub struct CondoElectionBuilder {
    options_input: Vec<String>,
    minimum_fraction_quorum: Percentage,
}

impl Default for CondoElectionBuilder {
    fn default() -> Self {
        Self {
            options_input: Vec::new(),
            minimum_fraction_quorum: Percentage::ZERO,
        }
    }
}

impl CondoElectionBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_option(mut self, choice: &str) -> Self {
        self.options_input.push(choice.to_string());
        self
    }

    pub fn with_quorum(mut self, quorum: Percentage) -> Self {
        self.minimum_fraction_quorum = quorum;
        self
    }

    pub fn build(self) -> Result<Election<CondoStrategy, Open>, CondoElectionError> {
        if self.options_input.is_empty() {
            return Err(CondoElectionError::NoOptionsRegistered);
        }

        let mut valid_options = HashSet::new();

        for opt in self.options_input {
            let trimmed = opt.trim();
            if trimmed.is_empty() {
                return Err(CondoElectionError::InvalidOptionName(opt));
            }
            valid_options.insert(trimmed.to_string());
        }

        let strategy = CondoStrategy {
            valid_options,
            minimum_fraction_quorum: self.minimum_fraction_quorum,
        };

        Ok(Election::new(strategy))
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;

    use super::*;

    #[test]
    fn test_condo_success_lifecycle() {
        let mut election = CondoElectionBuilder::new()
            .add_option("this")
            .add_option("that")
            .add_option("neither")
            .with_quorum(Percentage::try_from(dec!(51)).unwrap())
            .build()
            .expect("Failed to build election");

        election
            .cast_vote(CondoVote {
                choice: "this".into(),
                fraction: Percentage::try_from(dec!(10)).unwrap(),
            })
            .unwrap();
        election
            .cast_vote(CondoVote {
                choice: "that".into(),
                fraction: Percentage::try_from(dec!(20)).unwrap(),
            })
            .unwrap();
        election
            .cast_vote(CondoVote {
                choice: "neither".into(),
                fraction: Percentage::try_from(dec!(40)).unwrap(),
            })
            .unwrap();

        let result = election.close().compute_result().unwrap();

        assert_eq!(result, CondoOutcome::Winner("neither".into()));
    }

    #[test]
    fn vote_serialization_preserves_fractional_json_values() {
        let vote = CondoVote {
            choice: "A".into(),
            fraction: Percentage::try_from(dec!(75)).unwrap(),
        };
        let json = serde_json::to_string(&vote).unwrap();

        assert_eq!(json, r#"{"choice":"A","fraction":"0.75"}"#);
        assert_eq!(serde_json::from_str::<CondoVote>(&json).unwrap(), vote);
        assert!(serde_json::from_str::<CondoVote>(r#"{"choice":"A","fraction":"1.01"}"#).is_err());
    }

    #[test]
    fn handles_invalid_votes_quorum_and_ties() {
        assert!(matches!(
            CondoElectionBuilder::new().build(),
            Err(CondoElectionError::NoOptionsRegistered)
        ));
        assert!(matches!(
            CondoElectionBuilder::new().add_option(" ").build(),
            Err(CondoElectionError::InvalidOptionName(_))
        ));

        let mut below_quorum = CondoElectionBuilder::new()
            .add_option("A")
            .add_option("B")
            .with_quorum(Percentage::try_from(dec!(80)).unwrap())
            .build()
            .unwrap();
        assert!(matches!(
            below_quorum.cast_vote(CondoVote {
                choice: "Unknown".into(),
                fraction: Percentage::ONE_HUNDRED,
            }),
            Err(ElectionError::InvalidVote(_))
        ));
        assert!(matches!(
            below_quorum.cast_vote(CondoVote {
                choice: "A".into(),
                fraction: Percentage::ZERO,
            }),
            Err(ElectionError::InvalidVote(_))
        ));
        below_quorum
            .cast_vote(CondoVote {
                choice: "A".into(),
                fraction: Percentage::try_from(dec!(40)).unwrap(),
            })
            .unwrap();
        assert_eq!(
            below_quorum.close().compute_result().unwrap(),
            CondoOutcome::QuorumNotMet
        );

        let mut tied = CondoElectionBuilder::new()
            .add_option("A")
            .add_option("B")
            .with_quorum(Percentage::try_from(dec!(70)).unwrap())
            .build()
            .unwrap();
        for choice in ["A", "B"] {
            tied.cast_vote(CondoVote {
                choice: choice.into(),
                fraction: Percentage::try_from(dec!(40)).unwrap(),
            })
            .unwrap();
        }
        assert_eq!(
            tied.close().compute_result().unwrap(),
            CondoOutcome::Tie(vec!["A".into(), "B".into()])
        );
    }
}
