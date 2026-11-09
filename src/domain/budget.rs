use rust_decimal::Decimal;
use std::collections::{HashMap, HashSet};

use crate::{domain::strategy::TallyStrategy, error::ElectionError};

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

    fn validate(&self, vote: &Self::Vote) -> Result<(), crate::error::ElectionError> {
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
                let project = self
                    .available_projects
                    .get(proj_name)
                    .expect("project should exist");
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
    available_projects: HashMap<String, Project>,
    budget_ceiling: Decimal,
}

impl Default for BudgetElectionBuilder {
    fn default() -> Self {
        Self {
            available_projects: HashMap::new(),
            budget_ceiling: Decimal::ZERO,
        }
    }
}

impl BudgetElectionBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_project(mut self, name: &str, cost: Decimal) -> Self {
        self.available_projects.insert(
            name.to_string(),
            Project {
                name: name.to_string(),
                cost,
            },
        );
        self
    }

    pub fn with_ceiling(mut self, ceiling: Decimal) -> Self {
        self.budget_ceiling = ceiling;
        self
    }

    pub fn build(self) -> Result<BudgetStrategy, String> {
        if self.budget_ceiling < Decimal::ZERO {
            return Err("Budget ceiling cannot be negative".into());
        }

        if self.available_projects.is_empty() {
            return Err("Cannot build a budget election without projects".into());
        }

        Ok(BudgetStrategy {
            available_projects: self.available_projects,
            budget_ceiling: self.budget_ceiling,
        })
    }
}
