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

    #[error("Failed to read events: {0}")]
    ReadFailed(String),
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum AcademicElectionError {
    #[error("Cannot start an election with zero slates")]
    NoSlatesRegistered,

    #[error("Invalid slate name: {0}")]
    InvalidSlateName(String),
}
