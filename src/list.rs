use crate::protocol::IrohAutomergeProtocol;

use anyhow::Result;
use automerge::{Automerge, ReadDoc, Value};
use clap::Args;
use iroh::{Endpoint, endpoint::presets, protocol::Router};
use tokio::sync::mpsc;

#[derive(clap::Subcommand)]
enum ListType {
    /// List all profiles. This will print the names of all profiles that exist in the document, including the global profile.
    Profiles,

    /// List all keys within a profile. This will print the names of all keys that exist within the specified profile. If the profile does not exist, no output will be printed.
    Keys {
        /// The profile to list keys from. If the profile does not exist, no output will be printed. By default, the global profile will be used.
        #[arg(short, long, default_value = crate::constants::GLOBAL_PROFILE)]
        profile: String,
    },
}

#[derive(Args)]
pub struct List {
    /// The type of list operation to perform: listing all profiles, or listing all keys within a specific profile.
    #[clap(subcommand)]
    list_type: ListType,

    /// The remote endpoint ID to connect to for syncing the latest state before performing the list operation.
    #[arg(short, long)]
    remote_id: iroh::EndpointId,
}

pub async fn run(
    List {
        list_type,
        remote_id,
    }: List,
) -> Result<()> {
    let automerge = IrohAutomergeProtocol::new(Automerge::new(), mpsc::channel(10).0);
    let endpoint = Endpoint::bind(presets::N0).await?;
    let iroh = Router::builder(endpoint)
        .accept(IrohAutomergeProtocol::ALPN, automerge.clone())
        .spawn();

    let endpoint_addr = iroh::EndpointAddr::new(remote_id);
    let conn = iroh
        .endpoint()
        .connect(endpoint_addr, IrohAutomergeProtocol::ALPN)
        .await?;

    automerge.clone().initiate_sync(conn).await?;

    iroh.shutdown().await?;

    let doc = automerge.fork_doc().await;

    match list_type {
        ListType::Profiles => {
            let keys = doc.keys(automerge::ROOT);
            for key in keys {
                if let Ok(Some((Value::Object(automerge::ObjType::Map), _id))) =
                    doc.get(automerge::ROOT, &key)
                {
                    println!("{key}");
                }
            }
        }
        ListType::Keys { profile } => {
            if let Some((Value::Object(automerge::ObjType::Map), id)) =
                doc.get(automerge::ROOT, &profile)?
            {
                let keys = doc.keys(&id);
                for key in keys {
                    println!("{key}");
                }
            } else {
                eprintln!("Profile '{profile}' not found.");
            }
        }
    }

    Ok(())
}
