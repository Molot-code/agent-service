// src/scheduler.rs
use windows::{
    core::{Interface, BSTR},
    Win32::System::{
        Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        },
        TaskScheduler::{
            IRegisteredTask, ITaskDefinition, ITaskFolder, ITaskService, TASK_CREATE_OR_UPDATE, TASK_LOGON_SERVICE_ACCOUNT,
            TASK_RUNLEVEL_HIGHEST,
        },
        Variant::VARIANT,
    },
};

// const CLSID_TASK_SERVICE: GUID = GUID::from_u128(0x0f87369f_a4e5_4cfc_bd3e_73e6154572dd);

const CLSID_TASK_SERVICE: windows::core::GUID =
    windows::core::GUID::from_u128(0x0f87369f_a4e5_4cfc_bd3e_73e6154572dd);

use crate::{log_error, log_info};

const TASK_NAME: &str = "ParentalControl_Watchdog";
const TASK_FOLDER: &str = "\\";

struct ComGuard;

impl ComGuard {
    fn new() -> Result<Self, String> {
        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if hr.is_err() {
                let err = hr.0;
                if err != 0x00000001 && err != 0x80010106u32 as i32 {
                    return Err(format!("CoInitializeEx: 0x{:08X}", err));
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

/// Создать или обновить задачу в планировщике.
pub fn ensure_watchdog_task(interval_minutes: u32, service_name: &str) -> Result<(), String> {
    let _guard = ComGuard::new()?;

    unsafe {
        let service: ITaskService =
            CoCreateInstance(&CLSID_TASK_SERVICE, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("CoCreateInstance: {e}"))?;

        // Подключение к локальному планировщику
        let empty = VARIANT::default();
        service
            .Connect(&empty, &empty, &empty, &empty)
            .map_err(|e| format!("Connect: {e}"))?;

        let root: ITaskFolder = service
            .GetFolder(&BSTR::from(TASK_FOLDER))
            .map_err(|e| format!("GetFolder: {e}"))?;

        // Пробуем удалить существующую задачу
        let _ = root.DeleteTask(&BSTR::from(TASK_NAME), 0);

        // Создаём новое определение задачи
        let task: ITaskDefinition = service.NewTask(0).map_err(|e| format!("NewTask: {e}"))?;

        // Регистрационная информация
        let reg_info = task
            .RegistrationInfo()
            .map_err(|e| format!("RegistrationInfo: {e}"))?;
        reg_info
            .SetAuthor(&BSTR::from("ParentalControl"))
            .map_err(|e| format!("SetAuthor: {e}"))?;
        reg_info
            .SetDescription(&BSTR::from("Проверка и запуск службы ParentalControl"))
            .map_err(|e| format!("SetDescription: {e}"))?;

        // Настройки: разрешаем запуск по требованию, не останавливаем при батарее
        let settings = task.Settings().map_err(|e| format!("Settings: {e}"))?;
        settings
            .SetAllowDemandStart(true.into())
            .map_err(|e| format!("SetAllowDemandStart: {e}"))?;
        settings
            .SetDisallowStartIfOnBatteries(false.into())
            .map_err(|e| format!("SetDisallowStartIfOnBatteries: {e}"))?;
        settings
            .SetStopIfGoingOnBatteries(false.into())
            .map_err(|e| format!("SetStopIfGoingOnBatteries: {e}"))?;
        settings
            .SetStartWhenAvailable(true.into())
            .map_err(|e| format!("SetStartWhenAvailable: {e}"))?;

        // Триггер: повторение каждые N минут, начиная с текущего момента
        let triggers = task.Triggers().map_err(|e| format!("Triggers: {e}"))?;
        let trigger = triggers
            .Create(windows::Win32::System::TaskScheduler::TASK_TRIGGER_TIME)
            .map_err(|e| format!("Create trigger: {e}"))?;

        let time_trigger: windows::Win32::System::TaskScheduler::ITimeTrigger = trigger
            .cast()
            .map_err(|e| format!("cast ITimeTrigger: {e}"))?;

        // StartBoundary: сейчас + 1 минута, в формате ISO 8601
        let start = chrono::Local::now() + chrono::Duration::minutes(1);
        time_trigger
            .SetStartBoundary(&BSTR::from(start.format("%Y-%m-%dT%H:%M:%S").to_string()))
            .map_err(|e| format!("SetStartBoundary: {e}"))?;

        // Repetition: каждые N минут, без окончания
        let repetition = time_trigger
            .Repetition()
            .map_err(|e| format!("Repetition: {e}"))?;
        repetition
            .SetInterval(&BSTR::from(format!("PT{}M", interval_minutes)))
            .map_err(|e| format!("SetInterval: {e}"))?;
        //        repetition
        //            .SetDuration(&BSTR::from("PT0S")) // 0 = бесконечно
        //            .map_err(|e| format!("SetDuration: {e}"))?;
        repetition
            .SetStopAtDurationEnd(false.into())
            .map_err(|e| format!("SetStopAtDurationEnd: {e}"))?;

        // Действие: запуск службы через sc.exe
        let actions = task.Actions().map_err(|e| format!("Actions: {e}"))?;
        let action = actions
            .Create(windows::Win32::System::TaskScheduler::TASK_ACTION_EXEC)
            .map_err(|e| format!("Create action: {e}"))?;
        let exec_action: windows::Win32::System::TaskScheduler::IExecAction = action
            .cast()
            .map_err(|e| format!("cast IExecAction: {e}"))?;

        exec_action
            .SetPath(&BSTR::from("sc.exe"))
            .map_err(|e| format!("SetPath: {e}"))?;
        exec_action
            .SetArguments(&BSTR::from(format!("start {}", service_name)))
            .map_err(|e| format!("SetArguments: {e}"))?;

        // Principal: SYSTEM, highest
        let principal = task.Principal().map_err(|e| format!("Principal: {e}"))?;
        principal
            .SetUserId(&BSTR::from("SYSTEM"))
            .map_err(|e| format!("SetUserId: {e}"))?;
        principal
            .SetLogonType(TASK_LOGON_SERVICE_ACCOUNT)
            .map_err(|e| format!("SetLogonType: {e}"))?;
        principal
            .SetRunLevel(TASK_RUNLEVEL_HIGHEST)
            .map_err(|e| format!("SetRunLevel: {e}"))?;

        // Регистрация
        let _registered: IRegisteredTask = root
            .RegisterTaskDefinition(
                &BSTR::from(TASK_NAME),
                &task,
                TASK_CREATE_OR_UPDATE.0,
                &VARIANT::default(), // SYSTEM не требует пароля
                &VARIANT::default(),
                TASK_LOGON_SERVICE_ACCOUNT,
                &VARIANT::default(),
            )
            .map_err(|e| format!("RegisterTaskDefinition: {e}"))?;

        log_info!(
            "[Scheduler] Задача '{}' создана/обновлена (интервал: {} мин)",
            TASK_NAME,
            interval_minutes
        );
    }

    Ok(())
}

/// Удалить задачу из планировщика.
pub fn remove_watchdog_task() -> Result<(), String> {
    let _guard = ComGuard::new()?;

    unsafe {
        let service: ITaskService =
            CoCreateInstance(&CLSID_TASK_SERVICE, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("CoCreateInstance: {e}"))?;

        let empty = VARIANT::default();
        service
            .Connect(&empty, &empty, &empty, &empty)
            .map_err(|e| format!("Connect: {e}"))?;

        let root: ITaskFolder = service
            .GetFolder(&BSTR::from(TASK_FOLDER))
            .map_err(|e| format!("GetFolder: {e}"))?;

        match root.DeleteTask(&BSTR::from(TASK_NAME), 0) {
            Ok(_) => log_info!("[Scheduler] Задача '{}' удалена", TASK_NAME),
            Err(e) => {
                let code = e.code().0;
                // 0x80070002 = task not found — это ок
                if code != 0x80070002u32 as i32 {
                    return Err(format!("DeleteTask: {e}"));
                }
            }
        }
    }

    Ok(())
}

/// Синхронизировать задачу с настройками БД.
/// Вызывается при старте и при изменении настроек через API.
pub fn sync_watchdog_task(enabled: bool, interval_minutes: u32, service_name: &str) {
    let result = if enabled {
        ensure_watchdog_task(interval_minutes, service_name)
    } else {
        remove_watchdog_task()
    };

    if let Err(e) = result {
        log_error!("[Scheduler] sync failed: {}", e);
    }
}
