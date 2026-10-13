# Alyson Database & Persistence Specification

## 1. Engine & Driver
- **Engine**: SQLite 3
- **Driver**: `sqlx` (v0.8) with `sqlite` driver and `runtime-tokio`
- **File / URL Strategy**:
  - URL format: `sqlite://alyson.db` or `sqlite:alyson.db?mode=rwc`
  - In-memory test URL: `sqlite::memory:`

---

## 2. Concurrency & Connection Constraints
SQLite operates in file-level or database-level lock regimes. To prevent `SQLITE_BUSY` database lock errors under concurrent asynchronous tasks:
- **Pool Max Connections**: 5 (configurable down to 1 for in-memory testing).
- **WAL Mode**: Write-Ahead Logging (`PRAGMA journal_mode=WAL;`) should be enabled during pool initialization to allow concurrent readers alongside a single writer.
- **Busy Timeout**: Set `PRAGMA busy_timeout = 5000;` (5 seconds) on connection setup.

---

## 3. Schema Design

### Messages Table (`messages`)
Stores all chat history for direct and room conversations.
```sql
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,          -- UNIX timestamp in milliseconds or seconds
    sender_id INTEGER NOT NULL,
    target_id INTEGER NOT NULL,          -- Client ID or Room ID
    is_room BOOLEAN NOT NULL,            -- 0 = Direct peer message, 1 = Room message
    content TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_target ON messages (target_id, is_room);
CREATE INDEX IF NOT EXISTS idx_messages_sender ON messages (sender_id);
```

### Rooms Table (`rooms`)
```sql
CREATE TABLE IF NOT EXISTS rooms (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL
);
```

### Room Members Table (`room_members`)
```sql
CREATE TABLE IF NOT EXISTS room_members (
    room_id INTEGER NOT NULL,
    client_id INTEGER NOT NULL,
    joined_at INTEGER NOT NULL,
    PRIMARY KEY (room_id, client_id),
    FOREIGN KEY (room_id) REFERENCES rooms(id) ON DELETE CASCADE
);
```
