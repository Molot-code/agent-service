use std::{net::SocketAddr, sync::Arc};

use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;

use crate::{config::Config, db::AppDb, log_debug, log_error, log_info, log_warn};

const DISCOVERY_PORT: u16 = 9090;

#[derive(Deserialize)]
struct DiscoveryRequest {
    r#type: String,
    service: String,
}

#[derive(Serialize)]
struct DiscoveryResponse {
    r#type: String,
    service: String,
    device_id: String,   // hostname Windows
    device_name: String, // 🆕 удобоваримое имя из settings.device_name
    ip: String,
    port: u16,
    version: u8,
}

pub async fn start_discovery_listener(config: Arc<Config>, db: Arc<AppDb>) {
    let socket = match UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)).await {
        Ok(s) => s,
        Err(e) => {
            log_error!(
                "❌ Не удалось запустить UDP обнаружение на порту {}: {}",
                DISCOVERY_PORT,
                e
            );
            return;
        }
    };

    log_info!(
        "👂 [Discovery] Слушаю UDP-запросы на порту {}...",
        DISCOVERY_PORT
    );

    // Имя хоста как уникальный идентификатор устройства
    let device_id = match hostname::get() {
        Ok(h) => h.to_string_lossy().to_string(),
        Err(e) => {
            log_warn!(
                "⚠️ Не удалось получить имя хоста ({}), использую fallback",
                e
            );
            "windows-pc".to_string()
        }
    };

    let api_port = config.service.api_port;
    log_info!(
        "🖥️ [Discovery] Идентификатор устройства: {}, порт API: {}",
        device_id,
        api_port
    );

    let mut buf = [0u8; 2048];
    loop {
        let (len, addr) = match socket.recv_from(&mut buf).await {
            Ok(v) => v,
            Err(e) => {
                log_error!("Ошибка UDP: {}", e);
                continue;
            }
        };

        let msg = String::from_utf8_lossy(&buf[..len]);

        // Совместимость со старым текстовым и новым JSON форматом
        let is_discover = msg.trim() == "AGENT_DISCOVER_REQ"
            || serde_json::from_str::<DiscoveryRequest>(&msg)
                .map(|r| r.r#type == "discover" && r.service == "parental-control")
                .unwrap_or(false);

        if !is_discover {
            log_debug!("[Discovery] Игнорирую пакет от {} (не наш запрос)", addr);
            continue;
        }

        log_info!("📱 [Discovery] Запрос от {}", addr);

        // Определяем локальный IP, который «виден» этому клиенту
        let local_ip = match pick_local_ip_for(addr) {
            Some(ip) => ip,
            None => {
                log_warn!("⚠️ Не удалось определить локальный IP для клиента {}", addr);
                "0.0.0.0".to_string()
            }
        };

        // 🆕 Читаем удобоваримое имя на каждый запрос — свежее значение из БД.
        //    Fallback — hostname (device_id).
        let device_name = db
            .get_setting("device_name")
            .ok()
            .flatten()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| device_id.clone());

        let resp = DiscoveryResponse {
            r#type: "response".into(),
            service: "parental-control".into(),
            device_id: device_id.clone(),
            device_name: device_name.clone(),
            ip: local_ip.clone(),
            port: api_port,
            version: 1,
        };

        match serde_json::to_vec(&resp) {
            Ok(payload) => {
                if let Err(e) = socket.send_to(&payload, addr).await {
                    log_error!("❌ Не удалось отправить ответ на {}: {}", addr, e);
                } else {
                    log_info!(
                        "📤 [Discovery] Ответ отправлен на {} (device_name: {}, мой IP: {}, порт: {})",
                        addr,
                        device_name,
                        local_ip,
                        api_port
                    );
                }
            }
            Err(e) => {
                log_error!("❌ Ошибка сериализации ответа: {}", e);
            }
        }
    }
}

/// Определяет локальный IP того интерфейса, через который виден клиент
fn pick_local_ip_for(client: SocketAddr) -> Option<String> {
    let probe = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    probe.connect((client.ip(), 80)).ok()?;
    Some(probe.local_addr().ok()?.ip().to_string())
}
