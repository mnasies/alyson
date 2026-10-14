use sqlx::Row;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::str::FromStr;
use std::time::Duration;

use crate::types::{ClientInfo, InboxEntry};

use crate::AlysonError;

pub async fn init_server_db(database_url: &str) -> Result<SqlitePool, AlysonError> {
    let options = SqliteConnectOptions::from_str(&database_url)
        .map_err(|e| AlysonError::Database(e.to_string()))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect_with(options)
        .await
        .map_err(|e| AlysonError::Database(e.to_string()))?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS clients (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           username TEXT NOT NULL UNIQUE,
           created_at INTEGER NOT NULL DEFAULT (unixepoch())
        );
        CREATE TABLE IF NOT EXISTS messages (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           sender_id INTEGER NOT NULL,

           recipient_client_id INTEGER,
           recipient_room_id INTEGER,

           is_to_client BOOLEAN NOT NULL,
           message TEXT NOT NULL,
           timestamp INTEGER NOT NULL,

           FOREIGN KEY(sender_id) REFERENCES clients(id),
           FOREIGN KEY(recipient_client_id) REFERENCES clients(id),
           FOREIGN KEY(recipient_room_id) REFERENCES rooms(id),

           CONSTRAINT chk_conditional_fk CHECK (
                   (is_to_client = TRUE  AND recipient_client_id IS NOT NULL AND recipient_room_id IS NULL) OR
                   (is_to_client = FALSE AND recipient_room_id IS NOT NULL AND recipient_client_id IS NULL)
               )
        );
        CREATE TABLE IF NOT EXISTS rooms (
           id INTEGER PRIMARY KEY,
           name TEXT NOT NULL,
           owner_id INTEGER NOT NULL,

           FOREIGN KEY(owner_id) REFERENCES clients(id)
        );
        CREATE TABLE IF NOT EXISTS room_members (
           room_id INTEGER NOT NULL,
           user_id INTEGER NOT NULL,
           PRIMARY KEY (room_id, user_id),

           FOREIGN KEY (room_id) REFERENCES rooms(id) ON DELETE CASCADE,
           FOREIGN KEY (user_id) REFERENCES clients(id) ON DELETE CASCADE
        );"
    )
    .execute(&pool)
    .await
    .map_err(|e| AlysonError::Database(e.to_string()))?;

    Ok(pool)
}

pub async fn insert_user(client: &ClientInfo, pool: &SqlitePool) -> Result<u64, AlysonError> {
    // check if user already exists
    let existing = sqlx::query("SELECT id FROM clients WHERE username = ?1")
        .bind(&client.username)
        .fetch_optional(pool)
        .await
        .map_err(|e| AlysonError::Database(e.to_string()))?;

    if let Some(row) = existing {
        let id: i64 = row
            .try_get("id")
            .map_err(|e| AlysonError::Database(e.to_string()))?;
        return Ok(id as u64);
    }

    let result = sqlx::query("INSERT INTO clients (username) VALUES (?1)")
        .bind(&client.username)
        .execute(pool)
        .await
        .map_err(|e| AlysonError::Database(e.to_string()))?;

    Ok(result.last_insert_rowid() as u64)
}

pub async fn insert_messages(message: &InboxEntry, pool: &SqlitePool) -> Result<(), AlysonError> {
    let ts = message
        .time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    if message.cli_or_room {
        sqlx::query("INSERT INTO messages (sender_id, recipient_client_id, is_to_client, message, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)")
            .bind(message.from as i64)
            .bind(message.to as i64)
            .bind(message.cli_or_room)
            .bind(&message.msg)
            .bind(ts)
            .execute(pool)
            .await
            .map_err(|e| AlysonError::Database(e.to_string()))?;
    } else {
        sqlx::query("INSERT INTO messages (sender_id, recipient_room_id, is_to_client, message, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)")
            .bind(message.from as i64)
            .bind(message.to as i64)
            .bind(message.cli_or_room)
            .bind(&message.msg)
            .bind(ts)
            .execute(pool)
            .await
            .map_err(|e| AlysonError::Database(e.to_string()))?;
    }

    Ok(())
}
