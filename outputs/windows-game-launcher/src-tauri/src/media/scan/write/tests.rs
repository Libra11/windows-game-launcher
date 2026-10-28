use super::*;
use crate::media::scan;

fn source(conn: &Connection, id: &str, root: &Path) {
    conn.execute(
        "INSERT INTO sources(id,path,label,created_at)VALUES(?1,?2,'test','')",
        params![id, root.to_string_lossy()],
    )
    .unwrap();
}
fn no_business_writes(conn: &Connection) {
    for table in [
        "resources",
        "media_items",
        "personal",
        "resource_links",
        "media_playback_coverage",
    ] {
        for action in ["INSERT", "UPDATE", "DELETE"] {
            conn.execute_batch(&format!("CREATE TRIGGER reject_{table}_{action} BEFORE {action} ON {table} BEGIN SELECT RAISE(ABORT,'unchanged scan attempted a business write'); END;")).unwrap();
        }
    }
}
fn allow_business_writes(conn: &Connection) {
    for table in [
        "resources",
        "media_items",
        "personal",
        "resource_links",
        "media_playback_coverage",
    ] {
        for action in ["INSERT", "UPDATE", "DELETE"] {
            conn.execute_batch(&format!("DROP TRIGGER reject_{table}_{action};"))
                .unwrap();
        }
    }
}
fn resource_id(conn: &Connection, path: &Path) -> String {
    conn.query_row(
        "SELECT id FROM resources WHERE path=?1 COLLATE NOCASE",
        [path.to_string_lossy().as_ref()],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn unchanged_scan_does_not_write_resource_item_link_personal_or_coverage_rows() {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    fs::write(media.path().join("Film.mkv"), b"film").unwrap();
    let conn = store::open(data.path()).unwrap();
    source(&conn, "s", media.path());
    let job = store::job_start(&conn, "scan", 0).unwrap();
    scan::scan_source(data.path(), "s", &job).unwrap();
    conn.execute("UPDATE resources SET updated_at='preserve-resource-stamp',position_seconds=42,duration_seconds=100,technical_json='{\"state\":\"ready\"}'",[]).unwrap();
    conn.execute("INSERT INTO media_playback_coverage(resource_id,segments_json) SELECT id,'[[0,42]]' FROM resources",[]).unwrap();
    conn.execute(
        "UPDATE media_items SET updated_at='preserve-item-stamp'",
        [],
    )
    .unwrap();
    no_business_writes(&conn);
    for _ in 0..2 {
        let result = scan::scan_source(data.path(), "s", &job).unwrap();
        assert_eq!(result["files"], 1);
        assert_eq!(result["changedResourceIds"], json!([]));
        assert_eq!(result["newItemIds"], json!([]));
    }
    let resource =
        store::resource(&conn, &resource_id(&conn, &media.path().join("Film.mkv"))).unwrap();
    assert_eq!(resource["updatedAt"], "preserve-resource-stamp");
    assert_eq!(resource["positionSeconds"], 42.0);
    assert_eq!(resource["technical"]["state"], "ready");
    assert_eq!(
        store::items(&conn).unwrap()[0]["updatedAt"],
        "preserve-item-stamp"
    );
    let last_scan: String = conn
        .query_row("SELECT last_scan FROM sources WHERE id='s'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(!last_scan.is_empty());
}

#[test]
fn new_file_scan_only_schedules_new_items_without_resetting_old_failures() {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    fs::write(media.path().join("old.mkv"), b"old").unwrap();
    let conn = store::open(data.path()).unwrap();
    source(&conn, "s", media.path());
    let job = store::job_start(&conn, "scan", 0).unwrap();
    let first = scan::scan_source(data.path(), "s", &job).unwrap();
    let old = first["newItemIds"][0].as_str().unwrap();
    conn.execute("INSERT INTO media_match_errors(item_id,error,checked_at) VALUES(?1,'保留失败原因','2026-10-28')", [old]).unwrap();
    let folder = media.path().join("season");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("new.mkv"), b"new").unwrap();
    let result = scan::scan_source(data.path(), "s", &job).unwrap();
    let ids = result["newItemIds"].as_array().unwrap();
    assert_eq!(ids.len(), 1);
    assert_ne!(ids[0], old);
    assert_eq!(result["files"], 2);
    assert_eq!(store::item(&conn, ids[0].as_str().unwrap()).unwrap()["title"], "new.mkv");
    let error: String = conn.query_row("SELECT error FROM media_match_errors WHERE item_id=?1", [old], |r| r.get(0)).unwrap();
    assert_eq!(error, "保留失败原因");
    assert_eq!(scan::scan_source(data.path(), "s", &job).unwrap()["newItemIds"], json!([]));
}

#[test]
fn missing_resource_is_written_once_and_same_version_recovery_keeps_progress() {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    let file = media.path().join("Film.mkv");
    fs::write(&file, b"film").unwrap();
    let original_time = fs::metadata(&file).unwrap().modified().unwrap();
    let conn = store::open(data.path()).unwrap();
    source(&conn, "s", media.path());
    let job = store::job_start(&conn, "scan", 0).unwrap();
    scan::scan_source(data.path(), "s", &job).unwrap();
    let id = resource_id(&conn, &file);
    let item = store::resource(&conn, &id).unwrap()["itemIds"][0]
        .as_str()
        .unwrap()
        .to_string();
    store::update_record(&conn, &item, &json!({"watched":true,"rating":4}), true).unwrap();
    conn.execute("UPDATE resources SET position_seconds=42,duration_seconds=100,technical_json='{\"state\":\"ready\"}'",[]).unwrap();
    conn.execute(
        "INSERT INTO media_playback_coverage(resource_id,segments_json)VALUES(?1,'[[0,42]]')",
        [&id],
    )
    .unwrap();
    fs::remove_file(&file).unwrap();
    scan::scan_source(data.path(), "s", &job).unwrap();
    let missing = store::resource(&conn, &id).unwrap();
    assert_eq!(missing["missing"], true);
    assert_eq!(store::item(&conn, &item).unwrap()["rating"], 4.0);
    no_business_writes(&conn);
    scan::scan_source(data.path(), "s", &job).unwrap();
    assert_eq!(store::resource(&conn, &id).unwrap(), missing);
    allow_business_writes(&conn);
    fs::write(&file, b"film").unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&file)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(original_time))
        .unwrap();
    let recovered = scan::scan_source(data.path(), "s", &job).unwrap();
    assert_eq!(recovered["changedResourceIds"], json!([]));
    let resource = store::resource(&conn, &id).unwrap();
    assert_eq!(resource["missing"], false);
    assert_eq!(resource["available"], true);
    assert_eq!(resource["positionSeconds"], 42.0);
    assert_eq!(resource["durationSeconds"], 100.0);
    assert_eq!(resource["technical"]["state"], "ready");
    assert_eq!(store::item(&conn, &item).unwrap()["rating"], 4.0);
    let coverage: String = conn
        .query_row(
            "SELECT segments_json FROM media_playback_coverage WHERE resource_id=?1",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(coverage, "[[0,42]]");
}

#[test]
fn unchanged_offline_resource_recovers_without_resetting_media_features() {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    fs::write(media.path().join("Film.mkv"), b"film").unwrap();
    let conn = store::open(data.path()).unwrap();
    source(&conn, "s", media.path());
    let job = store::job_start(&conn, "scan", 0).unwrap();
    scan::scan_source(data.path(), "s", &job).unwrap();
    conn.execute(
        "UPDATE resources SET available=0,position_seconds=42,duration_seconds=100",
        [],
    )
    .unwrap();
    let result = scan::scan_source(data.path(), "s", &job).unwrap();
    assert_eq!(result["changedResourceIds"], json!([]));
    let id = resource_id(&conn, &media.path().join("Film.mkv"));
    let resource = store::resource(&conn, &id).unwrap();
    assert_eq!(resource["available"], true);
    assert_eq!(resource["positionSeconds"], 42.0);
}

#[test]
fn incomplete_batch_keeps_unseen_resources_and_source_transfer_is_an_actual_change() {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    let file = media.path().join("Film.mkv");
    fs::write(&file, b"film").unwrap();
    let mut conn = store::open(data.path()).unwrap();
    source(&conn, "s", media.path());
    source(&conn, "other", &media.path().join("other-source"));
    let job = store::job_start(&conn, "scan", 0).unwrap();
    scan::scan_source(data.path(), "s", &job).unwrap();
    let id = resource_id(&conn, &file);
    no_business_writes(&conn);
    apply(&mut conn, "s", &job, &[], false).unwrap();
    assert_eq!(store::resource(&conn, &id).unwrap()["missing"], false);
    allow_business_writes(&conn);
    conn.execute(
        "UPDATE resources SET source_id='other',position_seconds=42",
        [],
    )
    .unwrap();
    let result = scan::scan_source(data.path(), "s", &job).unwrap();
    assert_eq!(result["changedResourceIds"], json!([]));
    let resource = store::resource(&conn, &id).unwrap();
    assert_eq!(resource["sourceId"], "s");
    assert_eq!(resource["positionSeconds"], 42.0);
}

#[test]
fn final_marker_check_deactivates_a_resource_already_in_the_seen_set() {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    let file = media.path().join("Film.mkv");
    fs::write(&file, b"film").unwrap();
    let mut conn = store::open(data.path()).unwrap();
    source(&conn, "s", media.path());
    let job = store::job_start(&conn, "scan", 0).unwrap();
    scan::scan_source(data.path(), "s", &job).unwrap();
    let id = resource_id(&conn, &file);
    conn.execute(
        "UPDATE resources SET position_seconds=42,duration_seconds=100",
        [],
    )
    .unwrap();
    let seen = HashSet::from([file.to_string_lossy().to_ascii_lowercase()]);
    // 模拟文件已被逐项处理，然后才放置标记，避免依赖线程或磁盘时序。
    fs::write(media.path().join(ignored::MARKER_FILE), b"").unwrap();
    let finalized = plan_finalization(&conn, "s", &seen, true, ignored::contains).unwrap();
    assert!(finalized.complete);
    let tx = conn.transaction().unwrap();
    assert!(apply_finalization(&tx, "s", &finalized).unwrap());
    tx.commit().unwrap();
    let resource = store::resource(&conn, &id).unwrap();
    assert_eq!(resource["missing"], true);
    assert_eq!(resource["available"], false);
    assert_eq!(resource["positionSeconds"], 42.0);
    assert_eq!(store::items(&conn).unwrap()[0]["independent"], true);
}

#[test]
fn incomplete_batch_still_excludes_marked_resources_but_preserves_other_unseen_files() {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    let skipped = media.path().join("Backup");
    fs::create_dir(&skipped).unwrap();
    let marked = skipped.join("Film.mkv");
    let unseen = media.path().join("Keep.mkv");
    fs::write(&marked, b"film").unwrap();
    fs::write(&unseen, b"film").unwrap();
    let mut conn = store::open(data.path()).unwrap();
    source(&conn, "s", media.path());
    let job = store::job_start(&conn, "scan", 0).unwrap();
    scan::scan_source(data.path(), "s", &job).unwrap();
    let marked_id = resource_id(&conn, &marked);
    let unseen_id = resource_id(&conn, &unseen);
    conn.execute("UPDATE resources SET position_seconds=42", [])
        .unwrap();
    fs::write(skipped.join(ignored::MARKER_FILE), b"").unwrap();
    let result = apply(&mut conn, "s", &job, &[], false).unwrap();
    assert_eq!(result["complete"], false);
    assert_eq!(store::resource(&conn, &marked_id).unwrap()["missing"], true);
    let retained = store::resource(&conn, &unseen_id).unwrap();
    assert_eq!(retained["missing"], false);
    assert_eq!(retained["available"], true);
    assert_eq!(retained["positionSeconds"], 42.0);
}

#[test]
fn unreadable_marker_check_preserves_unknown_missing_files_regardless_of_check_order() {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    let mut conn = store::open(data.path()).unwrap();
    for name in ["Normal.mkv", "Protected.mkv", "Explicit.mkv"] {
        fs::write(media.path().join(name), b"film").unwrap();
    }
    source(&conn, "s", media.path());
    let job = store::job_start(&conn, "scan", 0).unwrap();
    scan::scan_source(data.path(), "s", &job).unwrap();
    let normal = resource_id(&conn, &media.path().join("Normal.mkv"));
    let protected = resource_id(&conn, &media.path().join("Protected.mkv"));
    let explicit = resource_id(&conn, &media.path().join("Explicit.mkv"));
    let finalized =
        plan_finalization(
            &conn,
            "s",
            &HashSet::new(),
            true,
            |_, _, path| match Path::new(path).file_name().unwrap().to_str().unwrap() {
                "Protected.mkv" => Err("permission denied".into()),
                "Explicit.mkv" => Ok(true),
                _ => Ok(false),
            },
        )
        .unwrap();
    assert!(finalized.marker_unreadable);
    assert!(!finalized.complete);
    let tx = conn.transaction().unwrap();
    assert!(apply_finalization(&tx, "s", &finalized).unwrap());
    tx.commit().unwrap();
    for id in [normal, protected] {
        assert_eq!(store::resource(&conn, &id).unwrap()["missing"], false);
    }
    assert!(store::resource(&conn, &explicit).is_err());
    assert_eq!(store::items(&conn).unwrap().len(), 2);
}

#[test]
fn finalization_plan_does_not_write_a_resource_transferred_to_another_source() {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    let file = media.path().join("Film.mkv");
    fs::write(&file, b"film").unwrap();
    let mut conn = store::open(data.path()).unwrap();
    source(&conn, "s", media.path());
    source(&conn, "other", &media.path().join("other-source"));
    let job = store::job_start(&conn, "scan", 0).unwrap();
    scan::scan_source(data.path(), "s", &job).unwrap();
    let id = resource_id(&conn, &file);
    fs::write(media.path().join(ignored::MARKER_FILE), b"").unwrap();
    let plan = plan_finalization(&conn, "s", &HashSet::new(), true, ignored::contains).unwrap();
    assert_eq!(plan.missing, vec![id.clone()]);
    conn.execute("UPDATE resources SET source_id='other' WHERE id=?1", [&id])
        .unwrap();
    let tx = conn.transaction().unwrap();
    assert!(!apply_finalization(&tx, "s", &plan).unwrap());
    tx.commit().unwrap();
    let state = conn
        .query_row(
            "SELECT source_id,available,missing FROM resources WHERE id=?1",
            [&id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(state, ("other".to_string(), 1, 0));
}
