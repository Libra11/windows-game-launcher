use crate::model::Game;
use std::{
    collections::HashMap,
    path::PathBuf,
    process::{Child, Command},
    sync::{Mutex, OnceLock},
};
use tauri::Manager;

struct Probe {
    child: Child,
    directory: PathBuf,
}
static PROBES: OnceLock<Mutex<HashMap<String, Probe>>> = OnceLock::new();

pub(super) fn directory(app: &tauri::AppHandle, game: &Game) -> Result<PathBuf, String> {
    if !game
        .id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("游戏标识无效".into());
    }
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("xbox-local/64439fe5")
        .join(&game.id))
}

pub(super) fn ensure(app: &tauri::AppHandle, game: &Game) -> Result<(PathBuf, String), String> {
    let directory = directory(app, game)?;
    let mut probes = PROBES
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|_| "Xbox 捕获状态不可用")?;
    let live = if let Some(probe) = probes.get_mut(&game.id) {
        probe.child.try_wait().map_err(|e| e.to_string())?.is_none()
    } else {
        false
    };
    if !live {
        let previous =
            std::fs::read_to_string(directory.join("interface-ready.txt")).unwrap_or_default();
        if previous.starts_with("ERROR ") {
            return Err(previous.trim_start_matches("ERROR ").trim().to_owned());
        }
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        std::fs::write(directory.join("control.txt"), "running").map_err(|e| e.to_string())?;
        std::fs::write(directory.join("interface-ready.txt"), "PREPARING")
            .map_err(|e| e.to_string())?;
        let helper = app
            .path()
            .resolve(
                "resources/XboxLocalProbe.exe",
                tauri::path::BaseDirectory::Resource,
            )
            .map_err(|e| e.to_string())?;
        if !helper.is_file() {
            return Err("Xbox 捕获组件缺失".into());
        }
        let mut command = Command::new(helper);
        command
            .arg(&game.exe_path)
            .arg(&directory)
            .arg(std::process::id().to_string());
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        let child = command
            .spawn()
            .map_err(|e| format!("无法启动 Xbox 捕获组件：{e}"))?;
        probes.insert(
            game.id.clone(),
            Probe {
                child,
                directory: directory.clone(),
            },
        );
    }
    let ready = std::fs::read_to_string(directory.join("interface-ready.txt")).unwrap_or_default();
    if ready.starts_with("ERROR ") {
        return Err(format!(
            "Xbox 捕获失败：{}",
            ready.trim_start_matches("ERROR ").trim()
        ));
    }
    let message = if ready.starts_with("ARMED ") {
        "已读取 Xbox 本地事件；接口监听运行中，首次附加后 60 秒静默导入"
    } else {
        "Xbox 本地捕获等待游戏启动；请从启动器进入游戏"
    };
    Ok((directory.join("interface-events.log"), message.into()))
}

pub(crate) fn stop_all() {
    if let Some(probes) = PROBES.get() {
        if let Ok(probes) = probes.lock() {
            for probe in probes.values() {
                let _ = std::fs::write(probe.directory.join("control.txt"), "stop");
            }
        }
    }
}

pub(super) fn retain(ids: &[String]) {
    if let Some(probes) = PROBES.get() {
        if let Ok(mut probes) = probes.lock() {
            probes.retain(|id, probe| {
                if ids.contains(id) {
                    true
                } else {
                    let _ = std::fs::write(probe.directory.join("control.txt"), "stop");
                    false
                }
            });
        }
    }
}
