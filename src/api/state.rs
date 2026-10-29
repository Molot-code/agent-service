use std::{path::Path, sync::Arc};

use tokio::sync::mpsc;

use crate::{config::Config, db::AppDb, log_error, log_info, AdminCommand};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<AppDb>,
    pub api_token: String,
    pub admin_cmd_tx: mpsc::Sender<AdminCommand>,
    pub env: Arc<minijinja::Environment<'static>>,
    pub config: Arc<Config>,
}

/// Читает/создаёт api_token в БД и возвращает его. `None` — при ошибке записи.
pub fn init_api_token(db: &AppDb) -> Option<String> {
    match db.get_state("api_token") {
        Ok(Some(t)) if !t.is_empty() => Some(t),
        Ok(_) => {
            let token = format!(
                "agent_{}",
                uuid::Uuid::new_v4()
                    .simple()
                    .to_string()
                    .chars()
                    .take(8)
                    .collect::<String>()
            );
            match db.set_setting("api_token", &token) {
                Ok(_) => {
                    log_info!("🔑 [API] Сгенерирован новый api_token");
                    Some(token)
                }
                Err(e) => {
                    log_error!("❌ [API] Не удалось записать api_token: {}", e);
                    None
                }
            }
        }
        Err(e) => {
            log_error!("❌ [API] Ошибка чтения api_token: {}", e);
            None
        }
    }
}

/// Читает/создаёт tray_token, пишет его в `data_dir/tray_token.txt`.
pub fn init_tray_token(db: &AppDb, data_dir: &str) -> Option<String> {
    let token = match db.get_setting("tray_token") {
        Ok(Some(t)) if !t.is_empty() => t,
        _ => {
            let token = format!("tray_{}", uuid::Uuid::new_v4().simple());
            if let Err(e) = db.set_setting("tray_token", &token) {
                log_error!("[device] не удалось записать tray_token: {:?}", e);
                return None;
            }
            log_info!("🔑 [device] сгенерирован tray_token");
            token
        }
    };

    let path = Path::new(data_dir).join("tray_token.txt");
    if let Err(e) = std::fs::write(&path, &token) {
        log_error!("[device] не удалось записать tray_token.txt: {:?}", e);
    }
    Some(token)
}
