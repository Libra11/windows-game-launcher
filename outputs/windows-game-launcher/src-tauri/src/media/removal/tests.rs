use super::*;

fn setup() -> (tempfile::TempDir, Connection) {
    let temp = tempfile::tempdir().unwrap();
    let conn = store::open(temp.path()).unwrap();
    for source in ["s", "other"] {
        conn.execute(
            "INSERT INTO sources(id,path,label,created_at)VALUES(?1,?2,'不可访问的媒体源','')",
            params![source, format!("Z:/offline/{source}")],
        )
        .unwrap();
    }
    (temp, conn)
}
fn media_item(conn: &Connection, kind: &str, parent: Option<&str>, independent: bool) -> String {
    let id=store::new_item(conn,&json!({"title":"测试作品","kind":kind,"parentId":parent,"independent":independent,"recognized":true})).unwrap();
    conn.execute("INSERT INTO external_ids(provider,provider_type,external_id,item_id)VALUES('tmdb',?1,?2,?3)",params![kind,uuid::Uuid::new_v4().to_string(),id]).unwrap();
    id
}
fn resource(conn: &Connection, source: &str, item: Option<&str>) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO resources(id,source_id,path,updated_at)VALUES(?1,?2,?3,'original')",
        params![id, source, format!("Z:/offline/{source}/{id}.mkv")],
    )
    .unwrap();
    if let Some(item) = item {
        conn.execute(
            "INSERT INTO resource_links(resource_id,item_id)VALUES(?1,?2)",
            params![id, item],
        )
        .unwrap();
    }
    id
}
fn remove(conn: &mut Connection, ids: &[String]) -> Value {
    let tx = conn.transaction().unwrap();
    let value = resources(&tx, ids).unwrap();
    tx.commit().unwrap();
    value
}
fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .unwrap()
}
fn no_business_writes(conn: &Connection) {
    for table in [
        "resources",
        "media_items",
        "personal",
        "resource_links",
        "view_sessions",
        "media_playback_coverage",
        "external_ids",
        "media_item_metadata",
    ] {
        for action in ["INSERT", "UPDATE", "DELETE"] {
            conn.execute_batch(&format!("CREATE TRIGGER no_{table}_{action} BEFORE {action} ON {table} BEGIN SELECT RAISE(ABORT,'idempotent cleanup attempted a business write'); END;")).unwrap();
        }
    }
}

#[test]
fn tmdb_metadata_without_records_is_deleted_while_unrelated_independent_items_stay() {
    let (_temp, mut conn) = setup();
    let movie = media_item(&conn, "movie", None, false);
    let keep = media_item(&conn, "movie", None, true);
    let file = resource(&conn, "s", Some(&movie));
    conn.execute(
        "INSERT INTO media_item_metadata(item_id,air_date)VALUES(?1,'2024-01-01')",
        [&movie],
    )
    .unwrap();
    let result = remove(&mut conn, &[file.clone(), file, "already-deleted".into()]);
    assert_eq!(result["resources"], 1);
    assert_eq!(result["items"], 1);
    assert!(item(&conn, &movie).unwrap().is_none());
    assert!(item(&conn, &keep).unwrap().is_some());
    assert_eq!(count(&conn, "resources"), 0);
    assert_eq!(count(&conn, "media_item_metadata"), 0);
    no_business_writes(&conn);
    assert_eq!(remove(&mut conn, &["already-deleted".into()])["items"], 0);
}

#[test]
fn removing_multiple_seasons_cleans_the_shared_empty_parent_once() {
    let (_temp, mut conn) = setup();
    let series = media_item(&conn, "series", None, false);
    let first = media_item(&conn, "season", Some(&series), false);
    let second = media_item(&conn, "season", Some(&series), false);
    let a = media_item(&conn, "episode", Some(&first), false);
    media_item(&conn, "episode", Some(&first), false);
    let b = media_item(&conn, "episode", Some(&second), false);
    let files = [
        resource(&conn, "s", Some(&a)),
        resource(&conn, "s", Some(&b)),
    ];
    let result = remove(&mut conn, &files);
    assert_eq!(result["items"], 6);
    assert_eq!(count(&conn, "media_items"), 0);
    assert_eq!(count(&conn, "resources"), 0);
    assert_eq!(count(&conn, "pragma_foreign_key_check"), 0);
}

