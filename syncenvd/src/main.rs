use anyhow::Result;
use clap::Parser;
use common::prepare_dir;
use common::rpc::RpcService;
use database::Database;
use futures::StreamExt;
use futures::future;
use iroh_docs::store::Query;
use iroh_docs::DocTicket;
use iroh_docs::api::Doc;
use iroh_docs::api::protocol::{AddrInfoOptions, ShareMode};
use std::net::{IpAddr, Ipv6Addr};
use std::str::FromStr;
use std::sync::Arc;
use std::{path::PathBuf, sync::LazyLock};
use sync::Sync;
use tarpc::server::Channel;
use tarpc::server::incoming::Incoming;
use tarpc::tokio_serde::formats::Json;
use tarpc::{context::Context, server};
use tokio::sync::Mutex;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

mod database;
mod sync;

static CONFIG_DIR: LazyLock<String> = LazyLock::new(|| {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(env!("CARGO_PKG_NAME"))
        .to_string_lossy()
        .to_string()
});

static DATA_DIR: LazyLock<String> = LazyLock::new(|| {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(env!("CARGO_PKG_NAME"))
        .to_string_lossy()
        .to_string()
});

enum Server {
    Unconfigured {
        database: Arc<Mutex<Database>>,
        sync: Arc<Mutex<Sync>>,
    },
    Configured {
        database: Arc<Mutex<Database>>,
        sync: Arc<Mutex<Sync>>,
        ticket: DocTicket,
        doc: Doc,
    },
}

impl Server {
    async fn new() -> Result<Self> {
        let cd_path = PathBuf::from(&*CONFIG_DIR);
        let dd_path = PathBuf::from(&*DATA_DIR);
        let config_dir = prepare_dir(&cd_path).await?;
        let data_dir = prepare_dir(&dd_path).await?;
        let database = Arc::new(Mutex::new(
            Database::new(data_dir.join("env_vars.sqlite")).await?,
        ));
        let sync = Arc::new(Mutex::new(Sync::new(data_dir).await?));

        Ok(Self::Unconfigured { database, sync })
    }

    async fn join_or_create_doc(
        &mut self,
        ticket: Option<String>,
    ) -> std::result::Result<String, String> {
        match (&self, ticket) {
            (Server::Unconfigured { database, sync }, None) => {
                let doc = sync
                    .lock()
                    .await
                    .create()
                    .await
                    .map_err(|e| format!("Failed to create document: {}", e))?;
                let ticket = doc
                    .share(ShareMode::Write, AddrInfoOptions::RelayAndAddresses)
                    .await
                    .map_err(|e| format!("Failed to share document: {}", e))?;

                tracing::info!("Created new document with ticket: {}", ticket);
                *self = Server::Configured {
                    database: (*database).clone(),
                    sync: (*sync).clone(),
                    ticket: ticket.clone(),
                    doc,
                };
                Ok(ticket.to_string())
            }
            (Server::Unconfigured { database, sync }, Some(ticket)) => {
                let doc = sync
                    .lock()
                    .await
                    .join(
                        DocTicket::from_str(&ticket)
                            .map_err(|e| format!("Invalid ticket format: {}", e))?,
                    )
                    .await
                    .map_err(|e| format!("Failed to join document: {}", e))?;

                tracing::info!("Joined existing document with ticket: {}", ticket);
                *self = Server::Configured {
                    database: (*database).clone(),
                    sync: (*sync).clone(),
                    ticket: DocTicket::from_str(&ticket).unwrap(),
                    doc,
                };
                Ok(ticket)
            }
            (
                Server::Configured {
                    database: _,
                    ticket,
                    ..
                },
                None,
            ) => {
                tracing::info!("Already configured with ticket: {}", ticket);
                Ok(ticket.to_string())
            }
            (Server::Configured { ticket, .. }, Some(_)) => {
                tracing::warn!(
                    "Attempted to join/create document while already configured with ticket: {}",
                    ticket
                );
                Ok(ticket.to_string())
            }
        }
    }

    async fn get_env(
        &self,
        profile: String,
        key: String,
    ) -> std::result::Result<Option<String>, String> {
        let (sync, doc) = match self {
            Server::Unconfigured { .. } => {
                Err("Server is not configured with a document yet".to_string())
            }
            Server::Configured { sync, doc, .. } => Ok((sync.clone(), doc.clone())),
        }?;

        let lookup_key = format!("{}/{}", profile, key);
        let entry = doc
            .get_one(Query::single_latest_per_key().key_exact(lookup_key.as_bytes()))
            .await
            .map_err(|e| format!("Failed to query document: {}", e))?;

        match entry {
            None => Ok(None),
            Some(e) => {
                let hash = e.content_hash();
                let bytes = sync
                    .lock()
                    .await
                    .blobs()
                    .get_bytes(hash)
                    .await
                    .map_err(|e| format!("Failed to read blob: {}", e))?;
                String::from_utf8(bytes.to_vec())
                    .map(Some)
                    .map_err(|e| format!("Value is not valid UTF-8: {}", e))
            }
        }
    }

