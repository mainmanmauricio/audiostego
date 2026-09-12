//! Optional ChaCha20-Poly1305 payload encryption.

use anyhow::{bail, Context, Result};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use sha2::{Digest, Sha256};

pub fn derive_key(key_material: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"audiostego-aead-v1");
    hasher.update(key_material);
    let dig = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&dig);
    out
}

pub fn encrypt(key_material: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    let key = derive_key(key_material);
    let cipher = ChaCha20Poly1305::new_from_slice(&key).context("chacha key")?;
    // Deterministic nonce from key+len for stego reproducibility (not a general crypto pattern).
    let mut nh = Sha256::new();
    nh.update(b"nonce");
    nh.update(&key);
    nh.update((plaintext.len() as u64).to_le_bytes());
    let nd = nh.finalize();
    let nonce = Nonce::from_slice(&nd[..12]);
    let mut out = Vec::with_capacity(12 + plaintext.len() + 16);
    out.extend_from_slice(nonce);
    let ct = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("encrypt: {e}"))?;
    out.extend_from_slice(&ct);
    Ok(out)
}

pub fn decrypt(key_material: &[u8], blob: &[u8]) -> Result<Vec<u8>> {
    if blob.len() < 12 + 16 {
        bail!("ciphertext too short");
    }
    let key = derive_key(key_material);
    let cipher = ChaCha20Poly1305::new_from_slice(&key).context("chacha key")?;
    let nonce = Nonce::from_slice(&blob[..12]);
    cipher
        .decrypt(nonce, &blob[12..])
        .map_err(|e| anyhow::anyhow!("decrypt failed (bad key or corrupt payload): {e}"))
}
