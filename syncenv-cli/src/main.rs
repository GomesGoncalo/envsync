use clap::{Parser, Subcommand};
use common::rpc::RpcServiceClient;
use tarpc::{client, context, tokio_serde::formats::Json};

#[derive(Subcommand, Debug)]
enum Command {
    JoinOrCreate {
        #[clap(short, long)]
        ticket: Option<String>,
    },
    SetEnv {
        #[clap(short, long)]
        profile: String,
        #[clap(short, long)]
        key: String,
        #[clap(short, long)]
        val: String,
    },
    GetEnv {
        #[clap(short, long)]
        profile: String,
        #[clap(short, long)]
        key: String,
    },
}

#[derive(Debug, clap::Parser)]
struct Args {
    #[clap(short, long, default_value = "9999")]
    port: u16,

    #[clap(subcommand)]
    command: Command,
}

async fn run(client: RpcServiceClient, command: Command) -> anyhow::Result<()> {
    match command {
        Command::JoinOrCreate { ticket } => {
            let result = client
                .join_or_create_doc(context::current(), ticket)
                .await?
                .map_err(|e| anyhow::anyhow!(e))?;
            println!("{}", result);
        }
        Command::SetEnv { profile, key, val } => {
            client
                .set_env(context::current(), profile, key, val)
                .await?
                .map_err(|e| anyhow::anyhow!(e))?;
            println!("Environment variable set successfully");
        }
        Command::GetEnv { profile, key } => {
            let result = client
                .get_env(context::current(), profile, key)
                .await?
                .map_err(|e| anyhow::anyhow!(e))?;
            match result {
                Some(val) => println!("{}", val),
                None => println!("(not set)"),
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let mut transport =
        tarpc::serde_transport::tcp::connect(format!("[::1]:{}", args.port), Json::default);
    transport.config_mut().max_frame_length(usize::MAX);
    let client = RpcServiceClient::new(client::Config::default(), transport.await?).spawn();

    if let Err(e) = run(client, args.command).await {
        tracing::warn!("RPC call failed: {:?}", e);
        anyhow::bail!("RPC call failed: {:?}", e);
    }

    Ok(())
}
