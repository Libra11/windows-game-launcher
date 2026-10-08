use super::{aggregation, model::Query, store};
use crate::{
    activity, db,
    model::{AchievementDefinition, Game, UnlockEvent},
    steam_playtime,
};
use rusqlite::Connection;
use std::collections::BTreeMap;

fn fixture() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE games(id TEXT PRIMARY KEY,source TEXT,appid TEXT,title TEXT,exe_path TEXT,launch_uri TEXT,custom_unlock_path TEXT,metadata_json TEXT,scan_status TEXT DEFAULT '',source_file TEXT DEFAULT '',last_scan TEXT DEFAULT '',schema_source TEXT DEFAULT '');
         CREATE TABLE achievements(appid TEXT,api_name TEXT,name TEXT,description TEXT,icon TEXT,hidden INTEGER,PRIMARY KEY(appid,api_name));
         CREATE TABLE unlocks(game_id TEXT,api_name TEXT,source TEXT,unlocked_at TEXT,evidence TEXT,PRIMARY KEY(game_id,api_name));
         CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT);"
    ).unwrap();
    activity::initialize(&conn).unwrap();
    db::set_setting(
        &conn,
        "statistics_started_at",
        &chrono::Local::now().to_rfc3339(),
    )
    .unwrap();
    conn
}
fn query(source: &str) -> Query {
    Query {
        source: source.into(),
        period: "30".into(),
        day: None,
    }
}
fn game(conn: &Connection, id: &str, source: &str, schema: &str) -> Game {
    let game = Game {
        id: id.into(),
        source: source.into(),
        appid: id.into(),
        title: id.into(),
        schema_source: schema.into(),
        metadata_json: "{}".into(),
        ..Default::default()
    };
    db::upsert_game(conn, &game).unwrap();
    conn.execute(
        "UPDATE games SET schema_source=?1 WHERE id=?2",
        rusqlite::params![schema, id],
    )
    .unwrap();
    game
}
fn definitions(conn: &Connection, game: &Game, count: usize) {
    let values = (0..count)
        .map(|i| AchievementDefinition {
            api_name: i.to_string(),
            name: format!("成就{i}"),
            description: String::new(),
            icon: String::new(),
            hidden: false,
        })
        .collect::<Vec<_>>();
    for item in values {
        conn.execute(
            "INSERT INTO achievements VALUES(?1,?2,?3,?4,?5,?6)",
            rusqlite::params![
                game.appid,
                item.api_name,
                item.name,
                item.description,
                item.icon,
                item.hidden
            ],
        )
        .unwrap();
    }
}

#[test]
fn empty_unknown_and_cached_steam_time_are_distinct() {
    let conn = fixture();
    assert_eq!(
        aggregation::snapshot(&conn, &query("all"))
            .unwrap()
            .summary
            .game_count,
        0
    );
    let mut steam = game(&conn, "1", "steam", "Steam Web API（暂无成就）");
    assert!(aggregation::snapshot(&conn, &query("all"))
        .unwrap()
        .summary
        .steam_seconds
        .is_none());
    steam.metadata_json = r#"{"steamPlayedSeconds":3600}"#.into();
    steam_playtime::save_owned(&conn, "", &[steam]).unwrap();
    let stats = aggregation::snapshot(&conn, &query("all")).unwrap();
    assert_eq!(stats.summary.steam_seconds, Some(3600));
    assert!(stats.summary.steam_cached);
    assert!(stats.summary.completion_rate.is_none());
    assert_eq!(stats.games[0].definition_state, "none");
}

#[test]
fn complete_partial_and_manual_achievements_use_correct_denominators() {
    let conn = fixture();
    let full = game(&conn, "1", "steam", "Steam Web API");
    let partial = game(&conn, "2", "local", "本地记录（仅已解锁）");
    definitions(&conn, &full, 2);
    definitions(&conn, &partial, 1);
    for (id, api, source) in [
        ("1", "0", "steam"),
        ("1", "1", "manual"),
        ("2", "0", "local"),
    ] {
        db::add_unlock(&conn, id, api, source, &chrono::Utc::now().to_rfc3339(), "").unwrap();
    }
    let stats = aggregation::snapshot(&conn, &query("all")).unwrap();
    assert_eq!(stats.summary.unlocked, 3);
    assert_eq!(stats.summary.manual_unlocked, 1);
    assert_eq!(stats.summary.completed_games, 1);
    assert_eq!(stats.summary.completion_rate, Some(100.0));
    assert_eq!(
        stats
            .games
            .iter()
            .find(|g| g.game_id == "2")
            .unwrap()
            .completion_rate,
        None
    );
    assert_eq!(
        aggregation::snapshot(&conn, &query("local"))
            .unwrap()
            .summary
            .completed_games,
        0
    );
}

