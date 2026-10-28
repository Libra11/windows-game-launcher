use super::*;
use crate::media::store;

#[test]
fn recognition_status_uses_persisted_failure_category_for_list_and_detail() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let pending = new_item(&conn, &json!({"title":"待识别"})).unwrap();
    let unmatched = new_item(&conn, &json!({"title":"未匹配"})).unwrap();
    let failed = new_item(&conn, &json!({"title":"识别失败"})).unwrap();
    let legacy = new_item(&conn, &json!({"title":"旧错误"})).unwrap();
    let recognized = new_item(&conn, &json!({"title":"已识别"})).unwrap();
    conn.execute("UPDATE media_items SET recognized=1 WHERE id=?1", [&recognized]).unwrap();
    for (id, kind) in [(&unmatched, Some("unmatched")), (&failed, Some("transient")), (&legacy, None), (&recognized, Some("transient"))] {
        conn.execute("INSERT INTO media_match_errors VALUES(?1,'测试原因',?2)", params![id, now()]).unwrap();
        if let Some(kind) = kind {
            conn.execute("INSERT INTO media_match_retry VALUES(?1,?2,1,'','')", params![id, kind]).unwrap();
        }
    }
    let all = super::super::items(&conn).unwrap();
    for (id, status) in [(&pending, "pending"), (&unmatched, "unmatched"), (&failed, "failed"), (&legacy, "pending"), (&recognized, "recognized")] {
        let single = super::super::item(&conn, id).unwrap();
        assert_eq!(single["recognitionStatus"], status);
        assert_eq!(&single, all.iter().find(|item| item["id"] == *id).unwrap());
    }
    conn.execute("DELETE FROM media_match_errors WHERE item_id=?1", [&unmatched]).unwrap();
    assert_eq!(super::super::item(&conn, &unmatched).unwrap()["recognitionStatus"], "pending");
}

#[test]
fn indexed_single_snapshots_equal_full_list_with_hierarchy_resources_and_source_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let root = new_item(&conn, &json!({"title":"整剧","kind":"series"})).unwrap();
    let season = new_item(
        &conn,
        &json!({"title":"第二季","kind":"season","parentId":root,"seasonNumber":2}),
    )
    .unwrap();
    let first=new_item(&conn,&json!({"title":"第一集","kind":"episode","parentId":season,"seasonNumber":2,"episodeNumber":1})).unwrap();
    let second=new_item(&conn,&json!({"title":"第二集","kind":"episode","parentId":season,"seasonNumber":2,"episodeNumber":2})).unwrap();
    let movie = new_item(
        &conn,
        &json!({"title":"独立影片","kind":"movie","genres":["剧情"]}),
    )
    .unwrap();
    update_record(
        &conn,
        &first,
        &json!({"watched":true,"rating":5,"comment":"已保存","markedAt":"2024-01-02"}),
        true,
    )
    .unwrap();
    conn.execute(
        "UPDATE personal SET last_played='2024-05-06' WHERE item_id=?1",
        [&second],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO external_ids VALUES('douban','subject','42',?1)",
        [&season],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO douban_source_data VALUES('a','42',?1)",
        [json!({"detail":{"imdbId":"tt1234567"}}).to_string()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO media_item_metadata VALUES(?1,'2024-01-01',2)",
        [&root],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO media_match_errors VALUES(?1,'候选歧义',?2)",
        params![movie, now()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO media_collections VALUES('collection','收藏',0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO collection_items VALUES('collection',?1)",
        [&season],
    )
    .unwrap();
    conn.execute("INSERT INTO media_tags VALUES('tag','标签')", [])
        .unwrap();
    conn.execute("INSERT INTO media_item_tags VALUES(?1,'tag')", [&movie])
        .unwrap();
    conn.execute(
        "INSERT INTO sources(id,path,label,created_at)VALUES('source','C:/Media','测试',?1)",
        [now()],
    )
    .unwrap();
    for (id, path, available, missing, pos) in [
        ("online", "a.mkv", true, false, 950.0),
        ("offline", "b.mkv", false, false, 20.0),
        ("missing", "c.mkv", false, true, 30.0),
    ] {
        conn.execute("INSERT INTO resources(id,source_id,path,available,missing,position_seconds,duration_seconds,updated_at,size,modified_at)VALUES(?1,'source',?2,?3,?4,?5,1000,?6,123,'v1')",params![id,path,available,missing,pos,now()]).unwrap();
        conn.execute(
            "INSERT INTO resource_links(resource_id,item_id)VALUES(?1,?2)",
            params![id, first],
        )
        .unwrap();
    }
    // 同一资源关联多集，父季仍只计一次；排序和进度完成状态必须一致。
    conn.execute(
        "INSERT INTO resource_links(resource_id,item_id)VALUES('online',?1)",
        [&second],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO view_sessions VALUES('session',?1,'online','2024-02-03','',10,950)",
        [&first],
    )
    .unwrap();
    for n in 0..300 {
        new_item(&conn, &json!({"title":format!("无关条目{n}")})).unwrap();
    }
    conn.execute_batch("PRAGMA query_only=ON").unwrap();
    let all = super::super::items(&conn).unwrap();
    let selected = vec![root.clone(), season.clone(), first.clone(), movie.clone(), season.clone(), "不存在".into()];
    let batch = super::super::items_by_ids(&conn, &selected).unwrap();
    let expected: Vec<_> = all.iter().filter(|item| selected.iter().any(|id| item["id"] == *id)).cloned().collect();
    assert_eq!(batch, expected, "批量查询保持父子重叠、共享资源、来源资料和排序一致");
    assert!(super::super::items_by_ids(&conn, &[]).unwrap().is_empty());
    for expected in &all {
        assert_eq!(
            &super::super::item(&conn, text(expected, "id")).unwrap(),
            expected,
            "snapshot differs for {}",
            text(expected, "id")
        );
    }
    let detail = super::super::detail(&conn, &season).unwrap();
    assert_eq!(detail["item"], *all.iter().find(|value| value["id"] == season).unwrap());
    assert_eq!(detail["parents"], json!([all.iter().find(|value| value["id"] == root).unwrap()]));
    assert_eq!(detail["children"], json!([
        all.iter().find(|value| value["id"] == first).unwrap(),
        all.iter().find(|value| value["id"] == second).unwrap()
    ]));
    assert_eq!(detail["resources"].as_array().unwrap().len(), 3);
    let film_detail = super::super::detail(&conn, &movie).unwrap();
    assert_eq!(film_detail["item"], *all.iter().find(|value| value["id"] == movie).unwrap());
    assert_eq!(film_detail["parents"], json!([]));
    assert_eq!(film_detail["children"], json!([]));
    let season = super::super::item(&conn, &season).unwrap();
    assert_eq!(season["resourceCount"], 2);
    assert_eq!(season["playable"], true);
    assert_eq!(season["playbackCompleted"], true);
    assert_eq!(season["lastPlayed"], "2024-05-06");
    assert_eq!(season["imdbId"], "tt1234567");
    assert!(super::super::item(&conn, "missing-id").is_err());
}
