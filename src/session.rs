// src/session.rs
use std::{ffi::OsString, os::windows::ffi::OsStrExt};

//use windows::Win32::System::Threading::{
//    CreateProcessAsUserW, CREATE_UNICODE_ENVIRONMENT, STARTUPINFOW, PROCESS_INFORMATION,
// };
use windows::Win32::System::Environment::CreateEnvironmentBlock;
use windows::Win32::System::Threading::{
    CreateProcessAsUserW,
    GetExitCodeProcess, // <- Убедитесь, что они здесь есть
    WaitForSingleObject,
    CREATE_UNICODE_ENVIRONMENT,
    PROCESS_INFORMATION,
    STARTUPINFOW,
};
use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::{
            RemoteDesktop::{WTSGetActiveConsoleSessionId, WTSQueryUserToken},
            Shutdown::LockWorkStation,
        },
    },
};

/// Запускает процесс в активной пользовательской сессии
pub fn execute_in_user_session(exe_path: &str, args: &str) -> Result<(), String> {
    unsafe {
        // 1. Получаем ID активной консоли (сессия пользователя)
        let session_id = WTSGetActiveConsoleSessionId();
        if session_id == 0xFFFFFFFF {
            return Err("Нет активной пользовательской сессии".to_string());
        }

        // 2. Получаем токен пользователя
        let mut user_token = HANDLE::default();
        let result = WTSQueryUserToken(session_id, &mut user_token);
        if result.is_err() {
            return Err(format!(
                "Не удалось получить токен пользователя: {:?}",
                result
            ));
        }

        // 3. Подготавливаем строки (null-terminated wide strings)
        let exe_wide: Vec<u16> = OsString::from(exe_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // ВАЖНО: Оборачиваем путь в кавычки на случай пробелов в "Program Files"
        let cmd_line = format!("\"{}\" {}", exe_path, args);
        let mut cmd_wide: Vec<u16> = OsString::from(&cmd_line)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // 4. Настраиваем STARTUPINFOW
        let mut startup_info = STARTUPINFOW::default();
        startup_info.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        startup_info.lpDesktop = PWSTR::null();

        let mut process_info = PROCESS_INFORMATION::default();

        // 5. Создаём окружение для нового процесса
        let mut env_block = std::ptr::null_mut();
        let _ = CreateEnvironmentBlock(&mut env_block, Some(user_token), false);

        // 6. Вызываем CreateProcessAsUserW с правильными типами
        let result = CreateProcessAsUserW(
            Some(user_token),
            PCWSTR(exe_wide.as_ptr()),          // *const u16 -> PCWSTR
            Some(PWSTR(cmd_wide.as_mut_ptr())), // *mut u16 -> PWSTR, завёрнутый в Option
            None,
            None,
            false,
            CREATE_UNICODE_ENVIRONMENT,
            Some(env_block),
            PCWSTR::null(),
            &mut startup_info,
            &mut process_info,
        );

        // 7. Проверяем результат создания процесса
        if result.is_err() {
            let _ = CloseHandle(user_token);
            return Err(format!("CreateProcessAsUser failed: {:?}", result));
        }

        // 8. Ждем завершения процесса максимум 5500 миллисекунд (чуть больше таймаута в CLI)
        // WAIT_TIMEOUT (код 258) вернется, если процесс ВСЁ ЕЩЕ работает спустя это время.
        let wait_result = WaitForSingleObject(process_info.hProcess, 5500);

        let final_result = if wait_result == windows::Win32::Foundation::WAIT_TIMEOUT {
            // Процесс всё еще активен? Отлично, значит CLI прошел проверку и держит блокировку
            Ok(())
        } else {
            // Процесс завершился раньше времени. Вытаскиваем Exit Code
            let mut exit_code: u32 = 0;
            let _ = GetExitCodeProcess(process_info.hProcess, &mut exit_code);

            if exit_code == 0 {
                // Если вдруг CLI завершился с кодом 0 (хотя по логике он должен висеть)
                Ok(())
            } else {
                // CLI вернул ошибку (например, exit(1))
                Err(format!(
                    "Процесс блокировки завершился с ошибкой. Код: {}",
                    exit_code
                ))
            }
        };

        // 9. Освобождаем ресурсы
        let _ = CloseHandle(process_info.hProcess);
        let _ = CloseHandle(process_info.hThread);
        let _ = CloseHandle(user_token);

        final_result
    }
}

pub fn lock_session() -> Result<(), String> {
    unsafe { LockWorkStation().map_err(|e| format!("LockWorkStation {}", e)) }
}