    async fn set_env(
        &self,
        profile: String,
        key: String,
        val: String,
    ) -> std::result::Result<(), String> {
        let (database, sync, doc) = match self {
            Server::Unconfigured { .. } => {
                Err("Server is not configured with a document yet".to_string())
            }
            Server::Configured {
                database,
                sync,
                doc,
                ..
            } => Ok((database.clone(), sync.clone(), doc.clone())),
        }?;

        // write to database
        // database
        //     .lock()
        //     .await
        //     .set_env(profile.clone(), key.clone(), val.clone())
        //     .await
        //     .map_err(|e| format!("Failed to set environment variable in database: {}", e))?;

        // write to document
        let key = format!("{}/{}", profile, key);
        let author = sync
            .lock()
            .await
            .author()
            .await
            .map_err(|e| format!("Failed to get author ID: {}", e))?;

        doc.set_bytes(author, key.into_bytes(), val.into_bytes())
            .await
            .map_err(|e| format!("Failed to set environment variable in document: {}", e))?;

        Ok(())
    }
}

#[derive(Clone)]
struct RpcServer {
    server: Arc<Mutex<Server>>,
}

impl RpcService for RpcServer {
    async fn join_or_create_doc(
        self,
        _: Context,
        ticket: Option<String>,
    ) -> std::result::Result<String, String> {
        tracing::info!(
            "Received RPC call: join_or_create_doc with ticket: {:?}",
            ticket
        );

        let mut server = self.server.lock().await;
        server.join_or_create_doc(ticket).await
    }

    async fn get_env(
        self,
        _: Context,
        profile: String,
        key: String,
    ) -> std::result::Result<Option<String>, String> {
        tracing::info!("Received RPC call: get_env for key: {}/{}", profile, key);
        let server = self.server.lock().await;
        server.get_env(profile, key).await
    }

    async fn set_env(
        self,
        _: Context,
        profile: String,
        key: String,
        val: String,
    ) -> std::result::Result<(), String> {
        tracing::info!(
            "Received RPC call: set_env with key: {}, value: {}",
            key,
            val
        );

        let server = self.server.lock().await;
        server.set_env(profile, key, val).await
    }
}

async fn spawn(fut: impl Future<Output = ()> + Send + 'static) {
    tokio::spawn(fut);
}

#[derive(Debug, clap::Parser)]
struct Args {
    #[clap(short, long, default_value_t = 9999)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let server = Arc::new(Mutex::new(Server::new().await?));

    let server_addr = (IpAddr::V6(Ipv6Addr::LOCALHOST), args.port);

    let mut listener = tarpc::serde_transport::tcp::listen(&server_addr, Json::default).await?;
    tracing::info!("Listening on port {}", listener.local_addr().port());
    listener.config_mut().max_frame_length(usize::MAX);
    listener
        .filter_map(|r| future::ready(r.ok()))
        .map(server::BaseChannel::with_defaults)
        .max_channels_per_key(1, |t| t.transport().peer_addr().unwrap().ip())
        .map(|channel| {
            let server = RpcServer {
                server: server.clone(),
            };
            channel.execute(server.serve()).for_each(spawn)
        })
        .buffer_unordered(10)
        .for_each(|_| async {})
        .await;

    // // 5. The Event Loop
    // let mut status_events = doc.subscribe().await?;
    //
    // while let Some(event) = status_events.next().await {
    //     if let Ok(iroh_docs::LiveEvent::InsertRemote { entry, .. }) = event {
    //         let key_str = String::from_utf8_lossy(entry.key());
    //
    //         if let Some((profile, var_name)) = key_str.split_once('/') {
    //             let content_hash = entry.content_hash();
    //
    //             if let Ok(value_bytes) = blobs.get(&content_hash).await {
    //                 if let Ok(value_str) = String::from_utf8(value_bytes.to_vec()) {
    //                     let p = profile.to_string();
    //                     let k = var_name.to_string();
    //
    //                     // Execute the async database write directly in the event loop
    //                     let result = sqlx::query(
    //                         "INSERT INTO environment_variables (profile, key, value)
    //                          VALUES (?1, ?2, ?3)
    //                          ON CONFLICT(profile, key) DO UPDATE SET value = excluded.value",
    //                     )
    //                     .bind(&p)
    //                     .bind(&k)
    //                     .bind(&value_str)
    //                     .execute(&pool)
    //                     .await;
    //
    //                     match result {
    //                         Ok(_) => println!("💾 Saved to DB -> Profile: [{}], Key: [{}]", p, k),
    //                         Err(e) => eprintln!("Database error: {}", e),
    //                     }
    //                 }
    //             }
    //         }
    //     }
    // }

    Ok(())
}
