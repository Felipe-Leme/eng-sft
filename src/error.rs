use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ElectionError {
    #[error("Unexpected vote cast: {}", .0)]
    InvalidVote(String),

    #[error("Unable to tally votes: {}", .0)]
    TallyFailed(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AcademicElectionError {
    #[error("Cannot start an election with zero slates")]
    NoSlatesRegistered,
    #[error("Invalid slate name: {0}")]
    InvalidSlateName(String),
}
