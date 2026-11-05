//! Private persistence boundary.
use sqlx::{
    Connection, SqliteConnection,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous},
};
use std::{path::Path, time::Duration};
pub async fn open(path: &Path) -> Result<SqliteConnection, String> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .busy_timeout(Duration::from_secs(5));
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::migrate!("./migrations")
        .run_direct(None, &mut connection, false)
        .await
        .map_err(|e| e.to_string())?;
    Ok(connection)
}
