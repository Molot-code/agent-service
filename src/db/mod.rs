mod impls;
mod schema;

use std::{path::Path, sync::Mutex};

pub use impls::{ActivityLogEntry, SettingMeta};
use rusqlite::{Connection, Result};

use crate::config::Config;

pub const DEFAULT_DB_DIR: &str = r"C:\ProgramData\ParentalControl";

pub struct AppDb {
    pub(crate) conn: Mutex<Connection>,
}

impl AppDb {
    pub fn new<P: AsRef<Path>>(db_dir: P) -> Result<Self> {
        let db_dir = db_dir.as_ref();
        std::fs::create_dir_all(db_dir).ok();

        let conn = Connection::open(db_dir.join("data.db"))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        schema::apply(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn new_default() -> Result<Self> {
        let dir = Config::load()
            .map(|c| c.paths.data_dir)
            .unwrap_or_else(|_| DEFAULT_DB_DIR.to_string());
        Self::new(dir)
    }
}
