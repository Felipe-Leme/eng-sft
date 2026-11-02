use crate::{domain::strategy::TallyStrategy, error::ElectionError};

pub struct Election<S: TallyStrategy> {
    strategy: S,
    votes: Vec<S::Vote>,
    is_closed: bool,
}

impl<S: TallyStrategy> Election<S> {
    pub fn new(strategy: S) -> Self {
        Self {
            strategy,
            votes: Vec::new(),
            is_closed: false,
        }
    }

    pub fn cast_vote(&mut self, vote: S::Vote) -> Result<(), ElectionError> {
        if self.is_closed {
            return Err(ElectionError::ElectionClosed);
        }

        self.strategy.validate(&vote)?;

        self.votes.push(vote);

        Ok(())
    }

    pub fn close(&mut self) {
        self.is_closed = true;
    }

    pub fn compute_result(&self) -> Result<S::OutCome, ElectionError> {
        if !self.is_closed {
            return Err(ElectionError::ElectionStillOpen);
        }

        self.strategy.tally(&self.votes)
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::*;

    struct MockStrategy;

    impl TallyStrategy for MockStrategy {
        type Vote = u32;
        type OutCome = u32;

        fn validate(&self, vote: &Self::Vote) -> Result<(), ElectionError> {
            if *vote == 0 {
                Err(ElectionError::InvalidVote(
                    "Vote cannot be 0 in this instance".into(),
                ))
            } else {
                Ok(())
            }
        }

        fn tally(&self, votes: &[Self::Vote]) -> Result<Self::OutCome, ElectionError> {
            let total = votes.iter().sum();

            if total == 0 {
                Err(ElectionError::TallyFailed(
                    "Tally cannot be 0 in this instance".into(),
                ))
            } else {
                Ok(total)
            }
        }
    }

    #[test]
    fn test_cast_valid_vote() {
        let mut election = Election::new(MockStrategy);
        assert!(election.cast_vote(10).is_ok());
        assert_eq!(election.votes.len(), 1);
    }

    #[test]
    fn test_reject_invalid_vote() {
        let mut election = Election::new(MockStrategy);
        assert_matches!(election.cast_vote(0), Err(ElectionError::InvalidVote(_)));
        assert_eq!(election.votes.len(), 0);
    }

    #[test]
    fn test_reject_vote_after_close() {
        let mut election = Election::new(MockStrategy);
        election.close();
        assert_eq!(election.cast_vote(5), Err(ElectionError::ElectionClosed));
        assert_eq!(election.votes.len(), 0);
    }

    #[test]
    fn test_prevent_tally_while_open() {
        let election = Election::new(MockStrategy);
        assert_eq!(
            election.compute_result(),
            Err(ElectionError::ElectionStillOpen)
        );
    }

    #[test]
    fn test_failed_tally() {
        let mut election = Election::new(MockStrategy);
        election.close();

        assert_matches!(
            election.compute_result(),
            Err(ElectionError::TallyFailed(_))
        )
    }

    #[test]
    fn test_successful_tally() {
        let mut election = Election::new(MockStrategy);
        election.cast_vote(5).unwrap();
        election.cast_vote(20).unwrap();

        election.close();

        assert_eq!(election.compute_result(), Ok(25));
    }
}
