use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    domain::{
        election::{Closed, Election, Open},
        storage::{ElectionEvent, EventRecord, EventStore},
        strategy::TallyStrategy,
    },
    error::{IdentityError, VotingFacadeError},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VoterId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlindToken(Uuid);

impl BlindToken {
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }
}

pub struct EligibilityRegistry {
    voters: HashMap<VoterId, bool>,
}

impl EligibilityRegistry {
    pub fn new(eligible_voters: Vec<VoterId>) -> Self {
        let voters = eligible_voters.into_iter().map(|id| (id, false)).collect();

        Self { voters }
    }

    pub fn issue_token(
        &mut self,
        voter_id: &VoterId,
        ballot_box_allowlist: &mut TokenAllowList,
    ) -> Result<BlindToken, IdentityError> {
        let claimed = self
            .voters
            .get_mut(voter_id)
            .ok_or(IdentityError::IneligibleVoter)?;

        if *claimed {
            return Err(IdentityError::TokenAlreadyIssued);
        }

        *claimed = true;
        let token = BlindToken::generate();
        ballot_box_allowlist.authorize(token.clone());

        Ok(token)
    }
}

#[derive(Default)]
pub struct TokenAllowList {
    valid_tokens: HashSet<BlindToken>,
}

impl TokenAllowList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn authorize(&mut self, token: BlindToken) {
        self.valid_tokens.insert(token);
    }

    pub fn consume(&mut self, token: &BlindToken) -> bool {
        self.valid_tokens.remove(token)
    }
}

pub struct AnonymousBallotBox<S: TallyStrategy, E> {
    election: Election<S, Open>,
    store: E,
    allowlist: TokenAllowList,
}

impl<S, E> AnonymousBallotBox<S, E>
where
    S: TallyStrategy,
    S::Vote: Clone,
    S::Outcome: Clone,
    E: EventStore<S::Vote, S::Outcome>,
{
    pub fn new(
        election: Election<S, Open>,
        mut store: E,
        allowlist: TokenAllowList,
    ) -> Result<Self, VotingFacadeError> {
        let seq = store.current_sequence() + 1;
        store.append(EventRecord::new(seq, ElectionEvent::ElectionCreated))?;

        Ok(Self {
            election,
            store,
            allowlist,
        })
    }

    pub fn cast_anonymous_vote(
        &mut self,
        token: &BlindToken,
        vote: S::Vote,
    ) -> Result<(), VotingFacadeError> {
        if !self.allowlist.valid_tokens.contains(token) {
            return Err(IdentityError::InvalidOrSpentToken.into());
        }

        self.election.cast_vote(vote.clone())?;

        self.allowlist.consume(token);

        let seq = self.store.current_sequence() + 1;
        self.store
            .append(EventRecord::new(seq, ElectionEvent::VoteCast(vote)))?;

        Ok(())
    }

    pub fn close_and_tally(
        mut self,
    ) -> Result<(Election<S, Closed>, S::Outcome, E), VotingFacadeError> {
        let closed_election = self.election.close();

        let seq_close = self.store.current_sequence() + 1;
        self.store
            .append(EventRecord::new(seq_close, ElectionEvent::ElectionClosed))?;

        let outcome = closed_election.compute_result()?;

        let seq_tally = self.store.current_sequence() + 1;
        self.store.append(EventRecord::new(
            seq_tally,
            ElectionEvent::TallyComputed(outcome.clone()),
        ))?;

        Ok((closed_election, outcome, self.store))
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{
        academic::{AcademicElectionBuilder, AcademicOutcome, Slate},
        storage::MemoryEventStore,
    };

    use super::*;

    #[test]
    fn test_full_anonymous_voting_flow() {
        let student_a = VoterId("ra000000".into());
        let student_b = VoterId("ra111111".into());
        let outsider = VoterId("ra222222".into());

        let mut registry = EligibilityRegistry::new(vec![student_a.clone(), student_b.clone()]);
        let mut allowlist = TokenAllowList::new();

        let token_a = registry.issue_token(&student_a, &mut allowlist).unwrap();
        let token_b = registry.issue_token(&student_b, &mut allowlist).unwrap();

        assert_eq!(
            registry.issue_token(&outsider, &mut allowlist),
            Err(IdentityError::IneligibleVoter)
        );

        assert_eq!(
            registry.issue_token(&student_a, &mut allowlist),
            Err(IdentityError::TokenAlreadyIssued)
        );

        let election = AcademicElectionBuilder::new()
            .add_slate("ABC")
            .add_slate("XYZ")
            .with_quorum(2)
            .build()
            .unwrap();
        let store = MemoryEventStore::new();
        let mut ballot_box = AnonymousBallotBox::new(election, store, allowlist).unwrap();

        let xyz = Slate::new("XYZ").unwrap();
        ballot_box
            .cast_anonymous_vote(&token_a, xyz.clone())
            .unwrap();
        ballot_box
            .cast_anonymous_vote(&token_b, xyz.clone())
            .unwrap();

        assert_eq!(
            ballot_box.cast_anonymous_vote(&token_a, xyz.clone()),
            Err(VotingFacadeError::Identity(
                IdentityError::InvalidOrSpentToken
            ))
        );

        let forged = BlindToken::generate();
        assert_eq!(
            ballot_box.cast_anonymous_vote(&forged, xyz.clone()),
            Err(VotingFacadeError::Identity(
                IdentityError::InvalidOrSpentToken
            ))
        );

        let (_, outcome, final_store) = ballot_box.close_and_tally().unwrap();
        assert_eq!(outcome, AcademicOutcome::Winner(xyz));
        assert_eq!(final_store.get_all().unwrap().len(), 5) // Created + 2 Votes + Closed + Tallied.
    }
}
