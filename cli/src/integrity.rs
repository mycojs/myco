use base64::{engine::general_purpose::STANDARD, Engine as _};
use sha2::{Digest, Sha512};

pub fn calculate_integrity(bytes: &[u8]) -> String {
    let mut hasher = Sha512::new();
    hasher.update(bytes);
    let hash = hasher.finalize();
    format!("sha512-{}", STANDARD.encode(hash))
}
