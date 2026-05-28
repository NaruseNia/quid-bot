use aes_gcm::{
    aead::{Aead, KeyInit, OsRng, rand_core::RngCore},
    Aes256Gcm, Nonce,
};
use base64::{Engine, engine::general_purpose::STANDARD as B64};

fn master_key() -> [u8; 32] {
    let key_str = std::env::var("ENCRYPTION_KEY").unwrap_or_else(|_| {
        tracing::warn!("ENCRYPTION_KEY not set, using fallback — set it in .env for production");
        "quid-bot-default-key-change-me!!".to_string()
    });
    let mut key = [0u8; 32];
    let bytes = key_str.as_bytes();
    let len = bytes.len().min(32);
    key[..len].copy_from_slice(&bytes[..len]);
    key
}

pub fn encrypt(plaintext: &str) -> Result<String, crate::error::Error> {
    let cipher = Aes256Gcm::new_from_slice(&master_key())
        .map_err(|e| format!("cipher init: {}", e))?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("encrypt: {}", e))?;

    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(B64.encode(combined))
}

pub fn decrypt(encoded: &str) -> Result<String, crate::error::Error> {
    let combined = B64
        .decode(encoded)
        .map_err(|e| format!("base64 decode: {}", e))?;

    if combined.len() < 12 {
        return Err("invalid encrypted data".into());
    }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(&master_key())
        .map_err(|e| format!("cipher init: {}", e))?;

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("decrypt: {}", e))?;

    String::from_utf8(plaintext).map_err(|e| format!("utf8: {}", e).into())
}
