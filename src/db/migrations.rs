// src/db/migrations.rs
use rusqlite::{Connection, Result};

/// Список миграций. Порядок важен: каждая — отдельная, идемпотентная операция,
/// применяемая ровно один раз (отслеживается по `PRAGMA user_version`).
/// Новые миграции добавляются в конец списка, существующие не редактируются.
const MIGRATIONS: &[&str] = &[
    // 0001: колонка activity_log.state для существующих БД
    "ALTER TABLE activity_log ADD COLUMN state TEXT DEFAULT '';",
    // 0002: колонка users.is_active для существующих БД
    "ALTER TABLE users ADD COLUMN is_active INTEGER NOT NULL DEFAULT 1;",
];

/// Применяет все неотработанные миграции. Идемпотентно: уже применённые
/// (user_version >= их номеров) пропускаются.
pub fn apply(conn: &Connection) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current < 0 {
        return Err(rusqlite::Error::InvalidQuery);
    }

    let tx = conn.unchecked_transaction()?;
    for (i, sql) in MIGRATIONS.iter().enumerate() {
        let version = current + 1 + i as i64;
        if version <= current {
            continue;
        }
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
    }
    tx.commit()?;
    Ok(())
}
