use super::*;

#[tokio::test]
async fn matching_movie_is_not_vetoed_by_same_imdb_episode_or_series() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let id = imported(&conn, "梅格雷的亡者", "2016");
    enrich(&conn, &id, "movie", Value::Null);
    fixtures(
        dir.path(),
        vec![
            (
                "/find/tt1234567",
                find(
                    json!([{"id":200}]),
                    json!([{"id":100}]),
                    json!([{"id":900,"show_id":100,"season_number":1,"episode_number":2}]),
                ),
            ),
            (
                "/movie/200",
                json!({"id":200,"title":"梅格雷的亡者","release_date":"2016-12-25"}),
            ),
        ],
    );
    let result = auto_identify(dir.path(), &id).await.unwrap();
    assert_eq!(result["kind"], "movie");
    assert_eq!(result["watched"], true);
    assert_eq!(store::items(&conn).unwrap().len(), 1);
}

fn special_entries(saved_id: u64) -> Vec<(&'static str, Value)> {
    vec![
        (
            "/find/tt1234567",
            find(
                json!([]),
                json!([]),
                json!([{"id":900,"show_id":100,"season_number":0,"episode_number":2}]),
            ),
        ),
        ("/tv/100", series()),
        (
            "/tv/100/season/0",
            json!({"id":110,"name":"特别篇","season_number":0,"air_date":"2010-01-01","episodes":[
                {"id":800,"name":"前导篇","season_number":0,"episode_number":1,"air_date":"2010-01-01"},
                {"id":saved_id,"name":"OVA 重拨","season_number":0,"episode_number":2,"air_date":"2013-06-19"}
            ]}),
        ),
    ]
}

#[tokio::test]
async fn standalone_ova_maps_to_exact_special_episode_and_never_marks_its_whole_season() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let id = imported(&conn, "未来日记Redial", "2013");
    enrich(&conn, &id, "movie", Value::Null);
    fixtures(dir.path(), special_entries(900));
    let result = auto_identify(dir.path(), &id).await.unwrap();
    assert_eq!(result["kind"], "episode");
    assert_eq!(result["seasonNumber"], 0);
    assert_eq!(result["episodeNumber"], 2);
    for key in ["watched", "rating", "markedAt", "comment"] {
        assert_eq!(
            result[key],
            match key {
                "watched" => json!(true),
                "rating" => json!(4.0),
                "markedAt" => json!("2024-02-01"),
                _ => json!("保留短评"),
            }
        );
    }
    let items = store::items(&conn).unwrap();
    assert_eq!(items.len(), 4);
    for item in items.iter().filter(|v| v["id"] != result["id"]) {
        assert_eq!(item["watched"], false);
        assert!(item["imdbId"].as_str().unwrap_or("").is_empty());
    }
}

#[tokio::test]
async fn stale_find_or_changed_season_episode_never_moves_the_source_record() {
    for saved in [901, 0] {
        let dir = tempfile::tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        let id = imported(&conn, "某OVA", "2013");
        enrich(&conn, &id, "movie", Value::Null);
        let before = store::item(&conn, &id).unwrap();
        fixtures(dir.path(), special_entries(saved));
        assert!(auto_identify(dir.path(), &id)
            .await
            .unwrap_err()
            .contains("不一致"));
        assert_eq!(store::items(&conn).unwrap(), vec![before]);
    }
}

#[tokio::test]
async fn known_series_ignores_movie_copy_and_can_resolve_different_season_numbers_by_year() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let id = imported(&conn, "某剧 第三季", "2024");
    enrich(&conn, &id, "series", json!(3));
    fixtures(
        dir.path(),
        vec![
            (
                "/find/tt1234567",
                find(json!([{"id":200}]), json!([{"id":100}]), json!([])),
            ),
            ("/tv/100", series()),
            (
                "/tv/100/season/2",
                json!({"id":102,"name":"第二季","season_number":2,"air_date":"2024-01-01","episodes":[]}),
            ),
        ],
    );
    let value = auto_identify(dir.path(), &id).await.unwrap();
    assert_eq!(value["kind"], "season");
    assert_eq!(value["seasonNumber"], 2);
}

#[tokio::test]
async fn renumbered_episode_anchor_can_resolve_a_whole_season_only_with_matching_year() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let id = imported(&conn, "某剧 第三季", "2024");
    enrich(&conn, &id, "series", json!(3));
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
                json!({"id":102,"name":"第二季","season_number":2,"air_date":"2024-01-01","episodes":[]}),
            ),
        ],
    );
    assert_eq!(
        auto_identify(dir.path(), &id).await.unwrap()["seasonNumber"],
        2
    );
}

#[tokio::test]
async fn separate_cour_is_not_marked_as_the_whole_combined_tmdb_season() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let id = imported(&conn, "某剧 第二季", "2025");
    enrich(&conn, &id, "series", json!(2));
    fixtures(
        dir.path(),
        vec![
            (
                "/find/tt1234567",
                find(
                    json!([]),
                    json!([]),
                    json!([{"id":900,"show_id":100,"season_number":1,"episode_number":13,"air_date":"2025-01-01"}]),
                ),
            ),
            (
                "/tv/100",
                json!({"id":100,"name":"某剧","seasons":[{"season_number":1,"air_date":"2024-01-01","episode_count":25}]}),
            ),
        ],
    );
    assert!(auto_identify(dir.path(), &id)
        .await
        .unwrap_err()
        .contains("季范围冲突"));
    assert_eq!(store::items(&conn).unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_or_non_tv_episode_selection_never_writes_partial_items() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let id = imported(&conn, "某片", "2024");
    fixtures(
        dir.path(),
        vec![
            ("/movie/200", json!({"id":200,"title":"某片"})),
            ("/tv/100", series()),
        ],
    );
    for choice in [
        json!({"providerType":"movie","externalId":"200","episodeNumber":1}),
        json!({"providerType":"tv","externalId":"100","episodeNumber":1}),
        json!({"providerType":"tv","externalId":"100","seasonNumber":0,"episodeNumber":0}),
        json!({"providerType":"tv","externalId":"100","seasonNumber":0,"episodeNumber":"2"}),
    ] {
        assert!(identify_item(dir.path(), &id, &choice, true).await.is_err());
        assert_eq!(store::items(&conn).unwrap().len(), 1);
    }
}
