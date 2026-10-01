mod detection;
#[cfg(test)]
mod tests;
#[cfg(windows)]
mod windows;

use crate::{db, lock_db, model::Game, AppState};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Mutex, RwLock},
    time::Duration,
};
use tauri::{Emitter, Manager};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Checking,
    Installed,
    NotInstalled,
    #[cfg_attr(not(windows), allow(dead_code))]
    ClientMissing,
    Unknown,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub state: Status,
    pub reason: String,
    pub checked_at: String,
}
impl Info {
    fn new(state: Status, reason: impl Into<String>) -> Self {
        Self {
            state,
            reason: reason.into(),
            checked_at: chrono::Utc::now().to_rfc3339(),
        }
    }
    fn unknown(reason: impl Into<String>) -> Self {
        Self::new(Status::Unknown, reason)
    }
    pub fn require_installed(&self) -> Result<(), String> {
        if self.state == Status::Installed {
            Ok(())
        } else {
            Err(if self.reason.is_empty() {
                "尚未确认游戏安装，请稍后重试".into()
            } else {
                self.reason.clone()
            })
        }
    }
}

#[derive(Default)]
pub struct Monitor {
    // 串行扫描，防止后台旧结果覆盖启动前的重新检查；读取快照无需等待磁盘。
    scanning: Mutex<()>,
    snapshot: RwLock<HashMap<String, Info>>,
}
impl Monitor {
    pub fn info(&self, id: &str) -> Info {
        self.snapshot
            .read()
            .map(|items| items.get(id).cloned().unwrap_or_default())
            .unwrap_or_else(|_| Info::unknown("安装检测状态暂时不可用"))
    }
    pub fn refresh(&self, games: &[Game]) -> Result<bool, String> {
        let _scan = self.scanning.lock().map_err(|_| "安装检测暂时不可用")?;
        let next = if games.iter().any(|game| game.source == "steam") {
            detection::scan(games, locate())
        } else {
            HashMap::new()
        };
        self.replace(next)
    }
    fn replace(&self, next: HashMap<String, Info>) -> Result<bool, String> {
        let mut previous = self
            .snapshot
            .write()
            .map_err(|_| "安装检测状态暂时不可用")?;
        let changed = previous.len() != next.len()
            || next.iter().any(|(id, info)| {
                previous
                    .get(id)
                    .is_none_or(|old| old.state != info.state || old.reason != info.reason)
            });
        *previous = next;
        Ok(changed)
    }
    pub fn check_before_launch(&self, game: &Game) -> Result<Info, String> {
        self.check_with(game, locate)
    }
    fn check_with(
        &self,
        game: &Game,
        locate: impl FnOnce() -> Result<steamlocate::SteamDir, Info>,
    ) -> Result<Info, String> {
        let _scan = self.scanning.lock().map_err(|_| "安装检测暂时不可用")?;
        let mut next = detection::scan(std::slice::from_ref(game), locate());
        let info = next.remove(&game.id).ok_or("未取得 Steam 安装状态")?;
        self.snapshot
            .write()
            .map_err(|_| "安装检测状态暂时不可用")?
            .insert(game.id.clone(), info.clone());
        Ok(info)
    }
}

#[cfg(windows)]
fn locate() -> Result<steamlocate::SteamDir, Info> {
    windows::locate()
}
#[cfg(not(windows))]
fn locate() -> Result<steamlocate::SteamDir, Info> {
    Err(Info::unknown("Steam 本机安装检测仅在 Windows 桌面端提供"))
}

pub(crate) fn watch(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        let state = app.state::<AppState>();
        if let Ok(games) = lock_db(&state).and_then(|conn| db::games(&conn)) {
            match state.installation.refresh(&games) {
                Ok(true) => {
                    let _ = app.emit("library-changed", ());
                }
                Err(error) => eprintln!("安装状态检查失败：{error}"),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_secs(5));
    });
}
