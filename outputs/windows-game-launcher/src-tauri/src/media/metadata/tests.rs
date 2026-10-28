use super::*;
use std::collections::HashMap;
mod local;
mod imdb;
mod performance;
mod tiered;

#[test]
fn manual_merge_only_accepts_unrecognized_source_and_recognized_nonconflicting_target() {
    let unknown=json!({"recognized":false,"externalIds":[]});
    let known=json!({"recognized":true,"externalIds":[{"provider":"douban","providerType":"subject","externalId":"42"}]});
    assert!(validate_manual_merge(&unknown,&known).is_ok());
    assert!(validate_manual_merge(&known,&known).is_err());
    assert!(validate_manual_merge(&unknown,&unknown).is_err());
    assert!(validate_manual_merge(&json!({"recognized":false,"externalIds":[{"provider":"douban","providerType":"subject","externalId":"43"}]}),&known).is_err());
}

#[test]
fn unlink_douban_recognition_keeps_source_identity_files_and_personal_record() {
    let dir=tempfile::tempdir().unwrap();
    let conn=store::open(dir.path()).unwrap();
    let item=store::upsert_provider_item(&conn,"douban","subject","42",&json!({"title":"豆瓣季","kind":"season"}),true).unwrap();
    conn.execute("UPDATE media_items SET recognized=1 WHERE id=?1",[&item]).unwrap();
    store::update_record(&conn,&item,&json!({"rating":4,"comment":"保留记录","watched":true}),true).unwrap();
    let before=store::item(&conn,&item).unwrap();
    unlink_metadata(&conn,&item).unwrap();
    let after=store::item(&conn,&item).unwrap();
    assert_eq!(after["recognized"],false);
    for key in ["id","externalIds","rating","comment","watched","kind"] { assert_eq!(after[key],before[key],"{key}"); }
}
static FIXTURES: OnceLock<Mutex<HashMap<PathBuf, HashMap<String, Value>>>> = OnceLock::new();
pub(super) fn fixture(
    dir: &Path,
    path: &str,
    query: &[(&str, String)],
) -> Option<Result<Value, String>> {
    let map = FIXTURES.get_or_init(Default::default).lock().unwrap();
    let values = map.get(dir)?;
    let key = if path.starts_with("/search/") {
        format!(
            "{path}?{}",
            query
                .iter()
                .find(|(key, _)| *key == "query")
                .map(|(_, v)| v.as_str())
                .unwrap_or("")
        )
    } else {
        path.into()
    };
    let localized = query
        .iter()
        .find(|(key, _)| *key == "language")
        .map(|(_, language)| format!("{key}|language={language}"));
    Some(
        localized
            .as_ref()
            .and_then(|key| values.get(key))
            .or_else(|| values.get(&key))
            .cloned()
            .ok_or_else(|| format!("测试资料缺失：{key}")),
    )
}
fn fixtures(dir: &Path, entries: Vec<(&str, Value)>) {
    FIXTURES
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .insert(
            dir.to_path_buf(),
            entries.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        );
}
fn series() -> Value {
    json!({"id":100,"name":"某剧","original_name":"Some Show","first_air_date":"2020-01-01","overview":"整剧简介",
        "alternative_titles":{"results":[{"title":"别名"}]},
        "seasons":[{"season_number":0,"name":"特别篇","air_date":"2024-01-01"},
            {"season_number":1,"name":"第1季","air_date":"2020-01-01"},
            {"season_number":2,"name":"第2季","air_date":"2024-01-01"}]})
}

#[test]
fn candidate_identity_requires_provider_type_and_external_id() {
    for candidate in [
        json!({"kind":"movie","externalId":"200"}),
        json!({"mediaType":"movie","externalId":"200"}),
        json!({"providerType":"movie","id":200}),
        json!({"providerType":"series","externalId":"100"}),
        json!({"providerType":"movie","externalId":"200/path"}),
    ] {
        assert!(parse_id(&candidate).is_err());
    }
    for kind in ["movie", "tv"] {
        assert_eq!(
            parse_id(&json!({"providerType":kind,"externalId":"200"})).unwrap(),
            (kind.to_string(), "200".to_string())
        );
    }
}

