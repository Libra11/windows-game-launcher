use crate::model::Game;
use serde::Serialize;
use std::{
    collections::HashMap,
    path::Path,
    time::{Duration, Instant},
};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
mod clock;
mod daily;
mod launch;
mod recovery;
pub(crate) use recovery::Record as RecoveryRecord;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub state: String,
    pub message: String,
    pub elapsed_seconds: u64,
}
impl Default for RuntimeInfo {
    fn default() -> Self {
        Self {
            state: "idle".into(),
            message: String::new(),
            elapsed_seconds: 0,
        }
    }
}

#[derive(Clone)]
pub struct Update {
    pub game_id: String,
    pub session_id: String,
    pub started_at: Option<String>,
    pub seconds: u64,
    pub finished: bool,
    pub failed: bool,
    pub daily: std::collections::BTreeMap<String, u64>,
}

struct Session {
    id: String,
    game: Game,
    requested: Instant,
    launch_pending: bool,
    started: Option<Instant>,
    pending_start: Option<String>,
    known: HashMap<Pid, u64>,
    baseline: HashMap<Pid, u64>,
    empty_ticks: u8,
    credited: u64,
    clock: clock::Clock,
    daily: daily::DailyClock,
}

pub struct Tracker {
    system: System,
    sessions: HashMap<String, Session>,
    last: HashMap<String, RuntimeInfo>,
}
impl Default for Tracker {
    fn default() -> Self {
        Self {
            system: System::new(),
            sessions: HashMap::new(),
            last: HashMap::new(),
        }
    }
}
impl Tracker {
    fn refresh_processes(&mut self, steam: bool) {
        let mut kind = ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet);
        if steam {
            kind = kind.with_environ(UpdateKind::OnlyIfNotSet);
        }
        self.system
            .refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    }
    pub fn info(&self, game_id: &str) -> RuntimeInfo {
        if let Some(session) = self.sessions.get(game_id) {
            return RuntimeInfo {
                state: if session.started.is_some() {
                    "running"
                } else {
                    "starting"
                }
                .into(),
                message: if session.started.is_some() {
                    "游戏正在运行"
                } else {
                    "等待游戏进程启动"
                }
                .into(),
                elapsed_seconds: session.clock.seconds(),
            };
        }
        self.last.get(game_id).cloned().unwrap_or_default()
    }
    pub fn reserve(&mut self, game: &Game) -> Result<bool, String> {
        if self.sessions.contains_key(&game.id) {
            return Err("游戏已在启动或运行中，请勿重复启动".into());
        }
        self.refresh_processes(game.source == "steam");
        let known: HashMap<_, _> = self
            .system
            .processes()
            .iter()
            .filter(|(_, process)| {
                !helper_process(process)
                    && ((!game.exe_path.is_empty()
                        && process
                            .exe()
                            .is_some_and(|path| same_path(path, Path::new(&game.exe_path))))
                        || (game.source == "steam" && steam_process(process, &game.appid)))
            })
            .map(|(pid, process)| (*pid, process.start_time()))
            .collect();
        let already_running = !known.is_empty();
        self.sessions.insert(
            game.id.clone(),
            Session {
                id: uuid::Uuid::new_v4().to_string(),
                game: game.clone(),
                requested: Instant::now(),
                launch_pending: !already_running,
                started: already_running.then(Instant::now),
                pending_start: already_running.then(|| chrono::Utc::now().to_rfc3339()),
                known,
                baseline: self
                    .system
                    .processes()
                    .iter()
                    .map(|(pid, process)| (*pid, process.start_time()))
                    .collect(),
                empty_ticks: 0,
                credited: 0,
                clock: clock::Clock::new(0, already_running),
                daily: daily::DailyClock::new(0, Default::default()),
            },
        );
        self.last.remove(&game.id);
        Ok(!already_running)
    }
    pub fn attach(&mut self, game_id: &str, pid: Option<u32>) {
        self.refresh_processes(
            self.sessions
                .get(game_id)
                .is_some_and(|session| session.game.source == "steam"),
        );
        if let Some(session) = self.sessions.get_mut(game_id) {
            session.launch_pending = false;
            session.requested = Instant::now();
            if let Some(pid) = pid {
                let pid = Pid::from_u32(pid);
                if let Some(process) = self.system.process(pid) {
                    session.known.insert(pid, process.start_time());
                }
            }
        }
    }
    pub fn fail(&mut self, game_id: &str, message: &str) {
        self.sessions.remove(game_id);
        self.last.insert(
            game_id.into(),
            RuntimeInfo {
                state: "error".into(),
                message: message.into(),
                elapsed_seconds: 0,
            },
        );
    }
    pub fn uncredited_seconds(&self, game_id: &str) -> u64 {
        self.sessions
            .get(game_id)
            .and_then(|session| {
                session
                    .started
                    .map(|_| session.clock.seconds().saturating_sub(session.credited))
            })
            .unwrap_or(0)
    }
    pub fn forget(&mut self, game_id: &str) {
        self.last.remove(game_id);
    }
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
    pub fn tick(&mut self) -> Vec<Update> {
        if self.sessions.is_empty() {
            return Vec::new();
        }
        self.refresh_processes(
            self.sessions
                .values()
                .any(|session| session.game.source == "steam"),
        );
        let mut updates = Vec::new();
        let mut ended = Vec::new();
        for (game_id, session) in &mut self.sessions {
            // 保留已发现的子进程，启动程序退出后仍可跟踪真正的游戏。
            loop {
                let before = session.known.len();
                for (pid, process) in self.system.processes() {
                    if session.baseline.get(pid) == Some(&process.start_time())
                        || helper_process(process)
                    {
                        continue;
                    }
                    let child = process.parent().is_some_and(|parent| {
                        session.known.get(&parent).is_some_and(|birth| {
                            self.system
                                .process(parent)
                                .is_some_and(|parent| parent.start_time() == *birth)
                        })
                    });
                    let exact = !session.game.exe_path.is_empty()
                        && process
                            .exe()
                            .is_some_and(|path| same_path(path, Path::new(&session.game.exe_path)));
                    let steam = session.game.source == "steam"
                        && steam_process(process, &session.game.appid);
                    if child || exact || steam {
                        session.known.insert(*pid, process.start_time());
                    }
                }
                if before == session.known.len() {
                    break;
                }
            }
            let alive = session.known.iter().any(|(pid, birth)| {
                self.system
                    .process(*pid)
                    .is_some_and(|p| p.start_time() == *birth && !helper_process(p))
            });
            session.clock.sample(alive);
            if alive {
                session.empty_ticks = 0;
                if session.started.is_none() {
                    session.started = Some(Instant::now());
                    session.pending_start = Some(chrono::Utc::now().to_rfc3339());
                }
            } else {
                session.empty_ticks = session.empty_ticks.saturating_add(1);
            }
            let failed = !session.launch_pending
                && session.started.is_none()
                && session.requested.elapsed() >= Duration::from_secs(90);
            let finished = session.started.is_some() && session.empty_ticks >= 2;
            let seconds = session.clock.seconds();
            session
                .daily
                .sample(seconds, chrono::Local::now().fixed_offset());
            if session.pending_start.is_some()
                || finished
                || failed
                || seconds.saturating_sub(session.credited) >= 5
            {
                updates.push(Update {
                    game_id: game_id.clone(),
                    session_id: session.id.clone(),
                    started_at: session.pending_start.take(),
                    seconds,
                    finished,
                    failed,
                    daily: session.daily.days.clone(),
                });
                session.credited = seconds;
            }
            if finished || failed {
                self.last.insert(
                    game_id.clone(),
                    RuntimeInfo {
                        state: if failed { "unconfirmed" } else { "idle" }.into(),
                        message: if failed {
                            "未确认游戏进程。请检查是否安装，或在编辑游戏中指定实际启动程序。"
                        } else {
                            "游戏已结束"
                        }
                        .into(),
                        elapsed_seconds: 0,
                    },
                );
                ended.push(game_id.clone());
            }
        }
        for id in ended {
            self.sessions.remove(&id);
        }
        updates
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .replace('\\', "/")
        .eq_ignore_ascii_case(&right.to_string_lossy().replace('\\', "/"))
}
fn helper_process(process: &sysinfo::Process) -> bool {
    let name = process.name().to_string_lossy().to_ascii_lowercase();
    [
        "steam.exe",
        "steamwebhelper.exe",
        "gameoverlayui.exe",
        "steamservice.exe",
    ]
    .contains(&name.as_str())
        || name.contains("crashpad")
        || name.contains("crashreport")
}
fn steam_process(process: &sysinfo::Process, appid: &str) -> bool {
    !helper_process(process)
        && process.environ().iter().any(|value| {
            let text = value.to_string_lossy();
            text.split_once('=').is_some_and(|(key, value)| {
                (key.eq_ignore_ascii_case("SteamAppId") || key.eq_ignore_ascii_case("SteamGameId"))
                    && value == appid
            })
        })
}

