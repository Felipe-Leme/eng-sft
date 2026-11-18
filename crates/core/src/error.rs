use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum ElectionError {
    #[error("Unexpected vote cast: {}", .0)]
    InvalidVote(String),

    #[error("Unable to tally votes: {}", .0)]
    TallyFailed(String),
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum AcademicElectionError {
    #[error("Cannot start an election with zero slates")]
    NoSlatesRegistered,

    #[error("Invalid slate name: {0}")]
    InvalidSlateName(String),
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum BudgetElectionError {
    #[error("Cannot start an election with zero projects")]
    NoProjectsRegistered,

    #[error("Budget ceiling cannot be negative")]
    NegativeCeiling,

    #[error("Invalid project name: {0}")]
    InvalidProjectName(String),
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum CondoElectionError {
    #[error("Cannot start an election with zero options")]
    NoOptionsRegistered,

    #[error("Quorum must be between 0.0 and 1.0")]
    OutOfRangeQuorum,

    #[error("Invalid option name: {0}")]
    InvalidOptionName(String),
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum CustomElectionError {
    #[error("Custom election must have at least one option")]
    InvalidOptions,

    #[error("Custom election must have winner_count between 1 and total options")]
    OutOfRangeWinnerCount,

    #[error("Approval must have max_choices between 1 and total options")]
    OutOfRangeMaxChoices,

    #[error("Option names cannot be empty")]
    InvalidOptionName(String),

    #[error("Duplicate option names are not allowed")]
    DuplicateOptions,

    #[error("Minimum total weight cannot be negative")]
    NegativeMinimumWeight,
}

#[derive(Error, Debug, PartialEq, Eq)]
#[error("score must be greater equal to 0")]
pub struct ScoreError;

#[derive(Error, Debug, PartialEq, Eq)]
#[error("percentage must be between 0.0 and 100.0 (inclusive)")]
pub struct PercentageError;
