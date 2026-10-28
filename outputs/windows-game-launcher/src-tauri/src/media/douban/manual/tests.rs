use super::*;

#[test]
fn preview_artwork_requires_matching_item_token_and_unexpired_preview() {
    let dir = tempfile::tempdir().unwrap();
    let rt = runtime(dir.path());
    assert!(preview_poster(dir.path(), "item", "token").is_err());
    *rt.preview.lock().unwrap() = Some(Preview {
        token: "token".into(), item: "item".into(), identity: json!([]),
        detail: detail("movie"), created: Instant::now(),
    });
    assert!(preview_poster(dir.path(), "other", "token").is_err());
    assert!(preview_poster(dir.path(), "item", "wrong").is_err());
    assert!(preview_poster(dir.path(), "item", "").is_err());
    assert!(preview_poster(tempfile::tempdir().unwrap().path(), "item", "token").is_err());
    assert_eq!(preview_poster(dir.path(), "item", "token").unwrap(), detail("movie")["posterUrl"]);
    rt.preview.lock().unwrap().as_mut().unwrap().created = Instant::now() - Duration::from_secs(901);
    assert!(preview_poster(dir.path(), "item", "token").is_err());
}

fn detail(kind: &str) -> Value {
    normalize(&json!({"detail":{"subjectId":"42","title":"豆瓣作品","kind":kind,"year":"2024",
        "overview":"完整简介","posterUrl":"https://img1.doubanio.com/view/photo/s_ratio_poster/public/test.jpg",
        "genres":["剧情"],"imdbId":"tt1234567","seasonNumber":null}}),"42").unwrap()
}
fn persist(
    conn: &mut Connection,
    item: &str,
    detail: &Value,
    expected: &Value,
) -> Result<Value, String> {
    let tx = conn.transaction().unwrap();
    let result = save(&tx, item, detail, expected)?;
    tx.commit().unwrap();
    Ok(result)
}

#[test]
fn parses_only_douban_subject_urls_or_numbers() {
    for raw in [
        "42",
        " https://movie.douban.com/subject/42/?from=search#info ",
    ] {
        assert_eq!(subject_id(raw).unwrap(), "42");
    }
    for raw in [
        "",
        "0",
        "https://evil.test/subject/42/",
        "https://movie.douban.com@evil.test/subject/42/",
        "https://movie.douban.com/subject/42/photos/",
        "http://movie.douban.com/subject/42/",
        "https://movie.douban.com:444/subject/42/",
    ] {
        assert!(subject_id(raw).is_err(), "{raw}");
    }
}

#[test]
fn normalization_rejects_wrong_subject_unknown_type_and_restricted_pages() {
    for page in [
        json!({"restricted":true}),
        json!({"login":true}),
        json!({"detail":{"subjectId":"43","kind":"movie","title":"错误作品"}}),
        json!({"detail":{"subjectId":"42","kind":"","title":"类型未知"}}),
        json!({"detail":{"subjectId":"42","kind":"series","title":"作品","seasonNumber":-1}}),
    ] {
        assert!(normalize(&page, "42").is_err());
    }
    let raw = json!({"detail":{"subjectId":"42","title":"作品","kind":"movie","posterUrl":"https://evil.test/poster.jpg"}});
    assert_eq!(normalize(&raw, "42").unwrap()["posterUrl"], "");
}

