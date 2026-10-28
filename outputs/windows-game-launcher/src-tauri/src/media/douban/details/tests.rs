use super::*;
use crate::media::douban::import_page;

fn page() -> Value {
    json!({"loggedIn":true,"accountKey":"","detail":{"subjectId":"42","title":"某剧 第二季","imdbId":"tt1234567","kind":"series","seasonNumber":2}})
}

#[test]
fn validates_subject_login_identity_and_numbers() {
    assert!(validate(&page(), "42").is_ok());
    let mut public = page();
    public["loggedIn"] = json!(false);
    public["login"] = json!(true);
    public["accountKey"] = json!("another-account");
    assert!(validate(&public, "42").is_ok());
    for patch in [
        json!({"restricted":true}),
        json!({"detail":{"subjectId":"43"}}),
        json!({"detail":{"subjectId":"42","title":"x","imdbId":"tt123/../../"}}),
    ] {
        let mut value = page();
        for (k, v) in patch.as_object().unwrap() {
            value[k] = v.clone();
        }
        assert!(validate(&value, "42").is_err());
    }
}

#[test]
fn source_cache_preserves_records_survives_merge_and_dedupes_shared_imdb() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let rows = vec![
        json!({"subjectId":"42","title":"某剧 第一季","stars":4,"markedAt":"2020-01-02","comment":"原短评","tags":["测试"]}),
        json!({"subjectId":"43","title":"某剧 第二季","stars":3}),
    ];
    import_page(&conn, "a", &rows).unwrap();
    let mut item_ids = Vec::new();
    for row in &rows {
        let id: String = conn
            .query_row(
                "SELECT item_id FROM external_ids WHERE provider='douban' AND external_id=?1",
                [row["subjectId"].as_str().unwrap()],
                |r| r.get(0),
            )
            .unwrap();
        let before = store::item(&conn, &id).unwrap();
        let source = cache::source(&conn, &id).unwrap().unwrap();
        assert!(cache::due(&source.raw));
        cache::save(&conn, &id, &source, &Err("网络失败".into())).unwrap();
        assert!(!cache::due(
            &cache::source(&conn, &id).unwrap().unwrap().raw
        ));
        let detail = json!({"subjectId":row["subjectId"],"imdbId":"tt1234567","kind":"series"});
        cache::save(&conn, &id, &source, &Ok(detail)).unwrap();
        assert_eq!(store::item(&conn, &id).unwrap()["imdbId"], "tt1234567");
        let after = store::item(&conn, &id).unwrap();
        for key in ["rating", "comment", "markedAt", "watched", "tags"] {
            assert_eq!(before[key], after[key]);
        }
        let raw = cache::source(&conn, &id).unwrap().unwrap().raw;
        assert!(!cache::due(&raw));
        for (k, v) in row.as_object().unwrap() {
            assert_eq!(&raw[k], v);
        }
        item_ids.push(id);
    }
    assert_ne!(item_ids[0], item_ids[1]);
    let target = store::upsert_provider_item(
        &conn,
        "tmdb",
        "season",
        "100:1",
        &json!({"title":"第1季","kind":"season"}),
        true,
    )
    .unwrap();
    store::merge_items(&conn, &item_ids[0], &target).unwrap();
    assert_eq!(store::item(&conn, &target).unwrap()["imdbId"], "tt1234567");
    assert_eq!(import_page(&conn, "a", &rows).unwrap(), 0);
    let source = cache::source(&conn, &item_ids[1]).unwrap().unwrap();
    assert!(cache::save(&conn, &target, &source, &Ok(json!({}))).is_err());
}

#[test]
fn caches_absent_ids_and_retries_failures_without_unbounded_requests() {
    let now = Utc::now();
    let absent = json!({"detail":{"imdbId":""},"detailCheckedAt":now.to_rfc3339()});
    assert!(!cache::due(&absent));
    assert!(cache::due(
        &json!({"detail":{"imdbId":""},"detailCheckedAt":(now-chrono::Duration::days(8)).to_rfc3339()})
    ));
    assert!(cache::due(
        &json!({"detailError":"失败","detailCheckedAt":(now-chrono::Duration::minutes(16)).to_rfc3339()})
    ));
    assert!(cache::due(
        &json!({"detail":{"imdbId":""},"detailError":"失败","detailCheckedAt":(now-chrono::Duration::minutes(16)).to_rfc3339()})
    ));
    assert!(eligible(&json!({})));
    assert!(eligible(&json!({"connected":false,"needsLogin":true})));
    assert!(!eligible(&json!({"phase":"loggingIn"})));
    let rt = Arc::new(Runtime::default());
    rt.busy.store(true, Ordering::Release);
    {
        let _lease = BrowserLease(rt.clone());
    }
    assert!(!rt.busy.load(Ordering::Acquire));
    rt.generation.store(2, Ordering::Release);
    assert!(active(&rt, 1).is_err());
}

use chrono::Utc;
