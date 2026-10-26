// src/cloud_client.rs
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::{db::AppDb, AdminCommand};

pub async fn start_cloud_client(_db: Arc<AppDb>, _admin_cmd_tx: mpsc::Sender<AdminCommand>) {
    println!("🚨 STUB: Облачный клиент запущен (ожидание реализации WebSocket)");

    // В будущем здесь будет цикл с tokio_tungstenite::connect_async
    // и переподключением при обрыве связи.
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    }
}
/*
pub async fn start_cloud_client(
    db: Arc<AppDb>,
    admin_cmd_tx: mpsc::Sender<AdminCommand>,
) {
    loop {
        let server_url = match db.get_global_setting("cloud_server_url").ok().flatten() {
            Some(url) if !url.is_empty() => url,
            _ => {
                log_error("❌ [Cloud] URL облака не задан. Ожидание настройки...");
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                continue;
            }
        };

        let api_key = match db.get_global_setting("cloud_api_key").ok().flatten() {
            Some(key) if !key.is_empty() => key,
            _ => {
                log_error("❌ [Cloud] API-ключ не задан");
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                continue;
            }
        };

        log_info(&format!("🌐 [Cloud] Подключение к {}", server_url));

        match connect_and_listen(&server_url, &api_key, &admin_cmd_tx, &db).await {
            Ok(_) => log_info("🌐 [Cloud] Соединение закрыто штатно"),
            Err(e) => log_error(&format!("❌ [Cloud] Ошибка: {}. Переподключение через 10с", e)),
        }

        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}

async fn connect_and_listen(
    url: &str,
    api_key: &str,
    admin_cmd_tx: &mpsc::Sender<AdminCommand>,
    db: &Arc<AppDb>,
) -> Result<(), String> {
    // 🚨 STUB: Здесь будет реальный WebSocket-клиент
    // Использовать крейт tokio-tungstenite

    use tokio_tungstenite::connect_async;
    use futures_util::StreamExt;

    let (ws_stream, _) = connect_async(url).await
        .map_err(|e| format!("Ошибка подключения: {}", e))?;

    let (mut write, mut read) = ws_stream.split();

    // Отправляем авторизацию
    let device_id = db.get_global_setting("device_id").ok().flatten().unwrap_or_default();
    let auth_msg = serde_json::json!({
        "type": "auth",
        "device_id": device_id,
        "api_key": api_key
    });
    // ... отправка auth_msg ...

    // Слушаем команды от облака
    while let Some(msg) = read.next().await {
        match msg {
            Ok(msg) => {
                if let tungstenite::Message::Text(text) = msg {
                    if let Ok(cmd) = parse_cloud_command(&text) {
                        // 🎯 КЛЮЧЕВОЙ МОМЕНТ: кладём в ТОТ ЖЕ канал, что и локальный API
                        let _ = admin_cmd_tx.send(cmd).await;
                    }
                }
            }
            Err(e) => return Err(format!("WS error: {}", e)),
        }
    }

    Ok(())
}

fn parse_cloud_command(json: &str) -> Result<AdminCommand, String> {
    // 🚨 STUB: Парсинг команд от облака
    // Облако шлёт JSON вида: {"type": "lock"} или {"type": "unlock"}
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| format!("JSON parse error: {}", e))?;

    match v["type"].as_str() {
        Some("lock") => Ok(AdminCommand::ForceLock),
        Some("unlock") => Ok(AdminCommand::ForceUnlock),
        Some("sync_users") => Ok(AdminCommand::SyncUsers),
        _ => Err("Unknown command".to_string()),
    }
}

*/
