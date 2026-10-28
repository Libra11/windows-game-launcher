use super::*;
use crate::media::{scan, store};
use std::fs;

struct Fixture {
    _data: tempfile::TempDir,
    media: tempfile::TempDir,
    conn: Connection,
    old_series: String,
    old_season: String,
    old_episodes: Vec<String>,
    plan: Plan,
}
fn fixture() -> Fixture {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    let conn = store::open(data.path()).unwrap();
    let root = scan::normalized(media.path()).unwrap();
    conn.execute(
        "INSERT INTO sources(id,path,label,created_at)VALUES('s',?1,'测试',?2)",
        params![root.to_string_lossy(), store::now()],
    )
    .unwrap();
    let old_series = store::upsert_provider_item(
        &conn,
        "tmdb",
        "tv",
        "10",
        &json!({"kind":"series","title":"误识别作品"}),
        false,
    )
    .unwrap();
    let old_season = store::upsert_provider_item(
        &conn,
        "tmdb",
        "season",
        "10:1",
        &json!({"kind":"season","title":"旧第一季","parentId":old_series,"seasonNumber":1}),
        false,
    )
    .unwrap();
    let mut old_episodes = Vec::new();
    for n in 1..=2 {
        let item=store::upsert_provider_item(&conn,"tmdb","episode",&format!("10:1:{n}"),&json!({"kind":"episode","title":format!("旧第{n}集"),"parentId":old_season,"seasonNumber":1,"episodeNumber":n}),false).unwrap();
        let file = root.join(format!("Show.S01E{n:02}.mkv"));
        fs::write(&file, b"video").unwrap();
        let meta = fs::metadata(&file).unwrap();
        conn.execute("INSERT INTO resources(id,source_id,path,size,modified_at,position_seconds,duration_seconds,updated_at)VALUES(?1,'s',?2,?3,?4,35,100,?5)",params![format!("r{n}"),file.to_string_lossy(),meta.len() as i64,scan::stamp(&meta),store::now()]).unwrap();
        conn.execute(
            "INSERT INTO resource_links(resource_id,item_id)VALUES(?1,?2)",
            params![format!("r{n}"), item],
        )
        .unwrap();
        old_episodes.push(item);
    }
    let input = json!({"sourceId":"s","path":root,"recursive":false});
    let files = files::collect(&conn, &input).unwrap();
    let plan = Plan {
        dir: data.path().to_path_buf(),
        input,
        files,
        main: json!({"kind":"series","title":"正确作品"}),
        season: json!({"kind":"season","title":"正确第一季","seasonNumber":1,"episodes":[{"kind":"episode","title":"新第一集","seasonNumber":1,"episodeNumber":1},{"kind":"episode","title":"新第二集","seasonNumber":1,"episodeNumber":2}]}),
        external: "20".into(),
        created: Instant::now(),
        review: None,
    };
    Fixture {
        _data: data,
        media,
        conn,
        old_series,
        old_season,
        old_episodes,
        plan,
    }
}
fn mappings() -> Value {
    json!([{"resourceId":"r1","episodeNumbers":[1]},{"resourceId":"r2","episodeNumbers":[2]}])
}
fn review(f: &mut Fixture, mappings: &Value, choices: &Value) {
    f.plan.files = files::collect(&f.conn, &f.plan.input).unwrap();
    f.plan.review = Some(records::review(&f.conn, &f.plan, mappings, choices).unwrap());
}
fn target(conn: &Connection, kind: &str, external: &str) -> String {
    conn.query_row("SELECT item_id FROM external_ids WHERE provider='tmdb' AND provider_type=?1 AND external_id=?2",params![kind,external],|r|r.get(0)).unwrap()
}
fn business(conn: &Connection) -> Value {
    json!({"items":store::items(conn).unwrap(),"links":conn.prepare("SELECT resource_id||':'||item_id||':'||method||':'||locked FROM resource_links ORDER BY resource_id,item_id").unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().collect::<Result<Vec<_>,_>>().unwrap()})
}

