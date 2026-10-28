use rusqlite::{Connection, Result};

pub fn apply(conn: &Connection) -> Result<()> {
    conn.execute_batch(r#"
        CREATE TABLE IF NOT EXISTS device_info (
            device_id   TEXT PRIMARY KEY,
            device_name TEXT NOT NULL,
            created_at  TEXT DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS system_state (
            key        TEXT PRIMARY KEY,
            value      TEXT NOT NULL,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS settings (
            key              TEXT PRIMARY KEY,
            value            TEXT NOT NULL,
            category         TEXT NOT NULL DEFAULT 'general',
            value_type       TEXT NOT NULL DEFAULT 'string',
            label            TEXT NOT NULL DEFAULT '',
            description      TEXT NOT NULL DEFAULT '',
            sort_order       INTEGER NOT NULL DEFAULT 0,
            is_secret        INTEGER NOT NULL DEFAULT 0,
            is_user_editable INTEGER NOT NULL DEFAULT 1
        );

        INSERT OR IGNORE INTO settings
            (key, value, category, value_type, label, description, sort_order, is_secret)
        VALUES
            ('lock_message',    '🔒 Компьютер заблокирован', 'notifications', 'multiline', 'Сообщение на экране блокировки', '', 10, 0),
            ('lock_delay_sec',  '5',   'timing',    'int',      'Задержка блокировки (сек)', 'Сколько секунд показывать окно перед блокировкой', 10, 0),
            ('admin_pin',       '',    'security',  'password', 'PIN-код родителя', '', 10, 1),
            ('device_name',     '',    'general',   'string',   'Имя устройства', 'Как это устройство видят другие в сети', 10, 0),
            ('api_token',       '',    'internal',  'string',   'API токен', 'API токен', 10, 0),
            ('tray_token',      '',    'internal',  'string',   'Tray токен', 'Tray токен', 10, 0),
            ('lock_internet_on_screen_lock', '0', 'control',  'bool', 'Блокировать интернет при блокировке экрана', 'Если включено, при блокировке экрана интернет будет отключаться', 20, 0),
            ('scheduler_enabled',           '1', 'scheduler', 'bool', 'Создавать задачу в планировщике', 'Проверять и запускать службу через Task Scheduler', 10, 0),
            ('scheduler_interval_minutes',  '5', 'scheduler', 'int',  'Интервал проверки (мин)', 'Как часто планировщик проверяет состояние службы', 20, 0),
            ('agent_mode',                  'hybrid', 'general', 'enum', 'Режим работы агента', 'local = только локально, hybrid = локально + облако', 5, 0),
            ('log_level',                   'info',   'logs',  'enum', 'Уровень логирования', 'debug = всё, info = обычные события, warn = предупреждения, error = только ошибки', 10, 0),
            ('log_retention_days',          '14',     'logs',  'int',  'Хранить логи (дней)', 'Логи старше этого срока удаляются автоматически', 20, 0);

        CREATE TABLE IF NOT EXISTS users (
            sid             TEXT PRIMARY KEY,
            username        TEXT NOT NULL,
            is_controlled   INTEGER DEFAULT 1,
            control_enabled INTEGER DEFAULT 1,
            is_active       INTEGER NOT NULL DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS weekly_schedule (
            user_sid            TEXT NOT NULL,
            day_of_week         INTEGER NOT NULL,
            limit_sec           INTEGER DEFAULT 7200,
            internet_limit_sec  INTEGER DEFAULT 7200,
            start_time          TEXT DEFAULT '08:00',
            end_time            TEXT DEFAULT '20:00',
            PRIMARY KEY (user_sid, day_of_week)
        );

        CREATE TABLE IF NOT EXISTS daily_limits (
            user_sid                  TEXT NOT NULL,
            date                      TEXT NOT NULL,
            limit_sec                 INTEGER DEFAULT 7200,
            internet_limit_sec        INTEGER DEFAULT 7200,
            extra_time_min            INTEGER DEFAULT 0,
            extra_internet_time_min   INTEGER DEFAULT 0,
            start_time                TEXT DEFAULT '',
            end_time                  TEXT DEFAULT '',
            mode                      TEXT DEFAULT 'scheduled',
            manual_until              TEXT DEFAULT '',
            internet_paused           INTEGER DEFAULT 0,
            internet_paused_source    TEXT DEFAULT '',
            PRIMARY KEY (user_sid, date)
        );

        CREATE TABLE IF NOT EXISTS daily_usage (
            user_sid          TEXT NOT NULL,
            date              TEXT NOT NULL,
            screen_time_sec   INTEGER DEFAULT 0,
            internet_time_sec INTEGER DEFAULT 0,
            PRIMARY KEY (user_sid, date)
        );

        CREATE TABLE IF NOT EXISTS lock_events (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            user_sid  TEXT NOT NULL,
            locked_at TEXT NOT NULL,
            reason    TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_lock_events_user ON lock_events(user_sid, locked_at);
    "#)?;

    ensure_column(conn, "users", "is_active", "INTEGER NOT NULL DEFAULT 1")
}

/// Идемпотентно добавляет колонку, если её нет (SQLite не умеет ADD COLUMN IF NOT EXISTS).
fn ensure_column(conn: &Connection, table: &str, column: &str, decl: &str) -> Result<()> {
    let has = {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let names: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(Result::ok)
            .collect();
        names.iter().any(|n| n == column)
    };
    if !has {
        conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"))?;
    }
    Ok(())
}