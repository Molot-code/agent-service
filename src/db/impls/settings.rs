use rusqlite::{params, Result};

use crate::types::SettingItem;

use super::super::AppDb;

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