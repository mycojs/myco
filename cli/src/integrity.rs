use sha2::{Sha512, Digest};
use base64::{Engine as _, engine::general_purpose::STANDARD};

pub fn calculate_integrity(bytes: &[u8]) -> String {
    let mut hasher = Sha512::new();
    hasher.update(bytes);
    let hash = hasher.finalize();
    format!("sha512-{}", STANDARD.encode(hash))
}
