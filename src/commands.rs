use crate::{
    delete::{self, Delete},
    execute::{self, Execute},
    get::{self, Get},
    list::{self, List},
    serve::{self, Serve},
    set::{self, Set},
};

#[derive(clap::Subcommand)]
pub enum Commands {
    Serve(Serve),
    Get(Get),
    Set(Set),
    Execute(Execute),
    List(List),
    Delete(Delete),
}

impl Commands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Commands::Serve(serve) => serve::run(serve).await?,
            Commands::Set(set) => set::run(set).await?,
            Commands::Get(get) => get::run(get).await?,
            Commands::Execute(execute) => execute::run(execute).await?,
            Commands::List(list) => list::run(list).await?,
            Commands::Delete(delete) => delete::run(delete).await?,
        };
        Ok(())
    }
}
