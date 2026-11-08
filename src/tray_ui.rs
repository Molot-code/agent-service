// src/tray_ui.rs
use std::sync::{Arc, Mutex, OnceLock};

use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{HWND, RECT, *},
        Graphics::{
            Dwm::*,
            Gdi::{HDC, *},
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::SetFocus,
            WindowsAndMessaging::{FindWindowW, *},
        },
    },
};

use crate::log_error;

const CLASS_NAME: PCWSTR = w!("AgentTrayPopup");
const WINDOW_TITLE: PCWSTR = w!("Родительский контроль");

const COLOR_BG: u32 = 0x00F3F3F3;
const COLOR_TEXT: u32 = 0x00202020;
const COLOR_TEXT_GRAY: u32 = 0x00606060;
const COLOR_BUTTON: u32 = 0x00E6E6E6;

const WINDOW_W: i32 = 320;
const WINDOW_H: i32 = 240;

const PAD: i32 = 16;

const BTN_X: i32 = PAD - 8;
const BTN_W: i32 = WINDOW_W - PAD * 2;
const BTN_H: i32 = 34;
const BTN_Y: i32 = WINDOW_H - PAD - BTN_H - 8;

#[derive(Clone, Default)]
pub struct PopupData {
    pub username: String,
    pub screen_remaining_sec: u32,
    pub screen_limit_sec: u32,
    pub internet_paused: bool,
    // 🆕 интернет-таймеры
    pub internet_remaining_sec: u32,
    pub internet_limit_sec: u32,
    // 🆕 для кружков
    pub mode: String, // "scheduled" | "manual_lock" | "manual_unlock" | "control_disabled" | "no_user"
    pub is_locked: bool,
    pub time_to_event_minutes: Option<u32>,
}

struct PopupState {
    data: PopupData,
    on_toggle: Option<Box<dyn Fn(bool) + Send>>,
    on_close: Option<Box<dyn Fn() + Send>>,
}

static STATE: OnceLock<Arc<Mutex<PopupState>>> = OnceLock::new();

fn log_ui(msg: &str) {
    use std::io::Write;
    let timestamp = chrono::Local::now().format("[%Y-%m-%d %H:%M:%S]");
    let line = format!("{} [ui] {}\n", timestamp, msg);
    print!("{}", line);
    let _ = std::io::stdout().flush();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(r"C:\ProgramData\ParentalControl\tray.log")
    {
        let _ = f.write_all(line.as_bytes());
    }
}
// ===== Публичное API =====

pub fn show(
    data: PopupData,
    on_toggle: impl Fn(bool) + Send + 'static,
    on_close: impl Fn() + Send + 'static,
) {
    let t0 = std::time::Instant::now();
    log_ui("show: START");

    unsafe {
        static INIT: OnceLock<()> = OnceLock::new();
        INIT.get_or_init(|| {
            log_ui(&format!("show: регистрирую класс ({:?})", t0.elapsed()));

            let hmodule = GetModuleHandleW(None).unwrap();
            let hinstance = HINSTANCE(hmodule.0);

            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wndproc),
                hInstance: hinstance,
                hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0),
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };
            RegisterClassW(&wc);

            log_ui(&format!("show: класс OK ({:?})", t0.elapsed()));
        });
    }

    log_ui(&format!("show: до state ({:?})", t0.elapsed()));

    let state = STATE.get_or_init(|| {
        Arc::new(Mutex::new(PopupState {
            data: PopupData::default(),
            on_toggle: None,
            on_close: None,
        }))
    });

    {
        let mut s = state.lock().unwrap();
        s.data = data;
        s.on_toggle = Some(Box::new(on_toggle));
        s.on_close = Some(Box::new(on_close));
    }

    unsafe {
        let hmodule = GetModuleHandleW(None).unwrap();

        let hinstance = HINSTANCE(hmodule.0);

        let mut work_area = RECT::default();
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut work_area as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );

        let x = work_area.right - WINDOW_W - 16;
        let y = work_area.bottom - WINDOW_H - 16;

        let hwnd = match CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            CLASS_NAME,
            WINDOW_TITLE,
            WS_POPUP | WS_THICKFRAME,
            x,
            y,
            WINDOW_W,
            WINDOW_H,
            None,
            None,
            Some(hinstance),
            None,
        ) {
            Ok(h) => {
                log_ui(&format!("show: окно создано ({:?})", t0.elapsed()));
                h
            }
            Err(e) => {
                log_error!("[TrayUI] CreateWindowExW failed: {:?}", e);
                return;
            }
        };

        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const _,
            std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);

        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
    }
}

