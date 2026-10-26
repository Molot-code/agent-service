// src/config.rs
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub service: ServiceConfig,
    pub tray: TrayConfig,
    pub paths: PathsConfig,
    pub limits: LimitsConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServiceConfig {
    pub name: String,
    #[serde(default = "default_display_name")]
    pub display_name: String, // 🆕
    pub api_port: u16,
    #[serde(default = "default_tick")]
    pub tick_duration_secs: u64,
    pub log_level: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PathsConfig {
    pub log_dir: String,
    pub static_dir: String,
    #[serde(default)]
    pub agent_cli: Vec<String>,
    #[serde(default)] // 🆕
    pub agent_tray: Vec<String>, // 🆕
}

#[derive(Debug, Clone, Deserialize)]
pub struct LimitsConfig {
    pub default_limit_sec: u32,
    pub default_start_time: String,
    pub default_end_time: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrayConfig {
    #[serde(default = "default_tray_host")]
    pub host: String,
    #[serde(default = "default_tray_interval")]
    pub update_interval_sec: u64,
}

fn default_tray_host() -> String {
    "127.0.0.1".to_string()
}

fn default_tray_interval() -> u64 {
    5
}

fn default_tick_duration() -> u64 {
    1
}
fn default_static_dir() -> String {
    "static".to_string()
}
fn default_display_name() -> String {
    "Parental Control Service".to_string()
}
fn default_tick() -> u64 {
    1
}

impl Config {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Path::new("config.toml");

        if config_path.exists() {
            let content = std::fs::read_to_string(config_path)?;
            let config: Config = toml::from_str(&content)?;
            println!("✅ Конфигурация загружена из config.toml");
            println!("   Служба: {}", config.service.name);
            println!("   Порт API: {}", config.service.api_port);
            println!("   Статика: {}", config.paths.static_dir);
            println!("   log_dir: {}", config.paths.log_dir);
            Ok(config)
        } else {
            println!("⚠️ config.toml не найден, используем значения по умолчанию");
            Ok(Self::default_config())
        }
    }

    fn default_config() -> Self {
        Config {
            service: ServiceConfig {
                name: "ParentalControlSvc".to_string(),
                display_name: "Parental Control Service".to_string(),
                api_port: 8765,
                tick_duration_secs: 1,
                log_level: "info".to_string(),
            },
            tray: TrayConfig {
                host: "127.0.0.1".to_string(),
                update_interval_sec: 5,
            },
            paths: PathsConfig {
                log_dir: r"C:\ProgramData\ParentalControl".to_string(),
                static_dir: "static".to_string(),
                agent_cli: vec![
                    r"C:\Program Files\ParentalControl\agent-cli.exe".to_string(),
                    r"C:\ProgramData\ParentalControl\agent-cli.exe".to_string(),
                ],
                agent_tray: vec![
                    r"C:\Program Files\ParentalControl\agent-tray.exe".to_string(),
                    r"C:\ProgramData\ParentalControl\agent-tray.exe".to_string(),
                ],
            },
            limits: LimitsConfig {
                default_limit_sec: 7200,
                default_start_time: "08:00".to_string(),
                default_end_time: "20:00".to_string(),
            },
        }
    }

    /// Возвращает первый существующий путь из списка кандидатов.
    fn resolve_binary(candidates: &[String]) -> Option<PathBuf> {
        candidates.iter().map(PathBuf::from).find(|p| p.exists())
    }

    /// Путь к agent-cli.exe (низкоуровневый блокировщик).
    pub fn get_cli_path(&self) -> Option<PathBuf> {
        Self::resolve_binary(&self.paths.agent_cli)
    }

    /// Путь к agent-tray.exe (иконка в трее).
    pub fn get_tray_path(&self) -> Option<PathBuf> {
        Self::resolve_binary(&self.paths.agent_tray)
    }

    /// Для обратной совместимости (если где-то используется new())
    pub fn new() -> Self {
        Self::load().unwrap_or_else(|e| {
            eprintln!("⚠️ Ошибка загрузки конфига: {}, используем дефолтный", e);
            Self::default_config()
        })
    }
}
