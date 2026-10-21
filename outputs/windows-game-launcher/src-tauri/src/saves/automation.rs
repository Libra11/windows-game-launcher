use super::{commands, directory, remote, store, tasks};
use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, SystemTime},
};
use tauri::Emitter;
fn stamps(root: &Path) -> Result<BTreeMap<String, (u64, SystemTime)>, String> {
    let root = youji_save_archive::validate_root(root)?;
    let mut queue = vec![root.clone()];
    let mut out = BTreeMap::new();
    let mut count = 0;
    while let Some(dir) = queue.pop() {
        for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
            let p = entry.map_err(|e| e.to_string())?.path();
            count += 1;
            if count > youji_save_archive::MAX_FILES * 2 {
                return Err("存档目录条目过多".into());
            }
            youji_save_archive::check_tree_path(&p)?;
            let m = std::fs::metadata(&p).map_err(|e| e.to_string())?;
            if m.is_dir() {
                queue.push(p);
            } else if m.is_file() {
                out.insert(
                    p.strip_prefix(&root)
                        .map_err(|e| e.to_string())?
                        .to_string_lossy()
                        .into_owned(),
                    (m.len(), m.modified().map_err(|e| e.to_string())?),
                );
            } else {
                return Err("存档包含非普通文件".into());
            }
        }
    }
    Ok(out)
}
fn stable(app: &tauri::AppHandle, root: &Path) -> Result<(), String> {
    let mut stability = Stability::default();
    for _ in 0..30 {
        crate::backup::require_games_idle(app)?;
        let now = stamps(root)?;
        if stability.observe(now) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    Err("存档仍在写入，稍后重试".into())
}
#[derive(Default)]
struct Stability {
    last: Option<BTreeMap<String, (u64, SystemTime)>>,
    count: u8,
}
impl Stability {
    fn observe(&mut self, now: BTreeMap<String, (u64, SystemTime)>) -> bool {
        self.count = if self.last.as_ref() == Some(&now) {
            self.count.saturating_add(1)
        } else {
            1
        };
        self.last = Some(now);
        self.count >= 3
    }
}
pub fn game_finished(app: &tauri::AppHandle, game: &str) {
    let result = (|| -> Result<(), String> {
        let c = store::open(&directory(app)?)?;
        for b in store::profiles(&c, game)? {
            if tasks::settings(&c, &b.id)?.auto_backup {
                tasks::enqueue(&c, &b.id, "backup")?;
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        let _ = app.emit("save-automation-error", error);
    }
}
fn transient(error: &str) -> bool {
    [
        "无法连接",
        "读取云端响应",
        "存档仍在写入",
        "存档在读取期间发生变化",
        "timed out",
        "连接",
        "HTTP 5",
    ]
    .iter()
    .any(|s| error.contains(s))
}
fn busy(error: &str) -> bool {
    ["运行", "捕获", "维护", "另一项", "暂时不可用"]
        .iter()
        .any(|s| error.contains(s))
}
pub fn start(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let initialize = (|| -> Result<(), String> {
            let c = store::open(&directory(&app)?)?;
            for b in tasks::all_profiles(&c)? {
                let s = tasks::settings(&c, &b.id)?;
                if s.auto_backup {
                    tasks::enqueue(&c, &b.id, "backup")?;
                }
                if s.auto_upload {
                    tasks::enqueue(&c, &b.id, "upload")?;
                }
            }
            Ok(())
        })();
        if let Err(e) = initialize {
            let _ = app.emit("save-automation-error", e);
        }
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let jobs = directory(&app)
                .and_then(|d| store::open(&d))
                .and_then(|c| tasks::list(&c));
            let jobs = match jobs {
                Ok(j) => j,
                Err(e) => {
                    let _ = app.emit("save-automation-error", e);
                    continue;
                }
            };
            for task in jobs
                .into_iter()
                .filter(|t| t.phase != "attention" && t.next_at <= chrono::Utc::now().timestamp())
            {
                let result = process(&app, &task).await;
                if let Ok(c) = directory(&app).and_then(|d| store::open(&d)) {
                    let saved = match result {
                        Ok(true) => tasks::finish(&c, &task),
                        Ok(false) => tasks::defer(&c, &task, "validating", 5, None),
                        Err(ref e) if busy(e) => tasks::defer(&c, &task, "waitingIdle", 5, Some(e)),
                        Err(ref e) if transient(e) => tasks::defer(
                            &c,
                            &task,
                            "retrying",
                            [5, 15, 60, 300][(task.attempts as usize).min(3)],
                            Some(e),
                        ),
                        Err(ref e) => {
                            let _ = app.emit(
                                "launcher-error",
                                format!(
                                    "游戏存档需要处理：{e}。请在游戏详情的存档页面查看并重试。"
                                ),
                            );
                            tasks::defer(&c, &task, "attention", 0, Some(e))
                        }
                    };
                    if let Err(e) = saved {
                        let _ = app.emit("save-automation-error", e);
                    }
                }
                let _ = app.emit("save-automation-changed", &task.profile_id);
            }
        }
    });
}
async fn process(app: &tauri::AppHandle, t: &tasks::Task) -> Result<bool, String> {
    let (b, s) = {
        let c = store::open(&directory(app)?)?;
        (
            store::binding(&c, &t.profile_id)?,
            tasks::settings(&c, &t.profile_id)?,
        )
    };
    if t.kind == "backup" {
        if !s.auto_backup {
            return Ok(true);
        }
        {
            let c = store::open(&directory(app)?)?;
            tasks::defer(&c, t, "waitingStable", 0, None)?;
        }
        let _ = app.emit("save-automation-changed", &t.profile_id);
        let worker = app.clone();
        let root = b.root.clone();
        tauri::async_runtime::spawn_blocking(move || stable(&worker, Path::new(&root)))
            .await
            .map_err(|e| e.to_string())??;
        commands::backup(app.clone(), b.id, true).await?;
        Ok(true)
    } else {
        let result = remote::upload(app.clone(), b.id, Some(t)).await?;
        Ok(result["pending"] != true)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn three_samples_and_changes_restart_stability() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("progress");
        std::fs::write(&p, b"one").unwrap();
        let mut s = Stability::default();
        assert!(!s.observe(stamps(d.path()).unwrap()));
        assert!(!s.observe(stamps(d.path()).unwrap()));
        std::fs::write(&p, b"changed").unwrap();
        assert!(!s.observe(stamps(d.path()).unwrap()));
        assert!(!s.observe(stamps(d.path()).unwrap()));
        assert!(s.observe(stamps(d.path()).unwrap()));
        std::fs::remove_file(p).unwrap();
        assert!(!s.observe(stamps(d.path()).unwrap()));
    }
}
