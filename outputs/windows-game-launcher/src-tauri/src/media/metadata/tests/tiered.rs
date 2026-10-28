use super::*;

fn subject(dir: &Path, title: &str, year: &str) -> String {
    store::upsert_provider_item(
        &store::open(dir).unwrap(),
        "douban",
        "subject",
        "42",
        &json!({"title":title,"year":year}),
        true,
    )
    .unwrap()
}

#[tokio::test]
async fn unique_exact_title_and_year_skip_unrelated_details_even_when_ranked_last() {
    let dir = tempfile::tempdir().unwrap();
    let id = subject(dir.path(), "目标影片", "2024");
    fixtures(
        dir.path(),
        vec![
            (
                "/search/multi?目标影片",
                json!({"results":[
                    {"id":1,"title":"目标影片幕后","media_type":"movie","release_date":"2024-01-01"},
                    {"id":2,"name":"另一部剧","media_type":"tv"},
                    {"id":3,"title":"目标影片","media_type":"movie","release_date":"2024-01-01"}
                ]}),
            ),
            (
                "/movie/3",
                json!({"id":3,"title":"目标影片","release_date":"2024-01-01"}),
            ),
        ],
    );
    let value = diagnostics::track(dir.path(), &id, auto_identify(dir.path(), &id))
        .await
        .unwrap();
    assert!(value["externalIds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["provider"] == "tmdb" && e["externalId"] == "3"));
    let log = std::fs::read_to_string(dir.path().join("logs/media-metadata.jsonl")).unwrap();
    let stats: Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
    assert_eq!(stats["candidateDetails"], 1);
    assert_eq!(stats["candidatesSkipped"], 2);
}

#[tokio::test]
async fn every_source_alias_participates_before_exact_candidate_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let id = subject(dir.path(), "甲片 / Another Title", "2024");
    fixtures(
        dir.path(),
        vec![
            (
                "/search/multi?甲片",
                json!({"results":[{"id":1,"title":"甲片","media_type":"movie","release_date":"2024-01-01"}]}),
            ),
            (
                "/search/multi?Another Title",
                json!({"results":[{"id":2,"title":"Another Title","media_type":"movie","release_date":"2024-01-01"}]}),
            ),
            (
                "/movie/1",
                json!({"id":1,"title":"甲片","release_date":"2024-01-01"}),
            ),
            (
                "/movie/2",
                json!({"id":2,"title":"Another Title","release_date":"2024-01-01"}),
            ),
        ],
    );
    assert!(auto_identify(dir.path(), &id)
        .await
        .unwrap_err()
        .contains("多个"));
    assert_eq!(
        store::item(&store::open(dir.path()).unwrap(), &id).unwrap()["recognized"],
        false
    );
}

#[tokio::test]
async fn original_title_is_exact_evidence_and_duplicate_search_hits_are_one_identity() {
    let dir = tempfile::tempdir().unwrap();
    let id = subject(dir.path(), "Original Name / 原片", "2024");
    let result = json!({"results":[{"id":1,"title":"本地译名","original_title":"Original Name","media_type":"movie","release_date":"2024-01-01"},{"id":2,"name":"其他剧","media_type":"tv"}]});
    fixtures(
        dir.path(),
        vec![
            ("/search/multi?Original Name", result.clone()),
            ("/search/multi?原片", result),
            (
                "/movie/1",
                json!({"id":1,"title":"本地译名","original_title":"Original Name","release_date":"2024-01-01"}),
            ),
        ],
    );
    assert_eq!(
        auto_identify(dir.path(), &id).await.unwrap()["recognized"],
        true
    );
}

#[tokio::test]
async fn invalid_exact_season_expands_to_official_alias_instead_of_guessing() {
    let dir = tempfile::tempdir().unwrap();
    let id = subject(dir.path(), "某剧 第二季", "2024");
    fixtures(
        dir.path(),
        vec![
            (
                "/search/tv?某剧",
                json!({"results":[{"id":1,"name":"某剧","first_air_date":"2020-01-01"},{"id":100,"name":"另一译名","first_air_date":"2020-01-01"}]}),
            ),
            (
                "/tv/1",
                json!({"id":1,"name":"某剧","seasons":[{"season_number":1,"air_date":"2024-01-01"}]}),
            ),
            (
                "/tv/100",
                json!({"id":100,"name":"另一译名","alternative_titles":{"results":[{"title":"某剧"}]},"seasons":[{"season_number":2,"air_date":"2024-01-01"}]}),
            ),
            (
                "/tv/100/season/2",
                json!({"id":102,"name":"第二季","season_number":2,"air_date":"2024-01-01","episodes":[]}),
            ),
        ],
    );
    let value = auto_identify(dir.path(), &id).await.unwrap();
    assert_eq!(value["seasonNumber"], 2);
    assert!(value["externalIds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["providerType"] == "season" && e["externalId"] == "100:2"));
}

#[tokio::test]
async fn missing_year_still_checks_alias_only_candidates_for_ambiguity() {
    let dir = tempfile::tempdir().unwrap();
    let id = subject(dir.path(), "同名片", "");
    fixtures(
        dir.path(),
        vec![
            (
                "/search/multi?同名片",
                json!({"results":[{"id":1,"title":"同名片","media_type":"movie"},{"id":2,"title":"不同译名","media_type":"movie"}]}),
            ),
            (
                "/movie/1",
                json!({"id":1,"title":"同名片","release_date":"2024-01-01"}),
            ),
            (
                "/movie/2",
                json!({"id":2,"title":"不同译名","release_date":"1999-01-01","alternative_titles":{"titles":[{"title":"同名片"}]}}),
            ),
        ],
    );
    assert!(auto_identify(dir.path(), &id)
        .await
        .unwrap_err()
        .contains("多个"));
}

#[tokio::test]
async fn exact_movie_and_same_year_tv_season_remain_ambiguous() {
    let dir = tempfile::tempdir().unwrap();
    let id = subject(dir.path(), "长安的荔枝", "2025");
    fixtures(
        dir.path(),
        vec![
            (
                "/search/multi?长安的荔枝",
                json!({"results":[{"id":1,"title":"长安的荔枝","media_type":"movie","release_date":"2025-01-01"},{"id":2,"name":"长安的荔枝","media_type":"tv"}]}),
            ),
            (
                "/movie/1",
                json!({"id":1,"title":"长安的荔枝","release_date":"2025-01-01"}),
            ),
            (
                "/tv/2",
                json!({"id":2,"name":"长安的荔枝","seasons":[{"season_number":1,"air_date":"2025-01-01"}]}),
            ),
        ],
    );
    assert!(auto_identify(dir.path(), &id)
        .await
        .unwrap_err()
        .contains("多个"));
}

#[tokio::test]
async fn bilingual_evidence_can_resolve_two_earlier_single_name_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let id = subject(dir.path(), "再造淑女 / Selfie", "2014");
    let rows = json!({"results":[
        {"id":1,"title":"Selfie","media_type":"movie","release_date":"2014-01-01"},
        {"id":2,"title":"Selfie","media_type":"movie","release_date":"2014-01-01"},
        {"id":3,"name":"再造淑女","original_name":"Selfie","media_type":"tv","first_air_date":"2014-01-01"}
    ]});
    fixtures(
        dir.path(),
        vec![
            ("/search/multi?再造淑女", rows.clone()),
            ("/search/multi?Selfie", rows),
            (
                "/movie/1",
                json!({"id":1,"title":"Selfie","release_date":"2014-01-01"}),
            ),
            (
                "/movie/2",
                json!({"id":2,"title":"Selfie","release_date":"2014-01-01"}),
            ),
            (
                "/tv/3",
                json!({"id":3,"name":"再造淑女","original_name":"Selfie","seasons":[{"season_number":1,"air_date":"2014-01-01"}]}),
            ),
            (
                "/tv/3/season/1",
                json!({"id":31,"name":"第一季","season_number":1,"air_date":"2014-01-01","episodes":[]}),
            ),
        ],
    );
    assert_eq!(
        auto_identify(dir.path(), &id).await.unwrap()["kind"],
        "season"
    );
}

#[tokio::test]
async fn repeated_alias_punctuation_never_outvotes_a_different_title() {
    let dir = tempfile::tempdir().unwrap();
    let id = subject(dir.path(), "Selfie / SELFIE! / 另一片", "2014");
    let rows = json!({"results":[{"id":1,"title":"Selfie","media_type":"movie","release_date":"2014-01-01"},{"id":2,"title":"另一片","media_type":"movie","release_date":"2014-01-01"}]});
    fixtures(
        dir.path(),
        vec![
            ("/search/multi?Selfie", rows.clone()),
            ("/search/multi?SELFIE", rows.clone()),
            ("/search/multi?另一片", rows),
            (
                "/movie/1",
                json!({"id":1,"title":"Selfie","release_date":"2014-01-01"}),
            ),
            (
                "/movie/2",
                json!({"id":2,"title":"另一片","release_date":"2014-01-01"}),
            ),
        ],
    );
    let error = auto_identify(dir.path(), &id).await.unwrap_err();
    assert!(error.contains("多个"), "{error}");
}
