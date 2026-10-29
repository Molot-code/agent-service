# AGENTS.md

Краткая карта проекта для ИИ-агента. Подробное описание архитектуры —
в `docs/ARCHITECTURE.md`. Этот файл — про **куда лезть**, **что не трогать**
и **какие контракты соблюдать**.

## Стек

- Rust (edition 2021), Windows x64.
- `rusqlite` (SQLite, WAL), `axum` 0.8 (HTTP), `minijinja` (шаблоны),
  `tokio`, `chrono`, `serde`, `tower-http` (статика).
- **Фронтенд:** Alpine.js + Tailwind CSS (без npm-проекта, всё через npx/esbuild).
- Три бинарника в `src/bin/`: `service.rs`, `cli.rs`, `tray.rs`.
- Общий крейт — `src/lib.rs`.

## Структура `src/`

| Путь | Что там |
|---|---|
| `lib.rs` | Реэкспорты модулей, `AdminCommand`, `SessionState`. |
| `types.rs` | `ControlMode`, `ManualAction`, `TodayLimit`, `DaySchedule`, `SettingItem`, `TimeToEvent`, `AdminCommand`, `SessionState`. |
| `config.rs` | Чтение `config.toml`, `get_cli_path()`, `get_tray_path()`. |
| `db/mod.rs` | `AppDb`, конструкторы `new` / `new_default`, `DEFAULT_DB_DIR`. |
| `db/schema.rs` | DDL + дефолтные `settings` + миграция `ensure_column`. **Единственное место, где CREATE/ALTER TABLE.** |
| `db/impls/settings.rs` | `get_setting`, `set_setting`, `get_settings`, `get_setting_meta`, `get_state`, `set_state`. `SettingMeta` реэкспортируется как `crate::db::SettingMeta`. |
| `db/impls/users.rs` | Пользователи + `cleanup_stale_users` (ставит `is_active=0`). |
| `db/impls/schedule.rs` | `weekly_schedule`. |
| `db/impls/limits.rs` | `daily_limits`: today limit, mode, extra time, internet pause. |
| `db/impls/usage.rs` | `daily_usage`, `lock_events`, `activity_log` (log_activity, get_activity_log). |
| `policy.rs` | `evaluate_access`, `should_lock`, `calculate_time_to_event`, `do_sync_user_access`. |
| `api/mod.rs` | Реэкспорт `AppState`, `start_api_server`, DTO. |
| `api/state.rs` | `AppState`; `init_api_token`, `init_tray_token`. |
| `api/auth.rs` | `AuthLevel`, `check_auth_level`, middleware `require_auth`/`require_admin`. |
| `api/dto.rs` | Все Serde-структуры запросов/ответов (включая `DefaultsResponse`). |
| `api/util.rs` | `device_name`, `resolve_manual_until`, `apply_extra`, `set_internet_blocked`. |
| `api/router.rs` | Сборка `Router` (только `.route()` + группы middleware). |
| `api/server.rs` | `start_api_server`: токены, шаблоны, статика, bind, serve. |
| `api/handlers/web.rs` | Веб-UI: `/`, `/api/status`, `/api/users`, `/api/schedule/*`, `/api/today/*`, `/api/settings`, `/api/set_param`, `/api/logs/*`. |
| `api/handlers/control.rs` | Управление: `/api/manual_action`, `/api/extra_time`, `/api/set_extra_time`, `/api/users/toggle_control`, `/api/internet/*`. |
| `api/handlers/pin.rs` | `/api/check_pin`, `/api/set_pin`, `/api/is_pin_set` (без авторизации). |
| `api/handlers/system.rs` | `/health`, `/api/defaults`. |
| `firewall.rs` | Правило `ParentalControl_BlockUser_<SID>`. |
| `native_lock.rs` | Свой fullscreen-блокировщик (Windows). |
| `session.rs` | `execute_in_user_session`, `lock_session`. |
| `users.rs` | Windows SID/users, `get_active_user_sid`. |
| `discovery.rs`, `cloud_client.rs`, `scheduler.rs`, `logger.rs`, `error.rs`, `tray_ui.rs` | Как раньше. |
| `bin/service.rs` | SCM, главный цикл, `apply_lock_state`, `enforce_lock_procedure`. |
| `bin/cli.rs` | `lock <sec>`, `lock-system`, `unlock`, `check-pin <pin>`. |
| `bin/tray.rs` | Tao + tray-icon, опрос `/api/status`. |

**Не создавать новые файлы в корне `src/`** без нужды — они уже разложены по доменам. `db.rs` и `api.rs` (монолиты) больше не существуют.

## Структура `static/` (Фронтенд)

Веб-интерфейс — это SPA на Alpine.js. Сборка через `esbuild`.
**Исходники** лежат в `static/js/`, **артефакт** — `static/app.min.js` (генерируется, руками не править!).

