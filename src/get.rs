use crate::{protocol::IrohAutomergeProtocol, utils};

use anyhow::Result;
use automerge::{Automerge, ReadDoc, Value};
use clap::Args;
use iroh::{Endpoint, endpoint::presets, protocol::Router};
use tokio::sync::mpsc;

#[derive(Args)]
pub struct Get {
    /// The key to retrieve from the specified profile. If the key does not exist, no value will be printed.
    key: String,
    #[arg(short, long, default_value = crate::constants::GLOBAL_PROFILE)]

    /// The profile to retrieve the key from. If the profile does not exist, no value will be printed. By default, the global profile will be used.
    profile: String,

    /// The remote endpoint ID to connect to for syncing the latest state before retrieving the value.
    #[arg(short, long, env = "IROH_REMOTE_ID")]
    remote_id: iroh::EndpointId,
}

pub async fn run(
    Get {
        key,
        profile,
        remote_id,
    }: Get,
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

    if let Some((Value::Object(automerge::ObjType::Map), id)) =
        doc.get(automerge::ROOT, &profile)?
        && let Some((automerge::Value::Scalar(value), _)) = doc.get(&id, &key)?
    {
        println!("{key}={}", utils::clean_string(value));
    }

    Ok(())
}
