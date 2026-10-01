use super::*;
use crate::{achievement_platform, installation, model::AchievementDefinition, runtime};
use std::{collections::HashSet, fs, sync::Mutex};

#[test]
fn ini_history_new_unlock_duplicate_write_and_wrong_game() {
    let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    let public = root.join("Public");
    let file = public.join("Documents/Steam/RUNE/2456740/achievements.ini");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, include_str!("../fixtures/runtime-rune.ini")).unwrap();
    fs::write(root.join("steam_emu.ini"), "[Settings]\nAppId=2456740\n").unwrap();
    let state = AppState {
        db: Mutex::new(db::open(&root.join("games.sqlite")).unwrap()),
        initialized: Mutex::new(HashSet::new()),
        metadata_refreshing: Mutex::new(false),
        runtime: Mutex::new(runtime::Tracker::default()),
        runtime_pending: Mutex::new(Vec::new()),
        installation: installation::Monitor::default(),
        appdata: None,
        public: Some(public),
    };
    let mut game = Game {
        id: "local-grime2".into(),
        source: "local".into(),
        appid: "2529790".into(),
        title: "尘埃异变2".into(),
        exe_path: root.join("GRIME II.exe").to_string_lossy().into(),
        schema_source: "Steam Web API".into(),
        ..Default::default()
    };
    {
        let mut conn = lock_db(&state).unwrap();
        db::upsert_game(&conn, &game).unwrap();
        achievement_platform::save(&conn, &game, "steam", "", true).unwrap();
        let definitions = [("APPETIZER", "开胃菜"), ("ANVIL", "铁砧")]
            .into_iter()
            .map(|(api_name, name)| AchievementDefinition {
                api_name: api_name.into(),
                name: name.into(),
                description: String::new(),
                icon: String::new(),
                hidden: false,
            })
            .collect::<Vec<_>>();
        db::save_definitions(
            &mut conn,
            &game.id,
            &game.appid,
            &definitions,
            "Steam Web API",
        )
        .unwrap();
    }
    assert!(scan_one(&state, &game, false).unwrap().is_empty());
    {
        let conn = lock_db(&state).unwrap();
        let achievements = db::achievements(&conn, &game.id).unwrap();
        let appetizer = achievements
            .iter()
            .find(|a| a.api_name == "APPETIZER")
            .unwrap();
        assert_eq!(appetizer.unlock_source.as_deref(), Some("local"));
        assert_eq!(
            appetizer.unlocked_at.as_deref(),
            Some("2026-10-01T04:03:48+00:00")
        );
        assert_eq!(
            db::game(&conn, &game.id).unwrap().unwrap().source_file,
            file.to_string_lossy()
        );
    }
    let next = format!(
        "{}\n[ANVIL]\nAchieved=1\nUnlockTime=1790827500\n",
        include_str!("../fixtures/runtime-rune.ini")
    );
    fs::write(&file, &next).unwrap();
    let events = scan_one(&state, &game, true).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].achievement_name, "铁砧");
    fs::write(&file, &next).unwrap();
    assert!(scan_one(&state, &game, true).unwrap().is_empty());
    let moved = root.join("moved.ini");
    fs::rename(&file, &moved).unwrap();
    game.custom_unlock_path = moved.to_string_lossy().into();
    db::upsert_game(&*lock_db(&state).unwrap(), &game).unwrap();
    assert!(scan_one(&state, &game, true).unwrap().is_empty());
    fs::write(&moved, "[WRONG_GAME]\nAchieved=1\nUnlockTime=1790827428").unwrap();
    assert!(scan_one(&state, &game, true).unwrap().is_empty());
    {
        let conn = lock_db(&state).unwrap();
        assert!(db::game(&conn, &game.id)
            .unwrap()
            .unwrap()
            .scan_status
            .contains("不匹配"));
        assert_eq!(
            db::achievements(&conn, &game.id)
                .unwrap()
                .iter()
                .filter(|a| a.unlocked_at.is_some())
                .count(),
            2
        );
        assert!(!db::achievement_api_names(&conn, &game.appid)
            .unwrap()
            .contains("WRONG_GAME"));
    }
    drop(state);
    fs::remove_dir_all(root).unwrap();
}
