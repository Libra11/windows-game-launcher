use super::{archive, export, model::*, paths, restore, schema};
use crate::db;
use std::{collections::BTreeMap, io::{Read, Write}};

const TIME: &str = "2026-10-10T08:00:00+00:00";
fn appearance() -> Appearance {
    Appearance { theme:"dark".into(), theme_color:"amber".into(), fonts:vec!["Microsoft YaHei".into(),"system-ui".into()] }
}
fn source() -> rusqlite::Connection {
    let mut conn = db::open(std::path::Path::new(":memory:")).unwrap();
    let game = crate::model::Game { id:"local-one".into(),source:"local".into(),appid:"480".into(),title:"中文游戏".into(),
        exe_path:r"E:\Games\中文游戏\game.exe".into(),custom_unlock_path:r"E:\Games\中文游戏\record.ini".into(),metadata_json:"{}".into(),..Default::default() };
    db::upsert_game(&conn,&game).unwrap();
    crate::activity::set_favorite(&conn,&game.id,true).unwrap();
    crate::activity::start(&mut conn,"session-one",&game.id,TIME).unwrap();
    crate::activity::checkpoint(&mut conn,"session-one",125,false,&BTreeMap::from([("2026-10-10".into(),125)])).unwrap();
    conn.execute("INSERT INTO achievements VALUES('480','FIRST','首次','条件','',0)",[]).unwrap();
    conn.execute("INSERT INTO unlocks VALUES('local-one','FIRST','manual',NULL,'历史证据 E:/Original/record.ini')",[]).unwrap();
    for key in ["steam_api_key","epic_oauth_session","steam_family_session","runtime_recovery","achievement_definition_error:local-one"] {
        db::set_setting(&conn,key,"PRIVATE_SENTINEL_MUST_NOT_EXPORT").unwrap();
    }
    db::set_setting(&conn,"network_proxy",r#"{"mode":"direct","address":""}"#).unwrap();
    db::set_setting(&conn,"achievement_notifications","true").unwrap();
    conn
}

#[test]
fn exports_consistent_records_without_credentials_and_only_closes_copy() {
    let mut conn=source();
    let (data,prefs)=export::capture(&mut conn,appearance(),TIME).unwrap();
    let text=serde_json::to_string(&data).unwrap();
    assert!(!text.contains("PRIVATE_SENTINEL"));
    assert!(!data.settings.contains_key("network_proxy"));
    assert_eq!(data.tables["play_sessions"][0][3],TIME);
    assert_eq!(conn.query_row("SELECT ended_at FROM play_sessions",[],|row|row.get::<_,String>(0)).unwrap(),"");
    assert_eq!(data.tables["game_activity"][0][3],125);
    assert_eq!(schema::counts(&data,0).favorites,1);
    schema::validate(&data,&prefs).unwrap();
}

#[test]
fn archive_roundtrip_restores_history_and_proxy_without_logins_or_runtime() {
    let temp=tempfile::tempdir().unwrap();let covers=temp.path().join("source-covers");std::fs::create_dir(&covers).unwrap();
    let name=format!("{}.png",uuid::Uuid::new_v4());
    std::fs::write(covers.join(&name),include_bytes!("../../icons/icon.png")).unwrap();
    let mut conn=source();
    db::set_setting(&conn,"cover_cache:local-one:portrait",&serde_json::json!({"appid":"480","source":"local","file":name}).to_string()).unwrap();
    let (data,prefs)=export::capture(&mut conn,appearance(),TIME).unwrap();
    let path=temp.path().join("portable.youji-backup");
    let manifest=archive::write(&path,data,prefs,&covers,"0.2.5".into(),|_,_,_|Ok(())).unwrap();
    assert_eq!(manifest.counts.covers,1);
    let loaded=archive::read(&path).unwrap();
    let mut target=db::open(std::path::Path::new(":memory:")).unwrap();
    db::set_setting(&target,"steam_api_key","TARGET_CREDENTIAL").unwrap();
    db::set_setting(&target,"network_proxy","target-proxy").unwrap();
    restore::replace(&mut target,&loaded.data,&loaded.preferences,"fixture-id").unwrap();
    assert_eq!(db::setting(&target,"steam_api_key").unwrap(),"");
    assert_eq!(db::setting(&target,"network_proxy").unwrap(),"target-proxy");
    assert_eq!(db::setting(&target,"runtime_recovery").unwrap(),"[]");
    assert_eq!(crate::activity::all(&target).unwrap()["local-one"].played_seconds,125);
    assert_eq!(db::achievements(&target,"local-one").unwrap().len(),1);
    assert!(restore::appearance_script(&target).unwrap().unwrap().contains("launcher-font-settings"));
}

#[test]
fn rollback_preserves_target_data_if_restoring_fails() {
    let mut original=source();let (data,prefs)=export::capture(&mut original,appearance(),TIME).unwrap();
    let mut target=db::open(std::path::Path::new(":memory:")).unwrap();
    db::set_setting(&target,"steam_api_key","ORIGINAL_SECRET").unwrap();
    target.execute_batch("CREATE TRIGGER reject_game BEFORE INSERT ON games BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    assert!(restore::replace(&mut target,&data,&prefs,"fixture").is_err());
    assert_eq!(db::setting(&target,"steam_api_key").unwrap(),"ORIGINAL_SECRET");
    assert!(db::games(&target).unwrap().is_empty());
}

#[test]
fn directory_mapping_obeys_boundaries_drive_roots_and_preserves_historical_fields() {
    let mappings=vec![PathMapping{from:"E:/Games".into(),to:"D:/库".into()}];
    assert_eq!(paths::mapped(r"e:\Games\中文\game.exe",&mappings),"D:/库/中文/game.exe");
    assert_eq!(paths::mapped("E:/Games2/game.exe",&mappings),"E:/Games2/game.exe");
    assert_eq!(paths::mapped("E:/Games/game.exe",&[PathMapping{from:"E:/".into(),to:"D:/".into()}]),"D:/Games/game.exe");
    let mut conn=source();let (mut data,_)=export::capture(&mut conn,appearance(),TIME).unwrap();
    let evidence=data.tables["unlocks"][0][4].clone();let appid=data.tables["games"][0][2].clone();
    paths::apply(&mut data,&Relocation{mappings,overrides:vec![]}).unwrap();
    assert_eq!(data.tables["games"][0][2],appid);assert_eq!(data.tables["unlocks"][0][4],evidence);
}

#[test]
fn malformed_records_and_unsupported_archive_entries_are_rejected() {
    let mut conn=source();let (mut data,prefs)=export::capture(&mut conn,appearance(),TIME).unwrap();
    data.tables.get_mut("play_sessions").unwrap()[0][4]=(-1).into();
    assert!(schema::validate(&data,&prefs).is_err());
    let temp=tempfile::tempdir().unwrap();let path=temp.path().join("bad.youji-backup");
    let mut zip=zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    zip.start_file("../outside.txt",zip::write::SimpleFileOptions::default()).unwrap();zip.write_all(b"bad").unwrap();zip.finish().unwrap();
    assert!(archive::read(&path).is_err());assert!(!temp.path().join("outside.txt").exists());
}

#[test]
fn cancellation_does_not_publish_a_partial_file() {
    let temp=tempfile::tempdir().unwrap();let mut conn=source();let (data,prefs)=export::capture(&mut conn,appearance(),TIME).unwrap();
    let path=temp.path().join("cancelled.youji-backup");
    assert!(archive::write(&path,data,prefs,temp.path(),"0.2.5".into(),|_,_,_|Err("取消".into())).is_err());
    assert!(!path.exists());
}

#[test]
fn digest_failure_is_detected_before_replacing_any_data() {
    let temp=tempfile::tempdir().unwrap();let mut conn=source();let (data,prefs)=export::capture(&mut conn,appearance(),TIME).unwrap();
    let path=temp.path().join("valid.youji-backup");archive::write(&path,data,prefs,temp.path(),"0.2.5".into(),|_,_,_|Ok(())).unwrap();
    let mut zip=zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    let bad=temp.path().join("changed.youji-backup");let mut writer=zip::ZipWriter::new(std::fs::File::create(&bad).unwrap());
    for index in 0..zip.len(){let mut file=zip.by_index(index).unwrap();let name=file.name().unwrap().into_owned();let mut bytes=Vec::new();file.read_to_end(&mut bytes).unwrap();
        if name=="data.json"{bytes.push(b' ');}writer.start_file(name,zip::write::SimpleFileOptions::default()).unwrap();writer.write_all(&bytes).unwrap();}
    writer.finish().unwrap();assert!(archive::read(&bad).is_err());
}
