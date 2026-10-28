use rusqlite::{params, Result};

use crate::types::DaySchedule;

use super::super::AppDb;

impl AppDb {
    fn map_schedule(row: &rusqlite::Row<'_>) -> Result<DaySchedule> {
        Ok(DaySchedule {
            day_of_week: row.get(0)?,
            limit_sec: row.get(1)?,
            internet_limit_sec: row.get(2)?,
            start_time: row.get(3)?,
            end_time: row.get(4)?,
        })
    }

    pub fn get_week_schedule(&self, sid: &str) -> Result<Vec<DaySchedule>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT day_of_week, limit_sec, internet_limit_sec, start_time, end_time
             FROM weekly_schedule WHERE user_sid = ?1 ORDER BY day_of_week",
        )?;
        let schedule = stmt
            .query_map(params![sid], Self::map_schedule)?
            .collect();
        schedule
    }

    pub fn get_day_schedule(&self, sid: &str, day_of_week: u32) -> Result<Option<DaySchedule>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT day_of_week, limit_sec, internet_limit_sec, start_time, end_time
             FROM weekly_schedule WHERE user_sid = ?1 AND day_of_week = ?2",
        )?;
        match stmt.query_row(params![sid, day_of_week], Self::map_schedule) {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn update_day_schedule(&self, sid: &str, schedule: &DaySchedule) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE weekly_schedule
             SET limit_sec = ?1, internet_limit_sec = ?2, start_time = ?3, end_time = ?4
             WHERE user_sid = ?5 AND day_of_week = ?6",
            params![
                schedule.limit_sec,
                schedule.internet_limit_sec,
                schedule.start_time,
                schedule.end_time,
                sid,
                schedule.day_of_week
            ],
        )?;
        Ok(())
    }
}