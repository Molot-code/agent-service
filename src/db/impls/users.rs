use rusqlite::{params, Result};

use super::super::AppDb;

impl AppDb {
    pub fn get_all_users(&self) -> Result<Vec<(String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached("SELECT sid, username FROM users")?;
        let users = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect();
        users
    }

    pub fn get_or_create_user(&self, sid: &str, username: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO users (sid, username) VALUES (?1, ?2)
             ON CONFLICT(sid) DO UPDATE SET username = ?2",
            params![sid, username],
        )?;

        // 7 вставок в одной транзакции вместо 7 отдельных execute.
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR IGNORE INTO weekly_schedule
                    (user_sid, day_of_week, limit_sec, internet_limit_sec, start_time, end_time)
                 VALUES (?1, ?2, 7200, 7200, '08:00', '20:00')",
            )?;
            for day in 0..7 {
                stmt.execute(params![sid, day])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_username_by_sid(&self, sid: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached("SELECT username FROM users WHERE sid = ?1")?;
        let mut rows = stmt.query([sid])?;
        rows.next()?.map(|r| r.get(0)).transpose()
    }

    pub fn is_user_control_enabled(&self, sid: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT COALESCE(control_enabled, 1) FROM users WHERE sid = ?1",
        )?;
        let v: i32 = stmt.query_row([sid], |r| r.get(0)).unwrap_or(1);
        Ok(v != 0)
    }

    pub fn set_user_control_enabled(&self, sid: &str, enabled: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE users SET control_enabled = ?1 WHERE sid = ?2",
            params![enabled as i32, sid],
        )?;
        Ok(())
    }

    pub fn get_users_with_control_status(&self) -> Result<Vec<(String, String, bool)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT sid, username, COALESCE(control_enabled, 1) FROM users",
        )?;
        let users = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i32>(2)? != 0,
                ))
            })?
            .collect();
        users
    }

    /// Деактивирует пользователей, которых нет в active_sids.
    /// Возвращает количество деактивированных.
    pub fn cleanup_stale_users(&self, active_sids: &[String]) -> Result<usize> {
        if active_sids.is_empty() {
            return Ok(0);
        }
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;

        let db_sids: Vec<String> = {
            let mut stmt = tx.prepare_cached("SELECT sid FROM users WHERE is_active = 1")?;
            let rows = stmt
                .query_map([], |r| r.get(0))?
                .filter_map(Result::ok)
                .collect();
            rows
        };

        let mut n = 0;
        {
            let mut upd = tx.prepare_cached("UPDATE users SET is_active = 0 WHERE sid = ?1")?;
            for s in &db_sids {
                if !active_sids.iter().any(|a| a == s) {
                    n += upd.execute([s])?;
                }
            }
        }
        tx.commit()?;
        Ok(n)
    }
}