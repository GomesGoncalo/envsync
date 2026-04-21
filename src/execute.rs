use crate::{protocol::IrohAutomergeProtocol, utils};

use anyhow::Result;
use automerge::{Automerge, ReadDoc, Value};
use clap::Args;
use iroh::{Endpoint, endpoint::presets, protocol::Router};
use std::{collections::HashMap, process::Command};
use tokio::sync::mpsc;

#[derive(Args)]
pub struct Execute {
    /// If set, only variables from the specified profile will be included, and global variables will be ignored. By default, variables from the global profile will be included and overridden by any variables in the specified profile.
    #[arg(long)]
    exclusive: bool,

    /// The profile to execute with. Variables from this profile will be included, and if `exclusive` is not set, will override any variables from the global profile with the same key.
    #[arg(short, long, default_value = crate::constants::GLOBAL_PROFILE)]
    profile: String,

    /// The remote endpoint ID to connect to for syncing the latest state before executing the command.
    #[arg(short, long, env = "IROH_REMOTE_ID")]
    remote_id: iroh::EndpointId,
}

pub fn automerge_to_hashmap(doc: &Automerge, profile: &str) -> Option<HashMap<String, String>> {
    let mut map = HashMap::new();

    let Ok(Some((Value::Object(automerge::ObjType::Map), profile_id))) =
        doc.get(automerge::ROOT, profile)
    else {
        return None;
    };

    let keys = doc.keys(&profile_id);

    for key in keys {
        if let Ok(Some((Value::Scalar(value), _obj_id))) = doc.get(&profile_id, &key) {
            map.insert(key, utils::clean_string(value));
        }
    }

    Some(map)
}

pub async fn run(
    Execute {
        profile,
        remote_id,
        exclusive,
    }: Execute,
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
    let vars = automerge_to_hashmap(&doc, &profile)
        .ok_or_else(|| anyhow::anyhow!("Profile '{}' not found or not a map", profile))?;
    let global_vars =
        automerge_to_hashmap(&doc, crate::constants::GLOBAL_PROFILE).unwrap_or_default();
    let merged: HashMap<_, _> = if exclusive {
        vars
    } else {
        global_vars.into_iter().chain(vars).collect()
    };

    Command::new("bash").envs(merged).spawn()?.wait()?;
    Ok(())
}