| Путь | Что там |
|---|---|
| `static/js/app.js` | **Точка входа** (только импорты и мерж стейтов). |
| `static/app.min.js` | Скомпилированный бандл (подключается в `index.html`). |
| `static/js/api.js` | Обёртки `apiGet` / `apiPost` с авто-подстановкой `Bearer`. |
| `static/js/state-auth.js` | Авторизация (PIN, логин, смена PIN). |
| `static/js/state-defaults.js` | Загрузка дефолтов с бэка (`/api/defaults`). |
| `static/js/state-status.js` | Глобальный статус (`global`), авто-обновление, activeToday. |
| `static/js/state-users.js` | Список пользователей, выбор, загрузка today/schedule выбранного. |
| `static/js/state-extra.js` | Слайдеры доп. времени (preview/commit). |
| `static/js/state-schedule.js` | Расписание на неделю, сохранение today/schedule. |
| `static/js/state-settings.js` | Глобальные настройки (табы, группировка, автосохранение). |
| `static/js/state-logs.js` | Просмотр логов. |
| `static/js/state-activity.js` | Activity log: загрузка, построение сегментов, график активности. |
| `static/js/state-manual.js` | Ручная блокировка/разблокировка (lock/unlock/auto). |
| `static/js/format.js` | UI-хелперы (форматирование времени, проценты, цвета). |

**Правила для фронтенда:**
- Все файлы в `static/js/` должны использовать ES-модули (`export const ...`).
- В `static/js/app.js` импортируются все стейты и мержатся в один Alpine-компонент.
- Не подключать отдельные скрипты в `index.html` — только `/app.min.js`.
- Не хардкодить дефолты (лимиты, время) во фронтенде — они приходят с бэка через `/api/defaults`.

## Правила «куда лезть»

| Задача | Файл |
|---|---|
| Поведение блокировки экрана | `bin/service.rs::apply_lock_state`, `enforce_lock_procedure` |
| Правила расчёта политики | `policy.rs::check_screen_time_policy`, `should_lock` |
| Новое поле в TodayLimit | `types.rs::TodayLimit` → `db/impls/limits.rs` → `api/dto.rs::TodayLimitResponse` |
| Новый эндпоинт | `api/handlers/*.rs` + регистрация в `api/router.rs` |
| Блокировка интернета | `firewall.rs` + ветка в `bin/service.rs::apply_lock_state` |
| Трей | `bin/tray.rs` + `tray_ui.rs::PopupData` |
| Схема БД | **только** `db/schema.rs` (+ `ensure_column` для миграций) |
| Настройки (дефолты) | `db/schema.rs` (INSERT OR IGNORE) и `config.toml [limits]` |
| Логика «до конца дня» | `api/util.rs::resolve_manual_until` |
| Добавить поле в UI | `static/js/state-*.js` (логика) + `templates/index.html` (верстка) |
| Activity log (логирование событий) | `db/impls/usage.rs::log_activity` + вызовы в `bin/service.rs`, `api/handlers/control.rs` |

## Контракты API (не ломать без согласования)

- Авторизация: `Authorization: Bearer <token>`. Уровни:
  - `api_token` и `admin_pin` → Admin;
  - `tray_token` → Tray (только `/api/status`, `/api/internet/*`).
- Middleware: `require_auth` (Tray+) и `require_admin` (Admin) в `api/router.rs`. Хендлеры **не** проверяют токен вручную.
- `/api/check_pin`, `/api/set_pin`, `/api/is_pin_set`, `/api/defaults` — без авторизации.
- **`POST /api/check_pin`** принимает `{pin: "..."}`.
- `POST /api/manual_action` — `until: "YYYY-MM-DD HH:MM:SS"` или `"-1"`.
- `GET /api/status` → `StatusResponse` (см. `api/dto.rs`).
- `GET /api/defaults` → `DefaultsResponse` (источник правды для дефолтов UI).
- `GET /api/activity_log/{sid}` → `ActivityLogResponse` (последние 100 событий пользователя).

## Контракты БД

- Все запросы — через `AppDb`, напрямую `rusqlite::Connection` не использовать.
- Горячие запросы — `prepare_cached`, не `prepare`.
- Вставка >1 строки в одну таблицу — в транзакции (`unchecked_transaction`), как в `users::get_or_create_user`.
- Секреты (`admin_pin`, `api_token`, `tray_token`) хранятся в `settings` с `is_secret=1`.
- `users.control_enabled` — рабочий флаг; `users.is_controlled` — legacy, не использовать.
- `users.is_active` — новый, ставится в `0` через `cleanup_stale_users`. Существующие БД мигрируют через `ensure_column`.
- `activity_log` — журнал событий (блокировки, разблокировки, heartbeat). Записывается автоматически при смене состояния.

## Запреты

- Не добавлять SQL DDL вне `db/schema.rs`.
- Не дублировать `check_auth` в хендлерах — только middleware.
- Не создавать `Router::new()` вне `api/router.rs`.
- Не писать в `system_state` напрямую из API-хендлеров, кроме случаев, где это уже есть (`internet_blocked`, `lock_status`) — это зона `bin/service.rs`.
- Не менять сигнатуры публичных методов `AppDb` без обновления вызовов; методы возвращают `rusqlite::Result<...>`.
- **Не править `static/app.min.js` руками** — он перезаписывается esbuild'ом при `task js`.

## Сборка и запуск

- `cargo build` — быстрая проверка.
- `cargo build --release` — прод-бинарь.
- `task js` — пересобрать `app.min.js` (esbuild).
- `task` (Taskfile) — полный пайплайн + Inno Setup.
- Логи: `C:\ProgramData\ParentalControl\logs\YYYY-MM-DD\{service|agent_cli|agent_tray}.log`.
- БД: `C:\ProgramData\ParentalControl\data.db`.

## Что где подробнее

- Архитектура бинарников, потоки SCM, WTS, firewall per-user rules → `docs/ARCHITECTURE.md`.
- HTTP API — таблица эндпоинтов → `docs/API.md`.
- Установка и uninstall → `installer/setup.iss`.