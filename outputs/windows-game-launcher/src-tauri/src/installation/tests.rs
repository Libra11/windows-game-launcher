use super::{
    detection::{client_at, detect_game, scan},
    Info, Monitor, Status,
};
use crate::model::Game;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use steamlocate::Library;

struct Fixture {
    root: PathBuf,
    extra: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("launcher-install-{}", uuid::Uuid::new_v4()));
        let fixture = Self {
            extra: root.join("Second Library"),
            root,
        };
        fs::create_dir_all(fixture.root.join("steamapps/common")).unwrap();
        fs::create_dir_all(fixture.extra.join("steamapps/common")).unwrap();
        fs::write(fixture.root.join("steam.exe"), "fixture").unwrap();
        let escape = |path: &Path| path.to_string_lossy().replace('\\', "\\\\");
        fs::write(
            fixture.root.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} \"1\" {{ \"path\" \"{}\" }} }}",
                escape(&fixture.root),
                escape(&fixture.extra)
            ),
        )
        .unwrap();
        fixture
    }
    fn app(&self, library: &Path, flags: u64, directory: bool) {
        fs::write(library.join("steamapps/appmanifest_480.acf"), format!(
            "\"AppState\" {{ \"appid\" \"480\" \"installdir\" \"Test Game\" \"StateFlags\" \"{flags}\" }}")).unwrap();
        if directory {
            fs::create_dir_all(library.join("steamapps/common/Test Game")).unwrap();
        }
    }
    fn info(&self) -> Info {
        let game = Game {
            id: "steam-480".into(),
            source: "steam".into(),
            appid: "480".into(),
            ..Default::default()
        };
        scan(&[game], client_at(Some(&self.root)))
            .remove("steam-480")
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn detects_client_missing_and_uninstalled_game_separately() {
    assert_eq!(client_at(None).unwrap_err().state, Status::ClientMissing);
    let fixture = Fixture::new();
    assert_eq!(fixture.info().state, Status::NotInstalled);
    fs::remove_file(fixture.root.join("steam.exe")).unwrap();
    assert_eq!(fixture.info().state, Status::ClientMissing);
}

#[test]
fn finds_installed_game_in_second_library_and_rechecks_uninstall() {
    let fixture = Fixture::new();
    fixture.app(&fixture.extra, 4, true);
    let installed = fixture.info();
    assert_eq!(installed.state, Status::Installed);
    assert!(!installed.checked_at.is_empty());
    assert!(installed.require_installed().is_ok());
    // 直接删除清单后重扫，不依赖 libraryfolders 中可能过期的 apps 列表。
    fs::remove_file(fixture.extra.join("steamapps/appmanifest_480.acf")).unwrap();
    let uninstalled = fixture.info();
    assert_eq!(uninstalled.state, Status::NotInstalled);
    assert!(uninstalled.require_installed().is_err());
}

#[test]
fn incomplete_download_and_missing_directory_cannot_launch() {
    let fixture = Fixture::new();
    fixture.app(&fixture.root, 1026, true);
    assert_eq!(fixture.info().state, Status::NotInstalled);
    assert!(fixture.info().reason.contains("安装尚未完成"));
    fixture.app(&fixture.root, 4 | 1 << 17, true);
    assert_eq!(fixture.info().state, Status::NotInstalled);
    fixture.app(&fixture.root, 4, false);
    fs::remove_dir(fixture.root.join("steamapps/common/Test Game")).unwrap();
    assert_eq!(fixture.info().state, Status::NotInstalled);
    assert!(fixture.info().reason.contains("目录缺失"));
}

#[test]
fn malformed_manifest_or_library_list_is_unknown() {
    let fixture = Fixture::new();
    fs::write(
        fixture.root.join("steamapps/appmanifest_480.acf"),
        "not a manifest",
    )
    .unwrap();
    assert_eq!(fixture.info().state, Status::Unknown);
    assert!(fixture.info().reason.contains("清单"));
    fixture.app(&fixture.root, 4, true);
    fs::write(
        fixture.root.join("steamapps/libraryfolders.vdf"),
        "not a library list",
    )
    .unwrap();
    assert_eq!(fixture.info().state, Status::Unknown);
}

#[test]
fn disconnected_or_unreadable_library_does_not_claim_uninstalled() {
    let fixture = Fixture::new();
    fs::remove_dir_all(&fixture.extra).unwrap();
    assert_eq!(fixture.info().state, Status::Unknown);
    // 已在可访问库中确认安装的游戏，不受其他离线库影响。
    fixture.app(&fixture.root, 4, true);
    assert_eq!(fixture.info().state, Status::Installed);
    let libraries = [
        Ok(Library::from_dir(&fixture.root).unwrap()),
        Err("权限不足，无法读取另一游戏库".into()),
    ];
    let denied = detect_game("999", &libraries);
    assert_eq!(denied.state, Status::Unknown);
    assert!(denied.reason.contains("权限不足"));
}

#[test]
fn missing_flags_and_wrong_appid_are_unknown() {
    let fixture = Fixture::new();
    for manifest in [
        "\"AppState\" { \"appid\" \"480\" \"installdir\" \"Test Game\" }",
        "\"AppState\" { \"appid\" \"999\" \"installdir\" \"Test Game\" \"StateFlags\" \"4\" }",
    ] {
        fs::write(fixture.root.join("steamapps/appmanifest_480.acf"), manifest).unwrap();
        assert_eq!(fixture.info().state, Status::Unknown);
    }
}

#[test]
fn timestamps_do_not_trigger_change_events_and_local_games_are_excluded() {
    let monitor = Monitor::default();
    assert_eq!(monitor.info("steam-480").state, Status::Checking);
    let installed = Info::new(Status::Installed, "已安装");
    assert!(monitor
        .replace(HashMap::from([("steam-480".into(), installed.clone())]))
        .unwrap());
    let mut same = installed;
    same.checked_at = "later".into();
    assert!(!monitor
        .replace(HashMap::from([("steam-480".into(), same)]))
        .unwrap());
    assert!(monitor
        .replace(HashMap::from([(
            "steam-480".into(),
            Info::new(Status::NotInstalled, "未安装")
        )]))
        .unwrap());
    let local = Game {
        source: "local".into(),
        ..Default::default()
    };
    assert!(scan(&[local], Err(Info::unknown("不应影响本地游戏"))).is_empty());
}

#[test]
fn launch_recheck_replaces_stale_cache_without_affecting_other_games() {
    let fixture = Fixture::new();
    fixture.app(&fixture.root, 4, true);
    let monitor = Monitor::default();
    let game = Game {
        id: "steam-480".into(),
        source: "steam".into(),
        appid: "480".into(),
        ..Default::default()
    };
    monitor
        .replace(HashMap::from([(
            "another".into(),
            Info::new(Status::Installed, "已安装"),
        )]))
        .unwrap();
    let read = || client_at(Some(&fixture.root));
    assert_eq!(
        monitor.check_with(&game, read).unwrap().state,
        Status::Installed
    );
    fs::remove_file(fixture.root.join("steamapps/appmanifest_480.acf")).unwrap();
    // 缓存仍是已安装；启动前的检查必须读取新清单，而不是复用缓存。
    assert_eq!(monitor.info(&game.id).state, Status::Installed);
    assert!(monitor
        .check_with(&game, read)
        .unwrap()
        .require_installed()
        .is_err());
    assert_eq!(monitor.info(&game.id).state, Status::NotInstalled);
    assert_eq!(monitor.info("another").state, Status::Installed);
    fixture.app(&fixture.root, 4, true);
    assert!(monitor
        .check_with(&game, read)
        .unwrap()
        .require_installed()
        .is_ok());
}
