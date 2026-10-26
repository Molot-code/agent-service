// src/native_lock.rs
#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "windows")]
use std::sync::Mutex;

#[cfg(target_os = "windows")]
use windows::core::PCWSTR;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::*;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC, GetStockObject,
    ReleaseDC, SelectObject, SetBkMode, SetTextColor, BLACK_BRUSH, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DEFAULT_QUALITY, DT_CENTER, DT_NOCLIP, DT_VCENTER, DT_WORDBREAK, HBRUSH,
    HGDIOBJ, OUT_DEFAULT_PRECIS, PAINTSTRUCT, TRANSPARENT,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    BlockInput, RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_SHIFT, VK_U,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowW, GetMessageW, GetSystemMetrics,
    LoadCursorW, PostMessageW, PostQuitMessage, RegisterClassW, SetWindowPos, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, HWND_TOPMOST, IDC_ARROW, MSG, SM_CXSCREEN, SM_CYSCREEN, SWP_NOMOVE,
    SWP_NOSIZE, SWP_SHOWWINDOW, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_HOTKEY, WM_PAINT, WNDCLASSW,
    WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

use crate::log_info; // Подключаем макрос прямо из корня
                     // ==========================================
                     // СТАТИЧЕСКИЕ ПЕРЕМЕННЫЕ
                     // ==========================================
#[cfg(target_os = "windows")]
static IS_LOCKED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
static LOCK_HWND: Mutex<Option<isize>> = Mutex::new(None);
#[cfg(target_os = "windows")]
static LOCK_MESSAGE: Mutex<String> = Mutex::new(String::new());

#[cfg(target_os = "windows")]
const UNLOCK_HOTKEY_ID: i32 = 0x4242;

#[cfg(target_os = "windows")]
pub fn set_lock_message(message: String) {
    *LOCK_MESSAGE.lock().unwrap() = message;
}

