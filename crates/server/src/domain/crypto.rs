use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{domain::storage::ElectionEvent, error::StorageError};

pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HashedEventRecord<V, O> {
    pub sequence: u64,
    pub timestamp: String,
    pub prev_hash: String,
    pub current_hash: String,
    pub payload: ElectionEvent<V, O>,
}

impl<V: Serialize, O: Serialize> HashedEventRecord<V, O> {
    pub fn new(
        sequence: u64,
        timestamp: String,
        prev_hash: String,
        payload: ElectionEvent<V, O>,
    ) -> Result<Self, StorageError> {
        let mut record = Self {
            sequence,
            timestamp,
            prev_hash,
            current_hash: String::new(),
            payload,
        };

        record.current_hash = record.calculate_hash()?;

        Ok(record)
    }

    pub fn verify(&self) -> Result<bool, StorageError> {
        Ok(self.current_hash == self.calculate_hash()?)
    }

    pub fn calculate_hash(&self) -> Result<String, StorageError> {
        let mut hasher = Sha256::new();
        hasher.update(b"eng-sft:event:v1");
        hasher.update(self.sequence.to_be_bytes());
        hasher.update((self.timestamp.len() as u64).to_be_bytes());
        hasher.update(self.timestamp.as_bytes());
        hasher.update((self.prev_hash.len() as u64).to_be_bytes());
        hasher.update(self.prev_hash.as_bytes());

        let payload_bytes = serde_json::to_vec(&self.payload)
            .map_err(|e| StorageError::AppendFailed(format!("Serialization error: {}", e)))?;

        hasher.update((payload_bytes.len() as u64).to_be_bytes());
        hasher.update(&payload_bytes);

        Ok(encode_hex(hasher.finalize().as_ref()))
    }
}

pub fn verify_chain<V: Serialize, O: Serialize>(
    records: &[HashedEventRecord<V, O>],
) -> Result<bool, StorageError> {
    let mut previous_hash = GENESIS_HASH;

    for (index, record) in records.iter().enumerate() {
        if record.sequence != index as u64 + 1
            || record.prev_hash != previous_hash
            || !record.verify()?
        {
            return Ok(false);
        }

        previous_hash = &record.current_hash;
    }

    Ok(true)
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }

    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestRecord = HashedEventRecord<String, String>;

    fn event(sequence: u64, previous_hash: &str, vote: &str) -> TestRecord {
        HashedEventRecord::new(
            sequence,
            format!("2026-01-01T00:00:{:02}Z", sequence),
            previous_hash.to_owned(),
            ElectionEvent::VoteCast(vote.to_owned()),
        )
        .unwrap()
    }

    #[test]
    fn event_hash_is_sha256_and_verifies() {
        let record = event(1, GENESIS_HASH, "Vote: A");

        assert_eq!(record.current_hash.len(), 64);
        assert!(record.verify().unwrap());
    }

    #[test]
    fn sha256_digest_uses_canonical_lowercase_hex() {
        let digest = Sha256::digest(b"abc");

        assert_eq!(
            encode_hex(digest.as_ref()),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn modified_payload_or_metadata_fails_verification() {
        let record = event(1, GENESIS_HASH, "Vote: A");
        let mut changed_payload = record.clone();
        changed_payload.payload = ElectionEvent::VoteCast("Vote: B".to_owned());
        let mut changed_sequence = record;
        changed_sequence.sequence = 2;

        assert!(!changed_payload.verify().unwrap());
        assert!(!changed_sequence.verify().unwrap());
    }

    #[test]
    fn verifies_linked_event_chain_and_rejects_broken_links() {
        let first = event(1, GENESIS_HASH, "Vote: A");
        let second = event(2, &first.current_hash, "Vote: B");
        let records = vec![first.clone(), second.clone()];
        assert!(verify_chain(&records).unwrap());

        let broken_chain = vec![first, event(2, GENESIS_HASH, "Vote: B")];
        assert!(!verify_chain(&broken_chain).unwrap());
        assert!(verify_chain::<String, String>(&[]).unwrap());
    }
}
