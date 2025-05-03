// src/services/blowfin/auth.rs

use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::{engine::general_purpose, Engine as _};
use uuid::Uuid;
use chrono::Utc;

/// Millisecond timestamp
pub fn current_timestamp() -> String {
    Utc::now().timestamp_millis().to_string()
}

/// Random nonce for replay protection
pub fn generate_nonce() -> String {
    Uuid::new_v4().to_string()
}

/// Sign REST requests (HMAC SHA-256 + Base64)
pub fn sign_rest(
    secret: &str,
    method: &str,
    path: &str,
    timestamp: &str,
    nonce: &str,
    body: &str,
) -> String {
    let prehash = format!("{}{}{}{}{}", path, method, timestamp, nonce, body);
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key bits of any size");
    mac.update(prehash.as_bytes());
    general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}

/// Sign WebSocket login operation
pub fn sign_ws(
    secret: &str,
    timestamp: &str,
    nonce: &str,
) -> String {
    let path = "/users/self/verify";
    let method = "GET";
    let prehash = format!("{}{}{}{}", path, method, timestamp, nonce);
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key bits of any size");
    mac.update(prehash.as_bytes());
    general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}
