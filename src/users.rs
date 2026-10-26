use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL},
        NetworkManagement::NetManagement::{
            NetApiBufferFree, NetUserEnum, FILTER_NORMAL_ACCOUNT, USER_INFO_1,
        },
        Security::{Authorization::ConvertSidToStringSidW, LookupAccountNameW, PSID, SID_NAME_USE},
        System::RemoteDesktop::{WTSGetActiveConsoleSessionId, WTSQueryUserToken},
    },
};

use crate::{db::AppDb, log_error, log_info, log_warn};

const UF_NORMAL_ACCOUNT: u32 = 0x0200;
const UF_WORKSTATION_TRUST_ACCOUNT: u32 = 0x1000;
const UF_SERVER_TRUST_ACCOUNT: u32 = 0x2000;
const UF_INTERDOMAIN_TRUST_ACCOUNT: u32 = 0x0800;
const UF_ACCOUNTDISABLE: u32 = 0x0002;

#[cfg(target_os = "windows")]
unsafe fn lookup_sid_for_name(name: PWSTR) -> Option<String> {
    let mut sid_len: u32 = 0;
    let mut domain_len: u32 = 0;
    let mut sid_name_use = SID_NAME_USE(0);

    // Первый вызов — получаем размеры буферов
    let _ = LookupAccountNameW(
        PCWSTR::null(),
        PCWSTR(name.as_ptr()),
        None, // Option<PSID> — None для первого вызова
        &mut sid_len,
        None, // Option<PWSTR> — None для первого вызова
        &mut domain_len,
        &mut sid_name_use,
    );

    if sid_len == 0 || domain_len == 0 {
        return None;
    }

    let mut sid_buf = vec![0u8; sid_len as usize];
    let mut domain_buf = vec![0u16; domain_len as usize];

    // Второй вызов — получаем сам SID
    let result = LookupAccountNameW(
        PCWSTR::null(),
        PCWSTR(name.as_ptr()),
        Some(PSID(sid_buf.as_mut_ptr() as *mut _)), // Оборачиваем в Some
        &mut sid_len,
        Some(PWSTR(domain_buf.as_mut_ptr())), // Оборачиваем в Some
        &mut domain_len,
        &mut sid_name_use,
    );

    if result.is_err() {
        return None;
    }

    let mut str_sid: PWSTR = PWSTR(std::ptr::null_mut());
    if ConvertSidToStringSidW(PSID(sid_buf.as_mut_ptr() as *mut _), &mut str_sid).is_err() {
        return None;
    }

    let result = str_sid.to_string().ok();

    // Освобождаем память через LocalFree из Foundation
    if !str_sid.0.is_null() {
        let _ = LocalFree(Some(HLOCAL(str_sid.0 as *mut _)));
    }

    result
}

#[cfg(target_os = "windows")]
pub fn get_local_users_with_sids() -> Vec<(String, String)> {
    let mut users = Vec::new();
    let mut resume_handle = 0;
    let mut entries_read = 0;
    let mut total_entries = 0;

    unsafe {
        let mut buffer: *mut u8 = std::ptr::null_mut();

        loop {
            let status: u32 = NetUserEnum(
                None,
                1,
                FILTER_NORMAL_ACCOUNT,
                &mut buffer,
                0xFFFFFFFF,
                &mut entries_read,
                &mut total_entries,
                Some(&mut resume_handle),
            );

            if status == 0 || status == 2361 {
                if !buffer.is_null() {
                    let user_infos = std::slice::from_raw_parts(
                        buffer as *const USER_INFO_1,
                        entries_read as usize,
                    );

                    for user_info in user_infos {
                        let username = user_info.usri1_name.to_string().unwrap_or_default();
                        let flags_val = user_info.usri1_flags.0;

                        let is_normal_account = (flags_val & UF_NORMAL_ACCOUNT) != 0;
                        let is_trust_account = (flags_val
                            & (UF_WORKSTATION_TRUST_ACCOUNT
                                | UF_SERVER_TRUST_ACCOUNT
                                | UF_INTERDOMAIN_TRUST_ACCOUNT))
                            != 0;
                        let is_disabled = (flags_val & UF_ACCOUNTDISABLE) != 0;

                        if is_normal_account && !is_trust_account && !is_disabled {
                            if !username.is_empty()
                                && username.len() >= 2
                                && !username.eq_ignore_ascii_case("SYSTEM")
                                && !username.eq_ignore_ascii_case("Administrator")
                                && !username.eq_ignore_ascii_case("Guest")
                                && !username.contains("Default")
                                && !username.contains("WDAG")
                            {
                                if let Some(sid_str) = lookup_sid_for_name(user_info.usri1_name) {
                                    if sid_str.starts_with("S-1-5-21-") {
                                        users.push((username, sid_str));
                                    }
                                }
                            }
                        }
                    }
                    let _ = NetApiBufferFree(Some(buffer as *const _));
                }
            }

            if status != 2361 {
                break;
            }
        }
    }

    users
}

