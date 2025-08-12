//! Envelope-encryption helper
//! • AES-256-GCM for per-row data
//! • Libsodium sealed-box to wrap the random data-key
//! • once_cell singleton GLOBAL_CRYPTO, loaded from `.env`

use aes_gcm::{
    aead::{Aead, OsRng},
    Aes256Gcm, Key, KeyInit, Nonce,
};
use anyhow::Result;
use base64::engine::general_purpose as b64;
use once_cell::sync::Lazy;
use rand_core::RngCore;                                // gives fill_bytes()
use sodiumoxide::{
    crypto::{
        box_::{PublicKey, SecretKey},                  // <— correct types
        sealedbox,
    },
    init as sodium_init,
};
use std::env;
use base64::Engine;
use zeroize::Zeroizing;

// ──────────────────────────────────────────────────────────────
//  Struct & constructors
// ──────────────────────────────────────────────────────────────
#[derive(Clone)]
pub struct EnvelopeCrypto {
    master_pk: PublicKey,
    master_sk: SecretKey,
}

impl EnvelopeCrypto {
    pub fn new(pk: [u8; 32], sk: [u8; 32]) -> Self {
        Self {
            master_pk: PublicKey(pk),
            master_sk: SecretKey(sk),
        }
    }

    /// Load BASE64 keys from env (`MASTER_PK_B64`, `MASTER_SK_B64`)
    pub fn from_env() -> Result<Self> {
        sodium_init().map_err(|_| anyhow::anyhow!("libsodium init failed"))?;

        let pk_raw = b64::STANDARD.decode(env::var("MASTER_PK_B64")?)?;
        let sk_raw = b64::STANDARD.decode(env::var("MASTER_SK_B64")?)?;

        Ok(Self::new(
            pk_raw.try_into().map_err(|_| anyhow::anyhow!("pk length"))?,
            sk_raw.try_into().map_err(|_| anyhow::anyhow!("sk length"))?,
        ))
    }
}

/// Thread-safe singleton (fails fast if keys are missing).
pub static GLOBAL_CRYPTO: Lazy<EnvelopeCrypto> =
    Lazy::new(|| EnvelopeCrypto::from_env().expect("master keys in .env"));

// ──────────────────────────────────────────────────────────────
//  Envelope seal / open
// ──────────────────────────────────────────────────────────────
impl EnvelopeCrypto {
    /// Encrypt → (wrapped_data_key, nonce, ciphertext)
    pub fn seal(&self, plaintext: &[u8]) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        // 1) fresh 256-bit data key
        let mut dk = [0u8; 32];
        OsRng.fill_bytes(&mut dk);
        let data_key: Zeroizing<Vec<u8>> = Zeroizing::new(dk.to_vec());

        // 2) AES-GCM
        let cipher = Aes256Gcm::new(Key::from_slice(&data_key));
        let mut nonce = [0u8; 12];
        OsRng.fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext)
            .expect("AES-GCM encrypt");

        // 3) wrap data key
        let wrapped_key = sealedbox::seal(&data_key, &self.master_pk);

        (wrapped_key, nonce.to_vec(), ciphertext)
    }

    /// Decrypt triplet back to UTF-8 string
    pub fn open(&self, wrapped: &[u8], nonce: &[u8], cipher: &[u8]) -> Result<String> {
        let data_key =
            sealedbox::open(wrapped, &self.master_pk, &self.master_sk)
                .map_err(|_| anyhow::anyhow!("sealed-box unwrap failed"))?;

        let cipher_aes = Aes256Gcm::new(Key::from_slice(&data_key));
        let plaintext  = cipher_aes
            .decrypt(Nonce::from_slice(nonce), cipher)
            .map_err(|_| anyhow::anyhow!("AES-GCM decrypt failed"))?;

        Ok(String::from_utf8(plaintext)?)
    }
}
