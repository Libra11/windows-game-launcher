use super::{auth::Session, catalog, store};
use crate::{activity, db, model::Game};
use serde_json::json;

fn session() -> Session {
    Session { token: String::new(), steam_id: "100".into(), expires_at: i64::MAX,
        family_name: "测试家庭".into(), family_id: Some("10".into()) }
}

fn response() -> serde_json::Value {
    json!({"apps":[
        {"appid":480,"app_type":1,"name":"Shared","owner_steamids":["200","300"],"exclude_reason":0,"rt_playtime":999999},
        {"appid":481,"app_type":1,"name":"Owned","owner_steamids":["100","200"]},
        {"appid":482,"name":"Excluded","exclude_reason":2},
        {"appid":483,"app_type":32,"owner_steamids":["200"]}
    ]})
}

#[test]
fn classifies_actual_ownership_and_skips_excluded_content_without_importing_member_time() {
    let catalog = catalog::parse(&response(), "100").unwrap();
    assert_eq!(catalog.items.len(), 2);
    assert!(catalog.items[0].shared);
    assert!(!catalog.items[1].shared);
    assert_eq!(catalog.excluded, 2);
    let mut conn = db::open(std::path::Path::new(":memory:")).unwrap();
    let result = store::save(&mut conn, &session(), catalog).unwrap();
    assert_eq!((result.added, result.shared, result.owned), (2, 1, 1));
    let shared = db::game(&conn, "steam-480").unwrap().unwrap();
    let info: serde_json::Value = serde_json::from_str(&shared.metadata_json).unwrap();
    assert!(info.get("steamPlayedSeconds").is_none());
    assert_eq!(shared.launch_uri, "steam://rungameid/480");
}

#[test]
fn rejects_incomplete_duplicate_and_unknown_authorization_snapshots() {
    assert!(catalog::parse(&json!({}), "100").is_err());
    assert!(catalog::parse(&json!({"apps":[{"appid":480,"app_type":1}]}), "100").is_err());
    let mut duplicate = response();
    let row = duplicate["apps"][0].clone();
    duplicate["apps"].as_array_mut().unwrap().push(row);
    assert!(catalog::parse(&duplicate, "100").is_err());
    let mut invalid = response();
    invalid["apps"][0]["exclude_reason"] = "unknown".into();
    assert!(catalog::parse(&invalid, "100").is_err());
}

#[test]
fn repeated_import_and_store_refresh_preserve_artwork_paths_favorites_and_shared_state() {
    let mut conn = db::open(std::path::Path::new(":memory:")).unwrap();
    let existing = Game { id:"steam-480".into(), source:"steam".into(), appid:"480".into(),
        title:"已有中文名称".into(), exe_path:"D:/game/game.exe".into(),
        launch_uri:"steam://rungameid/480".into(), metadata_json:r#"{"cover":"https://example.com/original.jpg","description":"已有简介"}"#.into(),
        ..Default::default() };
    db::upsert_game(&conn, &existing).unwrap();
    activity::set_favorite(&conn, &existing.id, true).unwrap();
    let result = store::save(&mut conn, &session(), catalog::parse(&response(), "100").unwrap()).unwrap();
    assert_eq!(result.added, 1);
    let saved = db::game(&conn, &existing.id).unwrap().unwrap();
    assert_eq!(saved.title, existing.title);
    assert_eq!(saved.exe_path, existing.exe_path);
    assert!(activity::all(&conn).unwrap()[&existing.id].favorite);
    let info: serde_json::Value = serde_json::from_str(&saved.metadata_json).unwrap();
    assert_eq!(info["cover"], "https://example.com/original.jpg");
    db::save_metadata(&conn, &existing.id, &json!({"name":"刷新资料","description":"新简介"})).unwrap();
    assert!(store::is_shared(&db::game(&conn, &existing.id).unwrap().unwrap()));
    assert_eq!(store::save(&mut conn, &session(), catalog::parse(&response(), "100").unwrap()).unwrap().added, 0);
}

#[test]
fn valid_empty_snapshot_disables_shared_launch_and_owned_confirmation_restores_access() {
    let mut conn = db::open(std::path::Path::new(":memory:")).unwrap();
    store::save(&mut conn, &session(), catalog::parse(&response(), "100").unwrap()).unwrap();
    let empty = catalog::parse(&json!({"apps":[]}), "100").unwrap();
    assert_eq!(store::save(&mut conn, &session(), empty).unwrap().unavailable, 1);
    let revoked = db::game(&conn, "steam-480").unwrap().unwrap();
    assert!(store::require_available(&revoked).is_err());
    assert!(db::game(&conn, "steam-481").unwrap().is_some());
    store::promote_owned(&conn, "480", "100").unwrap();
    let owned = db::game(&conn, "steam-480").unwrap().unwrap();
    assert!(!store::is_shared(&owned));
    assert!(store::require_available(&owned).is_ok());
}

#[test]
fn uses_personal_minutes_and_never_the_family_usage_summary() {
    let mut data = response();
    data["apps"][0]["rt_playtime"] = 3864.into();
    data["apps"][0]["seconds_played"] = 433429.into();
    let parsed = catalog::parse(&data, "100").unwrap();
    assert_eq!(parsed.items[0].played_seconds, Some(231840));
    assert_eq!(parsed.items[1].played_seconds, None, "缺失的个人时长不能被当成零");
    let mut conn = db::open(std::path::Path::new(":memory:")).unwrap();
    store::save(&mut conn, &session(), parsed).unwrap();
    let game = db::game(&conn, "steam-480").unwrap().unwrap();
    let snapshot = crate::steam_playtime::read(&conn, "100").unwrap();
    let info = crate::steam_playtime::info(&game, 0, &snapshot, true);
    assert_eq!(info.seconds, Some(231840));
    assert_eq!(info.source, "steam-family");
    let owned = Game { id:"steam-481".into(), source:"steam".into(), appid:"481".into(),
        metadata_json:r#"{"steamPlayedSeconds":3600}"#.into(), ..Default::default() };
    crate::steam_playtime::save_owned(&conn, "100", &[owned]).unwrap();
    let refreshed = crate::steam_playtime::read(&conn, "100").unwrap();
    assert_eq!(crate::steam_playtime::info(&game, 0, &refreshed, true).seconds, Some(231840),
        "个人拥有库刷新不能清空共享游戏的本人时长缓存");
}
