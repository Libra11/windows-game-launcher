use super::*;
use rusqlite::params;
use std::fs;

fn setup() -> (tempfile::TempDir, tempfile::TempDir, Connection, String) {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    let conn = store::open(data.path()).unwrap();
    conn.execute(
        "INSERT INTO sources(id,path,label,created_at)VALUES('s',?1,'示例','')",
        [fs::canonicalize(media.path())
            .unwrap()
            .to_string_lossy()
            .as_ref()],
    )
    .unwrap();
    let job = store::job_start(&conn, "scan", 0).unwrap();
    (data, media, conn, job)
}
fn ignore(conn: &mut Connection, path: &Path) -> Value {
    save(conn, &json!({"sourceId":"s","path":path.to_string_lossy()})).unwrap()
}

#[test]
fn path_matching_respects_windows_case_prefixes_and_directory_boundaries() {
    let directory = Rule {
        path: r"\\?\D:\影视\Extras".into(),
        directory: true,
    };
    assert!(directory.matches("d:/影视/extras/clip.mkv"));
    assert!(directory.matches(r"D:\影视\EXTRAS"));
    assert!(!directory.matches("D:/影视/Extras2/clip.mkv"));
    let file = Rule {
        path: "D:/影视/a.mkv".into(),
        directory: false,
    };
    assert!(file.matches(r"\\?\d:\影视\A.MKV"));
    assert!(!file.matches("D:/影视/a.mkv.bak"));
    assert!(!file.matches("D:/影视/a.mkv/child"));
    let unc = Rule {
        path: r"\\?\UNC\NAS\Share\花絮".into(),
        directory: true,
    };
    assert!(unc.matches(r"\\nas\share\花絮\a.mkv"));
    assert!(!unc.matches(r"\\NAS\Share2\花絮\a.mkv"));
}

#[test]
fn ignored_directory_removes_empty_entries_and_does_not_return_on_scan() {
    let (data, media, mut conn, job) = setup();
    let folder = media.path().join("Extras");
    let sibling = media.path().join("Extras2");
    fs::create_dir(&folder).unwrap();
    fs::create_dir(&sibling).unwrap();
    let file = folder.join("Unknown.mkv");
    fs::write(&file, b"sample").unwrap();
    fs::write(sibling.join("Keep.mkv"), b"sample").unwrap();
    scan::scan_source(data.path(), "s", &job).unwrap();
    let item = store::items(&conn)
        .unwrap()
        .into_iter()
        .find(|v| v["title"] == "Unknown.mkv")
        .unwrap();
    let id = store::text(&item, "id");
    conn.execute(
        "INSERT INTO media_match_errors(item_id,error,checked_at)VALUES(?1,'不匹配','')",
        [id],
    )
    .unwrap();
    let result = ignore(&mut conn, &folder);
    assert_eq!(result["resources"], 1);
    assert_eq!(result["items"], 1);
    assert!(store::item(&conn, id).is_err());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM media_match_errors", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    fs::write(folder.join("New.mkv"), b"sample").unwrap();
    let result = scan::scan_source(data.path(), "s", &job).unwrap();
    assert_eq!(result["files"], 1);
    assert_eq!(store::items(&conn).unwrap().len(), 1);
    assert_eq!(catalog::source_list(&conn).unwrap()[0]["ignoredCount"], 1);
    assert!(file.is_file());
    save(
        &mut conn,
        &json!({"sourceId":"s","path":folder.to_string_lossy(),"remove":true}),
    )
    .unwrap();
    scan::scan_source(data.path(), "s", &job).unwrap();
    assert_eq!(store::items(&conn).unwrap().len(), 3);
}