pub fn spawn(game: &Game) -> Result<Option<u32>, String> {
    if game.source == "local" {
        launch::local(Path::new(&game.exe_path)).map(Some)
    } else {
        #[cfg(target_os = "windows")]
        let child = std::process::Command::new("explorer.exe")
            .arg(&game.launch_uri)
            .spawn();
        #[cfg(not(target_os = "windows"))]
        let child = std::process::Command::new("open")
            .arg(&game.launch_uri)
            .spawn();
        let mut child = child.map_err(|e| format!("无法打开游戏客户端：{e}"))?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn game(exe: &Path) -> Game {
        Game {
            id: "fixture".into(),
            source: "local".into(),
            appid: String::new(),
            title: "进程测试".into(),
            exe_path: exe.to_string_lossy().into_owned(),
            launch_uri: String::new(),
            custom_unlock_path: String::new(),
            metadata_json: "{}".into(),
            scan_status: String::new(),
            source_file: String::new(),
            last_scan: String::new(),
            schema_source: String::new(),
        }
    }
    #[test]
    fn process_fixture() {
        if std::env::var_os("LAUNCHER_PROCESS_FIXTURE").is_some() {
            std::thread::sleep(Duration::from_secs(3));
        }
    }
    #[test]
    fn tracks_real_process_and_prevents_duplicate_launch() {
        let root = std::env::temp_dir().join(format!("launcher-process-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let exe = root.join(if cfg!(windows) {
            "fixture.exe"
        } else {
            "fixture"
        });
        std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
        let game = game(&exe);
        let mut tracker = Tracker::default();
        assert!(tracker.reserve(&game).unwrap());
        assert_eq!(tracker.info(&game.id).state, "starting");
        assert!(tracker.reserve(&game).is_err());
        let mut child = std::process::Command::new(&exe)
            .args(["--exact", "runtime::tests::process_fixture"])
            .env("LAUNCHER_PROCESS_FIXTURE", "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        tracker.attach(&game.id, Some(child.id()));
        let started = tracker.tick();
        assert_eq!(tracker.info(&game.id).state, "running");
        assert_eq!(started.len(), 1);
        assert!(started[0].started_at.is_some());
        assert!(tracker.tick().is_empty());
        let mut existing = Tracker::default();
        assert!(!existing.reserve(&game).unwrap());
        assert_eq!(existing.info(&game.id).state, "running");
        let mut finished = Vec::new();
        while child.try_wait().unwrap().is_none() {
            finished.extend(tracker.tick().into_iter().filter(|update| update.finished));
            std::thread::sleep(Duration::from_millis(100));
        }
        for _ in 0..2 {
            finished.extend(tracker.tick().into_iter().filter(|update| update.finished));
        }
        assert_eq!(finished.len(), 1);
        assert!(finished[0].finished);
        assert!(finished[0].seconds >= 2);
        assert_eq!(finished[0].daily.values().sum::<u64>(), finished[0].seconds);
        assert_eq!(tracker.info(&game.id).state, "idle");
        assert!(tracker.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn launch_timeout_starts_after_authorization_returns() {
        let game = game(Path::new("missing-game.exe"));
        let mut tracker = Tracker::default();
        assert!(tracker.reserve(&game).unwrap());
        tracker.sessions.get_mut(&game.id).unwrap().requested =
            Instant::now() - Duration::from_secs(91);
        assert!(tracker.tick().is_empty());
        assert_eq!(tracker.info(&game.id).state, "starting");

        tracker.attach(&game.id, None);
        assert!(tracker.tick().is_empty());
        tracker.sessions.get_mut(&game.id).unwrap().requested =
            Instant::now() - Duration::from_secs(91);
        assert!(tracker.tick()[0].failed);
        assert_eq!(tracker.info(&game.id).state, "unconfirmed");
    }
    #[test]
    fn failed_launch_does_not_stay_busy() {
        let mut game = game(Path::new("missing-game.exe"));
        game.launch_uri = "missing-shortcut.lnk".into();
        let mut tracker = Tracker::default();
        assert!(tracker.reserve(&game).unwrap());
        assert_eq!(
            spawn(&game).unwrap_err(),
            "游戏启动文件不存在，请在编辑游戏中重新选择"
        );
        tracker.fail(&game.id, "启动文件不存在");
        assert_eq!(tracker.info(&game.id).state, "error");
        assert!(tracker.reserve(&game).unwrap());
    }
}