#[tokio::test]
async fn explicit_filename_id_uses_the_current_candidate_contract_without_search() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let item = store::new_item(&conn, &json!({"title":"Local.Name.1999.{tmdb-200}.mkv"})).unwrap();
    let source = uuid::Uuid::new_v4().to_string();
    let resource = uuid::Uuid::new_v4().to_string();
    let file = dir.path().join("Local.Name.1999.{tmdb-200}.mkv");
    conn.execute(
        "INSERT INTO sources(id,path,label,created_at) VALUES(?1,?2,'编号识别测试',?3)",
        params![source, dir.path().to_string_lossy(), store::now()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO resources(id,source_id,path,updated_at) VALUES(?1,?2,?3,?4)",
        params![resource, source, file.to_string_lossy(), store::now()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO resource_links(resource_id,item_id) VALUES(?1,?2)",
        params![resource, item],
    )
    .unwrap();
    fixtures(
        dir.path(),
        vec![(
            "/movie/200",
            json!({"id":200,"title":"明确编号的影片","release_date":"2024-01-01"}),
        )],
    );
    let value = auto_identify(dir.path(), &item).await.unwrap();
    assert_eq!(value["title"], "明确编号的影片");
    assert_eq!(value["recognized"], true);
    assert_eq!(value["externalIds"][0]["externalId"], "200");
    assert_eq!(value["externalIds"][0]["providerType"], "movie");
}

#[tokio::test]
async fn adding_requires_a_tmdb_movie_or_selected_season_and_leaves_no_placeholder() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    fixtures(dir.path(), vec![("/tv/100", series())]);
    for candidate in [
        json!({"title":"仅输入的名字","kind":"movie"}),
        json!({"provider":"tmdb","providerType":"tv","externalId":"100","kind":"series"}),
        json!({"provider":"tmdb","providerType":"tv","externalId":"100","kind":"season"}),
        json!({"provider":"tmdb","providerType":"movie","externalId":"404","kind":"movie"}),
    ] {
        assert!(create_item(dir.path(), &candidate).await.is_err());
        assert!(store::items(&conn).unwrap().is_empty());
    }
}

#[tokio::test]
async fn adding_a_movie_reuses_provider_identity_and_preserves_personal_record() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    fixtures(
        dir.path(),
        vec![(
            "/movie/200",
            json!({"id":200,"title":"某电影","release_date":"2024-01-01"}),
        )],
    );
    let candidate =
        json!({"provider":"tmdb","providerType":"movie","externalId":"200","kind":"movie"});
    let first = create_item(dir.path(), &candidate).await.unwrap();
    let id = first["id"].as_str().unwrap();
    assert_eq!(first["recognized"], true);
    assert_eq!(first["independent"], true);
    assert_eq!(first["resourceCount"], 0);
    store::update_record(
        &conn,
        id,
        &json!({"watched":true,"rating":5,"comment":"保留短评","markedAt":"2020-02-03"}),
        true,
    )
    .unwrap();
    fixtures(dir.path(), vec![]); // 重复添加无需再次访问 TMDB。
    let again = create_item(dir.path(), &candidate).await.unwrap();
    assert_eq!(again["id"], first["id"]);
    assert_eq!(again["rating"], 5.0);
    assert_eq!(again["comment"], "保留短评");
    assert_eq!(again["markedAt"], "2020-02-03");
    assert_eq!(store::items(&conn).unwrap().len(), 1);
    assert_eq!(store::item(&conn, id).unwrap()["independent"], true);
}

#[tokio::test]
async fn adding_a_season_is_atomic_independent_and_uses_final_search_results() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    fixtures(
        dir.path(),
        vec![
            (
                "/search/multi?某剧",
                json!({"results":[{"id":100,"name":"某剧","media_type":"tv"}]}),
            ),
            ("/tv/100", series()),
            (
                "/tv/100/season/2",
                json!({"id":102,"season_number":2,"name":"第二季","episodes":[{"episode_number":1,"name":"第一集"},{"episode_number":2,"name":"第二集"}]}),
            ),
        ],
    );
    let rows = matching::search_targets(dir.path(), "", "某剧", "all")
        .await
        .unwrap();
    assert_eq!(rows.len(), 2); // 正常季可直接选择，不包含未指定的特别篇。
    assert_eq!(rows[1]["seriesTitle"],"某剧");
    assert_eq!(rows[1]["title"],"某剧 · 第2季");
    assert!(store::items(&conn).unwrap().is_empty()); // 搜索不会创建条目。
    conn.execute_batch("CREATE TRIGGER reject_test_episode BEFORE INSERT ON media_items WHEN NEW.kind='episode' BEGIN SELECT RAISE(ABORT,'模拟分集保存失败'); END;").unwrap();
    assert!(create_item(dir.path(), &rows[1]).await.is_err());
    assert!(store::items(&conn).unwrap().is_empty()); // 主剧和季也整体回滚。
    conn.execute_batch("DROP TRIGGER reject_test_episode;")
        .unwrap();
    let season = create_item(dir.path(), &rows[1]).await.unwrap();
    assert_eq!(season["kind"], "season");
    assert_eq!(season["seasonNumber"], 2);
    assert_eq!(season["independent"], true);
    assert_eq!(season["playable"], false);
    let items = store::items(&conn).unwrap();
    assert_eq!(items.len(), 4); // 整剧、选中季与两集；没有额外收录第一季。
    fixtures(dir.path(), vec![]);
    let again = create_item(dir.path(), &rows[1]).await.unwrap();
    assert_eq!(again["id"], season["id"]);
    assert_eq!(store::items(&conn).unwrap().len(), 4);
}

