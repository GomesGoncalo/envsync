use crate::constants::MAGIC;

use argon2::Argon2;
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit},
};
use rand::RngCore;
use zeroize::Zeroize;

fn derive_key(pass: &str, salt: &[u8]) -> anyhow::Result<[u8; 32]> {
    let mut key = [0u8; 32];
    let argon2 = Argon2::default();
    argon2
        .hash_password_into(pass.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow::anyhow!(e))?;
    Ok(key)
}

pub fn encrypt_bytes(plaintext: &[u8], pass: &Option<String>) -> anyhow::Result<Vec<u8>> {
    if pass.is_none() {
        return Ok(plaintext.to_vec());
    }

    let pass = pass.as_ref().unwrap();

    let mut salt = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut salt);

    let key = derive_key(pass, &salt)?;
    let cipher = XChaCha20Poly1305::new(&key.into());

    let mut nonce = [0u8; 24];
    rand::rngs::OsRng.fill_bytes(&mut nonce);

    let ct = cipher
        .encrypt(XNonce::from_slice(&nonce), plaintext)
        .map_err(|e| anyhow::anyhow!(e))?;

    let mut key_owned = key;
    key_owned.zeroize();

    let mut out = Vec::with_capacity(MAGIC.len() + salt.len() + nonce.len() + ct.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

pub fn decrypt_bytes(encrypted: &[u8], pass: &Option<String>) -> anyhow::Result<Vec<u8>> {
    if pass.is_none() {
        return Ok(encrypted.to_vec());
    }

    let pass = pass.as_ref().unwrap();

    if encrypted.len() < MAGIC.len() + 16 + 24 {
        return Err(anyhow::anyhow!("Encrypted file too short"));
    }
    if &encrypted[..MAGIC.len()] != MAGIC {
        return Err(anyhow::anyhow!("Invalid magic"));
    }
    let salt_start = MAGIC.len();
    let salt = &encrypted[salt_start..salt_start + 16];
    let nonce = &encrypted[salt_start + 16..salt_start + 16 + 24];
    let ct = &encrypted[salt_start + 16 + 24..];

    let key = derive_key(pass, salt)?;
    let cipher = XChaCha20Poly1305::new(&key.into());
    let pt = cipher
        .decrypt(XNonce::from_slice(nonce), ct)
        .map_err(|e| anyhow::anyhow!(e))?;

    let mut key_owned = key;
    key_owned.zeroize();

    Ok(pt)
}