pub fn hide() {
    unsafe {
        // 🆕 FindWindowW возвращает Result<HWND>
        if let Ok(hwnd) = FindWindowW(CLASS_NAME, WINDOW_TITLE) {
            if !hwnd.is_invalid() {
                let _ = DestroyWindow(hwnd);
            }
        }
    }
}

pub fn is_visible() -> bool {
    unsafe {
        match FindWindowW(CLASS_NAME, WINDOW_TITLE) {
            Ok(hwnd) => !hwnd.is_invalid(),
            Err(_) => false,
        }
    }
}

pub fn update_data(data: PopupData) {
    if let Some(state) = STATE.get() {
        if let Ok(mut s) = state.lock() {
            s.data = data;
        }
        unsafe {
            if let Ok(hwnd) = FindWindowW(CLASS_NAME, WINDOW_TITLE) {
                if !hwnd.is_invalid() {
                    // 🆕 InvalidateRect принимает bool
                    let _ = InvalidateRect(Some(hwnd), None, true);
                }
            }
        }
    }
}

// ===== WndProc =====

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            let bg_brush = CreateSolidBrush(COLORREF(COLOR_BG));
            FillRect(hdc, &ps.rcPaint, bg_brush);
            let _ = DeleteObject(bg_brush.into());

            let data = STATE
                .get()
                .and_then(|s| s.lock().ok())
                .map(|s| s.data.clone())
                .unwrap_or_default();

            let pitch_family = 0u32;

            // ===== Шрифты =====

            // Большой жирный — акцентный остаток
            let font_big = CreateFontW(
                28,
                0,
                0,
                0,
                FW_BOLD.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                pitch_family,
                w!("Segoe UI"),
            );
            let font_big2 = CreateFontW(
                24,
                0,
                0,
                0,
                FW_BOLD.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                pitch_family,
                w!("Segoe UI"),
            );
            // Обычный — подписи (🆕 чуть больше: 16 → 18)
            let font_normal = CreateFontW(
                18,
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                pitch_family,
                w!("Segoe UI"),
            );
            // Жирный обычного размера — заголовок и кнопка (🆕 16 → 18)
            let font_bold = CreateFontW(
                18,
                0,
                0,
                0,
                FW_BOLD.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                pitch_family,
                w!("Segoe UI"),
            );
            // Мелкий — « / Kч Lм» и «Осталось:» (🆕 13 → 14)
            let font_small = CreateFontW(
                14,
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                pitch_family,
                w!("Segoe UI"),
            );

            SetBkMode(hdc, TRANSPARENT);

            if data.mode == "manual_unlock" {
                // ===== Ручная разблокировка =====
                SelectObject(hdc, font_bold.into());
                SetTextColor(hdc, COLORREF(COLOR_TEXT));
                draw_text(hdc, &format!("🛡️ ▶️ {}", data.username), 16, 16);

                SelectObject(hdc, font_normal.into());
                SetTextColor(hdc, COLORREF(0x0008b9ea));
                draw_text(hdc, "🔓 Разблокировано вручную", 16, 56);

                if let Some(minutes) = data.time_to_event_minutes {
                    SetTextColor(hdc, COLORREF(COLOR_TEXT_GRAY));
                    let time_text = format!("⏰ Осталось {}", format_secs(minutes * 60));
                    draw_text(hdc, &time_text, 16, 88);
                } else {
                    SetTextColor(hdc, COLORREF(COLOR_TEXT_GRAY));
                    draw_text(hdc, "♾️ Без ограничения по времени", 16, 88);
                }
            } else {
                // ===== Строка 1: пользователь + эмодзи статуса =====
                let status_emoji = match data.mode.as_str() {
                    "control_disabled" => "🚫",
                    "no_user" => "💤",
                    "manual_lock" | "manual_unlock" => "👤",
                    _ => {
                        if data.is_locked {
                            "🔒"
                        } else {
                            "✅"
                        }
                    }
                };

                SelectObject(hdc, font_bold.into());
                SetTextColor(hdc, COLORREF(COLOR_TEXT));
                draw_text(
                    hdc,
                    &format!("🛡️ {} {}", status_emoji, data.username),
                    16,
                    12,
                );

                // ===== Экран =====
                let limit_x = 30;
                let screen_used = data
                    .screen_limit_sec
                    .saturating_sub(data.screen_remaining_sec);
                let screen_color = color_for_percent(screen_used, data.screen_limit_sec);

                // «Осталось:» — мелким серым
                SelectObject(hdc, font_small.into());
                SetTextColor(hdc, COLORREF(COLOR_TEXT_GRAY));
                draw_text(hdc, "Осталось:", 16, 40);

                // «Экранного времени» — обычным
                SelectObject(hdc, font_normal.into());
                SetTextColor(hdc, COLORREF(COLOR_TEXT));
                draw_text(hdc, "Экранного времени", 16, 58);

                // Одна строка: «Nч Mм» (большим) + « / Kч Lм» (мелким)
                SelectObject(hdc, font_big.into());
                SetTextColor(hdc, screen_color);
                let screen_remaining_text = format_secs(data.screen_remaining_sec);
                draw_text(hdc, &screen_remaining_text, limit_x, 78);

                let mut big_size = SIZE::default();
                let _ = GetTextExtentPoint32W(
                    hdc,
                    &to_wide_no_nul(&screen_remaining_text),
                    &mut big_size,
                );

                SelectObject(hdc, font_small.into());
                SetTextColor(hdc, COLORREF(COLOR_TEXT_GRAY));
                let screen_limit_text = format!(" / {}", format_secs(data.screen_limit_sec));
                draw_text(hdc, &screen_limit_text, limit_x + big_size.cx + 4, 78 + 11);

                // ===== Интернет =====
                let internet_used = data
                    .internet_limit_sec
                    .saturating_sub(data.internet_remaining_sec);
                let internet_color = if data.internet_paused {
                    COLORREF(0x004444ef)
                } else {
                    color_for_percent(internet_used, data.internet_limit_sec)
                };

                // Заголовок «Интернет»
                SelectObject(hdc, font_normal.into());
                SetTextColor(hdc, COLORREF(COLOR_TEXT));
                let internet_label = if data.internet_paused {
                    "🌐 Интернет ⏸"
                } else {
                    "🌐 Интернет"
                };
                draw_text(hdc, internet_label, 16, 114);

                // Цифры интернета (шрифт 24px)
                SelectObject(hdc, font_big2.into());
                SetTextColor(hdc, internet_color);
                let internet_remaining_text = format_secs(data.internet_remaining_sec);
                draw_text(hdc, &internet_remaining_text, limit_x, 134);

                let mut big_size_int = SIZE::default();
                let _ = GetTextExtentPoint32W(
                    hdc,
                    &to_wide_no_nul(&internet_remaining_text),
                    &mut big_size_int,
                );

                // Маленький текст « / Kч Lм» (выравниваем по базовой линии 24px шрифта)
                SelectObject(hdc, font_small.into());
                SetTextColor(hdc, COLORREF(COLOR_TEXT_GRAY));
                let internet_limit_text = format!(" / {}", format_secs(data.internet_limit_sec));
                draw_text(
                    hdc,
                    &internet_limit_text,
                    limit_x + big_size_int.cx + 4,
                    134 + 6, // 🆕 изменили с 14 на 11, так как шрифт 24, а не 28
                );

                // ===== Кнопка =====
                let btn_brush = CreateSolidBrush(COLORREF(COLOR_BUTTON));
                let mut btn_rect = RECT {
                    left: BTN_X,
                    top: BTN_Y,
                    right: BTN_X + BTN_W,
                    bottom: BTN_Y + BTN_H,
                };
                FillRect(hdc, &btn_rect, btn_brush);
                let _ = DeleteObject(btn_brush.into());

                SelectObject(hdc, font_bold.into());
                SetTextColor(hdc, COLORREF(COLOR_TEXT));
                let btn_text = if data.internet_paused {
                    "▶️  Продолжить интернет"
                } else {
                    "⏸  Пауза интернета"
                };
                DrawTextW(
                    hdc,
                    &mut to_wide(btn_text),
                    &mut btn_rect,
                    DT_SINGLELINE | DT_CENTER | DT_VCENTER,
                );
            }

            let _ = DeleteObject(font_big.into());
            let _ = DeleteObject(font_big2.into());
            let _ = DeleteObject(font_normal.into());
            let _ = DeleteObject(font_bold.into());
            let _ = DeleteObject(font_small.into());

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

            if x >= BTN_X && x <= BTN_X + BTN_W && y >= BTN_Y && y <= BTN_Y + BTN_H {
                let current = STATE
                    .get()
                    .and_then(|s| s.lock().ok())
                    .map(|s| s.data.internet_paused)
                    .unwrap_or(false);

                if let Some(state) = STATE.get() {
                    if let Ok(mut s) = state.lock() {
                        if let Some(cb) = &s.on_toggle {
                            cb(!current);
                        }
                        s.data.internet_paused = !current;
                    }
                }
                let _ = InvalidateRect(Some(hwnd), None, true);
            }
            LRESULT(0)
        }

        WM_KEYDOWN => {
            if wparam.0 == 0x1B {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }

        WM_DESTROY => {
            if let Some(state) = STATE.get() {
                if let Ok(s) = state.lock() {
                    if let Some(cb) = &s.on_close {
                        cb();
                    }
                }
            }
            LRESULT(0)
        }

        WM_KILLFOCUS => {
            log_ui("WM_KILLFOCUS → закрываю окно");
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_SETCURSOR => {
            if let Ok(cursor) = LoadCursorW(None, IDC_ARROW) {
                let _ = SetCursor(Some(cursor));
            }
            LRESULT(1)
        }
        WM_NCHITTEST => LRESULT(HTCLIENT as isize),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ===== Вспомогательные =====
/// 🆕 Без завершающего \0 — для GetTextExtentPoint32W с явной длиной.
fn to_wide_no_nul(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn draw_text(hdc: HDC, text: &str, x: i32, y: i32) {
    unsafe {
        let wide = to_wide(text);
        let _ = TextOutW(hdc, x, y, &wide);
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn format_secs(sec: u32) -> String {
    let m = sec / 60;
    let h = m / 60;
    let rem = m % 60;
    if h > 0 {
        format!("{}ч {}м", h, rem)
    } else {
        format!("{}м", m)
    }
}

/// Возвращает COLORREF (BGR) для кружка по проценту использования.
/// - < 50%  → зелёный
/// - < 80%  → жёлтый
/// - >= 80% → красный
fn color_for_percent(used: u32, limit: u32) -> COLORREF {
    if limit == 0 {
        return COLORREF(0x009ca3af); // серый — нет лимита
    }
    let percent = (used as f32 / limit as f32 * 100.0) as u32;
    if percent < 50 {
        COLORREF(0x005ec522) // зелёный (#22c55e в BGR)
    } else if percent < 80 {
        COLORREF(0x0008b9ea) // жёлтый (#eab908 в BGR)
    } else {
        COLORREF(0x004444ef) // красный (#ef4444 в BGR)
    }
}
