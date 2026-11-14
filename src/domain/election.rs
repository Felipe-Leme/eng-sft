use std::marker::PhantomData;

use crate::{domain::strategy::TallyStrategy, error::ElectionError};

pub(crate) enum VoteCommitError<E> {
    Election(ElectionError),
    Commit(E),
}

pub struct Open;
pub struct Closed;

pub struct Election<S: TallyStrategy, State> {
    strategy: S,
    votes: Vec<S::Vote>,
    _state: PhantomData<State>,
}

impl<S: TallyStrategy> Election<S, Open> {
    pub const fn new(strategy: S) -> Self {
        Self {
            strategy,
            votes: Vec::new(),
            _state: PhantomData,
        }
    }

    pub fn cast_vote(&mut self, vote: S::Vote) -> Result<(), ElectionError> {
        match self.cast_vote_with(vote, |_| Ok::<_, std::convert::Infallible>(())) {
            Ok(()) => Ok(()),
            Err(VoteCommitError::Election(error)) => Err(error),
            Err(VoteCommitError::Commit(never)) => match never {},
        }
    }

    pub(crate) fn cast_vote_with<E>(
        &mut self,
        vote: S::Vote,
        commit: impl FnOnce(&S::Vote) -> Result<(), E>,
    ) -> Result<(), VoteCommitError<E>> {
        self.strategy
            .validate(&vote)
            .map_err(VoteCommitError::Election)?;
        commit(&vote).map_err(VoteCommitError::Commit)?;
        self.votes.push(vote);
        Ok(())
    }

    pub fn close(self) -> Election<S, Closed> {
        Election {
            strategy: self.strategy,
            votes: self.votes,
            _state: PhantomData,
        }
    }
}

impl<S: TallyStrategy> Election<S, Closed> {
    pub fn compute_result(&self) -> Result<S::Outcome, ElectionError> {
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
        type Outcome = u32;

        fn validate(&self, vote: &Self::Vote) -> Result<(), ElectionError> {
            if *vote == 0 {
                Err(ElectionError::InvalidVote(
                    "Vote cannot be 0 in this instance".into(),
                ))
            } else {
                Ok(())
            }
        }

        fn tally(&self, votes: &[Self::Vote]) -> Result<Self::Outcome, ElectionError> {
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
    fn test_failed_tally() {
        let election = Election::new(MockStrategy);
        let closed_election = election.close();

        assert_matches!(
            closed_election.compute_result(),
            Err(ElectionError::TallyFailed(_))
        );
    }

    #[test]
    fn test_successful_tally() {
        let mut election = Election::new(MockStrategy);
        election.cast_vote(5).unwrap();
        election.cast_vote(20).unwrap();

        let closed_election = election.close();

        assert_eq!(closed_election.compute_result(), Ok(25));
    }
}