#[cfg(not(target_os = "windows"))]
pub fn get_local_users_with_sids() -> Vec<(String, String)> {
    vec![("test_user".to_string(), "S-1-5-21-0-0-0-1001".to_string())]
}

pub fn initialize_users(db: &AppDb) {
    log_info!("🔍 Инициализация пользователей Windows...");

    let local_users = get_local_users_with_sids(); // ← исправлено имя функции
    log_info!("📋 Найдено локальных пользователей: {}", local_users.len());

    if local_users.is_empty() {
        log_warn!("⚠️ Не найдено локальных пользователей");
        return;
    }

    for (username, sid) in &local_users {
        match db.get_or_create_user(sid, username) {
            Ok(_) => log_info!("✅ Пользователь добавлен: {} (SID: {})", username, sid),
            Err(e) => log_error!("⚠️ Ошибка добавления пользователя {}: {}", username, e),
        }
    }

    log_info!(
        "✅ Инициализация {} пользователей завершена",
        local_users.len()
    );
}

pub fn sync_users(db: &AppDb, _log_fn: &dyn Fn(&str)) {
    let local_users = get_local_users_with_sids(); // ← исправлено имя функции

    let active_sids: Vec<String> = local_users.iter().map(|(_, sid)| sid.clone()).collect();

    for (username, sid) in &local_users {
        if let Err(e) = db.get_or_create_user(sid, username) {
            log_error!("⚠️ Ошибка синхронизации пользователя {}: {}", username, e);
        }
    }

    match db.cleanup_stale_users(&active_sids) {
        Ok(count) if count > 0 => {
            log_info!("🧹 Деактивировано устаревших пользователей: {}", count);
        }
        Err(e) => {
            log_error!("⚠️ Ошибка очистки устаревших пользователей: {}", e);
        }
        _ => {}
    }
}

#[cfg(target_os = "windows")]
unsafe fn extract_sid_from_token(token: HANDLE) -> Option<String> {
    use windows::{
        core::PWSTR,
        Win32::{
            Foundation::{LocalFree, HLOCAL},
            Security::{
                Authorization::ConvertSidToStringSidW, GetTokenInformation, TokenUser, PSID,
                TOKEN_USER,
            },
        },
    };

    let mut len: u32 = 0;
    let _ = GetTokenInformation(token, TokenUser, None, 0, &mut len);
    if len == 0 {
        return None;
    }

    let mut buffer = vec![0u8; len as usize];
    if GetTokenInformation(
        token,
        TokenUser,
        Some(buffer.as_mut_ptr() as *mut _),
        len,
        &mut len,
    )
    .is_err()
    {
        return None;
    }

    let token_user = buffer.as_ptr() as *const TOKEN_USER;
    let sid_ptr: PSID = (*token_user).User.Sid;

    let mut str_sid = PWSTR(std::ptr::null_mut());
    let sid_string = if ConvertSidToStringSidW(sid_ptr, &mut str_sid).is_ok() {
        let s = str_sid.to_string().ok();
        if !str_sid.0.is_null() {
            let _ = LocalFree(Some(HLOCAL(str_sid.0 as *mut _)));
        }
        s
    } else {
        None
    };

    sid_string
}

pub fn get_active_user_sid() -> Option<String> {
    unsafe {
        let session_id = WTSGetActiveConsoleSessionId();
        if session_id == 0xFFFFFFFF {
            return None;
        }
        let mut token = HANDLE::default();
        WTSQueryUserToken(session_id, &mut token).ok()?;
        let sid = extract_sid_from_token(token);
        let _ = CloseHandle(token);
        sid
    }
}
