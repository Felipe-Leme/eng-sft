use serde::{Deserialize, Serialize};

use crate::domain::{academic::Slate, condo::CondoVote, custom::CustomVote};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ElectionVote {
    Academic(Slate),
    Budget(Vec<String>),
    Condo(CondoVote),
    Custom(CustomVote),
}

pub trait TypedBallot: Sized {
    fn from_election_vote(vote: ElectionVote) -> Result<Self, String>;
}

impl TypedBallot for Slate {
    fn from_election_vote(vote: ElectionVote) -> Result<Self, String> {
        match vote {
            ElectionVote::Academic(vote) => Ok(vote),
            _ => Err("Ballot type does not match the election type".to_owned()),
        }
    }
}

impl TypedBallot for Vec<String> {
    fn from_election_vote(vote: ElectionVote) -> Result<Self, String> {
        match vote {
            ElectionVote::Budget(vote) => Ok(vote),
            _ => Err("Ballot type does not match the election type".to_owned()),
        }
    }
}

impl TypedBallot for CondoVote {
    fn from_election_vote(vote: ElectionVote) -> Result<Self, String> {
        match vote {
            ElectionVote::Condo(vote) => Ok(vote),
            _ => Err("Ballot type does not match the election type".to_owned()),
        }
    }
}

impl TypedBallot for CustomVote {
    fn from_election_vote(vote: ElectionVote) -> Result<Self, String> {
        match vote {
            ElectionVote::Custom(vote) => Ok(vote),
            _ => Err("Ballot type does not match the election type".to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;

    use crate::types::Percentage;

    use super::*;

    #[test]
    fn shared_ballot_enum_round_trips_each_ballot_shape() {
        let votes = [
            ElectionVote::Academic(Slate::new("A").unwrap()),
            ElectionVote::Budget(vec!["Roads".to_owned(), "Park".to_owned()]),
            ElectionVote::Condo(CondoVote {
                choice: "Repair".to_owned(),
                fraction: Percentage::try_from(dec!(25)).unwrap(),
            }),
            ElectionVote::Custom(CustomVote {
                choices: vec!["Option A".to_owned()],
                weight: dec!(1.25),
            }),
        ];

        for vote in votes {
            let json = serde_json::to_value(&vote).unwrap();
            let decoded: ElectionVote = serde_json::from_value(json).unwrap();
            assert_eq!(decoded, vote);
        }
    }

    #[test]
    fn ballot_payload_must_match_its_typed_variant() {
        assert!(
            Vec::<String>::from_election_vote(ElectionVote::Academic(Slate::new("A").unwrap()))
                .is_err()
        );
    }
}