#[test]
fn another_unremoved_version_preserves_the_complete_season_tree() {
    let (_temp, mut conn) = setup();
    let series = media_item(&conn, "series", None, false);
    let season = media_item(&conn, "season", Some(&series), false);
    let episode = media_item(&conn, "episode", Some(&season), false);
    let unavailable_episode = media_item(&conn, "episode", Some(&season), false);
    let removed = resource(&conn, "s", Some(&episode));
    let other = resource(&conn, "other", Some(&episode));
    assert_eq!(remove(&mut conn, &[removed])["items"], 0);
    assert_eq!(count(&conn, "media_items"), 4);
    assert!(item(&conn, &unavailable_episode).unwrap().is_some());
    assert_eq!(count(&conn, "resources"), 1);
    assert_eq!(remove(&mut conn, &[other])["items"], 4);
    assert_eq!(count(&conn, "media_items"), 0);
}

#[test]
fn retained_history_and_progress_are_not_rewritten_by_repeated_removal() {
    let (_temp, mut conn) = setup();
    let movie = media_item(&conn, "movie", None, false);
    let file = resource(&conn, "s", Some(&movie));
    store::update_record(
        &conn,
        &movie,
        &json!({"rating":4,"comment":"保留短评"}),
        true,
    )
    .unwrap();
    conn.execute("UPDATE resources SET available=0,missing=1,position_seconds=42,duration_seconds=100,updated_at='retained-stamp' WHERE id=?1",[&file]).unwrap();
    conn.execute(
        "INSERT INTO media_playback_coverage(resource_id,segments_json)VALUES(?1,'[[0,42]]')",
        [&file],
    )
    .unwrap();
    conn.execute("INSERT INTO view_sessions(id,item_id,resource_id,started_at,ended_at,seconds,position_seconds)VALUES('history',?1,?2,'2024-01-01','2024-01-01',42,42)",params![movie,file]).unwrap();
    assert_eq!(remove(&mut conn, &[file.clone()])["items"], 0);
    no_business_writes(&conn);
    assert_eq!(remove(&mut conn, &[file.clone()])["items"], 0);
    let saved: (String, f64, f64) = conn
        .query_row(
            "SELECT updated_at,position_seconds,duration_seconds FROM resources WHERE id=?1",
            [&file],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(saved, ("retained-stamp".into(), 42.0, 100.0));
    assert_eq!(count(&conn, "view_sessions"), 1);
    assert_eq!(count(&conn, "media_playback_coverage"), 1);
    assert_eq!(count(&conn, "resource_links"), 1);
    assert_eq!(count(&conn, "pragma_foreign_key_check"), 0);
}

#[test]
fn orphaned_cleanup_does_not_change_a_resource_now_linked_to_another_item() {
    let (_temp, mut conn) = setup();
    let old = media_item(&conn, "movie", None, false);
    let current = media_item(&conn, "movie", None, false);
    let file = resource(&conn, "s", Some(&current));
    let tx = conn.transaction().unwrap();
    assert_eq!(orphaned(&tx, &[old.clone()]).unwrap(), 1);
    tx.commit().unwrap();
    assert!(item(&conn, &old).unwrap().is_none());
    assert!(item(&conn, &current).unwrap().is_some());
    assert_eq!(
        conn.query_row(
            "SELECT available,missing FROM resources WHERE id=?1",
            [&file],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        )
        .unwrap(),
        (1, 0)
    );
}

#[test]
fn resource_progress_without_a_surviving_link_is_preserved() {
    let (_temp, mut conn) = setup();
    let file = resource(&conn, "s", None);
    conn.execute(
        "UPDATE resources SET position_seconds=42 WHERE id=?1",
        [&file],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO media_playback_coverage(resource_id,segments_json)VALUES(?1,'[[0,42]]')",
        [&file],
    )
    .unwrap();
    remove(&mut conn, &[file.clone()]);
    assert_eq!(count(&conn, "resources"), 1);
    assert_eq!(count(&conn, "media_playback_coverage"), 1);
    no_business_writes(&conn);
    remove(&mut conn, &[file]);
}
