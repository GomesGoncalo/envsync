use crate::protocol::IrohAutomergeProtocol;

use anyhow::Result;
use automerge::{Automerge, ObjType, ReadDoc, Value, transaction::Transactable};
use clap::Args;
use iroh::{Endpoint, endpoint::presets, protocol::Router};
use tokio::sync::mpsc;

#[derive(Args)]
pub struct Set {
    /// The key to set within the specified profile. If the key already exists, its value will be overwritten with the new value.
    key: String,

    /// The value to set for the specified key within the specified profile. If the key already exists, its value will be overwritten with this new value.
    value: String,
    #[arg(short, long, default_value = crate::constants::GLOBAL_PROFILE)]

    /// The profile to set the key-value pair within. If the profile does not exist, it will be created. By default, the global profile will be used.
    profile: String,

    /// The remote endpoint ID to connect to for syncing the latest state before setting the key-value pair.
    #[arg(short, long, env = "IROH_REMOTE_ID")]
    remote_id: iroh::EndpointId,
}

pub async fn run(
    Set {
        key,
        value,
        profile,
        remote_id,
    }: Set,
) -> Result<()> {
    let automerge = IrohAutomergeProtocol::new(Automerge::new(), mpsc::channel(10).0);
    let endpoint = Endpoint::bind(presets::N0).await?;
    let iroh = Router::builder(endpoint)
        .accept(IrohAutomergeProtocol::ALPN, automerge.clone())
        .spawn();

    let endpoint_id = iroh.endpoint().id();

    println!("Running\nEndpoint Id: {endpoint_id}",);

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

    let profile_id = match t.get(automerge::ROOT, &profile)? {
        Some((Value::Object(ObjType::Map), id)) => id,
        None => t.put_object(automerge::ROOT, profile, ObjType::Map)?,
        _ => {
            return Err(anyhow::anyhow!(
                "Key {} exists but is not a profile map!",
                profile
            ));
        }
    };

    t.put(&profile_id, key, value)?;
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
