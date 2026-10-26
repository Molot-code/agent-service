# Parental Control Agent (`agent-windows` v0.1.0)

Программа родительского контроля под Windows на Rust. Состоит из трёх бинарников:
служба под Windows (БД + HTTP API + HTML-панель + главный цикл),
запускаемый ею **внутри сессии пользователя** блокировщик экрана,
и иконка в трее. Целевая платформа — Windows x64.

---

## 1. Архитектура — три бинарника

| Бинарник | Исходник | Контекст запуска | Что делает |
|---|---|---|---|
| `ParentalControlSvc.exe` | `src/bin/service.rs` | Windows Service (`LocalSystem`-эквивалент, SCM-registered, start=`auto`) | SQLite БД, HTTP API на axum, HTML-панель, основной цикл контроля. Регистрация имени: `"ParentalControlSvc"` (см. `config.toml [service].name` и `service.rs::install_service`). |
| `agent-cli.exe` | `src/bin/cli.rs` | **Спавнится службой через `CreateProcessAsUserW` в сессии активного пользователя** (`session::execute_in_user_session`) | Рисует собственный fullscreen-блокирующий экран (`native_lock`), после grace-периода вызывает системный `LockWorkStation`. Также команды `unlock` и `check-pin <pin>`. |
| `agent-tray.exe` | `src/bin/tray.rs` | Спавнится службой в сессии пользователя после старта API | Иконка в System Tray; опрашивает `/api/status` каждые `tray.update_interval_sec` (по умолчанию 5 сек) с токеном; попап с прогресс-индикаторами лимитов; кнопка «пауза интернета». |

Все три бинарника делят общий крейт `agent-windows` (см. `src/lib.rs`). У каждого свой глобальный логгер (`logger::init_global_logger`) с `source`: `"service"`, `"agent_cli"`, `"agent_tray"`.

### Потоки спавна

```
[Service (SYSTEM)] -- execute_in_user_session --> [agent-cli.exe в user session]
                  -- execute_in_user_session --> [agent-tray.exe в user session]
[agent-tray.exe]  -- HTTP GET /api/status -------> [Service :8765]
[agent-tray.exe]  -- HTTP POST /api/internet/pause|resume --> [Service]
[Мобильное/админка] -- HTTP POST /api/* -------> [Service]
                  -- UDP 9090 (discovery) -----> [Service]
```

`execute_in_user_session`: `WTSGetActiveConsoleSessionId` → `WTSQueryUserToken` → `CreateEnvironmentBlock` → `CreateProcessAsUserW`. Ждёт процесс максимум 5500 мс (`WAIT_TIMEOUT` трактуется как «процесс живой — успех»); ошибка / ранний выход с ненулевым кодом → `Err`.

---

## 2. Режимы работы

Два независимых слоя:

### 2.1 `AgentMode` — откуда приходят команды

Файл `src/mode.rs`. Хранится в `settings.agent_mode` (по умолчанию `"hybrid"`).

| Значение | `needs_local_api()` | `needs_cloud_client()` | Фактический статус |
|---|---|---|---|
| `local` | ✅ | ❌ | **работает** |
| `cloud` | ❌ | ✅ | ❌ не реализовано (`cloud_client.rs` — заглушка) |
| `hybrid` | ✅ | ✅ | частично: API есть, cloud_client — стаб |

`service.rs` на старте читает режим через `get_current_mode(&db)` и запускает соответствующие таски. Сейчас фактически работает только локальный API; облачный канал закомментирован в `cloud_client.rs` (там же — заготовка под `tokio_tungstenite` WebSocket + `parse_cloud_command("lock"|"unlock"|"sync_users")`).

### 2.2 `ControlMode` — как блокировать для конкретного пользователя прямо сейчас

Файл `src/types.rs::ControlMode`. Единственный источник истины для `service.rs`.

| Вариант | Поведение `should_lock()` |
|---|---|
| `Disabled` | `false` — счётчики не тикают, блокировки нет. |
| `Manual { until, action: Lock }` | `true` — держать блокировку до `until`. |
| `Manual { until, action: Unlock }` | `false` — держать разблокировку до `until`. Лимиты/расписание игнорируются. |
| `Scheduled` | `!in_schedule || is_exceeded` — обычная логика по дневному расписанию и лимиту. |

`manual_until` хранится в `daily_limits.manual_until` (`"YYYY-MM-DD HH:MM:SS"` или `""`). `until = "-1"` в API-вызове трактуется как «до конца сегодняшних суток». Когда `manual_until` истёк, `evaluate_access` автоматически возвращает `Scheduled` (см. `policy::check_screen_time_policy`).

---

## 3. Хранилище (SQLite)

