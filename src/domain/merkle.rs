use sha2::{Digest, Sha256};

type Hash = [u8; 32];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashDirection {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleProofStep {
    pub hash: String,
    pub direction: HashDirection,
}

#[derive(Debug, Clone)]
pub struct MerkleProof {
    pub leaf_hash: String,
    pub steps: Vec<MerkleProofStep>,
}

impl MerkleProof {
    pub fn verify(&self, expected_root: &str) -> bool {
        let Some(mut current) = decode_hash(&self.leaf_hash) else {
            return false;
        };
        let Some(expected_root) = decode_hash(expected_root) else {
            return false;
        };

        for step in &self.steps {
            let Some(sibling) = decode_hash(&step.hash) else {
                return false;
            };

            current = match step.direction {
                HashDirection::Left => hash_pair(&sibling, &current),
                HashDirection::Right => hash_pair(&current, &sibling),
            };
        }

        current == expected_root
    }
}

pub struct MerkleTree {
    leaves: Vec<Hash>,
    layers: Vec<Vec<Hash>>,
}

impl MerkleTree {
    pub fn new(leaf_data: &[impl AsRef<[u8]>]) -> Self {
        let leaves: Vec<Hash> = if leaf_data.is_empty() {
            vec![hash_leaf(b"")]
        } else {
            leaf_data
                .iter()
                .map(|data| hash_leaf(data.as_ref()))
                .collect()
        };
        let mut layers = vec![leaves.clone()];

        while layers.last().is_some_and(|layer| layer.len() > 1) {
            let current_layer = layers.last().expect("a leaf layer always exists");
            let mut next_layer = Vec::with_capacity(current_layer.len().div_ceil(2));

            for chunk in current_layer.chunks(2) {
                let right = chunk.get(1).unwrap_or(&chunk[0]);
                next_layer.push(hash_pair(&chunk[0], right));
            }

            layers.push(next_layer);
        }

        Self { leaves, layers }
    }

    pub fn root(&self) -> String {
        encode_hash(&self.layers.last().expect("a root layer always exists")[0])
    }

    pub fn generate_proof(&self, index: usize) -> Option<MerkleProof> {
        if index >= self.leaves.len() {
            return None;
        }

        let leaf_hash = encode_hash(&self.leaves[index]);
        let mut steps = Vec::new();
        let mut current_idx = index;

        for layer in &self.layers[..self.layers.len() - 1] {
            let is_right_node = current_idx % 2 == 1;
            let sibling_idx = if is_right_node {
                current_idx - 1
            } else if current_idx + 1 < layer.len() {
                current_idx + 1
            } else {
                current_idx
            };

            let direction = if is_right_node {
                HashDirection::Left
            } else {
                HashDirection::Right
            };

            steps.push(MerkleProofStep {
                hash: encode_hash(&layer[sibling_idx]),
                direction,
            });

            current_idx /= 2;
        }

        Some(MerkleProof { leaf_hash, steps })
    }
}

fn hash_leaf(data: &[u8]) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update([0]);
    hasher.update(data);
    into_hash(hasher.finalize())
}

fn hash_pair(left: &Hash, right: &Hash) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update([1]);
    hasher.update(left);
    hasher.update(right);
    into_hash(hasher.finalize())
}

fn into_hash(digest: impl AsRef<[u8]>) -> Hash {
    let mut hash = [0; 32];
    hash.copy_from_slice(digest.as_ref());
    hash
}

fn encode_hash(hash: &Hash) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(hash.len() * 2);

    for byte in hash {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }

    encoded
}

fn decode_hash(value: &str) -> Option<Hash> {
    if value.len() != 64 {
        return None;
    }

    let mut hash = [0; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = decode_hex_digit(pair[0])?;
        let low = decode_hex_digit(pair[1])?;
        hash[index] = (high << 4) | low;
    }

    Some(hash)
}

fn decode_hex_digit(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proofs_verify_for_even_and_odd_leaf_counts() {
        for votes in [
            vec!["Vote: A", "Vote: B"],
            vec!["Vote: A", "Vote: B", "Vote: C"],
        ] {
            let tree = MerkleTree::new(&votes);
            let root = tree.root();

            for index in 0..votes.len() {
                assert!(tree.generate_proof(index).unwrap().verify(&root));
            }
        }
    }

    #[test]
    fn tampered_or_malformed_proofs_fail_verification() {
        let tree = MerkleTree::new(&["Vote: A", "Vote: B"]);
        let root = tree.root();
        let mut proof = tree.generate_proof(0).unwrap();
        proof.leaf_hash = encode_hash(&hash_leaf(b"Tampered vote"));
        assert!(!proof.verify(&root));

        let mut malformed = tree.generate_proof(0).unwrap();
        malformed.steps[0].hash = "not a SHA-256 hash".to_owned();
        assert!(!malformed.verify(&root));
        assert!(!tree.generate_proof(0).unwrap().verify("bad root"));
    }

    #[test]
    fn empty_and_single_leaf_trees_have_valid_proofs() {
        let empty_tree = MerkleTree::new(&[] as &[&str]);
        assert!(
            empty_tree
                .generate_proof(0)
                .unwrap()
                .verify(&empty_tree.root())
        );
        assert!(empty_tree.generate_proof(1).is_none());

        let single_leaf = MerkleTree::new(&["Vote: A"]);
        assert!(
            single_leaf
                .generate_proof(0)
                .unwrap()
                .verify(&single_leaf.root())
        );
    }
}
