use super::*;
use rusqlite::params;
use serde_json::json;

fn setup(dir: &Path, source: &Path, file: &Path) -> (Connection, String, String) {
    let conn = store::open(dir).unwrap();
    conn.execute(
        "INSERT INTO sources(id,path,label,created_at)VALUES('source',?1,'test',?2)",
        params![source.to_string_lossy(), store::now()],
    )
    .unwrap();
    let item = store::new_item(&conn, &json!({"title":"截图测试","kind":"other"})).unwrap();
    let meta = fs::metadata(file).unwrap();
    let modified = chrono::DateTime::<chrono::Utc>::from(meta.modified().unwrap())
        .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
    conn.execute("INSERT INTO resources(id,source_id,path,size,modified_at,duration_seconds,updated_at)VALUES('resource','source',?1,?2,?3,3,?4)",params![file.to_string_lossy(),meta.len() as i64,modified,store::now()]).unwrap();
    conn.execute(
        "INSERT INTO resource_links(resource_id,item_id)VALUES('resource',?1)",
        [&item],
    )
    .unwrap();
    let key = resource_key(&store::resource(&conn, "resource").unwrap());
    (conn, item, key)
}

#[test]
fn fallback_identity_tracks_file_version_and_respects_recognition_and_markers() {
    let data = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let file = source.path().join("video.mp4");
    fs::write(&file, b"fixture").unwrap();
    let (conn, item, original) = setup(data.path(), source.path(), &file);
    assert_eq!(
        store::item(&conn, &item).unwrap()["localArtworkKey"],
        original
    );
    assert!(candidate(&conn, &item, &original).unwrap().is_some());
    let old = store::resource(&conn, "resource").unwrap();
    fs::write(&file, b"changed fixture").unwrap();
    assert!(!same_file(&old));
    conn.execute("UPDATE resources SET size=size+1 WHERE id='resource'", [])
        .unwrap();
    assert_ne!(
        store::item(&conn, &item).unwrap()["localArtworkKey"],
        original
    );
    assert!(candidate(&conn, &item, &original).unwrap().is_none());
    conn.execute(
        "UPDATE resources SET size=?1 WHERE id='resource'",
        [old["size"].as_i64().unwrap()],
    )
    .unwrap();
    fs::write(source.path().join(".youji-ignore"), []).unwrap();
    assert!(candidate(&conn, &item, &original).unwrap().is_none());
    fs::remove_file(source.path().join(".youji-ignore")).unwrap();
    conn.execute("UPDATE media_items SET recognized=1 WHERE id=?1", [&item])
        .unwrap();
    assert!(candidate(&conn, &item, &original).unwrap().is_none());
    assert!(store::item(&conn, &item).unwrap()["localArtworkKey"].is_null());
}

#[test]
fn unsupported_images_and_confirmed_audio_never_become_screenshot_candidates() {
    let data = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let file = source.path().join("video.iso");
    fs::write(&file, b"fixture").unwrap();
    let (conn, item, key) = setup(data.path(), source.path(), &file);
    assert!(candidate(&conn, &item, &key).unwrap().is_none());
    conn.execute(
        "UPDATE resources SET path=?1,technical_json=?2 WHERE id='resource'",
        params![
            source.path().join("audio.mp4").to_string_lossy(),
            json!({"state":"ready","video":[]}).to_string()
        ],
    )
    .unwrap();
    assert!(candidate(&conn, &item, &key).unwrap().is_none());
    assert_eq!(positions(1.0), vec![0.1, 0.3, 0.0]);
    assert_eq!(positions(f64::NAN), vec![10.0, 0.0]);
}

