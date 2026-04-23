use anyhow::Result;
use common::prepare_dir;
use iroh::{Endpoint, endpoint::presets};
use iroh_blobs::store::fs::FsStore;
use iroh_docs::{AuthorId, DocTicket, api::Doc, protocol::Docs};
use iroh_gossip::Gossip;
use std::path::Path;

pub struct Sync {
    endpoint: Endpoint,
    blobs: FsStore,
    gossip: Gossip,
    docs: Docs,
    author: AuthorId,
}

impl Sync {
    pub async fn new(path: &Path) -> Result<Self> {
        let blobs_path = path.join("blobs");
        let docs_path = path.join("docs");
        prepare_dir(&blobs_path).await?;
        prepare_dir(&docs_path).await?;

        let endpoint = Endpoint::builder(presets::N0).bind().await?;
        let blobs = FsStore::load(blobs_path).await?;
        let gossip = Gossip::builder().spawn(endpoint.clone());
        let docs = Docs::persistent(docs_path)
            .spawn(endpoint.clone(), (*blobs).clone(), gossip.clone())
            .await?;
        let author = docs.author_default().await?;
        Ok(Self {
            endpoint,
            blobs,
            gossip,
            docs,
            author,
        })
    }

    pub async fn create(&self) -> Result<Doc> {
        self.docs.create().await
    }

    pub async fn join(&self, ticket: DocTicket) -> Result<Doc> {
        self.docs.import(ticket).await
    }

    pub async fn author(&self) -> Result<AuthorId> {
        Ok(self.author)
    }

    pub fn blobs(&self) -> &FsStore {
        &self.blobs
    }
}
