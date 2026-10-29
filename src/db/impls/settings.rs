use chrono::Local;
use rusqlite::{params, Result};

use super::super::AppDb;
use crate::types::SettingItem;

/// Метаданные настройки (для UI/валидации).
pub struct SettingMeta {
    pub value_type: String,
    pub label: String,
    pub description: String,
    pub category: String,
}

impl AppDb {
    // ----- settings -----

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached("SELECT value FROM settings WHERE key = ?1")?;
        match stmt.query_row(params![key], |r| r.get(0)) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.prepare_cached(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )?
        .execute(params![key, value])?;
        Ok(())
    }

    pub fn get_settings(&self) -> Result<Vec<SettingItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT key, value, category, value_type, label, description, sort_order, is_secret
             FROM settings
             WHERE category != 'internal'
             ORDER BY category, sort_order, key",
        )?;
        let items = stmt
            .query_map([], |row| {
                Ok(SettingItem {
                    key: row.get(0)?,
                    value: row.get(1)?,
                    category: row.get(2)?,
                    value_type: row.get(3)?,
                    label: row.get(4)?,
                    description: row.get(5)?,
                    sort_order: row.get(6)?,
                    is_secret: row.get(7)?,
                })
            })?
            .collect();
        items
    }

    pub fn get_setting_meta(&self, key: &str) -> Result<Option<SettingMeta>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT value_type, label, description, category FROM settings WHERE key = ?1",
        )?;
        match stmt.query_row(params![key], |row| {
            Ok(SettingMeta {
                value_type: row.get(0)?,
                label: row.get(1)?,
                description: row.get(2)?,
                category: row.get(3)?,
            })
        }) {
            Ok(m) => Ok(Some(m)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    // ----- system_state -----

    pub fn get_state(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached("SELECT value FROM system_state WHERE key = ?1")?;
        match stmt.query_row(params![key], |r| r.get(0)) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn set_state(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.prepare_cached(
            "INSERT INTO system_state (key, value, updated_at)
             VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = datetime('now')",
        )?
        .execute(params![key, value])?;
        Ok(())
    }
}

impl AppDb {
    /// Пишет state.lock_status. Возвращает true, если состояние изменилось.
    pub fn apply_lock_status(&self, locked: bool) -> Result<bool> {
        let prev_locked = self.get_state("lock_status").ok().flatten().as_deref() == Some("true");
        if prev_locked == locked {
            return Ok(false);
        }
        self.set_state("lock_status", if locked { "true" } else { "false" })?;
        Ok(true)
    }

    /// Атомарно пишет per-user internet_paused И глобальный system_state.internet_blocked.
    /// Возвращает true, если состояние фактически изменилось.
    /// Кладём сюда же, рядом с set_state — логически это «состояния».
    pub fn apply_internet_block(&self, sid: &str, blocked: bool, source: &str) -> Result<bool> {
        use rusqlite::OptionalExtension;

        let today = Local::now().format("%Y-%m-%d").to_string();
        let conn = self.conn.lock().unwrap();

        let prev: Option<i32> = conn
            .query_row(
                "SELECT internet_paused FROM daily_limits
                 WHERE user_sid = ?1 AND date = ?2",
                params![sid, today],
                |r| r.get(0),
            )
            .optional()?;
        let prev_blocked = prev.map(|v| v != 0).unwrap_or(false);

        conn.execute(
            "INSERT INTO daily_limits
                (user_sid, date, internet_paused, internet_paused_source)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(user_sid, date) DO UPDATE SET
                internet_paused = ?3, internet_paused_source = ?4",
            params![sid, today, blocked as i32, source],
        )?;

        conn.prepare_cached(
            "INSERT INTO system_state (key, value, updated_at)
             VALUES ('internet_blocked', ?1, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value = ?1, updated_at = datetime('now')",
        )?
        .execute(params![if blocked { "true" } else { "false" }])?;

        Ok(prev_blocked != blocked)
    }
}
