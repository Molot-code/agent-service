// src/logger.rs
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use chrono::{Local, NaiveDate};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn tag(&self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO ",
            LogLevel::Warn => "WARN ",
            LogLevel::Error => "ERROR",
        }
    }

    fn emoji(&self) -> &'static str {
        match self {
            LogLevel::Debug => "🔍",
            LogLevel::Info => "ℹ️",
            LogLevel::Warn => "⚠️",
            LogLevel::Error => "❌",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "debug" => LogLevel::Debug,
            "warn" => LogLevel::Warn,
            "error" => LogLevel::Error,
            "infor" => LogLevel::Info,
            _ => LogLevel::Info,
        }
    }
}

pub struct Logger {
    log_dir: PathBuf, // базовая папка (например, C:\ProgramData\ParentalControl\logs)
    source: String,   // "service" / "agent_cli" / "agent_tray"
    min_level: LogLevel,
    retention_days: u32,
    current_date: Mutex<Option<NaiveDate>>,
}

impl Logger {
    pub fn new(log_dir: impl AsRef<Path>, source: &str) -> Self {
        let dir = log_dir.as_ref().to_path_buf();
        let _ = fs::create_dir_all(&dir);

        let logger = Logger {
            log_dir: dir,
            source: source.to_string(),
            min_level: LogLevel::Info,
            retention_days: 14,
            current_date: Mutex::new(None),
        };

        logger.ensure_today_dir();
        logger.cleanup_old_dirs();
        logger
    }

    pub fn set_min_level(&mut self, level: LogLevel) {
        self.min_level = level;
    }

    pub fn set_retention(&mut self, days: u32) {
        self.retention_days = days;
    }

    /// Папка за сегодня: logs/YYYY-MM-DD/
    fn today_dir(&self) -> PathBuf {
        self.log_dir
            .join(Local::now().format("%Y-%m-%d").to_string())
    }

    fn ensure_today_dir(&self) {
        let today = Local::now().date_naive();
        let mut guard = self.current_date.lock().unwrap();

        if guard.as_ref() != Some(&today) {
            let _ = fs::create_dir_all(self.today_dir());
            *guard = Some(today);
            drop(guard); // чтобы не держать мьютекс во время IO
            self.cleanup_old_dirs(); // 🆕 раз в сутки чистим
        }
    }

    /// Удаляет папки старше retention_days.
    fn cleanup_old_dirs(&self) {
        let cutoff = Local::now().date_naive() - chrono::Duration::days(self.retention_days as i64);

        let Ok(entries) = fs::read_dir(&self.log_dir) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };

            let Ok(date) = NaiveDate::parse_from_str(name, "%Y-%m-%d") else {
                continue;
            };

            if date < cutoff {
                let _ = fs::remove_dir_all(&path);
            }
        }
    }

    pub fn log(&self, level: LogLevel, msg: &str) {
        if level < self.min_level {
            return;
        }

        self.ensure_today_dir();

        let timestamp = Local::now().format("[%Y-%m-%d %H:%M:%S]");
        let tag = level.tag();
        let line = format!("{} [{}] [{}] {}", timestamp, tag, self.source, msg);

        // 1. Консоль — с emoji, для живого отладочного вывода
        let console = format!("{} {} {}", timestamp, level.emoji(), msg);
        if level == LogLevel::Error {
            eprintln!("{}", console);
        } else {
            println!("{}", console);
        }

        // 2. Основной файл бинарника: service.log / agent_cli.log / agent_tray.log
        if level != LogLevel::Debug {
            let main_path = self.today_dir().join(format!("{}.log", self.source));
            Self::append(&main_path, &line);
        }
        // 3. error.log — дубликат всех Error
        if level == LogLevel::Error {
            let err_path = self.today_dir().join("error.log");
            Self::append(&err_path, &line);
        }

        // 4. debug.log — только Debug (чтобы отделить шум)
        if level == LogLevel::Debug {
            let dbg_path = self.today_dir().join("debug.log");
            Self::append(&dbg_path, &line);
        }
    }

    fn append(path: &Path, line: &str) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{}", line);
        }
    }

    pub fn debug(&self, msg: &str) {
        self.log(LogLevel::Debug, msg);
    }
    pub fn info(&self, msg: &str) {
        self.log(LogLevel::Info, msg);
    }
    pub fn warn(&self, msg: &str) {
        self.log(LogLevel::Warn, msg);
    }
    pub fn error(&self, msg: &str) {
        self.log(LogLevel::Error, msg);
    }
}

// ===== Глобальный логгер =====

static GLOBAL_LOGGER: OnceLock<Logger> = OnceLock::new();

pub fn init_global_logger(log_dir: &str, source: &str, min_level: LogLevel, retention_days: u32) {
    let mut logger = Logger::new(log_dir, source);
    logger.set_min_level(min_level);
    logger.set_retention(retention_days);
    let _ = GLOBAL_LOGGER.set(logger);
}

pub fn get_logger() -> &'static Logger {
    GLOBAL_LOGGER.get_or_init(|| Logger::new(r"C:\ProgramData\ParentalControl\logs", "service"))
}

// ===== МАКРОСЫ =====

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logger::get_logger().info(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::logger::get_logger().error(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::logger::get_logger().warn(&format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        $crate::logger::get_logger().debug(&format!($($arg)*))
    };
}
