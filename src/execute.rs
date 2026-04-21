use crate::{protocol::IrohAutomergeProtocol, utils};

use anyhow::Result;
use automerge::{Automerge, ReadDoc, Value};
use clap::Args;
use iroh::{Endpoint, endpoint::presets, protocol::Router};
use std::{collections::HashMap, process::Command, env};
use tokio::sync::mpsc;

#[derive(Args)]
pub struct Execute {
    /// If set, only variables from the specified profile will be included, and global variables will be ignored.
    #[arg(long)]
    exclusive: bool,

    /// The profile to execute with. Variables from this profile will be included, and if `exclusive` is not set, will override any variables from the global profile with the same key.
    #[arg(short, long, default_value = crate::constants::GLOBAL_PROFILE)]
    profile: String,

    /// The remote endpoint ID to connect to for syncing the latest state before executing the command.
    #[arg(short, long, env = "IROH_REMOTE_ID")]
    remote_id: iroh::EndpointId,

    /// Command and arguments to execute. If omitted, an interactive shell is launched.
    #[arg(trailing_var_arg = true)]
    command: Vec<String>,
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

fn get_shell_command() -> (String, Vec<String>) {
    // Prefer explicit shells from env vars; fall back to sensible defaults per-platform.
    if cfg!(windows) {
        if let Ok(comspec) = env::var("COMSPEC") {
            return (comspec, Vec::new());
        }
        ("cmd.exe".to_string(), Vec::new())
    } else {
        if let Ok(shell) = env::var("SHELL") {
            return (shell, Vec::new());
        }
        ("sh".to_string(), Vec::new())
    }
}

pub async fn run(
    Execute {
        profile,
        remote_id,
        exclusive,
        command,
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

    // Build command: either user-specified or fallback to shell.
    let mut cmd = if command.is_empty() {
        let (shell, args) = get_shell_command();
        let mut c = Command::new(shell);
        if !args.is_empty() {
            c.args(args);
        }
        c
    } else {
        let mut c = Command::new(&command[0]);
        if command.len() > 1 {
            c.args(&command[1..]);
        }
        c
    };

    cmd.envs(&merged);
    let status = cmd.status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("Command exited with non-zero status: {}", status));
    }
    Ok(())
}