#[test]
fn snapshot_skips_unsupported_resources_and_keeps_frame_when_resume_changes() {
    let data = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let file = source.path().join("b.mp4");
    fs::write(&file, b"fixture").unwrap();
    let (conn, item, original) = setup(data.path(), source.path(), &file);
    for (id, name, technical) in [("iso", "a.iso", "{}"), ("audio", "a.mp4", r#"{"state":"ready","video":[]}"#), ("later", "c.mp4", "{}") ] {
        conn.execute("INSERT INTO resources(id,source_id,path,size,modified_at,technical_json,updated_at) VALUES(?1,'source',?2,7,'v1',?3,'')",params![id,source.path().join(name).to_string_lossy(),technical]).unwrap();
        conn.execute("INSERT INTO resource_links(resource_id,item_id) VALUES(?1,?2)",params![id,item]).unwrap();
    }
    assert_eq!(store::item(&conn, &item).unwrap()["localArtworkKey"], original);
    conn.execute("INSERT INTO view_sessions VALUES('view',?1,'later','2099-01-01','',10,10)", [&item]).unwrap();
    let snapshot = store::item(&conn, &item).unwrap();
    assert_eq!(snapshot["resumeResourceId"], "later");
    assert_eq!(snapshot["localArtworkKey"], original);
    assert_eq!(store::detail(&conn, &item).unwrap()["item"]["localArtworkKey"], original);
}

#[test]
fn cache_lookup_never_extracts_and_recognition_roundtrip_reuses_resource_cache() {
    let data = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let file = source.path().join("video.mp4");
    fs::write(&file, b"fixture").unwrap();
    let (conn, item, key) = setup(data.path(), source.path(), &file);
    assert!(generate(data.path(), cache.path(), &item, &key, true).unwrap().is_none());
    let destination = cache.path().join(format!("media-frame-{key}.jpg"));
    let mut jpeg = vec![0; 600];
    jpeg[..3].copy_from_slice(&[0xff,0xd8,0xff]);
    jpeg[598..].copy_from_slice(&[0xff,0xd9]);
    fs::write(&destination, &jpeg).unwrap();
    assert_eq!(generate(data.path(), cache.path(), &item, &key, true).unwrap(), Some(destination.clone()));
    conn.execute("UPDATE media_items SET recognized=1 WHERE id=?1", [&item]).unwrap();
    assert!(generate(data.path(), cache.path(), &item, &key, true).unwrap().is_none());
    conn.execute("UPDATE media_items SET recognized=0 WHERE id=?1", [&item]).unwrap();
    assert_eq!(generate(data.path(), cache.path(), &item, &key, true).unwrap(), Some(destination.clone()));
    // 解除资源关联产生新条目时，资源身份不变，复用同一缓存。
    let new_item = store::new_item(&conn, &json!({"title":"重新关联"})).unwrap();
    conn.execute("UPDATE resource_links SET item_id=?1 WHERE resource_id='resource'", [&new_item]).unwrap();
    assert_eq!(generate(data.path(), cache.path(), &new_item, &key, true).unwrap(), Some(destination));
    assert_eq!(jpeg, fs::read(cache.path().join(format!("media-frame-{key}.jpg"))).unwrap());
}

#[test]
#[ignore = "使用 YOUJI_TEST_FFMPEG 指定本机 ffmpeg，只生成临时合成视频"]
fn real_ffmpeg_generates_reusable_cache_without_business_writes() {
    let program =
        PathBuf::from(std::env::var_os("YOUJI_TEST_FFMPEG").expect("set YOUJI_TEST_FFMPEG"));
    let data = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let file = source.path().join("video 空格.mp4");
    let status = Command::new(&program)
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=10",
            "-t",
            "3",
            "-an",
            "-c:v",
            "mpeg4",
            "-y",
        ])
        .arg(&file)
        .status()
        .unwrap();
    assert!(status.success());
    let (conn, item, key) = setup(data.path(), source.path(), &file);
    store::set_setting(&conn, "ffmpegPath", &json!(program)).unwrap();
    for table in [
        "resources",
        "media_items",
        "personal",
        "resource_links",
        "media_playback_coverage",
    ] {
        for action in ["INSERT", "UPDATE", "DELETE"] {
            conn.execute_batch(&format!("CREATE TRIGGER forbid_{table}_{action} BEFORE {action} ON {table} BEGIN SELECT RAISE(ABORT,'screenshot must not write business rows'); END;")).unwrap();
        }
    }
    let first = generate(data.path(), cache.path(), &item, &key, false)
        .unwrap()
        .unwrap();
    assert!(cached(&first));
    let bytes = fs::read(&first).unwrap();
    store::set_setting(&conn, "ffmpegPath", &json!("Z:/missing/ffmpeg.exe")).unwrap();
    assert_eq!(
        generate(data.path(), cache.path(), &item, &key, false).unwrap(),
        Some(first.clone())
    );
    assert_eq!(bytes, fs::read(&first).unwrap());
    let fallback = tempfile::tempdir().unwrap();
    let mut unknown = store::resource(&conn, "resource").unwrap();
    unknown["durationSeconds"] = json!(0);
    let short = fallback.path().join("short.jpg");
    extract(&program, &unknown, &short).unwrap();
    assert!(cached(&short));
    fs::write(source.path().join(".youji-ignore"), []).unwrap();
    assert!(generate(data.path(), cache.path(), &item, &key, false)
        .unwrap()
        .is_none());
    assert_eq!(fs::read_dir(cache.path()).unwrap().count(), 1);
}