#[tokio::test]
async fn local_episode_identification_saves_full_season_and_refresh_preserves_independent_records()
{
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let source = store::new_item(&conn, &json!({"title":"Some.Show.S02E02.mkv"})).unwrap();
    conn.execute(
        "INSERT INTO sources(id,path,label,created_at)VALUES('source','C:/Media','测试',?1)",
        [store::now()],
    )
    .unwrap();
    conn.execute("INSERT INTO resources(id,source_id,path,updated_at)VALUES('file','source','C:/Media/Some.Show.S02E02.mkv',?1)",[store::now()]).unwrap();
    conn.execute(
        "INSERT INTO resource_links(resource_id,item_id)VALUES('file',?1)",
        [&source],
    )
    .unwrap();
    let mut season = json!({"id":102,"season_number":2,"name":"第二季","episodes":[
        {"episode_number":1,"season_number":2,"name":"第一集","air_date":"2024-01-01"},
        {"episode_number":2,"season_number":2,"name":"第二集","air_date":"2024-01-08"},
        {"episode_number":3,"season_number":2,"name":"第三集","air_date":"2099-01-01"}
    ]});
    let mut main = series();
    main["number_of_seasons"] = json!(2);
    fixtures(
        dir.path(),
        vec![("/tv/100", main), ("/tv/100/season/2", season.clone())],
    );
    let episode = identify_item(
        dir.path(),
        &source,
        &json!({"providerType":"tv","externalId":"100","seasonNumber":2}),
        true,
    )
    .await
    .unwrap();
    assert_eq!(episode["episodeNumber"], 2);
    let season_id = episode["parentId"].as_str().unwrap().to_owned();
    let same_season = identify_item(dir.path(), &season_id,
        &json!({"providerType":"tv","externalId":"100","seasonNumber":2}),true).await.unwrap();
    assert_eq!(same_season["id"],season_id); // 整季不能因首个资源是E02而降成单集。
    assert_eq!(same_season["kind"],"season");
    let values = store::items(&conn).unwrap();
    assert_eq!(values.len(), 5); // 只创建所选季，以及这一季的完整三集。
    let root = values.iter().find(|v| v["kind"] == "series").unwrap();
    assert_eq!(root["seasonCount"], 2);
    let first = values.iter().find(|v| v["episodeNumber"] == 1).unwrap();
    let first_id = first["id"].as_str().unwrap().to_owned();
    assert_eq!(first["playable"], false);
    assert_eq!(
        values.iter().find(|v| v["episodeNumber"] == 3).unwrap()["airDate"],
        "2099-01-01"
    );
    store::update_record(&conn, &season_id, &json!({"watched":true}), true).unwrap();
    assert_eq!(
        store::item(&conn, episode["id"].as_str().unwrap()).unwrap()["watched"],
        false
    );
    store::update_record(
        &conn,
        &first_id,
        &json!({"watched":true,"rating":5,"comment":"无资源也可记录"}),
        true,
    )
    .unwrap();
    assert_eq!(store::items(&conn).unwrap().len(), 5);
    season["episodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"episode_number":4,"name":"第四集"}));
    fixtures(dir.path(), vec![("/tv/100/season/2", season)]);
    refresh_known(dir.path(), &season_id).await.unwrap();
    let first = store::item(&conn, &first_id).unwrap();
    assert_eq!(first["watched"], true);
    assert_eq!(first["rating"], 5.0);
    assert_eq!(first["comment"], "无资源也可记录");
    assert_eq!(
        store::items(&conn)
            .unwrap()
            .iter()
            .filter(|v| v["kind"] == "episode")
            .count(),
        4
    );
    assert_eq!(store::resources_for(&conn, &season_id).unwrap().len(), 1);
    assert_eq!(
        store::item(&conn, root["id"].as_str().unwrap()).unwrap()["watched"],
        false
    );
    let before = store::items(&conn).unwrap();
    fixtures(
        dir.path(),
        vec![(
            "/tv/100/season/2",
            json!({"id":102,"name":"损坏列表","season_number":2,"episodes":[{"episode_number":1},{"episode_number":1}]}),
        )],
    );
    assert!(refresh_known(dir.path(), &season_id).await.is_err());
    assert_eq!(store::items(&conn).unwrap(), before);
}

#[tokio::test]
async fn manual_tv_without_a_season_never_creates_an_empty_series_entry() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let source = store::new_item(
        &conn,
        &json!({"title":"某剧","kind":"season","independent":true}),
    )
    .unwrap();
    fixtures(dir.path(), vec![("/tv/100", series())]);
    assert!(identify_item(
        dir.path(),
        &source,
        &json!({"providerType":"tv","externalId":"100"}),
        true
    )
    .await
    .is_err());
    assert_eq!(store::items(&conn).unwrap().len(), 1);
}
#[tokio::test]
async fn season_without_number_and_movie_alias_are_identified_automatically() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let show = store::upsert_provider_item(
        &conn,
        "douban",
        "subject",
        "1",
        &json!({"title":"某剧","year":"2020"}),
        true,
    )
    .unwrap();
    let film = store::upsert_provider_item(
        &conn,
        "douban",
        "subject",
        "2",
        &json!({"title":"电影别名","year":"2023"}),
        true,
    )
    .unwrap();
    let mut single = series();
    single["seasons"]
        .as_array_mut()
        .unwrap()
        .retain(|v| v["season_number"] != 2);
    fixtures(
        dir.path(),
        vec![
            (
                "/search/multi?某剧",
                json!({"results":[{"id":100,"name":"某剧","media_type":"tv"}]}),
            ),
            ("/tv/100", single),
            (
                "/tv/100/season/1",
                json!({"id":101,"season_number":1,"name":"第1季","air_date":"2020-01-01","episodes":[]}),
            ),
            (
                "/search/multi?电影别名",
                json!({"results":[{"id":200,"title":"正式片名","media_type":"movie"}]}),
            ),
            (
                "/movie/200",
                json!({"id":200,"title":"正式片名","release_date":"2023-02-01","alternative_titles":{"titles":[{"title":"电影别名"}]}}),
            ),
        ],
    );
    let season = auto_identify(dir.path(), &show).await.unwrap();
    assert_eq!(season["kind"], "season");
    assert_eq!(season["seasonNumber"], 1);
    let movie = auto_identify(dir.path(), &film).await.unwrap();
    assert_eq!(movie["kind"], "movie");
    assert_eq!(movie["title"], "正式片名");
    assert_eq!(movie["recognized"], true);
}
#[tokio::test]
async fn ambiguous_seasons_never_create_a_series_or_move_records() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let id = store::upsert_provider_item(
        &conn,
        "douban",
        "subject",
        "1",
        &json!({"title":"某剧"}),
        true,
    )
    .unwrap();
    store::update_record(&conn, &id, &json!({"rating":5,"comment":"保留"}), false).unwrap();
    fixtures(
        dir.path(),
        vec![
            (
                "/search/multi?某剧",
                json!({"results":[{"id":100,"name":"某剧","media_type":"tv"}]}),
            ),
            ("/tv/100", series()),
        ],
    );
    let before = store::item(&conn, &id).unwrap();
    assert!(auto_identify(dir.path(), &id).await.is_err());
    assert_eq!(store::item(&conn, &id).unwrap(), before);
    assert_eq!(store::items(&conn).unwrap().len(), 1);
}
#[tokio::test]
async fn imported_season_fetches_metadata_and_merges_existing_local_season() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    super::super::douban::import_page(&conn,"a",&[json!({"subjectId":"42","title":"某剧 第二季 / Some Show Season 2","year":"2024","stars":4,"comment":"第二季短评","markedAt":"2024-02-01"})]).unwrap();
    let source = store::items(&conn).unwrap().remove(0)["id"]
        .as_str()
        .unwrap()
        .to_string();
    let parent = store::upsert_provider_item(
        &conn,
        "tmdb",
        "tv",
        "100",
        &json!({"title":"某剧","kind":"series"}),
        false,
    )
    .unwrap();
    let local_season = store::upsert_provider_item(
        &conn,
        "tmdb",
        "season",
        "100:2",
        &json!({"title":"第2季","kind":"season","parentId":parent,"seasonNumber":2}),
        false,
    )
    .unwrap();
    fixtures(
        dir.path(),
        vec![
            (
                "/search/tv?某剧",
                json!({"results":[{"id":100,"name":"某剧","original_name":"Some Show"}]}),
            ),
            (
                "/search/tv?Some Show",
                json!({"results":[{"id":100,"name":"某剧"}]}),
            ),
            ("/tv/100", series()),
            (
                "/tv/100/season/2",
                json!({"id":102,"season_number":2,"name":"第2季","overview":"第二季简介","air_date":"2024-01-01","episodes":[]}),
            ),
        ],
    );
    let item = auto_identify(dir.path(), &source).await.unwrap();
    assert_eq!(item["id"], local_season);
    assert_eq!(item["kind"], "season");
    assert_eq!(item["recognized"], true);
    assert_eq!(item["overview"], "第二季简介");
    assert_eq!(item["watched"], true);
    assert_eq!(item["comment"], "第二季短评");
    assert_eq!(item["rating"], 4.0);
    assert_eq!(item["markedAt"], "2024-02-01");
    assert_eq!(store::items(&conn).unwrap().len(), 2);
    assert_eq!(store::item(&conn, &parent).unwrap()["watched"], false);
    assert_eq!(item["externalIds"].as_array().unwrap().len(), 2);
    assert_eq!(
        super::super::douban::import_page(
            &conn,
            "a",
            &[json!({"subjectId":"42","title":"某剧 第二季","stars":1})]
        )
        .unwrap(),
        0
    );
}
#[tokio::test]
async fn failed_season_fetch_cannot_mark_source_recognized_or_create_partial_hierarchy() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let source = store::upsert_provider_item(
        &conn,
        "douban",
        "subject",
        "42",
        &json!({"title":"某剧 第二季","year":"2024"}),
        true,
    )
    .unwrap();
    fixtures(
        dir.path(),
        vec![
            (
                "/search/tv?某剧",
                json!({"results":[{"id":100,"name":"某剧"}]}),
            ),
            ("/tv/100", series()),
        ],
    );
    assert!(auto_identify(dir.path(), &source).await.is_err());
    assert_eq!(store::item(&conn, &source).unwrap()["recognized"], false);
    assert_eq!(store::items(&conn).unwrap().len(), 1);
}
#[tokio::test]
async fn manual_search_returns_final_seasons_with_one_selection() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let source = store::upsert_provider_item(
        &conn,
        "douban",
        "subject",
        "42",
        &json!({"title":"某剧"}),
        true,
    )
    .unwrap();
    fixtures(
        dir.path(),
        vec![
            (
                "/search/multi?某剧",
                json!({"results":[{"id":100,"name":"某剧","media_type":"tv"}]}),
            ),
            ("/tv/100", series()),
            (
                "/tv/100/season/2",
                json!({"id":102,"season_number":2,"name":"第2季","air_date":"2024-01-01","episodes":[]}),
            ),
        ],
    );
    let rows = matching::search_targets(dir.path(), &source, "某剧", "all")
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["seasonNumber"], 2);
    assert_eq!(rows[1]["title"], "某剧 · 第2季");
    let item = identify_item(dir.path(), &source, &rows[1], true)
        .await
        .unwrap();
    assert_eq!(item["kind"], "season");
    assert_eq!(item["seasonNumber"], 2);
}
#[test]
fn pending_metadata_retries_after_delay_and_invalid_details_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let id = store::new_item(&conn, &json!({"title":"待识别"})).unwrap();
    assert!(retry_due(&conn, &id));
    conn.execute(
        "INSERT INTO media_match_errors VALUES(?1,'暂时失败',?2)",
        params![id, store::now()],
    )
    .unwrap();
    assert!(!retry_due(&conn, &id));
    conn.execute(
        "UPDATE media_match_errors SET checked_at=?2 WHERE item_id=?1",
        params![
            id,
            (chrono::Utc::now() - chrono::Duration::minutes(16)).to_rfc3339()
        ],
    )
    .unwrap();
    assert!(retry_due(&conn, &id));
    assert!(validate_details(&json!({"id":100,"name":"某剧"}), "100", "tv").is_ok());
    assert!(validate_details(&json!({"id":999,"name":"错误作品"}), "100", "tv").is_err());
    assert!(validate_details(&json!({"id":100}), "100", "tv").is_err());
}
#[test]
fn unlinking_last_resource_deletes_empty_identity_and_preserves_recorded_identity() {
    for recorded in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        let file = media.path().join("Local.Movie.mp4");
        std::fs::write(&file, b"video fixture").unwrap();
        let mut conn = store::open(dir.path()).unwrap();
        let old_id = store::upsert_provider_item(
            &conn,
            "tmdb",
            "movie",
            "123",
            &json!({"title":"已识别电影","kind":"movie"}),
            false,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sources(id,path,label,created_at)VALUES('source',?1,'本地',?2)",
            params![media.path().to_string_lossy(), store::now()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO resources(id,source_id,path,updated_at)VALUES('file','source',?1,?2)",
            params![file.to_string_lossy(), store::now()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO resource_links(resource_id,item_id)VALUES('file',?1)",
            [&old_id],
        )
        .unwrap();
        if recorded {
            store::update_record(&conn, &old_id, &json!({"watched":true,"comment":"保留记录"}), true).unwrap();
        }
        let tx = conn.transaction().unwrap();
        let new_id = unlink_resource(&tx, &old_id, "file").unwrap();
        tx.commit().unwrap();
        let new_item = store::item(&conn, &new_id).unwrap();
        assert_eq!(new_item["recognized"], false);
        assert_eq!(new_item["playable"], true);
        assert_eq!(new_item["resourceCount"], 1);
        assert_eq!(store::resources_for(&conn, &new_id).unwrap()[0]["id"], "file");
        assert_eq!(store::resource(&conn, "file").unwrap()["missing"], false);
        if recorded {
            let old_item = store::item(&conn, &old_id).unwrap();
            assert_eq!(old_item["watched"], true);
            assert_eq!(old_item["comment"], "保留记录");
            assert_eq!(old_item["playable"], false);
            assert_eq!(store::items(&conn).unwrap().len(), 2);
        } else {
            assert!(store::item(&conn, &old_id).is_err());
            assert_eq!(store::items(&conn).unwrap().len(), 1);
            let identities: i64 = conn.query_row(
                "SELECT COUNT(*) FROM external_ids WHERE item_id=?1", [&old_id], |row| row.get(0),
            ).unwrap();
            assert_eq!(identities, 0);
        }
    }
}
#[test]
fn unlinking_supplemental_metadata_retains_douban_identity_and_records() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let id = store::upsert_provider_item(
        &conn,
        "douban",
        "subject",
        "123",
        &json!({"title":"豆瓣电影"}),
        true,
    )
    .unwrap();
    store::update_record(
        &conn,
        &id,
        &json!({"watched":true,"comment":"原评论","recordSource":"douban"}),
        false,
    )
    .unwrap();
    conn.execute("INSERT INTO external_ids(provider,provider_type,external_id,item_id)VALUES('tmdb','movie','456',?1)",[&id]).unwrap();
    unlink_metadata(&conn, &id).unwrap();
    let item = store::item(&conn, &id).unwrap();
    assert_eq!(item["recognized"], false);
    assert_eq!(item["comment"], "原评论");
    assert_eq!(item["externalIds"].as_array().unwrap().len(), 1);
    assert_eq!(item["externalIds"][0]["provider"], "douban");
    let imported = store::upsert_provider_item(
        &conn,
        "douban",
        "subject",
        "123",
        &json!({"title":"来源新标题"}),
        true,
    )
    .unwrap();
    assert_eq!(imported, id);
    assert_eq!(store::items(&conn).unwrap().len(), 1);
    let local = store::upsert_provider_item(
        &conn,
        "tmdb",
        "movie",
        "789",
        &json!({"title":"仅本地识别"}),
        false,
    )
    .unwrap();
    unlink_metadata(&conn, &local).unwrap();
    assert_eq!(store::item(&conn, &local).unwrap()["recognized"], false);
}
