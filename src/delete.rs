use crate::protocol::IrohAutomergeProtocol;

use anyhow::Result;
use automerge::{Automerge, ObjType, ReadDoc, Value, transaction::Transactable};
use clap::Args;
use iroh::{Endpoint, endpoint::presets, protocol::Router};
use tokio::sync::mpsc;

#[derive(clap::Subcommand)]
enum DeleteType {
    /// Delete a specific key from a profile. If the profile or key does not exist, no changes will be made.
    Key { key: String },

    /// Delete an entire profile and all keys within it. If the profile does not exist, no changes will be made.
    Profile,
}

#[derive(Args)]
pub struct Delete {
    /// The type of delete operation to perform: deleting a specific key from a profile, or deleting an entire profile.
    #[clap(subcommand)]
    delete_type: DeleteType,

    /// The profile to delete from. If `delete_type` is `Key`, the specified key will be deleted from this profile. If `delete_type` is `Profile`, this entire profile will be deleted. By default, the global profile will be used.
    #[arg(short, long, default_value = crate::constants::GLOBAL_PROFILE)]
    profile: String,

    /// The remote endpoint ID to connect to for syncing the latest state before performing the delete operation.
    #[arg(short, long, env = "IROH_REMOTE_ID")]
    remote_id: iroh::EndpointId,
}

pub async fn run(
    Delete {
        delete_type,
        profile,
        remote_id,
    }: Delete,
) -> Result<()> {
    let automerge = IrohAutomergeProtocol::new(Automerge::new(), mpsc::channel(10).0);
    let endpoint = Endpoint::bind(presets::N0).await?;
    let iroh = Router::builder(endpoint)
        .accept(IrohAutomergeProtocol::ALPN, automerge.clone())
        .spawn();

    let endpoint_id = iroh.endpoint().id();
    let endpoint_addr = iroh::EndpointAddr::new(remote_id);

    if let Ok(conn) = iroh
        .endpoint()
        .connect(endpoint_addr.clone(), IrohAutomergeProtocol::ALPN)
        .await
    {
        // perform a sync to pull remote changes into our local doc
        automerge.clone().initiate_sync(conn).await?;
    } else {
        eprintln!(
            "Warning: could not connect to remote to pull latest state; proceeding with local write"
        );
    }

    let mut doc = automerge.fork_doc().await;
    let mut t = doc.transaction();

    match delete_type {
        DeleteType::Key { key } => {
            if let Some((Value::Object(ObjType::Map), profile_id)) =
                t.get(automerge::ROOT, &profile)?
            {
                t.delete(&profile_id, &key)?;
            } else {
                eprintln!("Profile {profile} does not exist; cannot delete key {key}");
            }
        }
        DeleteType::Profile => {
            if t.get(automerge::ROOT, &profile)?.is_some() {
                t.delete(automerge::ROOT, &profile)?;
            } else {
                eprintln!("Profile {profile} does not exist; cannot delete");
            }
        }
    }

    t.commit();

    automerge.merge_doc(&mut doc).await?;

    let conn = iroh
        .endpoint()
        .connect(endpoint_addr, IrohAutomergeProtocol::ALPN)
        .await?;

    automerge.initiate_sync(conn).await?;

    iroh.shutdown().await?;

    Ok(())
}
