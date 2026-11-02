use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ElectionError {
    #[error("Attempt to cast vote on closed election")]
    ElectionClosed,

    #[error("Attempt to tally on an open election")]
    ElectionStillOpen,

    #[error("Unexpected vote cast: {}", .0)]
    InvalidVote(String),

    #[error("Unable to tally votes: {}", .0)]
    TallyFailed(String),
}
