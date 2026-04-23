use anyhow::Result;
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use std::{
    convert::AsRef,
    path::{Path, PathBuf},
};

pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn new(path: impl Into<PathBuf> + AsRef<Path>) -> Result<Self> {
        let db_options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);

        let pool = SqlitePool::connect_with(db_options).await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS environment_variables (
            profile TEXT NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            PRIMARY KEY (profile, key)
        )",
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }
}
