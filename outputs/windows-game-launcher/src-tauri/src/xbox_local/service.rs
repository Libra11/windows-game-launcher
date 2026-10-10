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
    let state=app.state::<crate::AppState>();
    let _gate=crate::maintenance::lock_operation(&state.operation_gate,&state.maintenance)?;
    if state.maintenance.load(std::sync::atomic::Ordering::Acquire) { return Err("更新或恢复期间不能启动捕获".into()); }
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

pub(crate) fn has_active_capture() -> bool {
    PROBES.get().is_some_and(|probes| {
        probes.lock().map(|mut probes| probes.values_mut().any(|probe| probe.child.try_wait().map_or(true, |status| status.is_none()))).unwrap_or(true)
    })
}

// 调用方持有 operation_gate 并已进入维护状态，停止期间不能创建新捕获。
// 使用组件自己的退出协议，让已附加组件恢复寄存器并解除附加，不强杀进程。
pub(crate) fn stop_for_update() -> Result<(), String> {
    let Some(probes) = PROBES.get() else { return Ok(()); };
    stop_probes(probes, std::time::Duration::from_secs(10))
}

fn stop_probes(probes: &Mutex<HashMap<String, Probe>>, timeout: std::time::Duration) -> Result<(), String> {
    {
        let mut probes = probes.lock().map_err(|_| "Xbox 捕获状态不可用")?;
        for probe in probes.values_mut() {
            if probe.child.try_wait().map_err(|e| format!("无法检查 Xbox 捕获状态：{e}"))?.is_none() {
                std::fs::write(probe.directory.join("control.txt"), "stop")
                    .map_err(|e| format!("无法请求 Xbox 捕获停止：{e}"))?;
            }
        }
    }
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let mut active = false;
        {
            let mut probes = probes.lock().map_err(|_| "Xbox 捕获状态不可用")?;
            for probe in probes.values_mut() {
                active |= probe.child.try_wait().map_err(|e| format!("无法检查 Xbox 捕获状态：{e}"))?.is_none();
            }
        }
        if !active { return Ok(()); }
        if std::time::Instant::now() >= deadline {
            return Err("成就捕获停止超时，本次更新未安装，请稍后重试".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn waiting_probe(directory: &std::path::Path, obey_stop: bool) -> Probe {
        use std::os::windows::process::CommandExt;
        let script = if obey_stop {
            "while ((Get-Content -LiteralPath $env:YOUJI_TEST_CONTROL -Raw).Trim() -eq 'running') { Start-Sleep -Milliseconds 20 }"
        } else {
            "Start-Sleep -Seconds 30"
        };
        let child = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("YOUJI_TEST_CONTROL", directory.join("control.txt"))
            .creation_flags(0x08000000)
            .spawn().unwrap();
        Probe { child, directory: directory.to_owned() }
    }

    #[test]
    fn waiting_component_receives_stop_and_exits_before_installation() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("control.txt"), "running").unwrap();
        let probes = Mutex::new(HashMap::from([("test".into(), waiting_probe(directory.path(), true))]));
        let result = stop_probes(&probes, Duration::from_secs(10));
        // 即使用例失败也回收测试组件，不触碰真实捕获或游戏。
        let mut probes = probes.lock().unwrap();
        let child = &mut probes.get_mut("test").unwrap().child;
        let exited = child.try_wait().unwrap().is_some();
        if !exited { let _ = child.kill(); let _ = child.wait(); }
        assert!(result.is_ok(), "{result:?}");
        assert!(exited);
        assert_eq!(std::fs::read_to_string(directory.path().join("control.txt")).unwrap(), "stop");
    }

    #[test]
    fn stop_timeout_rejects_installation_without_killing_component() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("control.txt"), "running").unwrap();
        let probes = Mutex::new(HashMap::from([("test".into(), waiting_probe(directory.path(), false))]));
        let result = stop_probes(&probes, Duration::ZERO);
        let mut probes = probes.lock().unwrap();
        let child = &mut probes.get_mut("test").unwrap().child;
        let alive = child.try_wait().unwrap().is_none();
        let _ = child.kill(); let _ = child.wait();
        assert!(result.unwrap_err().contains("停止超时"));
        assert!(alive);
    }
}