#[test]
fn applies_season_in_place_without_episodes_and_preserves_files_progress_and_personal_data() {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = store::open(dir.path()).unwrap();
    let item =
        store::new_item(&conn, &json!({"title":"Local.E01.mkv","independent":true})).unwrap();
    store::update_record(&conn,&item,&json!({"rating":4,"comment":"我的短评","watched":true,"favorite":true,"markedAt":"2024-05-01"}),true).unwrap();
    conn.execute(
        "INSERT INTO sources(id,path,label,created_at)VALUES('s','M:/','本地',?1)",
        [store::now()],
    )
    .unwrap();
    conn.execute("INSERT INTO resources(id,source_id,path,position_seconds,duration_seconds,updated_at)VALUES('r','s','M:/E01.mkv',321,1200,?1)",[store::now()]).unwrap();
    conn.execute(
        "INSERT INTO resource_links(resource_id,item_id)VALUES('r',?1)",
        [&item],
    )
    .unwrap();
    conn.execute("INSERT INTO view_sessions(id,item_id,resource_id,started_at,seconds,position_seconds)VALUES('v',?1,'r',?2,40,321)",params![item,store::now()]).unwrap();
    conn.execute(
        "INSERT INTO media_match_errors(item_id,error,checked_at)VALUES(?1,'旧失败',?2)",
        params![item, store::now()],
    )
    .unwrap();
    let before = store::item(&conn, &item).unwrap();
    let result = persist(&mut conn, &item, &detail("series"), &identity(&before)).unwrap();
    assert_eq!(result["id"], item);
    assert_eq!(result["kind"], "season");
    assert_eq!(result["recognized"], true);
    assert_eq!(result["recognitionStatus"], "recognized");
    assert!(result["seasonNumber"].is_null());
    assert_eq!(result["imdbId"], "tt1234567");
    for key in [
        "rating",
        "comment",
        "watched",
        "favorite",
        "markedAt",
        "independent",
        "positionSeconds",
        "durationSeconds",
        "lastPlayed",
    ] {
        assert_eq!(result[key], before[key], "{key}");
    }
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM media_match_errors", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row(
            "SELECT position_seconds FROM resources WHERE id='r'",
            [],
            |r| r.get::<_, f64>(0)
        )
        .unwrap(),
        321.0
    );
    assert_eq!(
        conn.query_row("SELECT item_id FROM view_sessions WHERE id='v'", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        item
    );
    assert_eq!(
        conn.query_row(
            "SELECT locked FROM resource_links WHERE resource_id='r'",
            [],
            |r| r.get::<_, bool>(0)
        )
        .unwrap(),
        true
    );
}

#[test]
fn refresh_and_unlink_keep_import_provenance_and_personal_records() {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = store::open(dir.path()).unwrap();
    super::super::import_page(&conn,"account",&[json!({"subjectId":"42","title":"旧名称","stars":5,"markedAt":"2024-01-01","comment":"原始短评","tags":[]})]).unwrap();
    let item: String = conn
        .query_row(
            "SELECT item_id FROM external_ids WHERE external_id='42'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let before = store::item(&conn, &item).unwrap();
    let raw: String = conn
        .query_row(
            "SELECT raw_json FROM douban_source_data WHERE account_key='account'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let first = persist(&mut conn, &item, &detail("movie"), &identity(&before)).unwrap();
    let mut updated = detail("movie");
    updated["title"] = json!("更新后的作品名");
    let second = persist(&mut conn, &item, &updated, &identity(&first)).unwrap();
    assert_eq!(second["title"], "更新后的作品名");
    for key in ["rating", "comment", "markedAt", "watched", "recordSource"] {
        assert_eq!(second[key], before[key]);
    }
    assert_eq!(
        conn.query_row(
            "SELECT raw_json FROM douban_source_data WHERE account_key='account'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        raw
    );
    assert_eq!(
        super::super::import_page(
            &conn,
            "account",
            &[json!({"subjectId":"42","title":"重复"})]
        )
        .unwrap(),
        0
    );
}

#[test]
fn cannot_overwrite_tmdb_or_other_douban_identity_or_duplicate_an_existing_subject() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let tmdb = store::upsert_provider_item(
        &conn,
        "tmdb",
        "movie",
        "100",
        &json!({"title":"已有电影","kind":"movie"}),
        true,
    )
    .unwrap();
    assert!(check_item(&conn, &tmdb, "42").is_err());
    let douban = store::upsert_provider_item(
        &conn,
        "douban",
        "subject",
        "42",
        &json!({"title":"豆瓣"}),
        true,
    )
    .unwrap();
    assert!(check_item(&conn, &douban, "43").is_err());
    let other = store::new_item(&conn, &json!({"title":"另一个文件"})).unwrap();
    assert!(check_item(&conn, &other, "42").is_err());
}

#[test]
fn stale_preview_cannot_replace_new_identity_but_personal_edits_are_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = store::open(dir.path()).unwrap();
    let item = store::new_item(&conn, &json!({"title":"待识别"})).unwrap();
    let expected = identity(&store::item(&conn, &item).unwrap());
    conn.execute(
        "UPDATE media_items SET title='修改后的名称' WHERE id=?1",
        [&item],
    )
    .unwrap();
    assert!(persist(&mut conn, &item, &detail("movie"), &expected).is_err());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM external_ids", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let expected = identity(&store::item(&conn, &item).unwrap());
    store::update_record(
        &conn,
        &item,
        &json!({"rating":3,"comment":"预览期间编辑"}),
        true,
    )
    .unwrap();
    let after = persist(&mut conn, &item, &detail("movie"), &expected).unwrap();
    assert_eq!(after["comment"], "预览期间编辑");
}
