use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::{
    condo::CondoVote,
    custom::{CustomRuleSet, CustomVote},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Manifest {
    Typed(TypedManifest),
    Custom(crate::domain::custom::CustomElectionConfig),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TypedManifest {
    Academic {
        slates: Vec<String>,
        #[serde(default = "default_minimum_quorum")]
        minimum_quorum: usize,
        votes: Vec<String>,
    },
    Budget {
        projects: Vec<BudgetProject>,
        #[schemars(with = "String")]
        budget_ceiling: Decimal,
        votes: Vec<Vec<String>>,
    },
    Condo {
        options: Vec<String>,
        #[serde(default)]
        #[schemars(with = "String")]
        minimum_fraction_quorum: Decimal,
        votes: Vec<CondoVote>,
    },
    Custom {
        rules: CustomRuleSet,
        votes: Vec<CustomVote>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BudgetProject {
    pub name: String,
    #[schemars(with = "String")]
    pub cost: Decimal,
}

fn default_minimum_quorum() -> usize {
    1
}
