use super::*;

#[tokio::test]
async fn empty_search_pauses_but_network_and_invalid_search_are_transient() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let id = store::upsert_provider_item(
        &conn,
        "douban",
        "subject",
        "42",
        &json!({"title":"不存在的作品","year":"2024"}),
        true,
    )
    .unwrap();
    fixtures(
        dir.path(),
        vec![("/search/multi?不存在的作品", json!({"results":[]}))],
    );
    let error = auto_identify_typed(dir.path(), &id).await.unwrap_err();
    assert_eq!(error.kind, retry::Kind::Unmatched);
    assert!(error.needs_details);
    fixtures(dir.path(), vec![]);
    let error = auto_identify_typed(dir.path(), &id).await.unwrap_err();
    assert_eq!(error.kind, retry::Kind::Transient);
    fixtures(
        dir.path(),
        vec![("/search/multi?不存在的作品", json!({"status":"invalid"}))],
    );
    let error = auto_identify_typed(dir.path(), &id).await.unwrap_err();
    assert_eq!(error.kind, retry::Kind::Transient);
}

#[tokio::test]
async fn movie_year_prefilter_avoids_irrelevant_details_but_keeps_alias_only_and_unknown_dates() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let id = store::upsert_provider_item(
        &conn,
        "douban",
        "subject",
        "42",
        &json!({"title":"官方别名","year":"2024"}),
        true,
    )
    .unwrap();
    // 旧年份候选没有详情fixture：若请求它，会让识别失败。其他候选仍完整核对。
    fixtures(
        dir.path(),
        vec![
            (
                "/search/multi?官方别名",
                json!({"results":[{"id":1,"title":"旧片","release_date":"1999-01-01","media_type":"movie"},{"id":2,"title":"显示名不同","release_date":"2024-01-01","media_type":"movie"},{"id":3,"title":"无年份","media_type":"movie"}]}),
            ),
            (
                "/movie/2",
                json!({"id":2,"title":"显示名不同","release_date":"2024-01-01","alternative_titles":{"titles":[{"title":"官方别名"}]}}),
            ),
            (
                "/movie/3",
                json!({"id":3,"title":"无年份","release_date":"2023-01-01"}),
            ),
        ],
    );
    let result = diagnostics::track(dir.path(), &id, auto_identify(dir.path(), &id))
        .await
        .unwrap();
    assert_eq!(result["recognized"], true);
    assert!(result["externalIds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["provider"] == "tmdb" && e["externalId"] == "2"));
    let log = std::fs::read_to_string(dir.path().join("logs/media-metadata.jsonl")).unwrap();
    let stats: Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
    assert_eq!(stats["candidatesSkipped"], 1);
    assert_eq!(stats["candidateDetails"], 2);
    assert!(stats["databaseReads"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn confirmed_ambiguity_stops_without_querying_later_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let id = store::upsert_provider_item(
        &conn,
        "douban",
        "subject",
        "42",
        &json!({"title":"同名电影","year":"2024"}),
        true,
    )
    .unwrap();
    fixtures(
        dir.path(),
        vec![
            (
                "/search/multi?同名电影",
                json!({"results":[{"id":1,"media_type":"movie"},{"id":2,"media_type":"movie"},{"id":3,"media_type":"movie"}]}),
            ),
            (
                "/movie/1",
                json!({"id":1,"title":"同名电影","release_date":"2024-01-01"}),
            ),
            (
                "/movie/2",
                json!({"id":2,"title":"同名电影","release_date":"2024-02-01"}),
            ),
        ],
    );
    let error = auto_identify(dir.path(), &id).await.unwrap_err();
    assert!(matching::needs_details(&error));
    assert!(error.contains("多个"));
    assert_eq!(store::items(&conn).unwrap().len(), 1);
}

#[tokio::test]
async fn tv_search_year_does_not_filter_later_season_or_same_year_ambiguity() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let id = store::upsert_provider_item(
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
                json!({"results":[{"id":100,"name":"某剧","first_air_date":"2020-01-01"}]}),
            ),
            ("/tv/100", series()),
            (
                "/tv/100/season/2",
                json!({"id":102,"name":"第2季","season_number":2,"episodes":[],"air_date":"2024-01-01"}),
            ),
        ],
    );
    assert_eq!(
        auto_identify(dir.path(), &id).await.unwrap()["seasonNumber"],
        2
    );
}
