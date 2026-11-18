use std::{fs, path::Path};

use rust_decimal::Decimal;
use serde::Serialize;
use thiserror::Error;

use crate::{
    domain::{
        academic::{AcademicElectionBuilder, Slate},
        budget::BudgetElectionBuilder,
        condo::{CondoElectionBuilder, CondoVote},
        custom::CustomRuleStrategy,
        election::{Election, Open},
        manifest::{Manifest, TypedManifest},
        strategy::TallyStrategy,
        vote::{ElectionVote, TypedBallot},
    },
    error::{
        AcademicElectionError, BudgetElectionError, CondoElectionError, CustomElectionError,
        ElectionError,
    },
    types::Percentage,
};

#[derive(Debug, Error)]
pub enum ElectionFactoryError {
    #[error("Failed to read election manifest: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid election manifest: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Academic(#[from] AcademicElectionError),
    #[error(transparent)]
    Budget(#[from] BudgetElectionError),
    #[error(transparent)]
    Condo(#[from] CondoElectionError),
    #[error(transparent)]
    Custom(#[from] CustomElectionError),
    #[error(transparent)]
    Election(#[from] ElectionError),
}

pub struct ElectionFactory;

pub trait ElectionBoundary: Send + Sync {
    fn cast_vote(&mut self, vote: ElectionVote) -> Result<(), String>;
    fn close(self: Box<Self>) -> Result<serde_json::Value, String>;
}

pub type OpenElection = Box<dyn ElectionBoundary>;

impl<S> ElectionBoundary for Election<S, Open>
where
    S: TallyStrategy + Send + Sync + 'static,
    S::Vote: TypedBallot + Send + Sync,
    S::Outcome: Serialize,
{
    fn cast_vote(&mut self, vote: ElectionVote) -> Result<(), String> {
        let vote = S::Vote::from_election_vote(vote)?;
        Election::cast_vote(self, vote).map_err(|error| error.to_string())
    }

    fn close(self: Box<Self>) -> Result<serde_json::Value, String> {
        let result = (*self)
            .close()
            .compute_result()
            .map_err(|error| error.to_string())?;
        serde_json::to_value(result).map_err(|error| error.to_string())
    }
}

impl ElectionFactory {
    pub fn from_file(path: impl AsRef<Path>) -> Result<OpenElection, ElectionFactoryError> {
        let json = fs::read_to_string(path)?;
        Self::from_json(&json)
    }

    pub fn from_json(json: &str) -> Result<OpenElection, ElectionFactoryError> {
        Self::create(Self::parse_manifest(json)?)
    }

    pub fn parse_manifest(json: &str) -> Result<Manifest, ElectionFactoryError> {
        let value: serde_json::Value = serde_json::from_str(json)?;
        let manifest = if value.get("type").is_some() {
            Manifest::Typed(serde_json::from_value(value)?)
        } else {
            Manifest::Custom(serde_json::from_value(value)?)
        };
        Ok(manifest)
    }

    pub fn create(manifest: Manifest) -> Result<OpenElection, ElectionFactoryError> {
        match manifest {
            Manifest::Typed(TypedManifest::Academic {
                slates,
                minimum_quorum,
                votes,
            }) => {
                let builder = slates
                    .iter()
                    .fold(AcademicElectionBuilder::new(), |builder, slate| {
                        builder.add_slate(slate)
                    })
                    .with_quorum(minimum_quorum);
                let mut election = builder.build()?;
                for vote in votes {
                    election.cast_vote(Slate::new(&vote)?)?;
                }
                Ok(Box::new(election))
            }
            Manifest::Typed(TypedManifest::Budget {
                projects,
                budget_ceiling,
                votes,
            }) => {
                let builder = projects
                    .iter()
                    .fold(BudgetElectionBuilder::new(), |builder, project| {
                        builder.add_project(&project.name, project.cost)
                    })
                    .with_ceiling(budget_ceiling);
                let mut election = builder.build()?;
                for vote in votes {
                    election.cast_vote(vote)?;
                }
                Ok(Box::new(election))
            }
            Manifest::Typed(TypedManifest::Condo {
                options,
                minimum_fraction_quorum,
                votes,
            }) => {
                let builder = options
                    .iter()
                    .fold(CondoElectionBuilder::new(), |builder, option| {
                        builder.add_option(option)
                    })
                    .with_quorum(
                        Percentage::try_from(
                            minimum_fraction_quorum
                                .checked_mul(Decimal::ONE_HUNDRED)
                                .ok_or(CondoElectionError::OutOfRangeQuorum)?,
                        )
                        .map_err(|_| CondoElectionError::OutOfRangeQuorum)?,
                    );
                let mut election = builder.build()?;
                for vote in votes {
                    election.cast_vote(CondoVote {
                        choice: vote.choice,
                        fraction: vote.fraction,
                    })?;
                }
                Ok(Box::new(election))
            }
            Manifest::Typed(TypedManifest::Custom { rules, votes })
            | Manifest::Custom(crate::domain::custom::CustomElectionConfig { rules, votes }) => {
                let strategy = CustomRuleStrategy::new(rules)?;
                let mut election = Election::new(strategy);
                for vote in votes {
                    election.cast_vote(vote)?;
                }
                Ok(Box::new(election))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_tallies_each_standard_election_type() {
        let mut academic = ElectionFactory::from_json(
            r#"{"type":"academic","slates":["A","B"],"minimum_quorum":2,"votes":["A","A"]}"#,
        )
        .unwrap();
        academic
            .cast_vote(ElectionVote::Academic(Slate::new("A").unwrap()))
            .unwrap();
        let academic = academic.close().unwrap();
        assert_eq!(academic, serde_json::json!({"Winner": "A"}));

        let budget = ElectionFactory::from_json(
            r#"{"type":"budget","projects":[{"name":"P","cost":"2"}],"budget_ceiling":"2","votes":[["P"]]}"#,
        )
        .unwrap();
        let budget = budget.close().unwrap();
        assert_eq!(
            budget["selected_projects"][0]["name"],
            serde_json::json!("P")
        );

        let condo = ElectionFactory::from_json(
            r#"{"type":"condo","options":["A","B"],"minimum_fraction_quorum":"0.5","votes":[{"choice":"A","fraction":"0.75"}]}"#,
        )
        .unwrap();
        let condo = condo.close().unwrap();
        assert_eq!(condo, serde_json::json!({"Winner": "A"}));
    }

    #[test]
    fn loads_the_existing_untagged_custom_manifest() {
        let json = r#"{
            "rules": {
                "options": ["VERY EVIL CANDIDATE", "SLIGHTLY LESS EVIL CANDIDATE"],
                "scoring": {"type": "plurality"},
                "weighting": {"type": "uniform"},
                "quorum": {"type": "min_ballots", "min": 3},
                "tie_breaker": {"type": "registration_order"},
                "winner_count": 1
            },
            "votes": [
                {"choices": ["VERY EVIL CANDIDATE"]},
                {"choices": ["SLIGHTLY LESS EVIL CANDIDATE"]},
                {"choices": ["SLIGHTLY LESS EVIL CANDIDATE"]}
            ]
        }"#;
        let election = ElectionFactory::from_json(json).unwrap();
        let result = election.close().unwrap();
        assert_eq!(
            result,
            serde_json::json!({"Winners": ["SLIGHTLY LESS EVIL CANDIDATE"]})
        );

        let mut tagged: serde_json::Value = serde_json::from_str(json).unwrap();
        tagged["type"] = "custom".into();
        let election = ElectionFactory::from_json(&tagged.to_string()).unwrap();
        assert_eq!(election.close().unwrap(), result);
    }

    #[test]
    fn rejects_votes_for_another_election_type() {
        let mut election =
            ElectionFactory::from_json(r#"{"type":"academic","slates":["A"],"votes":[]}"#).unwrap();
        assert!(matches!(
            election.cast_vote(ElectionVote::Budget(vec!["P".to_owned()])),
            Err(message) if message.contains("Ballot type does not match")
        ));
    }

    #[test]
    fn rejects_unknown_manifest_type() {
        assert!(matches!(
            ElectionFactory::from_json(r#"{"type":"unknown","rules":{},"votes":[]}"#),
            Err(ElectionFactoryError::Json(_))
        ));
    }

    #[test]
    fn academic_manifest_defaults_to_one_vote_quorum() {
        let manifest =
            ElectionFactory::parse_manifest(r#"{"type":"academic","slates":["A"],"votes":[]}"#)
                .unwrap();
        assert!(matches!(
            manifest,
            Manifest::Typed(TypedManifest::Academic {
                minimum_quorum: 1,
                ..
            })
        ));
    }
}
