use super::*;

fn resource(dir: &Path, filename: &str) -> String {
    let conn = store::open(dir).unwrap();
    let item = store::new_item(&conn, &json!({"title":filename})).unwrap();
    let source = uuid::Uuid::new_v4().to_string();
    let resource = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO sources(id,path,label,created_at) VALUES(?1,?2,'识别测试',?3)",
        params![source, dir.to_string_lossy(), store::now()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO resources(id,source_id,path,updated_at) VALUES(?1,?2,?3,?4)",
        params![
            resource,
            source,
            dir.join(filename).to_string_lossy(),
            store::now()
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO resource_links(resource_id,item_id) VALUES(?1,?2)",
        params![resource, item],
    )
    .unwrap();
    item
}

fn episode_list(number: u64, year: &str) -> Value {
    json!({"id":100+number,"season_number":number,"name":format!("第 {number} 季"),"air_date":format!("{year}-01-01"),"episodes":(1..=12).map(|n|json!({"episode_number":n,"name":format!("第 {n} 集"),"air_date":format!("{year}-01-01")})).collect::<Vec<_>>()})
}

#[tokio::test]
async fn local_movie_matches_official_romanized_or_english_alias_exactly() {
    for (filename, name, year) in [
        ("Birthday.Card.2016.1080p.mkv", "Birthday Card", "2016"),
        ("Kaibutsu.2023.1080p.mkv", "Kaibutsu", "2023"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let item = resource(dir.path(), filename);
        let raw = json!({"id":200,"title":"正式片名","original_title":"原名","release_date":format!("{year}-02-01"),"alternative_titles":{"titles":[{"title":name}]}});
        fixtures(
            dir.path(),
            vec![
                (
                    if name == "Kaibutsu" {
                        "/search/multi?Kaibutsu"
                    } else {
                        "/search/multi?Birthday Card"
                    },
                    json!({"results":[{"id":200,"title":"正式片名","media_type":"movie","release_date":format!("{year}-02-01")}]}),
                ),
                ("/movie/200", raw),
            ],
        );
        let value = auto_identify(dir.path(), &item).await.unwrap();
        assert_eq!(value["title"], "正式片名");
        assert_eq!(value["externalIds"][0]["externalId"], "200");
    }
}

#[tokio::test]
async fn later_local_season_checks_season_air_year_and_real_episode() {
    let dir = tempfile::tempdir().unwrap();
    let item = resource(dir.path(), "The.Wire.S03E12.2004.1080p.mkv");
    let main = json!({"id":100,"name":"火线","original_name":"The Wire","first_air_date":"2002-06-02","seasons":[{"season_number":1,"air_date":"2002-01-01"},{"season_number":3,"air_date":"2004-09-19"}]});
    fixtures(
        dir.path(),
        vec![
            (
                "/search/tv?The Wire",
                json!({"results":[{"id":100,"name":"火线","original_name":"The Wire","first_air_date":"2002-06-02"}]}),
            ),
            ("/tv/100", main),
            ("/tv/100/season/3", episode_list(3, "2004")),
        ],
    );
    let value = auto_identify(dir.path(), &item).await.unwrap();
    assert_eq!(value["kind"], "episode");
    assert_eq!(value["seasonNumber"], 3);
    assert_eq!(value["episodeNumber"], 12);
    assert_eq!(value["externalIds"][0]["externalId"], "100:3:12");
}

#[tokio::test]
async fn standalone_episode_requires_a_unique_season_and_never_guesses_between_seasons() {
    for dated in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let item = resource(
            dir.path(),
            if dated {
                "Some.Show.E01.2024.mkv"
            } else {
                "Some.Show.E01.mkv"
            },
        );
        fixtures(
            dir.path(),
            vec![
                (
                    "/search/tv?Some Show",
                    json!({"results":[{"id":100,"name":"Some Show","first_air_date":"2020-01-01"}]}),
                ),
                (
                    "/tv/100",
                    json!({"id":100,"name":"Some Show","first_air_date":"2020-01-01","seasons":[{"season_number":1,"air_date":"2020-01-01"},{"season_number":2,"air_date":"2024-01-01"}]}),
                ),
                ("/tv/100/season/2", episode_list(2, "2024")),
            ],
        );
        let result = auto_identify(dir.path(), &item).await;
        if dated {
            let value = result.unwrap();
            assert_eq!(value["seasonNumber"], 2);
            assert_eq!(value["episodeNumber"], 1);
        } else {
            assert!(result.is_err());
            assert_eq!(
                store::items(&store::open(dir.path()).unwrap())
                    .unwrap()
                    .len(),
                1
            );
        }
    }
}

#[tokio::test]
async fn same_year_same_title_movies_remain_unrecognized() {
    let dir = tempfile::tempdir().unwrap();
    let item = resource(dir.path(), "Rocky.1976.1080p.mkv");
    fixtures(
        dir.path(),
        vec![
            (
                "/search/multi?Rocky",
                json!({"results":[{"id":200,"title":"Rocky","media_type":"movie","release_date":"1976-01-01"},{"id":201,"title":"Rocky","media_type":"movie","release_date":"1976-02-01"}]}),
            ),
            (
                "/movie/200",
                json!({"id":200,"title":"Rocky","release_date":"1976-01-01"}),
            ),
            (
                "/movie/201",
                json!({"id":201,"title":"Rocky","release_date":"1976-02-01"}),
            ),
        ],
    );
    assert!(auto_identify(dir.path(), &item).await.is_err());
    assert_eq!(
        store::items(&store::open(dir.path()).unwrap())
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn similar_alias_wrong_season_year_and_nonexistent_episode_never_link() {
    for filename in [
        "Birthday.Cards.2016.mkv",
        "Some.Show.S02E01.2023.mkv",
        "Some.Show.S02E99.2024.mkv",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let item = resource(dir.path(), filename);
        fixtures(
            dir.path(),
            vec![
                (
                    "/search/multi?Birthday Cards",
                    json!({"results":[{"id":200,"title":"生日卡片","media_type":"movie","release_date":"2016-01-01"}]}),
                ),
                (
                    "/movie/200",
                    json!({"id":200,"title":"生日卡片","release_date":"2016-01-01","alternative_titles":{"titles":[{"title":"Birthday Card"}]}}),
                ),
                (
                    "/search/tv?Some Show",
                    json!({"results":[{"id":100,"name":"Some Show","first_air_date":"2020-01-01"}]}),
                ),
                (
                    "/tv/100",
                    json!({"id":100,"name":"Some Show","first_air_date":"2020-01-01","seasons":[{"season_number":2,"air_date":"2024-01-01"}]}),
                ),
                ("/tv/100/season/2", episode_list(2, "2024")),
            ],
        );
        assert!(auto_identify(dir.path(), &item).await.is_err());
        assert_eq!(
            store::items(&store::open(dir.path()).unwrap())
                .unwrap()
                .len(),
            1
        );
    }
}

#[tokio::test]
async fn a_different_parent_series_does_not_replace_an_explicit_filename_identity() {
    let dir = tempfile::tempdir().unwrap();
    let item = resource(dir.path(), "ShowB.S01.2024/ShowA.S01E01.mkv");
    fixtures(
        dir.path(),
        vec![
            ("/search/tv?ShowA", json!({"results":[]})),
            (
                "/search/tv?ShowB",
                json!({"results":[{"id":100,"name":"ShowB","first_air_date":"2024-01-01"}]}),
            ),
            (
                "/tv/100",
                json!({"id":100,"name":"ShowB","seasons":[{"season_number":1,"air_date":"2024-01-01"}]}),
            ),
            ("/tv/100/season/1", episode_list(1, "2024")),
        ],
    );
    assert!(auto_identify(dir.path(), &item).await.is_err());
    assert_eq!(
        store::items(&store::open(dir.path()).unwrap())
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn a_different_season_title_does_not_prove_the_selected_season_identity() {
    let dir = tempfile::tempdir().unwrap();
    let item = resource(dir.path(), "SecondStory.S01E01.2020.mkv");
    fixtures(
        dir.path(),
        vec![
            (
                "/search/tv?SecondStory",
                json!({"results":[{"id":100,"name":"Series","first_air_date":"2020-01-01"}]}),
            ),
            (
                "/tv/100",
                json!({"id":100,"name":"Series","first_air_date":"2020-01-01","seasons":[{"season_number":1,"name":"FirstStory","air_date":"2020-01-01"},{"season_number":2,"name":"SecondStory","air_date":"2024-01-01"}]}),
            ),
            ("/tv/100/season/1", episode_list(1, "2020")),
        ],
    );
    assert!(auto_identify(dir.path(), &item).await.is_err());
    assert_eq!(
        store::items(&store::open(dir.path()).unwrap())
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn explicit_later_season_accepts_the_series_start_year_but_missing_season_does_not_borrow_it()
{
    for explicit in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let item = resource(
            dir.path(),
            if explicit {
                "Alice.in.Borderland.S03E05.2020.mkv"
            } else {
                "Alice.in.Borderland.E05.2020.mkv"
            },
        );
        fixtures(
            dir.path(),
            vec![
                (
                    "/search/tv?Alice in Borderland",
                    json!({"results":[{"id":100,"name":"Alice in Borderland","first_air_date":"2020-01-01"}]}),
                ),
                (
                    "/tv/100",
                    json!({"id":100,"name":"Alice in Borderland","first_air_date":"2020-01-01","seasons":[{"season_number":1,"air_date":"2020-01-01"},{"season_number":3,"air_date":"2025-01-01"}]}),
                ),
                ("/tv/100/season/1", episode_list(1, "2020")),
                ("/tv/100/season/3", episode_list(3, "2025")),
            ],
        );
        let value = auto_identify(dir.path(), &item).await.unwrap();
        assert_eq!(value["seasonNumber"], if explicit { 3 } else { 1 });
        assert_eq!(value["episodeNumber"], 5);
        assert_eq!(value["year"], if explicit { "2025" } else { "2020" });
    }
}

#[tokio::test]
async fn bilingual_filename_prefers_a_unique_work_proven_by_both_exact_names() {
    for contradictory in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let item = resource(dir.path(), "[时空恋旅人].About.Time.2013.1080p.mkv");
        let summary = json!({"results":[{"id":200,"title":"时空恋旅人","original_title":"About Time","media_type":"movie","release_date":"2013-01-01"},{"id":201,"title":"About Time","media_type":"movie","release_date":"2013-01-01"}]});
        let mut second = json!({"id":201,"title":"About Time","release_date":"2013-01-01"});
        if contradictory {
            second["alternative_titles"] = json!({"titles":[{"title":"时空恋旅人"}]});
        }
        fixtures(
            dir.path(),
            vec![
                ("/search/multi?时空恋旅人", summary.clone()),
                ("/search/multi?About Time", summary),
                (
                    "/movie/200",
                    json!({"id":200,"title":"时空恋旅人","original_title":"About Time","release_date":"2013-01-01"}),
                ),
                ("/movie/201", second),
            ],
        );
        let result = auto_identify(dir.path(), &item).await;
        if contradictory {
            assert!(result.is_err());
            assert_eq!(
                store::items(&store::open(dir.path()).unwrap())
                    .unwrap()
                    .len(),
                1
            );
        } else {
            assert_eq!(result.unwrap()["externalIds"][0]["externalId"], "200");
        }
    }
}

#[tokio::test]
async fn bilingual_series_filename_keeps_the_work_proven_by_both_names() {
    let dir = tempfile::tempdir().unwrap();
    let item = resource(dir.path(), "法医秦明Medical.Examiner.Dr.Qin.S01EP12.mp4");
    let summary = json!({"results":[{"id":100,"name":"法医秦明","original_name":"Medical Examiner Dr Qin","first_air_date":"2016-01-01"},{"id":200,"name":"Medical Examiner Dr Qin","first_air_date":"2016-01-01"}]});
    fixtures(
        dir.path(),
        vec![
            ("/search/tv?法医秦明", summary.clone()),
            ("/search/tv?Medical Examiner Dr Qin", summary),
            (
                "/tv/100",
                json!({"id":100,"name":"法医秦明","original_name":"Medical Examiner Dr Qin","first_air_date":"2016-01-01","seasons":[{"season_number":1,"air_date":"2016-01-01"}]}),
            ),
            (
                "/tv/200",
                json!({"id":200,"name":"Medical Examiner Dr Qin","first_air_date":"2016-01-01","seasons":[{"season_number":1,"air_date":"2016-01-01"}]}),
            ),
            ("/tv/100/season/1", episode_list(1, "2016")),
        ],
    );
    let value = auto_identify(dir.path(), &item).await.unwrap();
    assert_eq!(value["externalIds"][0]["externalId"], "100:1:12");
}

#[tokio::test]
async fn merged_language_search_pages_prune_impossible_first_season_years_before_details() {
    let dir = tempfile::tempdir().unwrap();
    let item = resource(dir.path(), "Calls.2021.S01E09.mkv");
    let mut first = vec![json!({"id":100,"name":"Calls","first_air_date":"2021-01-01"})];
    let mut second = first.clone();
    first.extend(
        (200..219).map(
            |id| json!({"id":id,"name":format!("Unrelated {id}"),"first_air_date":"2017-01-01"}),
        ),
    );
    second.extend(
        (300..319)
            .map(|id| json!({"id":id,"name":format!("Other {id}"),"first_air_date":"2018-01-01"})),
    );
    fixtures(
        dir.path(),
        vec![
            ("/search/tv?Calls", json!({"results":first})),
            ("/search/tv?Calls|language=en-US", json!({"results":second})),
            (
                "/tv/100",
                json!({"id":100,"name":"Calls","first_air_date":"2021-01-01","seasons":[{"season_number":1,"air_date":"2021-01-01"}]}),
            ),
            ("/tv/100/season/1", episode_list(1, "2021")),
        ],
    );
    let value = auto_identify(dir.path(), &item).await.unwrap();
    assert_eq!(value["externalIds"][0]["externalId"], "100:1:9");
}
