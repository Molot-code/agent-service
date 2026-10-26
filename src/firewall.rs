// src/firewall.rs
use windows::{
    core::{Interface, BSTR},
    Win32::{
        Foundation::{VARIANT_FALSE, VARIANT_TRUE},
        NetworkManagement::WindowsFirewall::{
            INetFwPolicy2, INetFwRule, INetFwRule3, INetFwRules, NetFwPolicy2, NetFwRule,
            NET_FW_ACTION_BLOCK, NET_FW_IP_PROTOCOL_ANY, NET_FW_RULE_DIR_OUT,
        },
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        },
    },
};

use crate::log_info;

const RULE_PREFIX: &str = "ParentalControl_BlockUser_";

fn rule_name(sid: &str) -> String {
    format!("{}{}", RULE_PREFIX, sid)
}

/// RAII-обёртка над CoInitializeEx/CoUninitialize.
/// Гарантирует, что CoUninitialize вызывается ПОСЛЕ того,
/// как все COM-объекты в текущей функции будут освобождены.
struct ComGuard;

impl ComGuard {
    fn new() -> Result<Self, String> {
        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if hr.is_err() {
                let err = hr.0;
                // S_FALSE (1) — COM уже инициализирован в этом потоке, это ок.
                // RPC_E_CHANGED_MODE (0x80010106) — другой режим, тоже ок.
                if err != 0x00000001 && err != 0x80010106u32 as i32 {
                    return Err(format!("CoInitializeEx failed: 0x{:08X}", err));
                }
            }
        }
        Ok(ComGuard)
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

/// Создаёт правило блокировки (выключенное) для пользователя.
pub fn ensure_block_rule(sid: &str) -> Result<(), String> {
    let name = rule_name(sid);

    if rule_exists(&name) {
        return Ok(());
    }

    log_info!("[Firewall] Создаю правило блокировки: {}", name);

    let _guard = ComGuard::new()?;

    unsafe {
        let policy: INetFwPolicy2 = CoCreateInstance(&NetFwPolicy2, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| format!("CoCreateInstance policy: {e}"))?;
        let rules: INetFwRules = policy.Rules().map_err(|e| format!("policy.Rules(): {e}"))?;
        let rule: INetFwRule = CoCreateInstance(&NetFwRule, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| format!("CoCreateInstance rule: {e}"))?;

        rule.SetName(&BSTR::from(name.as_str()))
            .map_err(|e| format!("SetName: {e}"))?;
        rule.SetDirection(NET_FW_RULE_DIR_OUT)
            .map_err(|e| format!("SetDirection: {e}"))?;
        rule.SetAction(NET_FW_ACTION_BLOCK)
            .map_err(|e| format!("SetAction: {e}"))?;
        rule.SetEnabled(VARIANT_FALSE)
            .map_err(|e| format!("SetEnabled: {e}"))?;
        rule.SetProfiles(0x7)
            .map_err(|e| format!("SetProfiles: {e}"))?;
        rule.SetProtocol(NET_FW_IP_PROTOCOL_ANY.0)
            .map_err(|e| format!("SetProtocol: {e}"))?;

        let rule3: INetFwRule3 = rule.cast().map_err(|e| format!("cast INetFwRule3: {e}"))?;

        let sddl = format!("O:LSD:(A;;CC;;;{})", sid);
        rule3
            .SetLocalUserAuthorizedList(&BSTR::from(sddl.as_str()))
            .map_err(|e| format!("SetLocalUserAuthorizedList: {e}"))?;

        rules.Add(&rule).map_err(|e| format!("rules.Add: {e}"))?;
    }

    if !rule_exists(&name) {
        return Err("правило не найдено после создания".to_string());
    }

    log_info!("[Firewall] Правило создано: {}", name);
    Ok(())
}

/// Включает/выключает правило по имени.
pub fn set_block_enabled(sid: &str, enabled: bool) -> Result<(), String> {
    let name = rule_name(sid);
    log_info!(
        "[Firewall] {} правило: {}",
        if enabled {
            "Включаю"
        } else {
            "Выключаю"
        },
        name
    );

    let _guard = ComGuard::new()?;

    unsafe {
        let policy: INetFwPolicy2 = CoCreateInstance(&NetFwPolicy2, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| format!("CoCreateInstance: {e}"))?;
        let rules: INetFwRules = policy.Rules().map_err(|e| format!("Rules: {e}"))?;

        let rule: INetFwRule = rules
            .Item(&BSTR::from(name.as_str()))
            .map_err(|e| format!("rule not found: {e}"))?;

        let enabled_val = if enabled { VARIANT_TRUE } else { VARIANT_FALSE };
        rule.SetEnabled(enabled_val)
            .map_err(|e| format!("SetEnabled: {e}"))?;
    }

    Ok(())
}

/// Удаляет правило.
pub fn remove_block_rule(sid: &str) -> Result<(), String> {
    let name = rule_name(sid);

    if !rule_exists(&name) {
        return Ok(());
    }

    log_info!("[Firewall] Удаляю правило: {}", name);

    let _guard = ComGuard::new()?;

    unsafe {
        let policy: INetFwPolicy2 = CoCreateInstance(&NetFwPolicy2, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| format!("CoCreateInstance: {e}"))?;
        let rules: INetFwRules = policy.Rules().map_err(|e| format!("Rules: {e}"))?;

        rules
            .Remove(&BSTR::from(name.as_str()))
            .map_err(|e| format!("Remove: {e}"))?;
    }

    Ok(())
}

/// Проверяет, существует ли правило.
fn rule_exists(name: &str) -> bool {
    let _guard = match ComGuard::new() {
        Ok(g) => g,
        Err(_) => return false,
    };

    unsafe {
        let result = (|| -> Result<bool, String> {
            let policy: INetFwPolicy2 = CoCreateInstance(&NetFwPolicy2, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("CoCreateInstance: {e}"))?;
            let rules: INetFwRules = policy.Rules().map_err(|e| format!("Rules: {e}"))?;
            Ok(rules.Item(&BSTR::from(name)).is_ok())
        })();

        result.unwrap_or(false)
    }
}