// ==========================================
// ОБРАБОТЧИК ОКНА (WndProc)
// ==========================================
#[cfg(target_os = "windows")]
unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let hdc = GetDC(Some(hwnd));
            let rect = RECT {
                left: 0,
                top: 0,
                right: GetSystemMetrics(SM_CXSCREEN),
                bottom: GetSystemMetrics(SM_CYSCREEN),
            };
            let _ = FillRect(hdc, &rect, HBRUSH(GetStockObject(BLACK_BRUSH).0));
            let _ = ReleaseDC(Some(hwnd), hdc);
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let message = LOCK_MESSAGE.lock().unwrap().clone();
            let text = if message.is_empty() {
                "🔒 КОМПЬЮТЕР ЗАБЛОКИРОВАН\r\n\r\nОбратитесь к администратору".to_string()
            } else {
                message
            };
            let mut rect = RECT {
                left: 20,
                top: GetSystemMetrics(SM_CYSCREEN) / 2 - 150,
                right: GetSystemMetrics(SM_CXSCREEN) - 20,
                bottom: GetSystemMetrics(SM_CYSCREEN) / 2 + 150,
            };

            let _ = SetBkMode(hdc, TRANSPARENT);
            let _ = SetTextColor(hdc, COLORREF(0x00FFFFFF));

            let font = CreateFontW(
                48,
                0,
                0,
                0,
                700,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                DEFAULT_QUALITY,
                0,
                PCWSTR::null(),
            );

            if !font.0.is_null() {
                let old_font = SelectObject(hdc, HGDIOBJ(font.0));
                let mut wide_text: Vec<u16> =
                    text.encode_utf16().chain(std::iter::once(0)).collect();
                let _ = DrawTextW(
                    hdc,
                    &mut wide_text,
                    &mut rect,
                    DT_CENTER | DT_VCENTER | DT_NOCLIP | DT_WORDBREAK,
                );
                let _ = SelectObject(hdc, old_font);
                let _ = DeleteObject(HGDIOBJ(font.0));
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_CLOSE | WM_DESTROY => {
            let _ = UnregisterHotKey(Some(hwnd), UNLOCK_HOTKEY_ID);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => {
            if msg == WM_HOTKEY {
                if wparam.0 as i32 == UNLOCK_HOTKEY_ID {
                    println!("🔓 [native_lock] Активирована хитрая комбинация разблокировки (Ctrl+Shift+Alt+U)!");
                    PostQuitMessage(0);
                    return LRESULT(0);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

// ==========================================
// ОСНОВНЫЕ ФУНКЦИИ
// ==========================================
#[cfg(target_os = "windows")]
pub fn lock_screen() -> Result<(), String> {
    if IS_LOCKED.load(Ordering::SeqCst) {
        return Ok(());
    }

    std::thread::spawn(move || {
        unsafe {
            let instance = GetModuleHandleW(None).unwrap();
            let class_name: Vec<u16> = "CustomLockScreenClass\0".encode_utf16().collect();
            let brush = HBRUSH(GetStockObject(BLACK_BRUSH).0);

            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: instance.into(),
                hIcon: Default::default(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: brush,
                lpszMenuName: PCWSTR::null(),
                lpszClassName: PCWSTR(class_name.as_ptr()),
            };
            let _ = RegisterClassW(&wc);

            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                PCWSTR(class_name.as_ptr()),
                PCWSTR(
                    "System Locked\0"
                        .encode_utf16()
                        .collect::<Vec<_>>()
                        .as_ptr(),
                ),
                WS_POPUP | WS_VISIBLE | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
                0,
                0,
                GetSystemMetrics(SM_CXSCREEN),
                GetSystemMetrics(SM_CYSCREEN),
                None,
                None,
                Some(instance.into()),
                None,
            )
            .unwrap();

            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
            *LOCK_HWND.lock().unwrap() = Some(hwnd.0 as isize);

            let modifiers = MOD_CONTROL | MOD_SHIFT | MOD_ALT;
            let register_result =
                RegisterHotKey(Some(hwnd), UNLOCK_HOTKEY_ID, modifiers, VK_U.0 as u32);

            if register_result.is_ok() {
                //                log_info!("✅ [native_lock] Хитрая комбинация разблокировки активирована: Ctrl+Shift+Alt+U");
            }

            let _ = BlockInput(true);
            IS_LOCKED.store(true, Ordering::SeqCst);
            log_info!("✅ [native_lock] Блокировка активна");
            println!("✅ [native_lock] Блокировка активна");

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, Some(hwnd), 0, 0).0 != 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            let _ = BlockInput(false);
            IS_LOCKED.store(false, Ordering::SeqCst);
            *LOCK_HWND.lock().unwrap() = None;
            log_info!("🛑 [native_lock] Окно закрыто, разблокировано");
            println!("🛑 [native_lock] Окно закрыто, разблокировано");
        }
    });
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn unlock_screen() -> Result<(), String> {
    unsafe {
        let class_name: Vec<u16> = "CustomLockScreenClass\0".encode_utf16().collect();
        let hwnd = FindWindowW(PCWSTR(class_name.as_ptr()), PCWSTR::null());

        if let Ok(hwnd) = hwnd {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            IS_LOCKED.store(false, Ordering::SeqCst);
            log_info!("️✅ [native_lock] Разблокировка завершена");
            println!("✅ [native_lock] Разблокировка завершена");
            return Ok(());
        }

        log_info!("️ [native_lock] Окно не найдено, пытаюсь завершить процесс agent-cli.exe...");
        println!("️ [native_lock] Окно не найдено, пытаюсь завершить процесс agent-cli.exe...");
        let _ = std::process::Command::new("taskkill")
            .args(&["/F", "/IM", "agent-cli.exe"])
            .output();

        IS_LOCKED.store(false, Ordering::SeqCst);
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn is_locked() -> bool {
    IS_LOCKED.load(Ordering::SeqCst)
}

// ==========================================
// ЗАГЛУШКИ ДЛЯ НЕ-WINDOWS
// ==========================================
#[cfg(not(target_os = "windows"))]
pub fn lock_screen() -> Result<(), String> {
    Err("Only Windows".into())
}
#[cfg(not(target_os = "windows"))]
pub fn unlock_screen() -> Result<(), String> {
    Err("Only Windows".into())
}
#[cfg(not(target_os = "windows"))]
pub fn is_locked() -> bool {
    false
}
#[cfg(not(target_os = "windows"))]
pub fn set_lock_message(_: String) {}