#[test]
fn historical_sessions_never_become_daily_records_and_new_checkpoints_are_idempotent() {
    let mut conn = fixture();
    game(&conn, "1", "local", "");
    activity::start(&mut conn, "old", "1", "2020-01-01T00:00:00Z").unwrap();
    activity::checkpoint(&mut conn, "old", 7200, true, &BTreeMap::new()).unwrap();
    let today = chrono::Local::now().date_naive().to_string();
    activity::start(&mut conn, "new", "1", &chrono::Local::now().to_rfc3339()).unwrap();
    let days = BTreeMap::from([(today.clone(), 60)]);
    activity::checkpoint(&mut conn, "new", 60, false, &days).unwrap();
    activity::checkpoint(&mut conn, "new", 60, false, &days).unwrap();
    let stats = aggregation::snapshot(&conn, &query("all")).unwrap();
    assert_eq!(stats.summary.local_seconds, 7260);
    assert_eq!(stats.period.seconds, 60);
    assert_eq!(stats.period.sessions, 1);
    assert!(stats
        .daily
        .iter()
        .filter(|d| d.date < today)
        .all(|d| !d.recorded));
    let all = Query {
        period: "all".into(),
        ..query("all")
    };
    let page = aggregation::session_page(&conn, &all, 1, 20).unwrap();
    assert_eq!(page.total, 2);
    assert!(!page.items[0].daily_recorded);
    let day = Query {
        day: Some(today),
        ..query("all")
    };
    assert_eq!(
        aggregation::session_page(&conn, &day, 0, 20).unwrap().total,
        1
    );
    assert!(aggregation::snapshot(&conn, &query("bad")).is_err());
}

#[test]
fn only_new_matching_automatic_unlock_events_enter_trends() {
    let conn = fixture();
    let steam = game(&conn, "1", "steam", "Steam Web API");
    definitions(&conn, &steam, 2);
    for api in ["0", "1"] {
        db::add_unlock(
            &conn,
            "1",
            api,
            "steam",
            &chrono::Utc::now().to_rfc3339(),
            "",
        )
        .unwrap();
    }
    assert_eq!(
        aggregation::snapshot(&conn, &query("all"))
            .unwrap()
            .period
            .new_unlocks,
        0
    );
    let event = UnlockEvent {
        game_id: "1".into(),
        game_title: "1".into(),
        api_name: "0".into(),
        achievement_name: "成就0".into(),
    };
    store::record_unlock(&conn, &event).unwrap();
    store::record_unlock(&conn, &event).unwrap();
    assert_eq!(
        aggregation::snapshot(&conn, &query("all"))
            .unwrap()
            .period
            .new_unlocks,
        1
    );
    conn.execute("UPDATE unlocks SET source='manual' WHERE api_name='0'", [])
        .unwrap();
    assert_eq!(
        aggregation::snapshot(&conn, &query("all"))
            .unwrap()
            .period
            .new_unlocks,
        0
    );
}

#[test]
fn official_first_snapshot_is_history_and_later_new_unlocks_are_recorded_once() {
    let mut conn = fixture();
    let steam = game(&conn, "1", "steam", "Steam Web API");
    definitions(&conn, &steam, 3);
    let now = chrono::Utc::now().to_rfc3339();
    store::save_steam_snapshot(&mut conn, &steam, &[("0".into(), now.clone())]).unwrap();
    assert_eq!(
        aggregation::snapshot(&conn, &query("all"))
            .unwrap()
            .period
            .new_unlocks,
        0
    );
    let snapshot = [
        ("0".into(), now.clone()),
        ("1".into(), now.clone()),
        ("2".into(), "2020-01-01T00:00:00Z".into()),
    ];
    store::save_steam_snapshot(&mut conn, &steam, &snapshot).unwrap();
    store::save_steam_snapshot(&mut conn, &steam, &snapshot).unwrap();
    assert_eq!(
        aggregation::snapshot(&conn, &query("all"))
            .unwrap()
            .period
            .new_unlocks,
        1
    );
    let day = Query {
        day: Some(chrono::Local::now().date_naive().to_string()),
        ..query("all")
    };
    assert_eq!(
        aggregation::snapshot(&conn, &day)
            .unwrap()
            .day_unlocks
            .len(),
        1
    );
}

#[test]
fn failed_daily_write_rolls_back_the_playtime_checkpoint() {
    let mut conn = fixture();
    game(&conn, "1", "local", "");
    activity::start(&mut conn, "new", "1", &chrono::Utc::now().to_rfc3339()).unwrap();
    let invalid = BTreeMap::from([("2026-10-02".into(), u64::MAX)]);
    assert!(activity::checkpoint(&mut conn, "new", 60, false, &invalid).is_err());
    assert_eq!(activity::all(&conn).unwrap()["1"].played_seconds, 0);
    assert!(store::session_days(&conn, "new").unwrap().is_empty());
    let valid = BTreeMap::from([("2026-10-02".into(), 60)]);
    activity::checkpoint(&mut conn, "new", 60, false, &valid).unwrap();
    assert_eq!(activity::all(&conn).unwrap()["1"].played_seconds, 60);
}

#[test]
fn epic_events_match_epic_definitions_and_do_not_mix_with_steam() {
    let conn = fixture();
    let epic = game(&conn, "3", "epic", "Epic 官方成就");
    conn.execute(
        "INSERT INTO achievements VALUES('epic:3','FIRST','Epic 成就','','',0)",
        [],
    )
    .unwrap();
    store::official_changes(&conn, &epic, &[]).unwrap();
    db::add_unlock(
        &conn,
        "3",
        "FIRST",
        "epic",
        &chrono::Utc::now().to_rfc3339(),
        "",
    )
    .unwrap();
    store::official_changes(&conn, &epic, &[]).unwrap();
    store::official_changes(&conn, &epic, &[]).unwrap();
    let stats = aggregation::snapshot(&conn, &query("epic")).unwrap();
    assert_eq!(stats.period.new_unlocks, 1);
    assert_eq!(stats.summary.completed_games, 1);
    assert_eq!(stats.recent_unlocks[0].name, "Epic 成就");
    assert_eq!(
        aggregation::snapshot(&conn, &query("steam"))
            .unwrap()
            .period
            .new_unlocks,
        0
    );
}
