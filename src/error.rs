use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ElectionError {
    #[error("Unexpected vote cast: {}", .0)]
    InvalidVote(String),

    #[error("Unable to tally votes: {}", .0)]
    TallyFailed(String),
}