Файл: `<log_dir>/data.db` (`<log_dir>` из `config.toml`, по умолчанию `C:\ProgramData\ParentalControl\`).
Драйвер — `rusqlite`, `journal_mode=WAL`, `foreign_keys=ON`.

Схема создаётся в `db::AppDb::new` через `CREATE TABLE IF NOT EXISTS`.

### Таблицы

| Таблица | Колонки | Назначение |
|---|---|---|
| `device_info` | `device_id PK, device_name, created_at` | Идентификация устройства (на будущее, сейчас почти не используется). |
| `system_state` | `key PK, value, updated_at` | Внутренние KV-флаги: `lock_status`, `session_locked`, `internet_blocked`, `device_uuid`, `control_mode` (глобальный), `agent_mode` (дубликат settings). Пишутся и читаются только кодом. |
| `settings` | `key PK, value, category, value_type, label, description, sort_order, is_secret, is_user_editable` | Редактируемые родителем параметры. API `category='internal'` отбрасывает. `is_secret=true` выдаётся как пустая строка. |
| `users` | `sid PK, username, is_controlled DEFAULT 1, control_enabled DEFAULT 1` | Локальные пользователи Windows. `is_controlled` не используется, рабочий флаг — `control_enabled`. |
| `weekly_schedule` | `user_sid, day_of_week (0=Вс..6=Сб), limit_sec, internet_limit_sec, start_time 'HH:MM', end_time 'HH:MM'` (PK `(user_sid, day_of_week)`) | Шаблон на неделю. |
| `daily_limits` | `user_sid, date 'YYYY-MM-DD', limit_sec, internet_limit_sec, extra_time_min, extra_internet_time_min, start_time, end_time, mode ('scheduled'\|'manual_lock'\|'manual_unlock'), manual_until, internet_paused 0\|1, internet_paused_source ''\|'manual'\|'limit'\|'screen_lock'` (PK `(user_sid, date)`) | Рабочие лимиты на сегодняшние сутки. |
| `daily_usage` | `user_sid, date, screen_time_sec, internet_time_sec` (PK `(user_sid, date)`) | Счётчики потребления. |
| `lock_events` | `id PK AUTOINCREMENT, user_sid, locked_at, reason` | Журнал блокировок. |

Индексы: `idx_daily_usage_user_date`, `idx_daily_limits_user_date`.

### Ключевые методы `AppDb` (все `pub fn`, возвращают `rusqlite::Result`)

- Настройки: `get_setting`, `set_setting`, `get_settings`, `get_setting_meta`.
- Состояния: `get_state`, `set_state`.
- Пользователи: `get_all_users`, `get_or_create_user` (также создаёт 7 строк `weekly_schedule` с дефолтом), `get_username_by_sid`, `is_user_control_enabled`, `set_user_control_enabled`, `get_users_with_control_status`, `cleanup_stale_users`.
- Расписание на неделю: `get_week_schedule`, `get_day_schedule`, `update_day_schedule`.
- Лимиты на сегодня: `ensure_today_limit`, `set_today_limit`, `add_extra_time`, `add_extra_internet_time`, `set_today_mode`, `get_today_mode`, `return_to_scheduled`, `restore_limit_from_schedule`.
- Счётчики: `add_screen_time`, `add_internet_time`, `get_today_usage`.
- Прочее: `log_lock_event`, `set_internet_paused`, `is_internet_paused`, `get_internet_paused_source`.

### `ensure_today_limit(sid)`

Читает `daily_limits` для `today` и `sid`. Если override существует — возвращает его.
Иначе читает `weekly_schedule[sid, today_dow]` и вставляет строку в `daily_limits` с `mode='scheduled'`, `manual_until=''`. Если и в расписании пусто — берёт глобальные `default_limit_sec=7200`, `default_start_time='08:00'`, `default_end_time='20:00'` (из `config.toml [limits]`).

`today_dow` вычисляется как `Local::now().weekday().num_days_from_sunday()` (Вс=0, Пн=1...Сб=6).

---

## 4. Главный цикл службы

Точка входа: `service.rs::main`.

### Запуск процесса

1. `set_hook` для паник → пишет в `panic.log` за текущую дату.
2. `env::set_current_dir(<папка exe>)` — фиксирует CWD, чтобы `config.toml`, `templates/`, `static/` грузились корректно в «как service»-окружении.
3. Аргументы:
   - `install` → `install_service`: создаёт SCM-запись `OWN_PROCESS`, `AutoStart`, `Normal`.
   - `uninstall` → удаляет SCM-запись.
   - иначе → `service_dispatcher::start("ParentalControlSvc", ffi_service_main)` (запускает тело под SCM).

### `run_service` (последовательность)

1. `Config::load` (читает `config.toml` рядом с exe).
2. `init_global_logger(<log_dir>\logs, "service", LogLevel::from_str(level), 14)`.
3. `tokio::sync::mpsc::channel::<SessionEvent>(100)` — события от SCM-хэндлера.
4. Регистрация `service_control_handler::register`:
   - `ServiceControl::SessionChange` → `Lock/Unlock` события по `change.reason` (SessionLock/Unlock, RemoteConnect/Disconnect, ConsoleConnect/Disconnect).
   - `ServiceControl::Stop | Shutdown` → `SessionEvent::Stop`.
   `ServiceStatus`: `Running`, accepts `STOP | SHUTDOWN | SESSION_CHANGE`.
5. `AppDb::new(<log_dir>)`; если `system_state.device_uuid` пуст — сгенерировать `Uuid::new_v4()`.
6. `users::initialize_users(&db)` — `NetUserEnum` + `LookupAccountNameW` для всех нормальных локальных аккаунтов; для каждого создаётся пользователь и 7 строк `weekly_schedule` с дефолтами.
7. Каналы: `mpsc::channel::<AdminCommand>(10)` и `watch::channel(SessionState::Active)`.
8. `sync_active_session_state(&db, &session_state_tx)` — на старте сверяет реальное состояние через WTS (`check_session_state_via_wts`) и при расхождении правит `system_state.session_locked`.
9. `scheduler::sync_watchdog_task(settings.scheduler_enabled, settings.scheduler_interval_minutes, name)` — создаёт задачу Windows Task Scheduler `ParentalControl_Watchdog`, если выключено — удаляет.
10. `reconcile_lock_state_on_startup(&db)`:
    - `cleanup_stale_agents()` — `taskkill /F /IM agent-cli.exe /T` и то же для tray.
    - Если `lock_status=true` (а процесса нет) → `set_state("lock_status","false")`.
    - Если `internet_paused` и `source="screen_lock"` → снять блок интернета и обнулить флаги.
    - `apply_lock_state(...)` для активного пользователя.

### `tokio::runtime` (в `std::thread::spawn`)

Запускаются параллельно:

- `start_api_server` (axum, `0.0.0.0:<api_port>`, по умолчанию 8765) — если `needs_local_api`.
- `start_discovery_listener` (UDP 9090) — если `needs_local_api`.
- `start_cloud_client` (стаб) — если `needs_cloud_client`.
- С задержкой после старта API: опрашивает `GET /health` до 15 сек, затем `spawn_tray()` → `execute_in_user_session(agent-tray.exe, "")`.

### Главный цикл (tokio::select!)

```
loop {
  tokio::select! {
    cmd = admin_cmd_rx.recv()             => handle_admin_command(...),
    _    = session_state_rx.changed()     => handle_session_state_change(...),
    _    = tick_interval.tick()           => handle_background_tick(...),
    ev   = session_event_rx.recv()        => match SessionEvent::Lock/Unlock/Stop
  }
}
```

`tick_interval` — `Duration::from_secs(config.service.tick_duration_secs)` (по умолчанию 1 сек). `tick_counter` инкрементируется каждый тик; раз в 60 тиков вызывается `process_screen_time_procedure`.

### Обработчики

**`handle_admin_command`**
- `ForceLock` → `enforce_lock_procedure(sid, "admin_force")`.
- `ForceUnlock` → `do_unlock_procedure`.
- `AutoMode` → `set_state("control_mode", "scheduled")` + `db.return_to_scheduled(sid)` + `apply_lock_state`.
- `SyncUsers` → `users::sync_users` (заглушка, тело пустое).

**`handle_session_state_change`** — реакция на смену `SessionState`:
- `Locked|Suspended|Logoff` — лог «счётчик остановлен».
- `Active` — `do_sync_user_access` + `spawn_tray` (защита от дубля трея).

**`handle_background_tick`** — основная логика:
1. Если нет активного SID — выход.
2. `evaluate_access(db, Some(sid))` → `AccessState.policy = PolicyCheckResult`.
3. Если `policy.mode == Scheduled && !is_manual_expired(...)` оказался просроченным → `return_to_scheduled(sid)`.
4. `apply_lock_state(db, sid, policy)` → применяется или снимается реальный лок и блокировка интернета; возвращает `screen_locked`.
5. `should_credit = (mode != Disabled) && is_session_active(rx) && !screen_locked`. Если `true && tick_counter % 60 == 0` → `process_screen_time_procedure`.

**`process_screen_time_procedure`**
- `add_screen_time(sid, 60)` — всегда при кредите.
- Если `is_internet_paused` — стоп.
- Если `internet_time >= effective_internet_limit` — стоп.
- `add_internet_time(sid, 60)`.
- Warning: если `settings.warning_minutes > 0 && time_to_limit ≤ warning_minutes` — лог.

**`apply_lock_state`** (одна точка синхронизации политики ↔ реальности; идемпотентна):

1. **Экран.** `want_lock = should_lock(policy)`. Если `want_lock && !is_locked` → `enforce_lock_procedure(sid,"policy")`; если `!want_lock && is_locked` → `set_state("lock_status","false")` + `do_unlock_procedure`.
2. **Интернет по лимиту.** Если `policy.internet_exceeded && !paused` → `firewall::ensure_block_rule + set_block_enabled(true)`, `set_internet_paused(sid, true, "limit")`. Если `!exceeded && paused && source=="limit"` → снять.
3. **Интернет при блокировке экрана.** Если `settings.lock_internet_on_screen_lock=true && screen_locked && !paused` → `set_internet_paused(sid, true, "screen_lock")`. Если `!screen_locked && paused && source=="screen_lock"` → снять.

**`enforce_lock_procedure(sid, reason)`**
- `taskkill /F /IM agent-cli.exe /T`.
- `Config::get_cli_path()` (первый существующий из `paths.agent_cli`).
- `execute_in_user_session(<cli>, "lock <lock_delay_sec>")` (delay из `settings.lock_delay_sec`, по умолчанию 5 сек; ограничен `[1,300]`).
- `set_state("lock_status","true")`, `log_lock_event(sid, reason)`.

**`do_unlock_procedure`**
- `set_state("lock_status","false")` (если процесс CLI ещё жив — `unlock_screen()` через `execute_in_user_session`).

### События SCM

`SessionEvent::Lock(sid)` → `set_state("session_locked","true")` + `session_state_tx.send(Locked)`.
`SessionEvent::Unlock(sid)` → `set_state("session_locked","false")` + `session_state_tx.send(Active)` + `evaluate_access` + `apply_lock_state` + `spawn_tray()`.

---

## 5. Модули (по файлу `src/*.rs`)

| Файл | Назначение | Ключевые публичные функции |
|---|---|---|
| `lib.rs` | Реэкспорт всех модулей; тип `AdminCommand`, `SessionState` подняты наверх. | — |
| `types.rs` | Публичные типы. | `enum SessionState`, `enum TimeToEvent` (с `reason`), `enum AdminCommand`, `enum ControlMode { Disabled, Manual{until,action}, Scheduled }`, `enum ManualAction { Lock, Unlock }`, `struct TodayLimit`, `struct DaySchedule`, `struct SettingItem`. |
| `config.rs` | Чтение `config.toml`. | `Config::load() -> Result<Self,_>`, `Config::new()`, `get_cli_path() -> Option<PathBuf>`, `get_tray_path() -> Option<PathBuf>`. |
| `db.rs` | SQLite (`AppDb`). | См. раздел 3. |
| `policy.rs` | Политика доступа и `time_to_event`. | `evaluate_access(db, sid_opt) -> Option<AccessState>`, `check_screen_time_policy(...)`, `is_access_allowed(db, sid_opt) -> bool`, `should_lock(policy) -> bool`, `do_sync_user_access(db, tx, sid) -> Result<(), ()>`, `calculate_time_to_event(...)` (приватная). |
| `api.rs` | axum-роутер на `api_port`. | `start_api_server(db, config, admin_cmd_tx)`, `AppState`, `check_auth`. |
| `firewall.rs` | Правило `ParentalControl_BlockUser_<SID>`. | `ensure_block_rule`, `set_block_enabled`, `remove_block_rule`. |
| `native_lock.rs` | Свой fullscreen-блокировщик (Windows only, есть `cfg(not(windows))` заглушки). | `lock_screen() -> Result<(),String>`, `unlock_screen() -> Result<(),String>`, `is_locked() -> bool`, `set_lock_message(String)`. |
| `session.rs` | Старт процессов в user session + системный лок. | `execute_in_user_session(exe, args) -> Result<(),String>`, `lock_session() -> Result<(),String>` (`LockWorkStation`). |
| `users.rs` | Windows users + SID. | `get_local_users_with_sids() -> Vec<(username,sid)>`, `initialize_users(&AppDb)`, `sync_users`, `get_active_user_sid() -> Option<String>`. |
| `discovery.rs` | UDP/9090 для будущего мобильного клиента. | `start_discovery_listener(config, db)`. |
| `cloud_client.rs` | Заглушка. Запускается только в режиме `cloud|hybrid`, сейчас ничего не делает (бесконечный sleep). | `start_cloud_client(db, tx)` (стаб). |
| `scheduler.rs` | Задача `ParentalControl_Watchdog` в Task Scheduler. | `ensure_watchdog_task(interval_min, service_name) -> Result<(),String>`, `remove_watchdog_task`, `sync_watchdog_task(enabled, interval, service_name)`. |
| `logger.rs` | Файловый логгер с ротацией по дням. | `init_global_logger(dir, source, level, retention_days)`, макросы `log_info! / log_warn! / log_error! / log_debug!`. |
| `error.rs` | `AppError` + `IntoResponse` для axum. | — |
| `tray_ui.rs` | Popup-окно трея (`class=AgentTrayPopup`). | `show(data, on_toggle, on_close)`, `hide()`, `is_visible() -> bool`, `update_data(data)`, `struct PopupData`. |
| `bin/cli.rs` | Подкоманды: `lock <sec>`, `lock-system`, `unlock`, `check-pin <pin>`. | — |
| `bin/tray.rs` | Tao event loop + tray-icon. Меню: «Показать», «Пауза интернета», «Выход». Левый клик — toggle popup. | `main()`. Читает `tray_token.txt`. |
| `bin/service.rs` | Точка входа SCM-службы. install/uninstall argv, главный цикл. | `main`, `run_service`, `handle_admin_command`, `handle_session_state_change`, `handle_background_tick`, `process_screen_time_procedure`, `enforce_lock_procedure`, `do_unlock_procedure`, `apply_lock_state`, `reconcile_lock_state_on_startup`, `sync_active_session_state`, `cleanup_stale_agents`, `check_session_state_via_wts`, `spawn_tray`, `install_service`, `uninstall_service`. |

---

## 6. HTTP API

Сервер: axum 0.8 на `0.0.0.0:<service.api_port>` (по умолчанию 8765). Шаблонизатор — `minijinja` (`templates/` загружается рядом с exe). Статика (`tailwind.css`, `app.min.js`, иконки) раздаётся через `tower_http::services::ServeDir` как fallback.

### Авторизация

Заголовок `Authorization: Bearer <token>`. Уровни:

| Токен (из `settings`) | Уровень | Доступ |
|---|---|---|
| `api_token` | Admin | Все CRUD-эндпоинты. |
| `admin_pin` | Admin | Если не пуст — эквивалент api_token. Используется веб-панелью. |
| `tray_token` | Tray | Только `/api/status` и `/api/internet/*` (фактически — что вызывает трей). |

`admin_pin`, `api_token`, `tray_token` хранятся в `settings`. `tray_token` дополнительно дублируется в файл `<log_dir>\tray_token.txt` для чтения треем.

### Эндпоинты

| Метод | Путь | Auth | Тело / Query | Возвращает | Что делает |
|---|---|---|---|---|---|
| GET | `/` | — | — | HTML | `templates/index.html` через minijinja. |
| GET | `/health` | — | — | `{status:"ok", service}` | Лёгкий readiness-чек (используется перед стартом трея). |
| GET | `/api/status` | Admin\|Tray | — | `StatusResponse` | Полный «сейчас»: см. §6.1. |
| POST | `/api/set_param` | Admin | `{key,value}` | `{success,message}` | `settings.set_setting`. Для `scheduler_enabled`/`scheduler_interval_minutes` пересоздаёт Task Scheduler-задачу. Для `agent_mode` валидирует ∈ {`local,hybrid,cloud`}. |
| GET | `/api/users` | Admin | — | `[{sid,username,is_control_enabled}]` | Список всех локальных пользователей. |
| GET | `/api/schedule/{sid}` | Admin | — | `{sid, schedule:[DaySchedule]}` | `weekly_schedule` для пользователя. |
| POST | `/api/schedule/{sid}` | Admin | `{schedule:[DaySchedule]}` | `{success,message}` | `update_day_schedule` × N, затем `do_sync_user_access`. |
| GET | `/api/today/{sid}` | Admin | — | `TodayLimitResponse` | `ensure_today_limit` + `get_today_usage`. |
| POST | `/api/today/{sid}` | Admin | `{limit_sec, internet_limit_sec, extra_time_min, extra_internet_time_min, start_time, end_time}` | `{success,message}` | `set_today_limit` (mode сохраняется из текущего), затем sync. |
| POST | `/api/manual_action` | Admin | `{action:"lock"\|"unlock"\|"auto", until:"YYYY-MM-DD HH:MM:SS"\|"-1"}` | `{success,message}` | При `auto`: `set_today_mode(sid,"scheduled","")` + `AdminCommand::AutoMode`. При `lock/unlock`: `set_today_mode(sid,"manual_lock"\|"manual_unlock", until)` + отправляет `AdminCommand::ForceLock/Unlock`. |
| POST | `/api/extra_time` | Admin | `{sid, screen_minutes?, internet_minutes?}` | `{success, sid, screen_minutes, internet_minutes}` | `add_extra_time` / `add_extra_internet_time` + sync. |
| GET | `/api/settings` | Admin | — | `Vec<SettingItem>` (секреты очищены) | Все настройки кроме `category='internal'`. |
| POST | `/api/check_pin` | — | `{old_pin?, new_pin}` | `{success,message}` | Сравнивает `new_pin` с `settings.admin_pin`. |
| POST | `/api/set_pin` | — | `{old_pin?, new_pin}` | `{success,message}` | Если уже задан — требует `old_pin`. |
| GET | `/api/is_pin_set` | — | — | `{is_set:bool}` | Админка решает — показать экран входа или экран установки PIN. |
| POST | `/api/users/toggle_control` | Admin | `{sid, enabled}` | `{success:true}` | Включает/выключает `control_enabled`. Если выключили активного — `set_state("lock_status","false")` + sync. |
| POST | `/api/internet/pause` | Admin | — | `{success,message}` | `firewall::ensure_block_rule + set_block_enabled(true)`; `set_internet_paused(sid,true,"manual")`. |
| POST | `/api/internet/resume` | Admin | — | `{success,message}` | Обратная операция. |
| GET | `/api/internet/status` | Admin | — | `{paused, blocked, source, ...}` | Из БД. |
| GET | `/api/logs` | Admin | — | `[{date:"YYYY-MM-DD"}]` | Список подпапок `<log_dir>/logs`. |
| GET | `/api/logs/{date}` | Admin | `?source=service&level=info&limit=500` | `{date,source,level,count,lines:[...]}` | Фильтр по `[TAG]` и реверс порядка. |
| GET | `/switch-server` | — | — | 302 → `/` | Переключение на «другой» инстанс (роутинг в админке). |

#### 6.1 `GET /api/status` → `StatusResponse`

```text
{
  screen_time_sec, daily_limit_sec,           // реальный экран сегодня
  is_locked, session_locked,                 // реальный лок
  lock_message,                              // из settings.lock_message
  unlock_until,                              // ISO string | null (только в Manual)
  today_enabled, current_user,
  mode: "control_disabled" | "manual_lock" | "manual_unlock" | "scheduled" | "no_user",
  time_to_event: { kind: "lock_in"|"unlock_in", minutes, reason:"schedule"|"manual"|"limit"|"" },
  internet_paused, internet_blocked,         // paused = флаг паузы на сегодня; blocked = актуальное состояние правила firewall
  internet_time_sec, internet_limit_sec,
  device_name                                // из settings.device_name || COMPUTERNAME
}
```

---

## 7. Блокировка экрана

### Поток

1. `service::apply_lock_state` решает `want_lock = true` → вызывает `enforce_lock_procedure(sid, "policy"|"admin_force")`.
2. `enforce_lock_procedure` подбирает путь к CLI (`Config::get_cli_path()`), убивает зависшие процессы, спавнит **`agent-cli.exe lock <delay_sec>`** через `execute_in_user_session`.
3. CLI ловит аргумент:
   - `native_lock::lock_screen()` поднимает fullscreen topmost окно класса `CustomLockScreenClass`, `BlockInput(true)`, регистрирует глобальный hotkey `Ctrl+Shift+Alt+U` (id `0x4242`).
   - Цикл `GetMessageW / DispatchMessageW`. `WM_PAINT` рисует `LOCK_MESSAGE` (по умолчанию `"🔒 КОМПЬЮТЕР ЗАБЛОКИРОВАН …"`).
   - Ждёт `is_locked() == true` (до 5 сек).
   - `grace_period = delay_sec`: если за это время кто-то нажал `Ctrl+Shift+Alt+U` → `WM_HOTKEY` → `PostQuitMessage` → `BlockInput(false)` → `unlock_screen` → CLI выходит без системного лока.
   - Иначе → `session::lock_session()` (`LockWorkStation`).
   - Если `LockWorkStation` упал — CLI держит кастомный экран, пока ребёнок сам его не снимет.
4. После успеха сервис пишет `set_state("lock_status","true")` и `log_lock_event`.

### `unlock_screen` (для системного сценария «разблокировали Win+L»)

- `FindWindowW("CustomLockScreenClass")` → `PostMessageW(WM_CLOSE)` → `is_locked = false`.
- Если окна нет — `taskkill /F /IM agent-cli.exe` (фолбэк).

### `unlock_screen` из `do_unlock_procedure` (отмена политики)

- `execute_in_user_session(<cli>, "unlock")` → CLI вызывает `native_lock::unlock_screen()` → путь аналогичный.

---

## 8. Блокировка интернета

Реализована через правило **исходящего** трафика Windows Firewall, персонально для каждого SID:

- Имя правила: `ParentalControl_BlockUser_<SID>`.
- `NET_FW_RULE_DIR_OUT`, `NET_FW_ACTION_BLOCK`, `protocol = ANY`, `profiles = 0x7` (Domain + Private + Public).
- `INetFwRule3::SetLocalUserAuthorizedList("O:LSD:(A;;CC;;;<sid>)")` — действие правила применяется только к указанному пользователю (Windows Firewall per-user rule).
- Создаётся выключенным (`ensure_block_rule`). Включается/выключается через `set_block_enabled(sid, bool)`.

### Источники паузы (`daily_limits.internet_paused_source`)

| Значение | Кто ставит | Снимается автоматически? |
|---|---|---|
| `""` | — | — |
| `"manual"` | `/api/internet/pause` или трей | Нет — родитель снимает сам через `/api/internet/resume`. |
| `"limit"` | `apply_lock_state` при `policy.internet_exceeded` | Да — когда лимит в норме. |
| `"screen_lock"` | `apply_lock_state` при `screen_locked && settings.lock_internet_on_screen_lock` | Да — когда экран разблокирован. |

В логике `apply_lock_state` снятие блокировки интернета выполняется только для «своего» источника; чужие флаги не трогает.

---

## 9. Потоки данных (high-level)

```
           ┌── HTTP /api/*  (admin_pin / api_token, Bearer) ──┐
UI / Mobile┤                                                  ├──► service.rs
           └── HTTP /api/status (tray_token, Bearer) ────────┤
Tray ──────┘                                                  │
                                                              ▼
   AppDb (SQLite) ──────────────────────────────────┬──► policy::evaluate_access
   settings / system_state                          │        │
   daily_limits / daily_usage                       │        └──► PolicyCheckResult
   weekly_schedule / users                          │                  │
                                                    │                  ▼
                                            policy::should_lock(policy)
                                                              │
                                                              ▼
                                       service::apply_lock_state
                                                        │
                                            ┌───────────┴───────────┐
                                            ▼                       ▼
                          execute_in_user_session            firewall::set_block_enabled
                                ("agent-cli.exe", ...)               (Win32 INetFwPolicy2)
                                            │
                                            ▼
                          agent-cli.exe lock <sec>  ──► native_lock::lock_screen
                                                    └─► session::lock_session (LockWorkStation)
```

Альтернативные источники команд:

- `AdminCommand`-канал (от API, от reconcile-on-startup, от do_sync_user_access).
- Watch-канал `SessionState` (SCM SessionChange + ручные переключения `apply_lock_state` → `is_locked=true → send Locked`).

---

## 10. Сборка и установка

### Сборка (Taskfile.yml)

| Таск | Команда | Выход |
|---|---|---|
| `task styles` | `npx tailwindcss -i ./src/input.css -o ./static/tailwind.css --minify` | `static/tailwind.css` |
| `task js` | `npx esbuild ./static/app.js --minify --outfile=./static/app.min.js` | `static/app.min.js` |
| `task backend` | `cargo build --release` | `target/release/ParentalControlSvc.exe`, `agent-cli.exe`, `agent-tray.exe` |
| `task installer` | `task styles; task js; task backend` + `"C:\Program Files\Inno Setup 7\ISCC.exe" installer\setup.iss` | `installer\output\ParentalControlSetup.exe` |
| `task` (default) | = installer | |

### Установка (Inno Setup, `installer/setup.iss`)

- `${autopf}\ParentalControl\` — бинарь, `config.toml`, `static\`, `templates\`.
- `C:\ProgramData\ParentalControl\` создаётся с правами `users-full` (для БД и токен-файла).
- Firewall-правила (через `netsh`):
  - `Parental Control API` (TCP in 8765).
  - `Parental Control Discovery UDP` (UDP in 9090).
- Регистрация службы: запускает `ParentalControlSvc.exe install`; затем `sc config ... start=auto`; `sc start`; открывает админку `http://localhost:8765/`.

### Удаление

Inno Setup выполняет функцию `InitializeUninstall` с модальным диалогом ввода PIN. До 3 попыток; на каждой запускает `agent-cli.exe check-pin <введённый_PIN>`, интерпретируя exit-коды CLI (`0` ok, `1` wrong, `2` error). После 3 неудач — удаление отменяется. На успех: `sc stop / delete`, `ParentalControlSvc.exe uninstall`, удаление правил firewall.

### Пути во время выполнения

| Что | Где |
|---|---|
| Бинарь 1 | `C:\Program Files\ParentalControl\ParentalControlSvc.exe` (или `C:\ProgramData\ParentalControl\…` для dev) — берётся первый существующий из `config.toml [paths].agent_cli[*]` / `agent_tray[*]` |
| Бинарь 2 | `…\agent-cli.exe` |
| Бинарь 3 | `…\agent-tray.exe` |
| Конфиг | `<exe-dir>\config.toml` (отсутствует → дефолт) |
| Шаблоны | `<exe-dir>\templates\*.html` |
| Статика | `<exe-dir>\static\…` |
| БД | `C:\ProgramData\ParentalControl\data.db` |
| Логи | `C:\ProgramData\ParentalControl\logs\YYYY-MM-DD\{service\|agent_cli\|agent_tray}.log` (+ `error.log`, `debug.log`) |
| Tray-токен | `C:\ProgramData\ParentalControl\tray_token.txt` |
| Стандартные параметры | `config.toml [service]`, `[paths]`, `[tray]`, `[limits]` |

Исполняемые пути разрешаются через `Config::get_cli_path` / `get_tray_path` — первый существующий из массива.

---

## 11. Конфигурация (`config.toml`)

```text
[service]
name = "ParentalControlSvc"        # имя SCM-службы
display_name = "Parental Control Service"
api_port = 8765                    # на этом порту axum слушает 0.0.0.0
tick_duration_secs = 1             # период главного цикла
log_level = "info"                 # debug|info|warn|error

[tray]
host = "127.0.0.1"                 # к кому tray ходит за /api/status
update_interval_sec = 5            # как часто опрашивать

[paths]
log_dir    = "C:\\ProgramData\\ParentalControl"
static_dir = "static"
agent_cli  = ["…\\Program Files\\ParentalControl\\agent-cli.exe",
              "…\\ProgramData\\ParentalControl\\agent-cli.exe"]
agent_tray = […аналогично…]

[limits]
default_limit_sec  = 7200          # дефолт для новых пользователей и дней без override
default_start_time = "08:00"
default_end_time   = "20:00"
```

Если файла `config.toml` нет — используются встроенные дефолты (`Config::default_config`).

### Дефолтные `settings` (записываются в БД при первом старте)

`lock_message`, `lock_delay_sec=5`, `admin_pin=''`, `device_name=''`, `api_token=''`, `tray_token=''`, `lock_internet_on_screen_lock=0`, `scheduler_enabled=1`, `scheduler_interval_minutes=5`, `agent_mode='hybrid'`, `log_level='info'`, `log_retention_days=14`, `warning_minutes=0`. Категории: `notifications`, `timing`, `security`, `general`, `internal`, `control`, `scheduler`, `logs`.

---

## 12. Логи и диагностика

### Файлы (ежедневно ротируются по `log_retention_days`)

- `<log_dir>\logs\YYYY-MM-DD\<source>.log` — все события `>= min_level` от одного источника (`service`/`agent_cli`/`agent_tray`).
- `error.log` — дубликат всех `Error`.
- `debug.log` — дубликат всех `Debug` (отделён, чтобы не «засорять» основной лог).
- В консоль пишется та же строка с эмодзи-уровнем (🔍 debug, ℹ info, ⚠ warn, ❌ error).

### API

- `GET /api/logs` — список дат (подпапок).
- `GET /api/logs/{date}?source=service&level=info&limit=500` — читает `<date>\<source>.log`, фильтрует по `[INFO ]`/`[WARN ]`/`[ERROR]`, возвращает свежие строки в обратном порядке.

### Panic

`std::panic::set_hook` в `service.rs::main` пишет в `<log_dir>\logs\YYYY-MM-DD\panic.log` (с файлом, строкой и сообщением). Аналогичных хуков в `cli.rs` и `tray.rs` нет — падения там логируются обычным логгером.

### Сторож

`ParentalControl_Watchdog` (Task Scheduler, root folder, SYSTEM, highest) каждые `scheduler_interval_minutes` запускает `sc.exe start ParentalControlSvc`. Если служба уже запущена — `sc` отвечает мгновенно. Если упала — сторож её поднимет.

---

## 13. Фронтенд (кратко)

- `templates/index.html` через `minijinja` рендерится в одну SPA, дальше рулит `static/app.js` (Alpine.js).
- PIN-логин: при открытии админки `GET /api/is_pin_set`. Если PIN не задан — экран установки (`POST /api/set_pin`). Если задан — поле ввода, затем `POST /api/check_pin`. Полученный «пиновый» токен сохраняется в `localStorage.agent_pin` и используется как `Authorization: Bearer <pin>` (админский уровень доступа).
- Табы: `status`, `users`, `global` (настройки), `logs`.
- Per-user: week schedule и «сегодня»-override, кнопки «дай ещё N минут экрана / интернета», ручное lock/unlock/auto с дедлайном, toggle контроля.
- Полностью клиентский REST к `/api/*`, без отдельного SPA-роутинга на сервере.

---

## 14. Краткий «cheat sheet» для агента

- Изменить поведение блокировки экрана → `apply_lock_state` и `enforce_lock_procedure` в `src/bin/service.rs`.
- Изменить правила расчёта политики → `src/policy.rs::check_screen_time_policy` и `should_lock`.
- Добавить новое поле в «сегодня» → `src/types.rs::TodayLimit` + `src/db.rs::ensure_today_limit/set_today_limit` + `src/api.rs::TodayLimitResponse`.
- Добавить новый эндпоинт → `src/api.rs` (зарегистрировать в `start_api_server`).
- Изменить блокировку интернета → `src/firewall.rs` (правило по SID) + ветка в `apply_lock_state`.
- Изменить логику трея → `src/bin/tray.rs` + `src/tray_ui.rs::PopupData`.
- Изменить критерии «можно ли сейчас» для `Manual {Unlock}` → `policy::should_lock` + `apply_lock_state`.