#[test]
fn preview_is_read_only_and_suggestions_do_not_override_conflicting_seasons_or_multiepisodes() {
    let f = fixture();
    f.conn.execute_batch("PRAGMA query_only=ON").unwrap();
    assert_eq!(files::collect(&f.conn, &f.plan.input).unwrap().len(), 2);
    assert_eq!(
        suggestion(&f.plan.files[0], &f.plan.season)["suggestedEpisodes"],
        json!([1])
    );
    for name in ["Show.S02E01.mkv", "Show.S01E01E02.mkv", "Unknown.mkv"] {
        let file =
            json!({"id":"x","path":f.media.path().join(name),"name":name,"links":[],"error":""});
        let suggestion = suggestion(&file, &f.plan.season);
        assert_eq!(suggestion["suggestedEpisodes"], json!([]));
        if name.contains("S02") {
            assert_eq!(suggestion["canOverrideSeason"], true);
        }
    }
    let file = json!({"id":"x","path":f.media.path().join("02.mkv"),"name":"02.mkv","links":[],"error":""});
    assert_eq!(
        suggestion(&file, &f.plan.season)["suggestedEpisodes"],
        json!([2])
    );
    records::review(&f.conn, &f.plan, &mappings(), &json!({})).unwrap();
}
#[test]
fn full_correction_moves_records_and_sessions_without_old_identity_or_empty_old_work() {
    let mut f = fixture();
    let empty=store::upsert_provider_item(&f.conn,"tmdb","episode","10:1:9",&json!({"kind":"episode","title":"无记录空集目","parentId":f.old_season,"seasonNumber":1,"episodeNumber":9}),false).unwrap();
    store::update_record(
        &f.conn,
        &f.old_season,
        &json!({"rating":4,"comment":"整季短评","watched":true,"favorite":true}),
        true,
    )
    .unwrap();
    store::update_record(
        &f.conn,
        &f.old_episodes[0],
        &json!({"comment":"第一集笔记"}),
        true,
    )
    .unwrap();
    f.conn
        .execute(
            "INSERT INTO view_sessions VALUES('session',?1,'r1','2024-01-01','2024-01-02',30,35)",
            [&f.old_episodes[0]],
        )
        .unwrap();
    f.conn
        .execute(
            "INSERT INTO media_playback_coverage VALUES('r1','[[0,35]]')",
            [],
        )
        .unwrap();
    f.conn
        .execute("INSERT INTO media_tags VALUES('tag','标签')", [])
        .unwrap();
    f.conn
        .execute(
            "INSERT INTO media_item_tags VALUES(?1,'tag')",
            [&f.old_season],
        )
        .unwrap();
    f.conn
        .execute(
            "INSERT INTO external_ids VALUES('douban','subject','123',?1)",
            [&f.old_season],
        )
        .unwrap();
    review(&mut f, &mappings(), &json!({}));
    assert!(f.plan.review.as_ref().unwrap().output["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["itemId"] == empty)
        .unwrap()["reason"]
        .as_str()
        .unwrap()
        .contains("清理旧空条目"));
    let result = apply(&mut f.conn, &f.plan, &json!({})).unwrap();
    assert!(store::item(&f.conn, &empty).is_err());
    assert_eq!(result["matched"], 2);
    let new = store::item(&f.conn, store::text(&result, "itemId")).unwrap();
    assert_eq!(new["rating"], 4.0);
    assert_eq!(new["comment"], "整季短评");
    assert_eq!(new["watched"], true);
    assert_eq!(new["tags"][0]["id"], "tag");
    for id in [
        &f.old_series,
        &f.old_season,
        &f.old_episodes[0],
        &f.old_episodes[1],
    ] {
        assert!(store::item(&f.conn, id).is_err(), "旧空条目应清理");
    }
    let first = target(&f.conn, "episode", "20:1:1");
    assert_eq!(
        store::item(&f.conn, &first).unwrap()["comment"],
        "第一集笔记"
    );
    let history: String = f
        .conn
        .query_row(
            "SELECT item_id FROM view_sessions WHERE id='session'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(history, first);
    let resource = store::resource(&f.conn, "r1").unwrap();
    assert_eq!(resource["positionSeconds"], 35.0);
    assert_eq!(resource["itemIds"], json!([first]));
    assert_eq!(
        new["externalIds"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["provider"] == "tmdb")
            .count(),
        1
    );
    assert_eq!(
        new["externalIds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["provider"] == "tmdb")
            .unwrap()["externalId"],
        "20:1"
    );
    let locked: i64 = f
        .conn
        .query_row(
            "SELECT COUNT(*) FROM resource_links WHERE locked=1 AND method='manual'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(locked, 2);
}
#[test]
fn partial_correction_keeps_other_files_parent_records_and_retry_state() {
    let mut f = fixture();
    store::update_record(&f.conn, &f.old_season, &json!({"rating":3}), true).unwrap();
    f.conn
        .execute(
            "INSERT INTO media_match_errors VALUES(?1,'保留失败状态','2024-01-01')",
            [&f.old_episodes[1]],
        )
        .unwrap();
    let rows = json!([{"resourceId":"r1","episodeNumbers":[1]}]);
    review(&mut f, &rows, &json!({}));
    apply(&mut f.conn, &f.plan, &json!({})).unwrap();
    assert_eq!(store::item(&f.conn, &f.old_season).unwrap()["rating"], 3.0);
    assert_eq!(
        store::resource(&f.conn, "r2").unwrap()["itemIds"],
        json!([f.old_episodes[1]])
    );
    let count: i64 = f
        .conn
        .query_row(
            "SELECT COUNT(*) FROM media_match_errors WHERE item_id=?1",
            [&f.old_episodes[1]],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}
#[test]
fn record_conflict_requires_explicit_choice_and_failure_rolls_back_everything() {
    let mut f = fixture();
    store::update_record(
        &f.conn,
        &f.old_season,
        &json!({"rating":4,"comment":"原短评"}),
        true,
    )
    .unwrap();
    let series =
        store::upsert_provider_item(&f.conn, "tmdb", "tv", "20", &f.plan.main, false).unwrap();
    let mut season = f.plan.season.clone();
    season["parentId"] = json!(series);
    let dest =
        store::upsert_provider_item(&f.conn, "tmdb", "season", "20:1", &season, false).unwrap();
    store::update_record(
        &f.conn,
        &dest,
        &json!({"rating":5,"comment":"目标短评"}),
        true,
    )
    .unwrap();
    review(&mut f, &mappings(), &json!({}));
    assert_eq!(
        f.plan.review.as_ref().unwrap().output["conflicts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let before = business(&f.conn);
    assert!(apply(&mut f.conn, &f.plan, &json!({})).is_err());
    assert_eq!(business(&f.conn), before);
    let resolutions = json!({"season:20:1/rating":f.old_season,"season:20:1/comment":dest});
    apply(&mut f.conn, &f.plan, &resolutions).unwrap();
    let item = store::item(&f.conn, &dest).unwrap();
    assert_eq!(item["rating"], 4.0);
    assert_eq!(item["comment"], "目标短评");
}
#[test]
fn stale_file_link_and_personal_record_each_reject_without_writes() {
    for change in ["file", "link", "record"] {
        let mut f = fixture();
        review(&mut f, &mappings(), &json!({}));
        match change {
            "file" => fs::write(f.media.path().join("Show.S01E01.mkv"), b"changed video").unwrap(),
            "link" => {
                f.conn
                    .execute(
                        "UPDATE resource_links SET locked=1 WHERE resource_id='r1'",
                        [],
                    )
                    .unwrap();
            }
            _ => {
                store::update_record(&f.conn, &f.old_season, &json!({"rating":5}), true).unwrap();
            }
        }
        let before = business(&f.conn);
        assert!(apply(&mut f.conn, &f.plan, &json!({})).is_err(), "{change}");
        assert_eq!(business(&f.conn), before);
    }
}
#[test]
fn invalid_and_outside_mappings_are_rejected_and_multiversion_or_combined_files_are_supported() {
    let mut f = fixture();
    for rows in [
        json!([]),
        json!([{"resourceId":"outside","episodeNumbers":[1]}]),
        json!([{"resourceId":"r1","episodeNumbers":[3]}]),
        json!([{"resourceId":"r1","episodeNumbers":[1,1]}]),
        json!([{"resourceId":"r1","episodeNumbers":[1]},{"resourceId":"r1","episodeNumbers":[2]}]),
    ] {
        assert!(validate_mappings(&f.plan, &rows).is_err());
    }
    let rows = json!([{"resourceId":"r1","episodeNumbers":[1,2]},{"resourceId":"r2","episodeNumbers":[2]}]);
    review(&mut f, &rows, &json!({}));
    apply(&mut f.conn, &f.plan, &json!({})).unwrap();
    assert_eq!(
        store::resource(&f.conn, "r1").unwrap()["itemIds"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let second = target(&f.conn, "episode", "20:1:2");
    assert_eq!(store::resources_for(&f.conn, &second).unwrap().len(), 2);
}
#[test]
fn scope_rechecks_markers_source_disabling_and_component_boundaries() {
    let mut f = fixture();
    let outside = tempfile::tempdir().unwrap();
    let mut scope = f.plan.input.clone();
    scope["path"] = json!(outside.path());
    assert!(files::collect(&f.conn, &scope).is_err());
    fs::write(f.media.path().join(".youji-ignore"), b"").unwrap();
    assert!(files::collect(&f.conn, &f.plan.input).is_err());
    fs::remove_file(f.media.path().join(".youji-ignore")).unwrap();
    review(&mut f, &mappings(), &json!({}));
    store::set_setting(&f.conn, "source.disabled.s", &json!(true)).unwrap();
    assert!(apply(&mut f.conn, &f.plan, &json!({})).is_err());
}
#[test]
fn preview_tokens_are_profile_bound_expiring_and_one_use() {
    let f = fixture();
    let token = remember(f.plan.clone()).unwrap();
    assert!(take_plan(Path::new("C:/wrong-profile"), &token).is_err());
    assert!(take_plan(&f.plan.dir, &token).is_ok());
    assert!(take_plan(&f.plan.dir, &token).is_err());
    let mut old = f.plan.clone();
    old.created = Instant::now() - Duration::from_secs(901);
    let token = remember(old).unwrap();
    assert!(take_plan(&f.plan.dir, &token).is_err());
}

#[test]
fn multiple_source_records_for_one_episode_require_choice_and_do_not_duplicate_identities() {
    let mut f = fixture();
    store::update_record(&f.conn, &f.old_episodes[0], &json!({"rating":4}), true).unwrap();
    store::update_record(&f.conn, &f.old_episodes[1], &json!({"rating":5}), true).unwrap();
    let rows =
        json!([{"resourceId":"r1","episodeNumbers":[1]},{"resourceId":"r2","episodeNumbers":[1]}]);
    review(&mut f, &rows, &json!({}));
    assert_eq!(
        f.plan.review.as_ref().unwrap().output["conflicts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let choice = json!({"episode:20:1:1/rating":f.old_episodes[1]});
    apply(&mut f.conn, &f.plan, &choice).unwrap();
    let id = target(&f.conn, "episode", "20:1:1");
    assert_eq!(store::item(&f.conn, &id).unwrap()["rating"], 5.0);
    assert_eq!(store::resources_for(&f.conn, &id).unwrap().len(), 2);
    assert!(store::item(&f.conn, &f.old_season).is_err());
}
#[test]
fn record_destination_can_be_chosen_for_missing_episode_and_keep_is_respected() {
    for keep in [false, true] {
        let mut f = fixture();
        let extra=store::upsert_provider_item(&f.conn,"tmdb","episode","10:1:9",&json!({"kind":"episode","title":"旧第九集笔记","parentId":f.old_season,"seasonNumber":1,"episodeNumber":9}),false).unwrap();
        store::update_record(&f.conn, &extra, &json!({"comment":"保留这条笔记"}), true).unwrap();
        let choice = if keep {
            json!({extra.clone():"keep"})
        } else {
            json!({extra.clone():"episode:20:1:2"})
        };
        review(&mut f, &mappings(), &choice);
        apply(&mut f.conn, &f.plan, &json!({})).unwrap();
        if keep {
            assert_eq!(
                store::item(&f.conn, &extra).unwrap()["comment"],
                "保留这条笔记"
            );
            assert!(store::item(&f.conn, &f.old_season).is_ok());
        } else {
            assert!(store::item(&f.conn, &extra).is_err());
            let id = target(&f.conn, "episode", "20:1:2");
            assert_eq!(
                store::item(&f.conn, &id).unwrap()["comment"],
                "保留这条笔记"
            );
            assert!(store::item(&f.conn, &f.old_season).is_err());
        }
    }
}
#[test]
fn conflicting_douban_sources_are_not_silently_attached_to_one_target() {
    let mut f = fixture();
    for (subject, item) in [("123", &f.old_episodes[0]), ("456", &f.old_episodes[1])] {
        f.conn
            .execute(
                "INSERT INTO external_ids VALUES('douban','subject',?1,?2)",
                params![subject, item],
            )
            .unwrap();
    }
    let rows =
        json!([{"resourceId":"r1","episodeNumbers":[1]},{"resourceId":"r2","episodeNumbers":[1]}]);
    review(&mut f, &rows, &json!({}));
    let effects = f.plan.review.as_ref().unwrap().output["effects"]
        .as_array()
        .unwrap();
    for id in &f.old_episodes {
        assert!(effects.iter().find(|v| v["itemId"] == *id).unwrap()["targetKey"].is_null());
    }
    apply(&mut f.conn, &f.plan, &json!({})).unwrap();
    for id in &f.old_episodes {
        assert!(store::item(&f.conn, id).is_ok());
    }
    let id = target(&f.conn, "episode", "20:1:1");
    assert!(store::item(&f.conn, &id).unwrap()["externalIds"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["provider"] == "tmdb"));
}
#[test]
fn recursive_scope_is_opt_in_and_target_record_changes_invalidate_final_review() {
    let mut f = fixture();
    let sub = f.media.path().join("versions");
    fs::create_dir(&sub).unwrap();
    let file = sub.join("EP01.mkv");
    fs::write(&file, b"video").unwrap();
    let meta = fs::metadata(&file).unwrap();
    f.conn.execute("INSERT INTO resources(id,source_id,path,size,modified_at,updated_at)VALUES('sub','s',?1,?2,?3,?4)",params![file.to_string_lossy(),meta.len() as i64,scan::stamp(&meta),store::now()]).unwrap();
    assert_eq!(files::collect(&f.conn, &f.plan.input).unwrap().len(), 2);
    let mut scope = f.plan.input.clone();
    scope["recursive"] = json!(true);
    assert_eq!(files::collect(&f.conn, &scope).unwrap().len(), 3);
    let series =
        store::upsert_provider_item(&f.conn, "tmdb", "tv", "20", &f.plan.main, false).unwrap();
    let mut season = f.plan.season.clone();
    season["parentId"] = json!(series);
    let dest =
        store::upsert_provider_item(&f.conn, "tmdb", "season", "20:1", &season, false).unwrap();
    review(&mut f, &mappings(), &json!({}));
    store::update_record(&f.conn, &dest, &json!({"rating":5}), true).unwrap();
    let before = business(&f.conn);
    assert!(apply(&mut f.conn, &f.plan, &json!({})).is_err());
    assert_eq!(before, business(&f.conn));
}
