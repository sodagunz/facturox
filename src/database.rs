//! SQLite configuration and development schema setup.
use std::{path::Path, time::Duration};

use sqlx::{
    Connection, SqliteConnection,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous},
};

use crate::catalog::{CatalogError, storage};

pub(crate) async fn open(path: &Path) -> Result<SqliteConnection, CatalogError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    tokio::fs::create_dir_all(parent).await.map_err(storage)?;
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .busy_timeout(Duration::from_secs(2));
    let mut conn = SqliteConnection::connect_with(&options)
        .await
        .map_err(storage)?;
    let mut tx = conn.begin_with("BEGIN IMMEDIATE").await.map_err(storage)?;
    sqlx::raw_sql(include_str!("schema.sql"))
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
    tx.commit().await.map_err(storage)?;
    Ok(conn)
}
