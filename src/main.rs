use anyhow::Result;
use clap::Parser;
use commands::Commands;

mod commands;
mod constants;
mod crypto;
mod delete;
mod execute;
mod get;
mod list;
mod protocol;
mod serve;
mod set;
mod utils;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// The command to execute.
    #[command(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() -> Result<()> {
    let opts = Cli::parse();
    opts.command.run().await?;
    Ok(())
}
