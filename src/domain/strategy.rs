use crate::error::ElectionError;

pub trait TallyStrategy {
    type Vote;
    type Outcome;

    /// Validates if a vote conforms to the context's specific rules (e.g. valid candidate).
    fn validate(&self, vote: &Self::Vote) -> Result<(), ElectionError>;

    /// Executes the specific counting logic for the election's context.
    fn tally(&self, votes: &[Self::Vote]) -> Result<Self::Outcome, ElectionError>;
}
