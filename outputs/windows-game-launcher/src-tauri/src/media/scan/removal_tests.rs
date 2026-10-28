use super::*;

fn linked_item(conn: &Connection, path: &Path) -> String {
    conn.query_row("SELECT l.item_id FROM resources r JOIN resource_links l ON l.resource_id=r.id WHERE r.path=?1 COLLATE NOCASE",[path.to_string_lossy().as_ref()],|row|row.get(0)).unwrap()
}

#[test]
fn removing_source_deletes_matched_empty_entries_and_preserves_records_and_other_versions() {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    for name in [
        "Empty.mkv",
        "Matched.mkv",
        "Rated.mkv",
        "Standalone.mkv",
        "Multi.mkv",
    ] {
        fs::write(media.path().join(name), b"fixture").unwrap();
    }
    let mut conn = store::open(data.path()).unwrap();
    for (id, path) in [("s", media.path()), ("other", other.path())] {
        conn.execute(
            "INSERT INTO sources(id,path,label,created_at)VALUES(?1,?2,'test','')",
            params![id, path.to_string_lossy()],
        )
        .unwrap();
    }
    let job = store::job_start(&conn, "scan", 0).unwrap();
    scan_source(data.path(), "s", &job).unwrap();
    let empty = linked_item(&conn, &media.path().join("Empty.mkv"));
    let matched = linked_item(&conn, &media.path().join("Matched.mkv"));
    let rated = linked_item(&conn, &media.path().join("Rated.mkv"));
    let standalone = linked_item(&conn, &media.path().join("Standalone.mkv"));
    let multi = linked_item(&conn, &media.path().join("Multi.mkv"));
    conn.execute(
        "UPDATE media_items SET recognized=1,kind='movie' WHERE id=?1",
        [&matched],
    )
    .unwrap();
    conn.execute("INSERT INTO external_ids(provider,provider_type,external_id,item_id)VALUES('tmdb','movie','42',?1)",[&matched]).unwrap();
    store::update_record(
        &conn,
        &rated,
        &json!({"watched":true,"rating":4,"favorite":true,"comment":"保留记录"}),
        true,
    )
    .unwrap();
    let rated_resource = store::resources_for(&conn, &rated).unwrap()[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    conn.execute(
        "UPDATE resources SET position_seconds=42,duration_seconds=100 WHERE id=?1",
        [&rated_resource],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO media_playback_coverage VALUES(?1,'[[0,42]]')",
        [&rated_resource],
    )
    .unwrap();
    conn.execute("INSERT INTO view_sessions(id,item_id,resource_id,started_at,seconds)VALUES('history',?1,?2,'2026-10-27',42)",params![rated,rated_resource]).unwrap();
    conn.execute(
        "UPDATE media_items SET independent=1 WHERE id=?1",
        [&standalone],
    )
    .unwrap();
    let other_file = other.path().join("OtherVersion.mkv");
    fs::write(&other_file, b"another version").unwrap();
    scan_source(data.path(), "other", &job).unwrap();
    let other_item = linked_item(&conn, &other_file);
    store::merge_items(&conn, &other_item, &multi).unwrap();

    disable_source(&mut conn, "s").unwrap();
    assert!(store::item(&conn, &empty).is_err());
    assert!(store::item(&conn, &matched).is_err());
    let kept = store::item(&conn, &rated).unwrap();
    assert_eq!(kept["watched"], true);
    assert_eq!(kept["rating"], 4.0);
    assert_eq!(kept["comment"], "保留记录");
    assert_eq!(kept["playable"], false);
    let historical = store::resource(&conn, &rated_resource).unwrap();
    assert_eq!(historical["missing"], true);
    assert_eq!(historical["positionSeconds"], 42.0);
    assert!(!ignored::metadata_allowed(&conn, &rated).unwrap());
    assert!(store::item(&conn, &standalone).is_ok());
    let versions = store::resources_for(&conn, &multi).unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0]["sourceId"], "other");
    assert_eq!(versions[0]["available"], true);
    assert_eq!(store::items(&conn).unwrap().len(), 3);
    assert_eq!(catalog::source_list(&conn).unwrap().len(), 1);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap(),
        0
    );
    for name in [
        "Empty.mkv",
        "Matched.mkv",
        "Rated.mkv",
        "Standalone.mkv",
        "Multi.mkv",
    ] {
        assert!(media.path().join(name).is_file());
    }
    // 重新添加后，已保留记录沿用同一身份；无记录的空条目重新扫描创建。
    store::set_setting(&conn, "source.disabled.s", &json!(false)).unwrap();
    scan_source(data.path(), "s", &job).unwrap();
    assert_eq!(linked_item(&conn, &media.path().join("Rated.mkv")), rated);
    assert_eq!(
        store::resource(&conn, &rated_resource).unwrap()["positionSeconds"],
        42.0
    );
    assert_ne!(
        linked_item(&conn, &media.path().join("Matched.mkv")),
        matched
    );
}

#[test]
fn complete_scan_and_marker_remove_empty_entries_but_incomplete_scan_preserves_unknown_files() {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    let file = media.path().join("Matched.mkv");
    fs::write(&file, b"fixture").unwrap();
    let mut conn = store::open(data.path()).unwrap();
    conn.execute(
        "INSERT INTO sources(id,path,label,created_at)VALUES('s',?1,'test','')",
        [media.path().to_string_lossy().as_ref()],
    )
    .unwrap();
    let job = store::job_start(&conn, "scan", 0).unwrap();
    scan_source(data.path(), "s", &job).unwrap();
    let original = linked_item(&conn, &file);
    conn.execute(
        "UPDATE media_items SET recognized=1,kind='movie' WHERE id=?1",
        [&original],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO external_ids VALUES('tmdb','movie','42',?1)",
        [&original],
    )
    .unwrap();
    fs::remove_file(&file).unwrap();
    apply_scan(&mut conn, "s", &job, &[], false).unwrap();
    assert!(store::item(&conn, &original).is_ok());
    scan_source(data.path(), "s", &job).unwrap();
    assert!(store::item(&conn, &original).is_err());
    assert!(store::items(&conn).unwrap().is_empty());
    fs::write(&file, b"fixture").unwrap();
    scan_source(data.path(), "s", &job).unwrap();
    fs::write(media.path().join(ignored::MARKER_FILE), b"").unwrap();
    apply_scan(&mut conn, "s", &job, &[], false).unwrap();
    assert!(store::items(&conn).unwrap().is_empty());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM resources", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(file.is_file());
}
