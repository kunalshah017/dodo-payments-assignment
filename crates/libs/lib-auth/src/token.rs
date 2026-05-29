use sha2::{Digest, Sha256};

// region:    --- API Key Token Operations

/// Hash an API key using SHA-256 for secure storage.
/// The raw key is never stored — only this hash.
pub fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

// endregion: --- API Key Token Operations
