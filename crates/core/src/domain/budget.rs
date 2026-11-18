use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
};

use crate::{
    domain::{
        election::{Election, Open},
        scoreboard::Scoreboard,
        strategy::TallyStrategy,
    },
    error::{BudgetElectionError, ElectionError},
    types::Score,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub cost: Decimal,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetOutcome {
    pub selected_projects: Vec<Project>,
    pub total_spent: Decimal,
}

impl Display for BudgetOutcome {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(formatter, "Selected projects:")?;
        if self.selected_projects.is_empty() {
            writeln!(formatter, "  (none)")?;
        } else {
            for project in &self.selected_projects {
                writeln!(formatter, "  - {} (cost: {})", project.name, project.cost)?;
            }
        }
        write!(formatter, "Total spent: {}", self.total_spent)
    }
}

pub struct BudgetStrategy {
    available_projects: HashMap<String, Project>,
    budget_ceiling: Decimal,
}

impl TallyStrategy for BudgetStrategy {
    type Vote = Vec<String>;
    type Outcome = BudgetOutcome;

    fn validate(&self, vote: &Self::Vote) -> Result<(), ElectionError> {
        let mut seen = HashSet::new();
        for proj_name in vote {
            if !self.available_projects.contains_key(proj_name) {
                return Err(ElectionError::InvalidVote(format!(
                    "Unknown project: {}",
                    proj_name
                )));
            }

            if !seen.insert(proj_name) {
                return Err(ElectionError::InvalidVote(format!(
                    "Duplicate project: {}",
                    proj_name
                )));
            }
        }

        Ok(())
    }

    fn tally(&self, votes: &[Self::Vote]) -> Result<Self::Outcome, ElectionError> {
        let mut scoreboard = Scoreboard::new();

        for ranked_vote in votes {
            let n = ranked_vote.len();

            for (idx, proj_name) in ranked_vote.iter().enumerate() {
                let points = Score::new(Decimal::from(n - 1 - idx))
                    .map_err(|e| ElectionError::InvalidVote(e.to_string()))?;
                scoreboard.add_score(proj_name.clone(), points);
            }
        }

        let scored_projects = scoreboard.into_sorted_descending();

        let mut selected_projects = Vec::new();
        let mut total_spent = Decimal::ZERO;

        for (name, _) in scored_projects {
            let project = self.available_projects.get(&name).ok_or_else(|| {
                ElectionError::TallyFailed(format!("Scored project is not registered: {name}"))
            })?;
            if total_spent + project.cost <= self.budget_ceiling {
                selected_projects.push(project.clone());
                total_spent += project.cost;
            }
        }

        Ok(BudgetOutcome {
            selected_projects,
            total_spent,
        })
    }
}

pub struct BudgetElectionBuilder {
    projects_input: Vec<(String, Decimal)>,
    budget_ceiling: Decimal,
}

impl Default for BudgetElectionBuilder {
    fn default() -> Self {
        Self {
            projects_input: Vec::new(),
            budget_ceiling: Decimal::ZERO,
        }
    }
}

impl BudgetElectionBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_project(mut self, name: &str, cost: Decimal) -> Self {
        self.projects_input.push((name.to_string(), cost));
        self
    }

    pub fn with_ceiling(mut self, ceiling: Decimal) -> Self {
        self.budget_ceiling = ceiling;
        self
    }

    pub fn build(self) -> Result<Election<BudgetStrategy, Open>, BudgetElectionError> {
        if self.budget_ceiling < Decimal::ZERO {
            return Err(BudgetElectionError::NegativeCeiling);
        }

        if self.projects_input.is_empty() {
            return Err(BudgetElectionError::NoProjectsRegistered);
        }

        let mut available_projects = HashMap::new();

        for (name, cost) in self.projects_input {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return Err(BudgetElectionError::InvalidProjectName(name));
            }

            available_projects.insert(
                trimmed.to_string(),
                Project {
                    name: trimmed.to_string(),
                    cost,
                },
            );
        }

        let strategy = BudgetStrategy {
            available_projects,
            budget_ceiling: self.budget_ceiling,
        };

        Ok(Election::new(strategy))
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;

    use super::*;

    #[test]
    fn test_budget_success_lifecycle() {
        let mut election = BudgetElectionBuilder::new()
            .add_project("Park", dec!(50_000.0))
            .add_project("Library", dec!(30_000.0))
            .add_project("Roads", dec!(40_000.0))
            .with_ceiling(dec!(80_000.0))
            .build()
            .expect("Failed to build election");

        election
            .cast_vote(vec!["Roads".into(), "Park".into(), "Library".into()])
            .unwrap();
        election
            .cast_vote(vec!["Roads".into(), "Library".into(), "Park".into()])
            .unwrap();

        let result = election.close().compute_result().unwrap();

        assert_eq!(
            result.selected_projects,
            vec![
                Project {
                    name: "Roads".into(),
                    cost: dec!(40_000.0)
                },
                Project {
                    name: "Library".into(),
                    cost: dec!(30_000.0)
                }
            ]
        );
        assert_eq!(result.total_spent, dec!(70_000.0));
    }

    #[test]
    fn validates_budget_rules_and_respects_rank_scores() {
        assert!(matches!(
            BudgetElectionBuilder::new()
                .with_ceiling(dec!(-1))
                .add_project("A", dec!(1))
                .build(),
            Err(BudgetElectionError::NegativeCeiling)
        ));
        assert!(matches!(
            BudgetElectionBuilder::new().build(),
            Err(BudgetElectionError::NoProjectsRegistered)
        ));
        assert!(matches!(
            BudgetElectionBuilder::new()
                .add_project("  ", dec!(1))
                .with_ceiling(dec!(1))
                .build(),
            Err(BudgetElectionError::InvalidProjectName(_))
        ));

        let mut election = BudgetElectionBuilder::new()
            .add_project("A", dec!(1))
            .add_project("B", dec!(1))
            .with_ceiling(dec!(1))
            .build()
            .unwrap();
        assert!(matches!(
            election.cast_vote(vec!["Unknown".into()]),
            Err(ElectionError::InvalidVote(_))
        ));
        assert!(matches!(
            election.cast_vote(vec!["A".into(), "A".into()]),
            Err(ElectionError::InvalidVote(_))
        ));

        election.cast_vote(vec!["B".into(), "A".into()]).unwrap();
        let outcome = election.close().compute_result().unwrap();
        assert_eq!(
            outcome.selected_projects,
            vec![Project {
                name: "B".into(),
                cost: dec!(1)
            }]
        );
        assert_eq!(outcome.total_spent, dec!(1));
    }
}
