use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod academic;
pub mod budget;
pub mod condo;
pub mod custom;
pub mod election;
pub mod factory;
pub mod manifest;
pub mod scoreboard;
pub mod strategy;
pub mod vote;

pub use manifest::{BudgetProject, Manifest, TypedManifest};
pub use vote::{ElectionVote, TypedBallot};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElectionSummary {
    pub id: String,
    pub name: String,
    pub manifest: Manifest,
    pub is_closed: bool,
    pub outcome: Option<Value>,
}

#[cfg(test)]
mod outcome_display_tests {
    use rust_decimal::dec;

    use super::{
        academic::{AcademicOutcome, Slate},
        budget::{BudgetOutcome, Project},
        condo::CondoOutcome,
        custom::CustomOutcome,
    };

    #[test]
    fn election_outcomes_have_human_readable_display() {
        assert_eq!(
            AcademicOutcome::Winner(Slate::new("A").unwrap()).to_string(),
            "Winner: A"
        );
        assert_eq!(
            CondoOutcome::Tie(vec!["A".to_owned(), "B".to_owned()]).to_string(),
            "Tie: A, B"
        );
        assert_eq!(
            CustomOutcome::Winners(vec!["A".to_owned(), "B".to_owned()]).to_string(),
            "Winners: A, B"
        );

        let budget = BudgetOutcome {
            selected_projects: vec![Project {
                name: "Library".to_owned(),
                cost: dec!(10),
            }],
            total_spent: dec!(10),
        };
        assert_eq!(
            budget.to_string(),
            "Selected projects:\n  - Library (cost: 10)\nTotal spent: 10"
        );
    }
}
