use super::*;
use std::process::{Child, Command, Stdio};

struct Fixture {
    directory: tempfile::TempDir,
    children: Vec<Child>,
}
impl Fixture {
    fn new() -> Self { Self { directory:tempfile::tempdir().unwrap(), children:vec![] } }
    fn path(&self, name: &str) -> std::path::PathBuf { self.directory.path().join(name) }
    fn start(&mut self, name: &str, seconds: &str) -> u32 {
        let path = self.path(name);
        std::fs::copy(std::env::current_exe().unwrap(), &path).unwrap();
        let child = Command::new(path).args(["--exact", "runtime::launcher_tests::fixture_process"])
            .env("YOUJI_LAUNCHER_FIXTURE_SECONDS", seconds).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
        let pid = child.id(); self.children.push(child); pid
    }
    fn game(&self) -> Game {
        Game { id:"launcher-game".into(), source:"local".into(),
            exe_path:self.path("game.exe").to_string_lossy().into_owned(),
            launch_uri:self.path("launcher.exe").to_string_lossy().into_owned(), ..Default::default() }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) { for child in &mut self.children { let _=child.kill(); let _=child.wait(); } }
}

#[test]
fn fixture_process() {
    if let Ok(seconds) = std::env::var("YOUJI_LAUNCHER_FIXTURE_SECONDS") {
        std::thread::sleep(Duration::from_secs(seconds.parse().unwrap()));
    }
}

#[test]
fn launcher_wait_does_not_count_time_or_timeout_and_can_be_cancelled() {
    let mut fixture = Fixture::new();
    let launcher = fixture.start("launcher.exe", "30");
    let game = fixture.game();
    assert_eq!(game.local_launch_path(), game.launch_uri);
    let mut tracker = Tracker::default();
    assert!(tracker.reserve(&game).unwrap());
    tracker.attach(&game.id, Some(launcher));
    tracker.sessions.get_mut(&game.id).unwrap().requested = Instant::now() - Duration::from_secs(300);
    assert!(tracker.tick().is_empty());
    assert_eq!(tracker.info(&game.id).state, "starting");
    assert_eq!(tracker.info(&game.id).elapsed_seconds, 0);
    assert!(tracker.recovery_records().is_empty());
    assert!(tracker.reserve(&game).is_err());
    tracker.cancel_wait(&game.id).unwrap();
    assert!(tracker.is_empty());
    assert!(fixture.children[0].try_wait().unwrap().is_none());
}

#[test]
fn real_game_starts_and_finishes_independently_of_the_launcher() {
    let mut fixture = Fixture::new();
    let launcher = fixture.start("launcher.exe", "30");
    let game = fixture.game();
    let mut tracker = Tracker::default();
    tracker.reserve(&game).unwrap(); tracker.attach(&game.id, Some(launcher));
    assert!(tracker.tick().is_empty());
    fixture.start("game.exe", "3");
    let updates = tracker.tick();
    assert_eq!(tracker.info(&game.id).state, "running");
    assert!(updates.iter().any(|update| update.started_at.is_some()));
    assert!(tracker.cancel_wait(&game.id).is_err());
    let records = tracker.recovery_records();
    assert!(records[0].processes.iter().all(|process| process.pid != launcher));
    let mut recovered = Tracker::default();
    assert!(recovered.recover(&records[0], game.clone(), 0, Default::default()));
    let mut finished = Vec::new();
    while fixture.children[1].try_wait().unwrap().is_none() {
        finished.extend(tracker.tick().into_iter().filter(|update| update.finished));
        std::thread::sleep(Duration::from_millis(100));
    }
    for _ in 0..2 { finished.extend(tracker.tick().into_iter().filter(|update| update.finished)); }
    assert_eq!(finished.len(), 1);
    assert!(finished[0].seconds >= 2);
    assert!(fixture.children[0].try_wait().unwrap().is_none());
    assert!(tracker.is_empty());
    recovered.tick(); recovered.tick(); assert!(recovered.is_empty());
}

#[test]
fn an_existing_game_is_attached_without_launching_its_launcher() {
    let mut fixture = Fixture::new(); fixture.start("game.exe", "30");
    let game = fixture.game(); let mut tracker = Tracker::default();
    assert!(!tracker.reserve(&game).unwrap());
    assert_eq!(tracker.info(&game.id).state, "running");
    assert!(tracker.cancel_wait(&game.id).is_err());
    let mut direct = game; direct.launch_uri.clear();
    assert_eq!(direct.local_launch_path(), direct.exe_path);
}
