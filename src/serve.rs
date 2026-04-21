use crate::protocol::IrohAutomergeProtocol;

use anyhow::Result;
use argon2::Argon2;
use automerge::Automerge;
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit},
};
use clap::Args;
use iroh::{Endpoint, endpoint::presets, protocol::Router};
use rand::RngCore;
use std::{path::PathBuf, sync::LazyLock};
use tokio::sync::mpsc;
use zeroize::Zeroize;

#[derive(Args)]
pub struct Serve {
    /// The directory to store the Automerge document in. By default, this will be a directory named `envsync` within the user's config directory (e.g. `~/.config/envsync` on Linux).
    #[arg(short, long, default_value = &**CONFIG_DIR)]
    data_store: PathBuf,

    /// If set, the Automerge document will be encrypted with a passphrase. The passphrase will be prompted for on startup. By default, encryption is enabled.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    encryption: bool,
}

static CONFIG_DIR: LazyLock<String> = LazyLock::new(|| {
    dirs_next::config_dir()
        .map(|d| {
            let mut p = d;
            p.push("envsync");
            p.to_string_lossy().to_string()
        })
        .unwrap_or_else(|| ".".to_string())
});

fn derive_key(pass: &str, salt: &[u8]) -> anyhow::Result<[u8; 32]> {
    let mut key = [0u8; 32];
    let argon2 = Argon2::default();
    argon2
        .hash_password_into(pass.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow::anyhow!(e))?;
    Ok(key)
}

fn encrypt_bytes(plaintext: &[u8], pass: &Option<String>) -> anyhow::Result<Vec<u8>> {
    if pass.is_none() {
        return Ok(plaintext.to_vec());
    }

    let pass = pass.as_ref().unwrap();

    const MAGIC: &[u8; 8] = b"ENVSYNC1";
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

fn decrypt_bytes(encrypted: &[u8], pass: &Option<String>) -> anyhow::Result<Vec<u8>> {
    if pass.is_none() {
        return Ok(encrypted.to_vec());
    }

    let pass = pass.as_ref().unwrap();

    const MAGIC: &[u8; 8] = b"ENVSYNC1";
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

    // zeroize key material
    let mut key_owned = key;
    key_owned.zeroize();

    Ok(pt)
}

pub async fn run(
    Serve {
        data_store,
        encryption,
    }: Serve,
) -> Result<()> {
    let (sync_sender, mut sync_finished) = mpsc::channel(10);

    tokio::fs::create_dir_all(&data_store).await?;
    let db_file = data_store.join("doc.automerge");

    let passphrase: Option<String> = if encryption {
        Some(rpassword::prompt_password("Passphrase: ")?)
    } else {
        None
    };

    let initial_doc = if tokio::fs::metadata(&db_file).await.is_ok() {
        let bytes = tokio::fs::read(&db_file).await?;
        let decrypted = decrypt_bytes(&bytes, &passphrase)?;
        Automerge::load(&decrypted)?
    } else {
        Automerge::new()
    };

    let automerge = IrohAutomergeProtocol::new(initial_doc, sync_sender);
    let endpoint = Endpoint::bind(presets::N0).await?;
    let iroh = Router::builder(endpoint)
        .accept(IrohAutomergeProtocol::ALPN, automerge.clone())
        .spawn();

    let endpoint_id = iroh.endpoint().id();

    println!("Running\nEndpoint Id: {endpoint_id}",);

    loop {
        tokio::select! {
             _ = tokio::signal::ctrl_c() => {
                println!("Shutting down...");
                break;
            }
            Some(doc) = sync_finished.recv() => {
                let bytes = doc.save();
                match encrypt_bytes(&bytes, &passphrase) {
                    Ok(enc) => {
                        let tmp = data_store.join("doc.automerge.tmp");
                        if let Err(e) = tokio::fs::write(&tmp, &enc).await {
                            eprintln!("Failed to write tmp file: {e}");
                        } else if let Err(e) = tokio::fs::rename(&tmp, &db_file).await {
                            eprintln!("Failed to rename tmp to db file: {e}");
                        }
                    }
                    Err(e) => eprintln!("Failed to encrypt Automerge doc: {e}"),
                }
            }
        }
    }

    iroh.shutdown().await?;

    Ok(())
}
