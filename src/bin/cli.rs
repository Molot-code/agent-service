// src/bin/cli.rs
use std::{thread, time::Duration};

use agent_windows::{config::Config, db::AppDb, log_debug, log_error, log_warn, native_lock};

fn main() {
    // CLI запускается из службы в сессии пользователя.
    // Инициализируем свой логгер с source="agent_cli".
    let config = Config::new();
    let logs_path = std::path::Path::new(&config.paths.log_dir).join("logs");
    agent_windows::logger::init_global_logger(
        &logs_path.to_string_lossy(),
        "agent_cli",
        agent_windows::logger::LogLevel::Info,
        14,
    );

    let args: Vec<String> = std::env::args().collect();

    if args.len() <= 1 {
        log_warn!(
            "Использование: agent-cli.exe [lock <delay_sec>|unlock|lock-system|check-pin <pin>]"
        );
        return;
    }

    match args[1].as_str() {
        "lock" => {
            let delay_sec: u64 = args
                .get(2)
                .and_then(|s| s.parse::<u64>().ok())
                .map(|v| v.clamp(1, 300))
                .unwrap_or(5);

            log_debug!("🔒 [CLI] Инициируем блокировку (grace: {}с)...", delay_sec);

            // 🆕 Читаем lock_message из БД и передаём в native_lock
            match AppDb::new(&config.paths.data_dir) {
                Ok(db) => {
                    if let Ok(Some(msg)) = db.get_setting("lock_message") {
                        if !msg.is_empty() {
                            native_lock::set_lock_message(msg);
                            log_debug!("[CLI] lock_message загружен из БД");
                        }
                    }
                }
                Err(e) => {
                    log_warn!("[CLI] Не удалось открыть БД для lock_message: {}", e);
                }
            }

            let _ = native_lock::lock_screen();

            let mut attempts = 0;
            while !native_lock::is_locked() && attempts < 50 {
                thread::sleep(Duration::from_millis(100));
                attempts += 1;
            }

            if !native_lock::is_locked() {
                log_error!(
                    "❌ [CLI] КРИТИЧЕСКАЯ ОШИБКА: Кастомная блокировка не активировалась за 5 сек!",
                );
                std::process::exit(1);
            }

            log_debug!("✅ [CLI] Кастомный экран активен.");

            let grace_period = Duration::from_secs(delay_sec);
            let start = std::time::Instant::now();
            let mut user_unlocked_early = false;

            while start.elapsed() < grace_period {
                if !native_lock::is_locked() {
                    user_unlocked_early = true;
                    break;
                }
                thread::sleep(Duration::from_millis(200));
            }

            if user_unlocked_early {
                log_debug!(
                    "🔓 [CLI] Пользователь разблокировал экран в течение grace-периода. Системная блокировка не требуется.",
                );
                return;
            }

            log_debug!(
                "⏰ [CLI] Grace-период истёк. Выполняю системную блокировку (LockWorkStation)...",
            );

            match agent_windows::session::lock_session() {
                Ok(_) => {
                    log_debug!("✅ [CLI] Системная блокировка отправлена. Завершаю CLI.");
                }
                Err(e) => {
                    log_error!(
                        "❌ [CLI] Системная блокировка не удалась: {}. Держу кастомный экран как fallback.",
                        e
                    );
                    while native_lock::is_locked() {
                        thread::sleep(Duration::from_millis(500));
                    }
                    log_debug!("🔓 [CLI] Кастомный экран снят пользователем.");
                }
            }
        }

        "lock-system" => {
            log_debug!("🔒 [CLI] Принудительная системная блокировка (lock-system)...");
            match agent_windows::session::lock_session() {
                Ok(_) => {
                    log_debug!("✅ [CLI] Системная блокировка выполнена.");
                }
                Err(e) => {
                    log_error!("❌ [CLI] Ошибка LockWorkStation: {}", e);
                    std::process::exit(1);
                }
            }
        }

        "unlock" => {
            log_debug!("🔓 [CLI] Принудительная разблокировка...");
            let _ = native_lock::unlock_screen();
        }

        // 🆕 Проверка PIN-кода родителя.
        // Коды выхода:
        //   0 — PIN верный
        //   1 — PIN неверный
        //   2 — ошибка (нет аргумента, БД недоступна, PIN в БД пуст)
        "check-pin" => {
            let Some(input_pin) = args.get(2) else {
                log_warn!("[CLI] check-pin: не передан PIN");
                std::process::exit(2);
            };

            // Открываем БД в data_dir (там же, где data.db)
            let db = match AppDb::new(&config.paths.data_dir) {
                Ok(db) => db,
                Err(e) => {
                    log_error!("[CLI] check-pin: не удалось открыть БД: {}", e);
                    std::process::exit(2);
                }
            };

            let stored_pin = match db.get_setting("admin_pin") {
                Ok(Some(pin)) => pin,
                Ok(None) => {
                    log_warn!("[CLI] check-pin: PIN в БД не найден");
                    std::process::exit(2);
                }
                Err(e) => {
                    log_error!("[CLI] check-pin: ошибка чтения PIN: {}", e);
                    std::process::exit(2);
                }
            };

            if stored_pin.is_empty() {
                log_warn!("[CLI] check-pin: PIN пуст (не задан)");
                std::process::exit(2);
            }

            if input_pin == &stored_pin {
                log_debug!("[CLI] check-pin: PIN верный");
                std::process::exit(0);
            } else {
                log_debug!("[CLI] check-pin: PIN неверный");
                std::process::exit(1);
            }
        }

        _ => {
            log_warn!("Неизвестная команда: {}", args[1]);
        }
    }
}
