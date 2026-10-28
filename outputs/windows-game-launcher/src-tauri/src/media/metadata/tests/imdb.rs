use super::*;
use crate::media::{douban, store::douban_source};
mod relaxed;

fn imported(conn: &rusqlite::Connection, title: &str, year: &str) -> String {
    douban::import_page(conn,"a",&[json!({"subjectId":"42","title":title,"year":year,"stars":4,"comment":"保留短评","markedAt":"2024-02-01"})]).unwrap();
    store::items(conn).unwrap()[0]["id"]
        .as_str()
        .unwrap()
        .into()
}
fn enrich(conn: &rusqlite::Connection, id: &str, kind: &str, season: Value) {
    let source = douban_source::source(conn, id).unwrap().unwrap();
    douban_source::save(
        conn,
        id,
        &source,
        &Ok(json!({"imdbId":"tt1234567","kind":kind,"seasonNumber":season})),
    )
    .unwrap();
}
fn find(movies: Value, tv: Value, episodes: Value) -> Value {
    json!({"movie_results":movies,"tv_results":tv,"tv_episode_results":episodes})
}

#[tokio::test]
async fn title_ambiguity_is_resolved_after_cached_imdb_enrichment_without_search() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let id = imported(&conn, "某电影", "2024");
    fixtures(
        dir.path(),
        vec![
            (
                "/search/multi?某电影",
                json!({"results":[{"id":200,"media_type":"movie"},{"id":201,"media_type":"movie"}]}),
            ),
            (
                "/movie/200",
                json!({"id":200,"title":"某电影","release_date":"2024-01-01"}),
            ),
            (
                "/movie/201",
                json!({"id":201,"title":"某电影","release_date":"2024-02-01"}),
            ),
        ],
    );
    let error = auto_identify(dir.path(), &id).await.unwrap_err();
    assert!(matching::needs_details(&error));
    enrich(&conn, &id, "movie", Value::Null);
    fixtures(
        dir.path(),
        vec![
            (
                "/find/tt1234567",
                find(json!([{"id":200}]), json!([]), json!([])),
            ),
            (
                "/movie/200",
                json!({"id":200,"title":"某电影","release_date":"2024-01-01"}),
            ),
        ],
    );
    let result = auto_identify(dir.path(), &id).await.unwrap();
    assert_eq!(result["recognized"], true);
    assert_eq!(result["imdbId"], "tt1234567");
    assert_eq!(result["rating"], 4.0);
    assert_eq!(result["comment"], "保留短评");
    assert_eq!(result["watched"], true);
    assert!(result["externalIds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["provider"] == "tmdb" && v["externalId"] == "200"));
}

#[tokio::test]
async fn episode_identity_selects_its_season_and_never_marks_episode_watched() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let id = imported(&conn, "名称不同的第二季", "2024");
    enrich(&conn, &id, "series", json!(2));
    fixtures(
        dir.path(),
        vec![
            (
                "/find/tt1234567",
                find(
                    json!([]),
                    json!([]),
                    json!([{"id":900,"show_id":100,"season_number":2,"episode_number":1}]),
                ),
            ),
            ("/tv/100", series()),
            (
                "/tv/100/season/2",
                json!({"id":102,"season_number":2,"name":"第2季","air_date":"2024-01-01","episodes":[{"episode_number":1,"season_number":2,"name":"第一集"}]}),
            ),
        ],
    );
    let result = auto_identify(dir.path(), &id).await.unwrap();
    assert_eq!(result["kind"], "season");
    assert_eq!(result["seasonNumber"], 2);
    assert_eq!(result["watched"], true);
    let items = store::items(&conn).unwrap();
    assert_eq!(items.len(), 3);
    for item in &items {
        if item["kind"] != "season" {
            assert_eq!(item["watched"], false);
        }
    }
    assert!(
        !store::items(&conn).unwrap().iter().any(|i| i["externalIds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["provider"] == "imdb"))
    );
}

#[tokio::test]
async fn series_identity_uses_douban_season_and_rejects_conflicting_episode_season() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let id = imported(&conn, "某剧 第二季", "2024");
    enrich(&conn, &id, "series", json!(2));
    fixtures(
        dir.path(),
        vec![
            (
                "/find/tt1234567",
                find(
                    json!([]),
                    json!([]),
                    json!([{"id":900,"show_id":100,"season_number":1,"episode_number":1}]),
                ),
            ),
            ("/tv/100", series()),
        ],
    );
    assert!(auto_identify(dir.path(), &id)
        .await
        .unwrap_err()
        .contains("冲突"));
    assert_eq!(store::items(&conn).unwrap().len(), 1);
    fixtures(
        dir.path(),
        vec![
            (
                "/find/tt1234567",
                find(json!([]), json!([{"id":100}]), json!([])),
            ),
            ("/tv/100", series()),
            (
                "/tv/100/season/2",
                json!({"id":102,"season_number":2,"name":"第2季","air_date":"2024-01-01","episodes":[]}),
            ),
        ],
    );
    assert_eq!(
        auto_identify(dir.path(), &id).await.unwrap()["seasonNumber"],
        2
    );
}

#[tokio::test]
async fn empty_find_falls_back_but_network_invalid_and_multiple_results_do_not() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let id = imported(&conn, "某电影", "2024");
    enrich(&conn, &id, "movie", Value::Null);
    for found in [
        json!({}),
        find(json!([{"id":200},{"id":201}]), json!([]), json!([])),
    ] {
        fixtures(
            dir.path(),
            vec![
                ("/find/tt1234567", found),
                ("/movie/200", json!({"id":200,"title":"某电影"})),
                ("/movie/201", json!({"id":201,"title":"某电影"})),
            ],
        );
        let error = auto_identify(dir.path(), &id).await.unwrap_err();
        assert!(!matching::needs_details(&error));
        assert_eq!(store::items(&conn).unwrap().len(), 1);
    }
    fixtures(
        dir.path(),
        vec![
            ("/find/tt1234567", find(json!([]), json!([]), json!([]))),
            ("/search/movie?某电影", json!({"results":[{"id":200}]})),
            (
                "/movie/200",
                json!({"id":200,"title":"某电影","release_date":"2024-01-01"}),
            ),
        ],
    );
    assert!(auto_identify(dir.path(), &id).await.is_ok());
    for error in [
        "TMDB 连接失败，请检查网络和代理设置",
        "TMDB 季资料不完整，已保留待识别",
        "人工关联受保护",
    ] {
        assert!(!matching::needs_details(error));
    }
}

#[tokio::test]
async fn detail_season_conflicts_and_missing_season_data_never_write_partial_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let id = imported(&conn, "某剧 第一季", "2024");
    enrich(&conn, &id, "series", json!(2));
    fixtures(
        dir.path(),
        vec![
            (
                "/find/tt1234567",
                find(json!([]), json!([{"id":100}]), json!([])),
            ),
            ("/tv/100", series()),
        ],
    );
    assert!(auto_identify(dir.path(), &id).await.is_err());
    assert_eq!(store::items(&conn).unwrap().len(), 1);
}
