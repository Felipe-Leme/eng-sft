use thiserror::Error;

pub use election_core::error::{
    AcademicElectionError, BudgetElectionError, CondoElectionError, CustomElectionError,
    ElectionError, PercentageError, ScoreError,
};

#[derive(Error, Debug, PartialEq, Eq)]
pub enum StorageError {
    #[error("Failed to append event: {0}")]
    AppendFailed(String),
    #[error("Failed to read events: {0}")]
    ReadFailed(String),
    #[error("Election {0} already exists")]
    ElectionAlreadyExists(String),
    #[error("Election {0} was not found")]
    ElectionNotFound(String),
    #[error("Election is closed")]
    ElectionClosed,
    #[error("Election name already exists")]
    DuplicateElectionName,
    #[error("This voter has already voted in the election")]
    VoterAlreadyVoted,
    #[error("Concurrency conflict: expected sequence {expected}, got {got}")]
    ConcurrencyConflict { expected: u64, got: u64 },
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
