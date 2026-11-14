use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum ElectionError {
    #[error("Unexpected vote cast: {}", .0)]
    InvalidVote(String),

    #[error("Unable to tally votes: {}", .0)]
    TallyFailed(String),
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum StorageError {
    #[error("Failed to append event: {0}")]
    AppendFailed(String),
    #[error("Concurrency conflict: expected sequence {expected}, got {got}")]
    ConcurrencyConflict { expected: u64, got: u64 },
    //#[error("Failed to read events: {0}")]
    //ReadFailed(String),
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
pub enum IdentityError {
    #[error("Voter is not on the eligible roll")]
    IneligibleVoter,

    #[error("Eligibility token has already been issued to this voter")]
    TokenAlreadyIssued,

    #[error("Invalid or already spent ballot token")]
    InvalidOrSpentToken,
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum VotingFacadeError {
    #[error(transparent)]
    Identity(#[from] IdentityError),

    #[error(transparent)]
    Election(#[from] ElectionError),

    #[error(transparent)]
    Storage(#[from] StorageError),
}

#[derive(Error, Debug, PartialEq, Eq)]
#[error("score must be greater equal to 0")]
pub struct ScoreError;

#[derive(Error, Debug, PartialEq, Eq)]
#[error("percentage must be between 0.0 and 100.0 (inclusive)")]
pub struct PercentageError;
