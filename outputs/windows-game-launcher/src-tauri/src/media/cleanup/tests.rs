use super::*;
use rusqlite::{params, OptionalExtension};

struct Fixture {
    _dir: tempfile::TempDir,
    conn: Connection,
    root: String,
    season: String,
    episode: String,
    other: String,
}
fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let root = store::upsert_provider_item(
        &conn,
        "tmdb",
        "tv",
        "255916",
        &json!({"kind":"series","title":"chongfu"}),
        false,
    )
    .unwrap();
    let season = store::upsert_provider_item(
        &conn,
        "tmdb",
        "season",
        "255916:2",
        &json!({"kind":"season","title":"Season 2","parentId":root,"seasonNumber":2}),
        false,
    )
    .unwrap();
    let episode=store::upsert_provider_item(&conn,"tmdb","episode","255916:2:1",&json!({"kind":"episode","title":"wl","parentId":season,"seasonNumber":2,"episodeNumber":1}),false).unwrap();
    let other = store::new_item(
        &conn,
        &json!({"kind":"season","title":"庆余年第二季","independent":true}),
    )
    .unwrap();
    store::update_record(&conn, &other, &json!({"rating":4,"watched":true}), true).unwrap();
    Fixture {
        _dir: dir,
        conn,
        root,
        season,
        episode,
        other,
    }
}
fn input(value: &Value, records: bool) -> Value {
    json!({"itemId":value["itemId"],"fingerprint":value["fingerprint"],"deleteRecords":records})
}
fn add_records(f: &Fixture) {
    store::update_record(
        &f.conn,
        &f.season,
        &json!({"rating":4,"watched":true,"favorite":true,"comment":"原有短评"}),
        true,
    )
    .unwrap();
    f.conn
        .execute(
            "INSERT INTO sources(id,path,label,created_at)VALUES('s','C:/Media','media',?1)",
            [store::now()],
        )
        .unwrap();
    f.conn.execute("INSERT INTO resources(id,source_id,path,position_seconds,duration_seconds,updated_at)VALUES('history','s','C:/Media/history.mkv',35,100,?1)",[store::now()]).unwrap();
    f.conn.execute("INSERT INTO view_sessions VALUES('session',?1,'history','2024-01-01','2024-01-02',30,35)",[&f.episode]).unwrap();
    f.conn
        .execute(
            "INSERT INTO media_playback_coverage VALUES('history','[[0,35]]')",
            [],
        )
        .unwrap();
    f.conn
        .execute(
            "INSERT INTO external_ids VALUES('douban','subject','42',?1)",
            [&f.season],
        )
        .unwrap();
    f.conn.execute("INSERT INTO douban_records(account_key,subject_id,item_id,imported_at)VALUES('a','42',?1,'2024-01-01')",[&f.season]).unwrap();
    f.conn
        .execute("INSERT INTO douban_source_data VALUES('a','42','{}')", [])
        .unwrap();
    f.conn
        .execute("INSERT INTO media_collections VALUES('c','收藏夹',0)", [])
        .unwrap();
    f.conn
        .execute("INSERT INTO collection_items VALUES('c',?1)", [&f.season])
        .unwrap();
    f.conn
        .execute("INSERT INTO media_tags VALUES('t','标签')", [])
        .unwrap();
    f.conn
        .execute("INSERT INTO media_item_tags VALUES(?1,'t')", [&f.season])
        .unwrap();
    f.conn
        .execute(
            "INSERT INTO media_match_errors VALUES(?1,'识别失败','2024-01-01')",
            [&f.episode],
        )
        .unwrap();
}
#[test]
fn chongfu_metadata_only_season_can_be_previewed_read_only_and_cleans_empty_parent() {
    let mut f = fixture();
    let other = store::item(&f.conn, &f.other).unwrap();
    f.conn.execute_batch("PRAGMA query_only=ON").unwrap();
    let plan = preview(&f.conn, &f.season).unwrap();
    assert_eq!(plan["allowed"], true);
    assert_eq!(plan["summary"]["items"], 3);
    assert_eq!(plan["summary"]["parents"], 1);
    assert_eq!(plan["hasRecords"], false);
    f.conn.execute_batch("PRAGMA query_only=OFF").unwrap();
    let result = apply(&mut f.conn, &input(&plan, false)).unwrap();
    assert_eq!(result["removed"], 3);
    for id in [&f.root, &f.season, &f.episode] {
        assert!(store::item(&f.conn, id).is_err());
    }
    assert_eq!(store::item(&f.conn, &f.other).unwrap(), other);
}
#[test]
fn records_require_separate_consent_and_then_delete_only_scoped_records() {
    let mut f = fixture();
    add_records(&f);
    let plan = preview(&f.conn, &f.season).unwrap();
    let before = plan["fingerprint"].clone();
    assert_eq!(plan["hasRecords"], true);
    for key in [
        "ratings",
        "comments",
        "favorites",
        "watched",
        "sessions",
        "doubanRecords",
        "collectionLinks",
        "tagLinks",
    ] {
        assert_eq!(plan["summary"][key], 1, "{key}");
    }
    assert!(apply(&mut f.conn, &input(&plan, false)).is_err());
    assert_eq!(preview(&f.conn, &f.season).unwrap()["fingerprint"], before);
    let other = store::item(&f.conn, &f.other).unwrap();
    apply(&mut f.conn, &input(&plan, true)).unwrap();
    assert_eq!(store::item(&f.conn, &f.other).unwrap(), other);
    for table in [
        "view_sessions",
        "douban_records",
        "collection_items",
        "media_item_tags",
        "douban_source_data",
        "media_match_errors",
    ] {
        assert_eq!(
            scalar(
                &f.conn,
                &format!("SELECT COUNT(*) FROM {table} WHERE ?1 IS NOT NULL"),
                "x"
            )
            .unwrap(),
            0,
            "{table}"
        );
    }
    for table in ["media_collections", "media_tags"] {
        assert_eq!(
            scalar(
                &f.conn,
                &format!("SELECT COUNT(*) FROM {table} WHERE ?1 IS NOT NULL"),
                "x"
            )
            .unwrap(),
            1
        );
    }
    let resource = store::resource(&f.conn, "history").unwrap();
    assert_eq!(resource["positionSeconds"], 35.0);
    assert_eq!(resource["itemIds"], json!([]));
    let coverage: String = f
        .conn
        .query_row(
            "SELECT segments_json FROM media_playback_coverage WHERE resource_id='history'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(coverage, "[[0,35]]");
    assert_eq!(
        f.conn
            .query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
            .optional()
            .unwrap(),
        None
    );
}
#[test]
fn offline_or_missing_resource_links_block_cleanup_even_with_record_consent() {
    for missing in [false, true] {
        let mut f = fixture();
        f.conn
            .execute(
                "INSERT INTO sources(id,path,label,created_at)VALUES('s','C:/Media','media',?1)",
                [store::now()],
            )
            .unwrap();
        f.conn.execute("INSERT INTO resources(id,source_id,path,available,missing,updated_at)VALUES('r','s','C:/Media/file.mkv',0,?1,?2)",params![missing,store::now()]).unwrap();
        f.conn
            .execute(
                "INSERT INTO resource_links(resource_id,item_id)VALUES('r',?1)",
                [&f.episode],
            )
            .unwrap();
        let plan = preview(&f.conn, &f.season).unwrap();
        assert_eq!(plan["allowed"], false);
        assert_eq!(plan["summary"]["linkedResources"], 1);
        assert!(apply(&mut f.conn, &input(&plan, true)).is_err());
        assert!(store::item(&f.conn, &f.episode).is_ok());
    }
}
#[test]
fn changed_record_children_or_parent_prevents_stale_confirmation() {
    for change in ["rating", "child", "parent"] {
        let mut f = fixture();
        store::update_record(&f.conn, &f.season, &json!({"rating":4}), true).unwrap();
        let plan = preview(&f.conn, &f.season).unwrap();
        match change {
            "rating" => {
                store::update_record(&f.conn, &f.season, &json!({"rating":5}), true).unwrap();
            }
            "child" => {
                store::new_item(
                    &f.conn,
                    &json!({"kind":"episode","title":"新增子项","parentId":f.season}),
                )
                .unwrap();
            }
            _ => {
                store::update_record(&f.conn, &f.root, &json!({"comment":"新父级记录"}), true)
                    .unwrap();
            }
        }
        let current = preview(&f.conn, &f.season).unwrap();
        assert!(apply(&mut f.conn, &input(&plan, true)).is_err());
        assert_eq!(
            preview(&f.conn, &f.season).unwrap()["fingerprint"],
            current["fingerprint"]
        );
    }
}
#[test]
fn cleanup_keeps_parents_with_records_or_other_children_and_all_unrelated_data() {
    for own_record in [true, false] {
        let mut f = fixture();
        let sibling = if own_record {
            store::update_record(&f.conn, &f.root, &json!({"rating":5}), true).unwrap();
            None
        } else {
            Some(
                store::new_item(
                    &f.conn,
                    &json!({"kind":"season","title":"保留另一季","parentId":f.root}),
                )
                .unwrap(),
            )
        };
        let plan = preview(&f.conn, &f.season).unwrap();
        assert_eq!(plan["summary"]["parents"], 0);
        let parent = store::item(&f.conn, &f.root).unwrap();
        apply(&mut f.conn, &input(&plan, false)).unwrap();
        assert_eq!(store::item(&f.conn, &f.root).unwrap(), parent);
        if let Some(id) = sibling {
            assert!(store::item(&f.conn, &id).is_ok());
        }
    }
}
#[test]
fn explicit_single_episode_cleanup_does_not_expand_to_other_episodes() {
    let mut f = fixture();
    let sibling = store::new_item(
        &f.conn,
        &json!({"kind":"episode","title":"保留同季另一集","parentId":f.season}),
    )
    .unwrap();
    let plan = preview(&f.conn, &f.episode).unwrap();
    assert_eq!(plan["summary"]["items"], 1);
    apply(&mut f.conn, &input(&plan, false)).unwrap();
    assert!(store::item(&f.conn, &sibling).is_ok());
    assert!(store::item(&f.conn, &f.season).is_ok());
}
#[test]
fn deletion_error_rolls_back_items_records_and_provenance_together() {
    let mut f = fixture();
    add_records(&f);
    let plan = preview(&f.conn, &f.season).unwrap();
    f.conn.execute_batch("CREATE TRIGGER fail_cleanup BEFORE DELETE ON media_items BEGIN SELECT RAISE(ABORT,'simulated failure'); END;").unwrap();
    assert!(apply(&mut f.conn, &input(&plan, true)).is_err());
    assert_eq!(
        preview(&f.conn, &f.season).unwrap()["fingerprint"],
        plan["fingerprint"]
    );
}
#[test]
fn shared_source_cache_is_retained_for_other_accounts_and_items() {
    let mut f = fixture();
    add_records(&f);
    f.conn.execute("INSERT INTO douban_records(account_key,subject_id,item_id,imported_at)VALUES('b','42',?1,'2024-01-01')",[&f.other]).unwrap();
    let plan = preview(&f.conn, &f.season).unwrap();
    apply(&mut f.conn, &input(&plan, true)).unwrap();
    assert_eq!(
        scalar(
            &f.conn,
            "SELECT COUNT(*) FROM douban_source_data WHERE subject_id=?1",
            "42"
        )
        .unwrap(),
        1
    );
    assert_eq!(
        scalar(
            &f.conn,
            "SELECT COUNT(*) FROM douban_records WHERE item_id=?1",
            &f.other
        )
        .unwrap(),
        1
    );
}
#[test]
fn explicit_independent_item_cleanup_is_allowed_but_empty_id_never_cleans_library() {
    let mut f = fixture();
    assert!(preview(&f.conn, "").is_err());
    let id = store::new_item(
        &f.conn,
        &json!({"kind":"movie","title":"主动收录的未看电影","independent":true}),
    )
    .unwrap();
    let plan = preview(&f.conn, &id).unwrap();
    assert_eq!(plan["hasRecords"], false);
    apply(&mut f.conn, &input(&plan, false)).unwrap();
    assert!(store::item(&f.conn, &id).is_err());
    assert!(store::item(&f.conn, &f.other).is_ok());
}
