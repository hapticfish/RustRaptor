// src/services/blowfin/auth.rs

use base64::{engine::general_purpose, Engine as _};
use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use uuid::Uuid;

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
pub fn sign_ws(secret: &str, timestamp: &str, nonce: &str) -> String {
    let path = "/users/self/verify";
    let method = "GET";
    let prehash = format!("{}{}{}{}", path, method, timestamp, nonce);
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key bits of any size");
    mac.update(prehash.as_bytes());
    general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}

// ======================================================================
// UNIT TESTS
// ======================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    // ---------- deterministic vectors (from Python calc) ----------
    const SECRET: &str = "mysecret";
    const TS: &str = "1690000000000";
    const NONCE: &str = "nonce123";
    const PATH: &str = "/api/v1/order";
    const BODY: &str = r#"{"foo":1}"#;
    const METHOD: &str = "POST";

    // pre-computed with:
    //   base64(hmac_sha256("mysecret",
    //        "/api/v1/orderPOST1690000000000nonce123{\"foo\":1}"))
    const EXPECT_REST: &str = "Jg5/kwP/ixremCZCe9Wzb8e0jA/FXxjJsFxEUJVrsx0=";

    //   base64(hmac_sha256("mysecret",
    //        "/users/self/verifyGET1690000000000nonce123"))
    const EXPECT_WS: &str = "XhySSqNux/AAnb1u41Alg7M1l0Aoc/ltBbJl08AAjJg=";

    // ---------- sign_rest happy path ----------
    #[test]
    fn sign_rest_matches_reference() {
        let sig = sign_rest(SECRET, METHOD, PATH, TS, NONCE, BODY);
        assert_eq!(sig, EXPECT_REST);
    }

    // ---------- sign_ws happy path ----------
    #[test]
    fn sign_ws_matches_reference() {
        let sig = sign_ws(SECRET, TS, NONCE);
        assert_eq!(sig, EXPECT_WS);
    }

    // ---------- empty-body + GET should still hash correctly ------
    #[test]
    fn sign_rest_empty_body_ok() {
        let sig = sign_rest(SECRET, "GET", "/api/ping", TS, NONCE, "");
        // ensure *something* is produced and Base64-valid
        assert!(base64::engine::general_purpose::STANDARD
            .decode(&sig)
            .is_ok());
    }

    // ---------- timestamp helper monotonicity ---------------------
    #[test]
    fn current_timestamp_monotonic() {
        let t1: i128 = current_timestamp().parse().unwrap();
        let t2: i128 = current_timestamp().parse().unwrap();
        assert!(t2 >= t1);
        // 13-digit epoch millis
        assert_eq!(current_timestamp().len(), 13);
    }

    // ---------- nonce helper is uuid-v4 like ----------------------
    #[test]
    fn nonce_is_uuid_v4() {
        let re =
            Regex::new(r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
                .unwrap();
        let n1 = generate_nonce();
        let n2 = generate_nonce();
        assert!(re.is_match(&n1));
        assert!(re.is_match(&n2));
        assert_ne!(n1, n2);
    }

    // ---------- empty secret key still hashes --------------------
    #[test]
    fn sign_with_empty_secret_does_not_panic() {
        let sig = sign_rest("", "GET", "/x", TS, NONCE, "");
        assert!(!sig.is_empty());
    }
}
