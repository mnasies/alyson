use sqlx::Row;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::collections::HashMap;
use std::collections::HashSet;
use std::str::FromStr;
use std::time::Duration;
use std::time::UNIX_EPOCH;

use crate::types::{InboxEntry, Room};

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

pub async fn insert_room(name: &str, owner_id: i64, pool: &SqlitePool) -> Result<u64, AlysonError> {
    // check if user already exists
    let existing = sqlx::query("SELECT id FROM rooms WHERE name = ?1")
        .bind(name)
        .fetch_optional(pool)
        .await
        .map_err(|e| AlysonError::Database(e.to_string()))?;

    if let Some(row) = existing {
        let id: i64 = row
            .try_get("id")
            .map_err(|e| AlysonError::Database(e.to_string()))?;
        return Ok(id as u64);
    }

    let result = sqlx::query("INSERT INTO rooms (name, owner_id) VALUES (?1, ?2)")
        .bind(name)
        .bind(owner_id)
        .execute(pool)
        .await
        .map_err(|e| AlysonError::Database(e.to_string()))?;

    Ok(result.last_insert_rowid() as u64)
}

pub async fn insert_user(username: &str, pool: &SqlitePool) -> Result<u64, AlysonError> {
    // check if user already exists
    let existing = sqlx::query("SELECT id FROM clients WHERE username = ?1")
        .bind(username)
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
        .bind(username)
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

    if message.is_to_client {
        sqlx::query("INSERT INTO messages (sender_id, recipient_client_id, is_to_client, message, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)")
            .bind(message.from as i64)
            .bind(message.to as i64)
            .bind(message.is_to_client)
            .bind(&message.msg)
            .bind(ts)
            .execute(pool)
            .await
            .map_err(|e| AlysonError::Database(e.to_string()))?;
    } else {
        sqlx::query("INSERT INTO messages (sender_id, recipient_room_id, is_to_client, message, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)")
            .bind(message.from as i64)
            .bind(message.to as i64)
            .bind(message.is_to_client)
            .bind(&message.msg)
            .bind(ts)
            .execute(pool)
            .await
            .map_err(|e| AlysonError::Database(e.to_string()))?;
    }

    Ok(())
}

pub async fn fetch_rooms(pool: &SqlitePool) -> Result<HashMap<u64, Room>, AlysonError> {
    let rows = sqlx::query(
        "
        SELECT id, name, owner_id
        FROM rooms
    ",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AlysonError::Database(e.to_string()))?;

    let mut rooms = HashMap::new();

    for row in rows {
        // fetch necessary info from db and create room
        let id: i64 = row
            .try_get("id")
            .map_err(|e| AlysonError::Database(e.to_string()))?;

        let members_rows = sqlx::query(
            "
                SELECT user_id
                FROM room_members
                WHERE room_id = ?1
            ",
        )
        .bind(id)
        .fetch_all(pool)
        .await
        .map_err(|e| AlysonError::Database(e.to_string()))?;

        let name: String = row
            .try_get("name")
            .map_err(|e| AlysonError::Database(e.to_string()))?;

        let owner: i64 = row
            .try_get("owner_id")
            .map_err(|e| AlysonError::Database(e.to_string()))?;

        let mut room = Room::new(id as u64, name, owner as u64);

        // fetch all members from db

        let mut members: HashSet<i64> = HashSet::new();

        for mem in members_rows {
            let mem_id: i64 = mem
                .try_get("user_id")
                .map_err(|e| AlysonError::Database(e.to_string()))?;

            members.insert(mem_id);
        }

        for member in members {
            room.add_member(member as u64);
        }

        rooms.insert(id as u64, room);
    }

    Ok(rooms)
}

pub async fn fetch_history_for_room(
    room_id: u64,
    pool: &SqlitePool,
) -> Result<HashMap<u64, InboxEntry>, AlysonError> {
    let rows = sqlx::query("
        SELECT id, sender_id, recipient_client_id, recipient_room_id, is_to_client, message, timestamp
        FROM messages
        WHERE is_to_client = 0
    ")
    .bind(room_id as i64)
    .fetch_all(pool)
    .await
    .map_err(|e| AlysonError::Database(e.to_string()))?;

    let mut history = HashMap::new();

    for row in rows {
        let id: i64 = row
            .try_get("id")
            .map_err(|e| AlysonError::Database(e.to_string()))?;

        let sender_id: i64 = row
            .try_get("sender_id")
            .map_err(|e| AlysonError::Database(e.to_string()))?;

        let recipient_id: i64 = row
            .try_get("recipient_room_id")
            .map_err(|e| AlysonError::Database(e.to_string()))?;

        let is_to_client: bool = row
            .try_get("is_to_client")
            .map_err(|e| AlysonError::Database(e.to_string()))?;

        let msg: String = row
            .try_get("message")
            .map_err(|e| AlysonError::Database(e.to_string()))?;

        let timestamp: i64 = row
            .try_get("timestamp")
            .map_err(|e| AlysonError::Database(e.to_string()))?;

        let time = UNIX_EPOCH + Duration::from_secs(timestamp as u64);

        history.insert(
            id as u64,
            InboxEntry::new(
                time,
                msg,
                sender_id as u64,
                recipient_id as u64,
                is_to_client,
            ),
        );
    }

    Ok(history)
}

pub async fn fetch_users(pool: &SqlitePool) -> Result<HashMap<u64, String>, AlysonError> {
    let rows = sqlx::query(
        "
        SELECT id, username, created_at
        FROM clients
    ",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AlysonError::Database(e.to_string()))?;

    let mut clients = HashMap::new();

    for row in rows {
        let id: i64 = row
            .try_get("id")
            .map_err(|e| AlysonError::Database(e.to_string()))?;

        let username: String = row
            .try_get("username")
            .map_err(|e| AlysonError::Database(e.to_string()))?;

        clients.insert(id as u64, username);
    }

    Ok(clients)
}

pub async fn fetch_history_for_user(
    client_id: u64,
    pool: &SqlitePool,
) -> Result<HashMap<u64, InboxEntry>, AlysonError> {
    let rows = sqlx::query("
        SELECT id, sender_id, recipient_client_id, recipient_room_id, is_to_client, message, timestamp
        FROM messages
        WHERE is_to_client = 1
          AND (sender_id = ?1 OR recipient_client_id = ?1)
    ")
    .bind(client_id as i64)
    .fetch_all(pool)
    .await
    .map_err(|e| AlysonError::Database(e.to_string()))?;

    let mut history = HashMap::new();

    for row in rows {
        let id: i64 = row
            .try_get("id")
            .map_err(|e| AlysonError::Database(e.to_string()))?;

        let sender_id: i64 = row
            .try_get("sender_id")
            .map_err(|e| AlysonError::Database(e.to_string()))?;

        let recipient_id: i64 = row
            .try_get("recipient_client_id")
            .map_err(|e| AlysonError::Database(e.to_string()))?;

        let is_to_client: bool = row
            .try_get("is_to_client")
            .map_err(|e| AlysonError::Database(e.to_string()))?;

        let msg: String = row
            .try_get("message")
            .map_err(|e| AlysonError::Database(e.to_string()))?;

        let timestamp: i64 = row
            .try_get("timestamp")
            .map_err(|e| AlysonError::Database(e.to_string()))?;

        let time = UNIX_EPOCH + Duration::from_secs(timestamp as u64);

        history.insert(
            id as u64,
            InboxEntry::new(
                time,
                msg,
                sender_id as u64,
                recipient_id as u64,
                is_to_client,
            ),
        );
    }

    Ok(history)
}
