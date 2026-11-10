use rust_decimal::Decimal;
use std::collections::{HashMap, HashSet};

use crate::{
    domain::{
        election::{Election, Open},
        strategy::TallyStrategy,
    },
    error::{BudgetElectionError, ElectionError},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Project {
    pub name: String,
    pub cost: Decimal,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BudgetOutcome {
    pub selected_projects: Vec<Project>,
    pub total_spent: Decimal,
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
        let mut scores: HashMap<&Project, usize> = HashMap::new();

        for ranked_vote in votes {
            let n = ranked_vote.len();

            for (idx, proj_name) in ranked_vote.iter().enumerate() {
                let project = self.available_projects.get(proj_name).unwrap();
                let points = n - 1 - idx;
                *scores.entry(project).or_insert(0) += points;
            }
        }

        let mut scored_projects: Vec<_> = scores.into_iter().collect();

        scored_projects.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name.cmp(&b.0.name)));

        let mut selected_projects = Vec::new();
        let mut total_spent = Decimal::ZERO;

        for (project, _) in scored_projects {
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
}
