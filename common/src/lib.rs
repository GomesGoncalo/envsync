use anyhow::Result;
use std::path::Path;

pub mod rpc;

pub async fn prepare_dir(path: &Path) -> Result<&Path> {
    tokio::fs::create_dir_all(&path).await?;
    Ok(path)
}
