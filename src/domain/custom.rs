use std::collections::{HashMap, HashSet};

use rust_decimal::{Decimal, dec, prelude::FromPrimitive};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    domain::{election::Election, strategy::TallyStrategy},
    error::{CustomElectionError, ElectionError},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScoringRule {
    /// Voter picks exactly 1 option; it receives one point per unit of weight.
    Plurality,

    /// Voter picks 1..=max_choices options; each selected option receives one point per unit of weight.
    Approval {
        /// Maximum options each voter may select; must be at least 1.
        #[schemars(range(min = 1))]
        max_choices: usize,
    },

    /// Voter ranks 1..=options.len() choices; rank i gets (N - 1 - i) * weight.
    RankedBorda,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WeightingRule {
    /// Every valid ballot has weight 1 (ignores ballot weight field).
    Uniform,

    /// Ballot weight is multiplied into the score (must be positive).
    Weighted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QuorumRule {
    None,
    MinBallots {
        min: usize,
    },
    /// Minimum participating weight, represented as a decimal string (for example `"0.75"`).
    MinTotalWeight {
        /// Non-negative decimal string, for example `"1.25"`.
        #[schemars(with = "String")]
        min: Decimal,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TieBreakerRule {
    /// Returns CustomOutcome::Tie when a tie crosses the winner cutoff.
    AllowTie,

    /// Breaks ties lexicographically (A -> Z).
    Alphabetical,

    /// Breaks ties using the order options were registered in the election config.
    RegistrationOrder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CustomRuleSet {
    /// Registered options. Their order is used by `registration_order` ties.
    #[schemars(length(min = 1))]
    pub options: Vec<String>,
    pub scoring: ScoringRule,
    pub weighting: WeightingRule,
    pub quorum: QuorumRule,
    pub tie_breaker: TieBreakerRule,
    /// Number of winners to return. Defaults to 1 when omitted.
    #[serde(default = "default_winner_count")]
    #[schemars(range(min = 1), default = "default_winner_count")]
    pub winner_count: usize,
}

fn default_winner_count() -> usize {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CustomVote {
    /// Selected or ranked options, in ballot order. Names must match registered options.
    #[schemars(length(min = 1), inner(length(min = 1)))]
    pub choices: Vec<String>,
    /// Positive decimal string when weighted voting is enabled; defaults to `"1.0"`.
    #[serde(default = "default_weight")]
    #[schemars(with = "String", default = "default_weight")]
    pub weight: Decimal,
}

fn default_weight() -> Decimal {
    dec!(1.0)
}

/// JSON configuration accepted by the custom-election runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CustomElectionConfig {
    pub rules: CustomRuleSet,
    /// Every ballot is validated and cast before the election is tallied.
    pub votes: Vec<CustomVote>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CustomOutcome {
    Winners(Vec<String>),
    Tie(Vec<String>),
    QuorumNotMet,
}

#[derive(Debug)]
pub struct CustomRuleStrategy {
    rules: CustomRuleSet,
    valid_options: HashSet<String>,
}

pub type CustomElection<State> = Election<CustomRuleStrategy, State>;

impl CustomRuleStrategy {
    pub fn new(rules: CustomRuleSet) -> Result<Self, CustomElectionError> {
        if rules.options.is_empty() {
            return Err(CustomElectionError::InvalidOptions);
        }

        if let Some(option) = rules.options.iter().find(|option| option.trim().is_empty()) {
            return Err(CustomElectionError::InvalidOptionName(option.clone()));
        }

        if rules.winner_count == 0 || rules.winner_count > rules.options.len() {
            return Err(CustomElectionError::OutOfRangeWinnerCount);
        }

        if let ScoringRule::Approval { max_choices } = &rules.scoring
            && (*max_choices == 0 || *max_choices > rules.options.len())
        {
            return Err(CustomElectionError::OutOfRangeMaxChoices);
        }

        if let QuorumRule::MinTotalWeight { min } = &rules.quorum
            && min < &Decimal::ZERO
        {
            return Err(CustomElectionError::NegativeMinimumWeight);
        }

        let valid_options: HashSet<String> = rules.options.iter().cloned().collect();
        if valid_options.len() != rules.options.len() {
            return Err(CustomElectionError::DuplicateOptions);
        }

        Ok(Self {
            rules,
            valid_options,
        })
    }

    fn effective_weight(&self, vote: &CustomVote) -> Decimal {
        match self.rules.weighting {
            WeightingRule::Uniform => dec!(1.0),
            WeightingRule::Weighted => vote.weight,
        }
    }
}

impl TallyStrategy for CustomRuleStrategy {
    type Vote = CustomVote;
    type Outcome = CustomOutcome;

    fn validate(&self, vote: &Self::Vote) -> Result<(), ElectionError> {
        if self.rules.weighting == WeightingRule::Weighted && vote.weight <= Decimal::ZERO {
            return Err(ElectionError::InvalidVote(
                "Weighted vote must have positive weight".into(),
            ));
        }

        match self.rules.scoring {
            ScoringRule::Plurality => {
                if vote.choices.len() != 1 {
                    return Err(ElectionError::InvalidVote(
                        "Plurality requires exactly 1 choice".into(),
                    ));
                }
            }
            ScoringRule::Approval { max_choices } => {
                if vote.choices.is_empty() || vote.choices.len() > max_choices {
                    return Err(ElectionError::InvalidVote(format!(
                        "Approval requires between 1 and {max_choices} choices"
                    )));
                }
            }
            ScoringRule::RankedBorda => {
                if vote.choices.is_empty() || vote.choices.len() > self.rules.options.len() {
                    return Err(ElectionError::InvalidVote(
                        "Ranked ballot has invalid number of choices".into(),
                    ));
                }
            }
        }

        let mut seen = HashSet::new();
        for choice in &vote.choices {
            if !self.valid_options.contains(choice) {
                return Err(ElectionError::InvalidVote(format!(
                    "Unknown option: {choice}"
                )));
            }
            if !seen.insert(choice) {
                return Err(ElectionError::InvalidVote(format!(
                    "Duplicate option in ballot: {choice}"
                )));
            }
        }

        Ok(())
    }

    fn tally(&self, votes: &[Self::Vote]) -> Result<Self::Outcome, ElectionError> {
        for vote in votes {
            self.validate(vote)?;
        }

        match self.rules.quorum {
            QuorumRule::None => {}
            QuorumRule::MinBallots { min } => {
                if votes.len() < min {
                    return Ok(CustomOutcome::QuorumNotMet);
                }
            }
            QuorumRule::MinTotalWeight { min } => {
                let total_weight = votes.iter().try_fold(Decimal::ZERO, |total, vote| {
                    total
                        .checked_add(self.effective_weight(vote))
                        .ok_or_else(|| {
                            ElectionError::TallyFailed("Total vote weight overflowed".into())
                        })
                })?;
                if total_weight < min {
                    return Ok(CustomOutcome::QuorumNotMet);
                }
            }
        }

        let mut scores: HashMap<&String, Decimal> = self
            .rules
            .options
            .iter()
            .map(|opt| (opt, dec!(0.0)))
            .collect();

        for vote in votes {
            let weight = self.effective_weight(vote);

            match self.rules.scoring {
                ScoringRule::Plurality | ScoringRule::Approval { .. } => {
                    for choice in &vote.choices {
                        let score = scores.get_mut(choice).expect("validated vote option");
                        *score = score.checked_add(weight).ok_or_else(|| {
                            ElectionError::TallyFailed("Option score overflowed".into())
                        })?;
                    }
                }
                ScoringRule::RankedBorda => {
                    for (idx, choice) in vote.choices.iter().enumerate() {
                        let points = Decimal::from_usize(self.rules.options.len() - 1 - idx)
                            .ok_or_else(|| {
                                ElectionError::TallyFailed("Borda score is out of range".into())
                            })?;
                        let weighted_points = points.checked_mul(weight).ok_or_else(|| {
                            ElectionError::TallyFailed("Weighted Borda score overflowed".into())
                        })?;
                        let score = scores.get_mut(choice).expect("validated vote option");
                        *score = score.checked_add(weighted_points).ok_or_else(|| {
                            ElectionError::TallyFailed("Option score overflowed".into())
                        })?;
                    }
                }
            }
        }

        let mut ranked: Vec<(&String, Decimal)> = scores.into_iter().collect();
        ranked.sort_by(|(opt_a, score_a), (opt_b, score_b)| {
            score_b
                .cmp(score_a)
                .then_with(|| match self.rules.tie_breaker {
                    TieBreakerRule::Alphabetical => opt_a.cmp(opt_b),
                    TieBreakerRule::RegistrationOrder | TieBreakerRule::AllowTie => {
                        let idx_a = self.rules.options.iter().position(|x| x == *opt_a);
                        let idx_b = self.rules.options.iter().position(|x| x == *opt_b);
                        idx_a.cmp(&idx_b)
                    }
                })
        });

        let cutoff_idx = self.rules.winner_count - 1;
        let cutoff_score = ranked[cutoff_idx].1;

        if self.rules.tie_breaker == TieBreakerRule::AllowTie
            && ranked.len() > self.rules.winner_count
            && ranked[self.rules.winner_count].1 == cutoff_score
        {
            let tied: Vec<String> = ranked
                .into_iter()
                .filter(|(_, score)| *score == cutoff_score)
                .map(|(opt, _)| opt.clone())
                .collect();
            return Ok(CustomOutcome::Tie(tied));
        }

        let winners = ranked
            .into_iter()
            .take(self.rules.winner_count)
            .map(|(opt, _)| opt.clone())
            .collect();

        Ok(CustomOutcome::Winners(winners))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(scoring: ScoringRule) -> CustomRuleSet {
        CustomRuleSet {
            options: vec!["A".into(), "B".into(), "C".into()],
            scoring,
            weighting: WeightingRule::Uniform,
            quorum: QuorumRule::None,
            tie_breaker: TieBreakerRule::RegistrationOrder,
            winner_count: 1,
        }
    }

    fn vote(choices: &[&str]) -> CustomVote {
        CustomVote {
            choices: choices.iter().map(|choice| (*choice).into()).collect(),
            weight: Decimal::ONE,
        }
    }

    fn tally(rules: CustomRuleSet, votes: &[CustomVote]) -> CustomOutcome {
        CustomRuleStrategy::new(rules)
            .unwrap()
            .tally(votes)
            .unwrap()
    }

    #[test]
    fn rejects_invalid_rule_sets() {
        let mut config = rules(ScoringRule::Plurality);
        config.options.clear();
        assert_eq!(
            CustomRuleStrategy::new(config).unwrap_err(),
            CustomElectionError::InvalidOptions
        );

        let mut config = rules(ScoringRule::Plurality);
        config.options[1] = "A".into();
        assert_eq!(
            CustomRuleStrategy::new(config).unwrap_err(),
            CustomElectionError::DuplicateOptions
        );

        let mut config = rules(ScoringRule::Plurality);
        config.options[1] = "  ".into();
        assert!(matches!(
            CustomRuleStrategy::new(config),
            Err(CustomElectionError::InvalidOptionName(_))
        ));

        let mut config = rules(ScoringRule::Plurality);
        config.winner_count = 0;
        assert_eq!(
            CustomRuleStrategy::new(config).unwrap_err(),
            CustomElectionError::OutOfRangeWinnerCount
        );

        let mut config = rules(ScoringRule::Approval { max_choices: 0 });
        assert_eq!(
            CustomRuleStrategy::new(config.clone()).unwrap_err(),
            CustomElectionError::OutOfRangeMaxChoices
        );
        config.scoring = ScoringRule::Approval { max_choices: 4 };
        assert_eq!(
            CustomRuleStrategy::new(config).unwrap_err(),
            CustomElectionError::OutOfRangeMaxChoices
        );

        let mut config = rules(ScoringRule::Plurality);
        config.quorum = QuorumRule::MinTotalWeight { min: dec!(-1) };
        assert_eq!(
            CustomRuleStrategy::new(config).unwrap_err(),
            CustomElectionError::NegativeMinimumWeight
        );
    }

    #[test]
    fn validates_ballot_shape_options_and_duplicate_choices() {
        let strategy =
            CustomRuleStrategy::new(rules(ScoringRule::Approval { max_choices: 2 })).unwrap();

        assert!(strategy.validate(&vote(&[])).is_err());
        assert!(strategy.validate(&vote(&["A", "B", "C"])).is_err());
        assert!(strategy.validate(&vote(&["A", "A"])).is_err());
        assert!(strategy.validate(&vote(&["unknown"])).is_err());
        assert!(strategy.validate(&vote(&["A", "B"])).is_ok());

        let strategy = CustomRuleStrategy::new(rules(ScoringRule::Plurality)).unwrap();
        assert!(strategy.validate(&vote(&["A", "B"])).is_err());
        assert!(strategy.validate(&vote(&["A"])).is_ok());
    }

    #[test]
    fn applies_plurality_and_approval_scores() {
        let plurality = tally(
            rules(ScoringRule::Plurality),
            &[vote(&["B"]), vote(&["A"]), vote(&["B"])],
        );
        assert_eq!(plurality, CustomOutcome::Winners(vec!["B".into()]));

        let approval = tally(
            rules(ScoringRule::Approval { max_choices: 2 }),
            &[vote(&["A", "B"]), vote(&["B"])],
        );
        assert_eq!(approval, CustomOutcome::Winners(vec!["B".into()]));
    }

    #[test]
    fn applies_borda_points_using_the_number_of_registered_options() {
        let outcome = tally(
            rules(ScoringRule::RankedBorda),
            &[vote(&["A"]), vote(&["B", "C"])],
        );

        assert_eq!(outcome, CustomOutcome::Winners(vec!["A".into()]));
    }

    #[test]
    fn uses_exact_decimal_weights_for_scores_and_quorum() {
        let mut config = rules(ScoringRule::Plurality);
        config.weighting = WeightingRule::Weighted;
        config.quorum = QuorumRule::MinTotalWeight { min: dec!(0.6) };
        config.tie_breaker = TieBreakerRule::AllowTie;
        let strategy = CustomRuleStrategy::new(config).unwrap();
        let votes = [
            CustomVote {
                choices: vec!["A".into()],
                weight: dec!(0.1),
            },
            CustomVote {
                choices: vec!["A".into()],
                weight: dec!(0.2),
            },
            CustomVote {
                choices: vec!["B".into()],
                weight: dec!(0.3),
            },
        ];

        assert_eq!(
            strategy.tally(&votes).unwrap(),
            CustomOutcome::Tie(vec!["A".into(), "B".into()])
        );
        assert_eq!(
            strategy.tally(&votes[..2]).unwrap(),
            CustomOutcome::QuorumNotMet
        );
    }

    #[test]
    fn enforces_weighted_vote_weights_but_ignores_them_for_uniform_voting() {
        let mut config = rules(ScoringRule::Plurality);
        config.weighting = WeightingRule::Weighted;
        let weighted = CustomRuleStrategy::new(config).unwrap();
        let invalid = CustomVote {
            choices: vec!["A".into()],
            weight: Decimal::ZERO,
        };
        assert!(weighted.validate(&invalid).is_err());
        assert!(weighted.tally(&[invalid]).is_err());

        let uniform = CustomRuleStrategy::new(rules(ScoringRule::Plurality)).unwrap();
        let ignored_weight = CustomVote {
            choices: vec!["A".into()],
            weight: dec!(-10),
        };
        assert_eq!(
            uniform.tally(&[ignored_weight]).unwrap(),
            CustomOutcome::Winners(vec!["A".into()])
        );
    }

    #[test]
    fn applies_all_tie_breaker_rules_at_the_winner_cutoff() {
        let votes = [vote(&["B"]), vote(&["A"])];

        let mut config = rules(ScoringRule::Plurality);
        config.tie_breaker = TieBreakerRule::AllowTie;
        assert_eq!(
            tally(config, &votes),
            CustomOutcome::Tie(vec!["A".into(), "B".into()])
        );

        let mut config = rules(ScoringRule::Plurality);
        config.tie_breaker = TieBreakerRule::Alphabetical;
        assert_eq!(
            tally(config, &votes),
            CustomOutcome::Winners(vec!["A".into()])
        );

        let mut config = rules(ScoringRule::Plurality);
        config.tie_breaker = TieBreakerRule::RegistrationOrder;
        assert_eq!(
            tally(config, &votes),
            CustomOutcome::Winners(vec!["A".into()])
        );
    }

    #[test]
    fn supports_multiple_winners_and_ballot_count_quorum() {
        let mut config = rules(ScoringRule::Plurality);
        config.winner_count = 2;
        config.quorum = QuorumRule::MinBallots { min: 2 };
        config.tie_breaker = TieBreakerRule::AllowTie;
        assert_eq!(
            tally(config.clone(), &[vote(&["B"])]),
            CustomOutcome::QuorumNotMet
        );
        assert_eq!(
            tally(config, &[vote(&["B"]), vote(&["A"])]),
            CustomOutcome::Winners(vec!["A".into(), "B".into()])
        );

        let mut config = rules(ScoringRule::Plurality);
        config.winner_count = 2;
        config.tie_breaker = TieBreakerRule::AllowTie;
        assert_eq!(
            tally(config, &[vote(&["A"]), vote(&["B"]), vote(&["C"])]),
            CustomOutcome::Tie(vec!["A".into(), "B".into(), "C".into()])
        );
    }

    #[test]
    fn serde_defaults_weight_and_winner_count() {
        let vote: CustomVote = serde_json::from_str(r#"{"choices":["A"]}"#).unwrap();
        assert_eq!(vote.weight, Decimal::ONE);

        let config = rules(ScoringRule::Plurality);
        let json = serde_json::to_value(&config).unwrap();
        let mut json = json;
        json.as_object_mut().unwrap().remove("winner_count");
        let config: CustomRuleSet = serde_json::from_value(json).unwrap();
        assert_eq!(config.winner_count, 1);
    }

    #[test]
    fn ranked_borda_validates_ballots_and_returns_winners_at_the_cutoff() {
        let strategy = CustomRuleStrategy::new(rules(ScoringRule::RankedBorda)).unwrap();
        assert!(strategy.validate(&vote(&[])).is_err());
        assert!(strategy.validate(&vote(&["A", "B", "C", "A"])).is_err());
        assert!(strategy.validate(&vote(&["A", "B", "C"])).is_ok());

        assert_eq!(
            tally(rules(ScoringRule::RankedBorda), &[vote(&["A", "B", "C"])]),
            CustomOutcome::Winners(vec!["A".into()])
        );

        let mut config = rules(ScoringRule::Plurality);
        config.winner_count = config.options.len();
        config.tie_breaker = TieBreakerRule::AllowTie;
        assert_eq!(
            tally(config, &[vote(&["A"]), vote(&["B"]), vote(&["C"])]),
            CustomOutcome::Winners(vec!["A".into(), "B".into(), "C".into()])
        );
    }

    #[test]
    fn reports_weight_and_score_overflow() {
        let mut config = rules(ScoringRule::Plurality);
        config.weighting = WeightingRule::Weighted;
        config.quorum = QuorumRule::MinTotalWeight { min: Decimal::ZERO };
        let strategy = CustomRuleStrategy::new(config.clone()).unwrap();
        let maximum_weight_vote = CustomVote {
            choices: vec!["A".into()],
            weight: Decimal::MAX,
        };
        assert!(matches!(
            strategy.tally(&[maximum_weight_vote.clone(), maximum_weight_vote]),
            Err(ElectionError::TallyFailed(message)) if message == "Total vote weight overflowed"
        ));

        config.quorum = QuorumRule::None;
        let strategy = CustomRuleStrategy::new(config).unwrap();
        let maximum_weight_vote = CustomVote {
            choices: vec!["A".into()],
            weight: Decimal::MAX,
        };
        assert!(matches!(
            strategy.tally(&[maximum_weight_vote.clone(), maximum_weight_vote]),
            Err(ElectionError::TallyFailed(message)) if message == "Option score overflowed"
        ));

        let mut config = rules(ScoringRule::RankedBorda);
        config.weighting = WeightingRule::Weighted;
        let strategy = CustomRuleStrategy::new(config).unwrap();
        let maximum_weight_vote = CustomVote {
            choices: vec!["A".into()],
            weight: Decimal::MAX,
        };
        assert!(matches!(
            strategy.tally(&[maximum_weight_vote]),
            Err(ElectionError::TallyFailed(message)) if message == "Weighted Borda score overflowed"
        ));
    }
}
