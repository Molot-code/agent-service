// src/db/migrations.rs
use rusqlite::{Connection, Result};

/// Миграция: (таблица, колонка, SQL для добавления).
const MIGRATIONS: &[(&str, &str, &str)] = &[(
    "activity_log",
    "state",
    "ALTER TABLE activity_log ADD COLUMN state TEXT DEFAULT '';",
)];

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn apply(conn: &Connection) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

    let tx = conn.unchecked_transaction()?;
    for (i, (table, column, sql)) in MIGRATIONS.iter().enumerate() {
        let idx = i as i64;
        // Миграция с индексом idx уже применена, если user_version > idx.
        if idx < current {
            continue;
        }
        // Идемпотентность на случай «грязной» БД: колонка уже есть — пропускаем.
        if !column_exists(&tx, table, column)? {
            tx.execute_batch(sql)?;
        }
        tx.pragma_update(None, "user_version", idx + 1)?;
    }
    tx.commit()?;
    Ok(())
}
