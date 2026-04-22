use crate::protocol::IrohAutomergeProtocol;

use anyhow::Result;
use automerge::Automerge;
use clap::Args;
use iroh::{Endpoint, endpoint::presets, protocol::Router};
use std::{path::PathBuf, sync::LazyLock};
use tokio::sync::mpsc;

#[derive(Args)]
struct File {
    /// The directory to store the Automerge document in. By default, this will be a directory named `envsync` within the user's config directory (e.g. `~/.config/envsync` on Linux).
    #[arg(short, long, default_value = &**CONFIG_DIR)]
    data_store: PathBuf,

    /// If set, the Automerge document will be encrypted with a passphrase. The passphrase will be prompted for on startup. By default, encryption is enabled.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    encryption: bool,
}

#[derive(clap::Subcommand)]
enum StoreType {
    /// Store the Automerge document in a local file. This should be used for persistent storage across restarts. By default, the document will be stored in a local file.
    File(File),
    // In memory store
    Memory,
}

#[derive(Args)]
pub struct Serve {
    /// The type of storage to use for the Automerge document. By default, the document will be stored in a local file.
    #[clap(subcommand)]
    store_type: StoreType,
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

enum ServeImpl {
    File {
        data_store: PathBuf,
        db_file: PathBuf,
        passphrase: Option<String>,
    },
    Memory,
}

impl ServeImpl {
    async fn new(store_type: StoreType) -> anyhow::Result<Self> {
        match store_type {
            StoreType::File(File {
                data_store,
                encryption,
            }) => {
                tokio::fs::create_dir_all(&data_store).await?;
                let db_file = data_store.join(crate::constants::DB_FILE_NAME);

                let passphrase: Option<String> = if encryption {
                    Some(rpassword::prompt_password("Passphrase: ")?)
                } else {
                    None
                };

                Ok(Self::File {
                    data_store,
                    db_file,
                    passphrase,
                })
            }
            StoreType::Memory => Ok(Self::Memory),
        }
    }

    async fn load_doc(&self) -> anyhow::Result<Automerge> {
        match self {
            ServeImpl::File {
                db_file,
                passphrase,
                ..
            } => {
                if tokio::fs::metadata(db_file).await.is_ok() {
                    let bytes = tokio::fs::read(db_file).await?;
                    let decrypted = crate::crypto::decrypt_bytes(&bytes, passphrase)?;
                    Automerge::load(&decrypted).map_err(|e| anyhow::anyhow!(e))
                } else {
                    Ok(Automerge::new())
                }
            }
            ServeImpl::Memory => Ok(Automerge::new()),
        }
    }

    async fn save_doc(&self, doc: &Automerge) -> anyhow::Result<()> {
        match self {
            ServeImpl::File {
                data_store,
                db_file,
                passphrase,
            } => {
                let bytes = doc.save();
                let enc = crate::crypto::encrypt_bytes(&bytes, passphrase)?;
                let tmp = data_store.join(format!("{}.tmp", crate::constants::DB_FILE_NAME));
                tokio::fs::write(&tmp, &enc).await?;
                tokio::fs::rename(&tmp, db_file).await?;
                Ok(())
            }
            ServeImpl::Memory => Ok(()),
        }
    }
}

pub async fn run(Serve { store_type }: Serve) -> Result<()> {
    let (sync_sender, mut sync_finished) = mpsc::channel(10);

    let impl_ = ServeImpl::new(store_type).await?;
    let initial_doc = impl_.load_doc().await?;

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
                if let Err(e) = impl_.save_doc(&doc).await {
                    eprintln!("Failed to save Automerge doc: {e}");
                }
            }
        }
    }

    iroh.shutdown().await?;

    Ok(())
}