#[tokio::test]
async fn personal_history_and_independent_items_are_preserved_but_tasks_are_skipped() {
    for kind in [
        "rating",
        "watched",
        "favorite",
        "comment",
        "collection",
        "tag",
        "history",
        "position",
        "independent",
        "douban",
    ] {
        let (data, media, mut conn, job) = setup();
        let file = media.path().join("Unknown.mkv");
        fs::write(&file, b"sample").unwrap();
        scan::scan_source(data.path(), "s", &job).unwrap();
        let item = store::items(&conn).unwrap().remove(0);
        let id = store::text(&item, "id");
        let resource = store::resources_for(&conn, id).unwrap().remove(0);
        let resource_id = store::text(&resource, "id");
        match kind {
            "rating" => store::update_record(&conn, id, &json!({"rating":4}), true).unwrap(),
            "watched" => store::update_record(&conn, id, &json!({"watched":true}), true).unwrap(),
            "favorite" => store::update_record(&conn, id, &json!({"favorite":true}), true).unwrap(),
            "comment" => store::update_record(&conn, id, &json!({"comment":"保留"}), true).unwrap(),
            "collection" => {
                conn.execute(
                    "INSERT INTO media_collections(id,name)VALUES('c','收藏夹')",
                    [],
                )
                .unwrap();
                conn.execute("INSERT INTO collection_items VALUES('c',?1)", [id])
                    .unwrap();
            }
            "tag" => {
                conn.execute("INSERT INTO media_tags(id,name)VALUES('t','标签')", [])
                    .unwrap();
                conn.execute("INSERT INTO media_item_tags VALUES(?1,'t')", [id])
                    .unwrap();
            }
            "history" => {
                conn.execute("INSERT INTO view_sessions(id,item_id,resource_id,started_at,ended_at,seconds)VALUES('v',?1,?2,'2026-01-01','2026-01-01',60)",params![id,resource_id]).unwrap();
            }
            "position" => {
                conn.execute(
                    "UPDATE resources SET position_seconds=10 WHERE id=?1",
                    [resource_id],
                )
                .unwrap();
            }
            "independent" => {
                conn.execute("UPDATE media_items SET independent=1 WHERE id=?1", [id])
                    .unwrap();
            }
            "douban" => {
                conn.execute(
                    "INSERT INTO external_ids VALUES('douban','subject','42',?1)",
                    [id],
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let result = ignore(&mut conn, &file);
        assert_eq!(result["items"], 0, "{kind}");
        assert!(store::item(&conn, id).is_ok(), "{kind}");
        assert!(!metadata_allowed(&conn, id).unwrap(), "{kind}");
        assert!(super::super::metadata::auto_identify(data.path(), id)
            .await
            .unwrap_err()
            .contains("已取消"));
        assert!(super::super::probe::probe_resource(data.path(), resource_id).is_err());
        scan::scan_source(data.path(), "s", &job).unwrap();
        let resource = store::resource(&conn, resource_id).unwrap();
        assert_eq!(resource["missing"], true);
        assert_eq!(resource["ignored"], true);
        assert_eq!(resource["available"], false);
        let violations = conn
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap();
        assert_eq!(violations, 0);
    }
}

#[test]
fn only_metadata_season_is_pruned_but_other_versions_keep_the_season() {
    for another_version in [false, true] {
        let (data, media, mut conn, job) = setup();
        let file = media.path().join("Unknown.mkv");
        fs::write(&file, b"sample").unwrap();
        scan::scan_source(data.path(), "s", &job).unwrap();
        let original = store::items(&conn).unwrap().remove(0);
        let original_id = store::text(&original, "id");
        let parent = store::upsert_provider_item(
            &conn,
            "tmdb",
            "tv",
            "100",
            &json!({"title":"剧","kind":"series"}),
            false,
        )
        .unwrap();
        let season = store::upsert_provider_item(
            &conn,
            "tmdb",
            "season",
            "100:1",
            &json!({"title":"季","kind":"season","parentId":parent}),
            false,
        )
        .unwrap();
        let episode = store::upsert_provider_item(
            &conn,
            "tmdb",
            "episode",
            "100:1:1",
            &json!({"title":"集","kind":"episode","parentId":season}),
            false,
        )
        .unwrap();
        store::upsert_provider_item(
            &conn,
            "tmdb",
            "episode",
            "100:1:2",
            &json!({"title":"无资源集","kind":"episode","parentId":season}),
            false,
        )
        .unwrap();
        store::merge_items(&conn, original_id, &episode).unwrap();
        if another_version {
            conn.execute(
                "INSERT INTO resources(id,source_id,path,updated_at)VALUES('other','s',?1,'')",
                [media.path().join("other.mkv").to_string_lossy().as_ref()],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO resource_links(resource_id,item_id)VALUES('other',?1)",
                [&episode],
            )
            .unwrap();
        }
        ignore(&mut conn, &file);
        if another_version {
            assert_eq!(store::items(&conn).unwrap().len(), 4);
            assert_eq!(store::item(&conn, &season).unwrap()["playable"], true);
            assert_eq!(store::resources_for(&conn, &season).unwrap().len(), 1);
        } else {
            assert!(store::items(&conn).unwrap().is_empty());
        }
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0
        );
    }
}

#[test]
fn rules_can_be_cancelled_offline_and_reject_outside_or_root_paths() {
    let (_data, media, mut conn, _job) = setup();
    let outside = tempfile::tempdir().unwrap();
    assert!(save(
        &mut conn,
        &json!({"sourceId":"s","path":outside.path().to_string_lossy()})
    )
    .is_err());
    assert!(save(
        &mut conn,
        &json!({"sourceId":"s","path":media.path().to_string_lossy()})
    )
    .is_err());
    let folder = media.path().join("Extras");
    fs::create_dir(&folder).unwrap();
    let file = folder.join("x.mkv");
    fs::write(&file, b"sample").unwrap();
    ignore(&mut conn, &file);
    ignore(&mut conn, &folder);
    assert_eq!(rules(&conn, "s").unwrap().len(), 2);
    fs::remove_file(&file).unwrap();
    fs::remove_dir(&folder).unwrap();
    save(
        &mut conn,
        &json!({"sourceId":"s","path":file.to_string_lossy(),"remove":true}),
    )
    .unwrap();
    assert!(contains(&conn, "s", file.to_string_lossy().as_ref()).unwrap());
    save(
        &mut conn,
        &json!({"sourceId":"s","path":folder.to_string_lossy(),"remove":true}),
    )
    .unwrap();
    assert!(rules(&conn, "s").unwrap().is_empty());
}

#[test]
fn marker_prunes_a_subtree_before_first_scan_and_honors_source_root() {
    let (data, media, conn, job) = setup();
    let skipped = media.path().join("Windows.old");
    let sibling = media.path().join("Windows.old2");
    fs::create_dir_all(skipped.join("Nested")).unwrap();
    fs::create_dir(&sibling).unwrap();
    fs::write(skipped.join("Nested/Skip.mkv"), b"sample").unwrap();
    fs::write(sibling.join("Keep.mkv"), b"sample").unwrap();
    fs::write(skipped.join(MARKER_FILE), b"").unwrap();
    let first = scan::scan_source(data.path(), "s", &job).unwrap();
    assert_eq!(first["files"], 1);
    assert_eq!(first["complete"], true);
    assert_eq!(store::items(&conn).unwrap()[0]["title"], "Keep.mkv");
    fs::write(media.path().join(MARKER_FILE), b"").unwrap();
    let second = scan::scan_source(data.path(), "s", &job).unwrap();
    assert_eq!(second["files"], 0);
    assert_eq!(second["complete"], true);
    assert!(store::items(&conn).unwrap().is_empty());
    assert!(sibling.join("Keep.mkv").is_file());
    assert!(skipped.join("Nested/Skip.mkv").is_file());
}

#[tokio::test]
async fn marker_blocks_pending_work_and_rescan_restores_the_same_resource_progress() {
    let (data, media, conn, job) = setup();
    let folder = media.path().join("Backup");
    fs::create_dir(&folder).unwrap();
    let file = folder.join("Movie.2024.mkv");
    fs::write(&file, b"sample").unwrap();
    scan::scan_source(data.path(), "s", &job).unwrap();
    let item = store::items(&conn).unwrap().remove(0);
    let item_id = store::text(&item, "id");
    let resource = store::resources_for(&conn, item_id).unwrap().remove(0);
    let resource_id = store::text(&resource, "id");
    store::update_record(
        &conn,
        item_id,
        &json!({"rating":4,"comment":"保留个人记录"}),
        true,
    )
    .unwrap();
    conn.execute("UPDATE resources SET position_seconds=123,duration_seconds=1000,technical_json='{\"state\":\"ready\"}' WHERE id=?1", [resource_id]).unwrap();
    conn.execute(
        "INSERT INTO media_playback_coverage(resource_id,segments_json) VALUES(?1,'[[0,123]]')",
        [resource_id],
    )
    .unwrap();

    let marker = folder.join(MARKER_FILE);
    fs::write(&marker, b"").unwrap();
    // 标记在扫描后新增，已有可用状态不影响所有入口立即拒绝该路径。
    assert_eq!(
        store::resource(&conn, resource_id).unwrap()["ignored"],
        true
    );
    assert!(!metadata_allowed(&conn, item_id).unwrap());
    assert!(super::super::probe::probe_resource(data.path(), resource_id).is_err());
    assert!(super::super::metadata::auto_identify(data.path(), item_id)
        .await
        .is_err());
    let skipped = scan::scan_source(data.path(), "s", &job).unwrap();
    assert_eq!(skipped["files"], 0);
    let retained = store::resource(&conn, resource_id).unwrap();
    assert_eq!(retained["missing"], true);
    assert_eq!(retained["positionSeconds"], 123.0);
    assert_eq!(store::item(&conn, item_id).unwrap()["rating"], 4.0);

    fs::remove_file(marker).unwrap();
    let restored = scan::scan_source(data.path(), "s", &job).unwrap();
    assert_eq!(restored["changedResourceIds"], json!([]));
    let resource = store::resource(&conn, resource_id).unwrap();
    assert_eq!(resource["ignored"], false);
    assert_eq!(resource["available"], true);
    assert_eq!(resource["missing"], false);
    assert_eq!(resource["positionSeconds"], 123.0);
    assert_eq!(resource["technical"]["state"], "ready");
    assert_eq!(
        conn.query_row(
            "SELECT segments_json FROM media_playback_coverage WHERE resource_id=?1",
            [resource_id],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "[[0,123]]"
    );
    assert_eq!(
        store::item(&conn, item_id).unwrap()["comment"],
        "保留个人记录"
    );
}

#[test]
fn metadata_tasks_require_active_resources_even_when_personal_records_are_retained() {
    let (data, media, conn, job) = setup();
    fs::write(media.path().join("Movie.mkv"), b"sample").unwrap();
    scan::scan_source(data.path(), "s", &job).unwrap();
    let item = store::items(&conn).unwrap().remove(0);
    let id = store::text(&item, "id");
    store::update_record(&conn, id, &json!({"rating":4}), true).unwrap();
    assert!(metadata_allowed(&conn, id).unwrap());
    store::set_setting(&conn, "source.disabled.s", &json!(true)).unwrap();
    assert!(!metadata_allowed(&conn, id).unwrap());
    store::set_setting(&conn, "source.disabled.s", &json!(false)).unwrap();
    conn.execute("UPDATE resources SET available=0", [])
        .unwrap();
    assert!(!metadata_allowed(&conn, id).unwrap());
    conn.execute("UPDATE resources SET available=1,missing=1", [])
        .unwrap();
    assert!(!metadata_allowed(&conn, id).unwrap());
    assert_eq!(store::item(&conn, id).unwrap()["rating"], 4.0);
    let independent =
        store::new_item(&conn, &json!({"title":"独立收录","independent":true})).unwrap();
    assert!(metadata_allowed(&conn, &independent).unwrap());
}
