use sha3::{Digest, Keccak256};
use std::path::Path;

pub fn keccak256(data: &[u8]) -> String {
    let mut hasher = Keccak256::new();
    hasher.update(data);
    format!("0x{}", hex::encode(hasher.finalize()))
}

pub fn hash_tensor(data: &[i32]) -> String {
    let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
    keccak256(&bytes)
}

pub fn compute_model_commitment(path: &Path) -> anyhow::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(keccak256(&bytes))
}
