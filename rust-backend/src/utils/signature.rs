//! HMAC helpers for the X-RR-SIG header (hardened version)

use actix_web::dev::ServiceRequest;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use std::time::{SystemTime, UNIX_EPOCH};
use actix_web::HttpMessage;
use log::warn;

/// Maximum allowed clock skew (seconds)
const MAX_SKEW_SECS: i64 = 10;

pub fn verify_hmac(req: &ServiceRequest) -> bool {
    // --- Parse signature header ---
    let sig_hdr = match req.headers().get("X-RR-SIG") {
        Some(h) => h,
        None => {
            warn!("X-RR-SIG header missing");
            return false;
        }
    };
    let sig_str = match sig_hdr.to_str() {
        Ok(s) if s.len() == 64 => s,
        _ => {
            warn!("X-RR-SIG header format/length invalid");
            return false;
        }
    };

    // --- Parse and validate timestamp header ---
    let ts_hdr = match req.headers().get("X-RR-TIMESTAMP") {
        Some(h) => h,
        None => {
            warn!("X-RR-TIMESTAMP header missing");
            return false;
        }
    };
    let ts_str = match ts_hdr.to_str() {
        Ok(s) => s,
        Err(_) => {
            warn!("X-RR-TIMESTAMP invalid utf-8");
            return false;
        }
    };
    let ts: i64 = match ts_str.parse() {
        Ok(n) => n,
        Err(_) => {
            warn!("X-RR-TIMESTAMP not parseable");
            return false;
        }
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    if (ts - now).abs() > MAX_SKEW_SECS {
        warn!("X-RR-TIMESTAMP out of allowed skew (got {}, now {})", ts, now);
        return false;
    }

    // --- Read request payload (from extensions) ---
    let body_bytes: &[u8] = req
        .extensions()
        .get::<Vec<u8>>()
        .map(|v| v.as_slice())
        .unwrap_or(&[]);

    // --- Compose HMAC input: timestamp (as bytes) || body ---
    let mut hmac_input = Vec::with_capacity(8 + body_bytes.len());
    hmac_input.extend_from_slice(ts_str.as_bytes());
    hmac_input.extend_from_slice(body_bytes);

    // --- Compute HMAC ---
    type HmacSha = Hmac<Sha256>;
    let key = std::env::var("RR_HMAC_SECRET").unwrap_or_default();
    let mut mac = HmacSha::new_from_slice(key.as_bytes()).expect("key length");
    mac.update(&hmac_input);
    let calc = mac.finalize().into_bytes();

    // --- Constant-time compare ---
    let given = match hex::decode(sig_str) {
        Ok(g) => g,
        Err(_) => {
            warn!("X-RR-SIG not valid hex");
            return false;
        }
    };

    let valid = calc.ct_eq(&given).into();
    if !valid {
        warn!("HMAC signature mismatch");
    }
    valid
}

/// Direct byte-slice variant – used for WS frames
pub fn verify_hmac_bytes(body: &[u8], secret: &str, sig_hex: &str) -> bool {
    if sig_hex.len() != 64 { return false; }
    type HmacSha = hmac::Hmac<sha2::Sha256>;
    let mut mac  = HmacSha::new_from_slice(secret.as_bytes()).expect("key length");
    mac.update(body);
    let calc = mac.finalize().into_bytes();
    match hex::decode(sig_hex) {
        Ok(given) => calc.ct_eq(&given).into(),
        Err(_)    => false,
    }
}
