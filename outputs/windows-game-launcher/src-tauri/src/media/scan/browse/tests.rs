use super::*;

#[test]
fn batched_browse_preserves_smart_folders_files_and_records() {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    let conn = store::open(data.path()).unwrap();
    conn.execute(
        "INSERT INTO sources(id,path,label,created_at)VALUES('s',?1,'media',?2)",
        params![media.path().to_string_lossy(), store::now()],
    )
    .unwrap();
    let root = normalized(media.path()).unwrap();
    let mut ids = Vec::new();
    for n in 0..30 {
        let name = format!("Movie{n:02}");
        let folder = root.join(&name);
        fs::create_dir(&folder).unwrap();
        let file = folder.join(format!("{name}.2024.mkv"));
        fs::write(&file, b"video").unwrap();
        let id = store::new_item(
            &conn,
            &json!({"title":name,"kind":"movie","recognized":true}),
        )
        .unwrap();
        store::update_record(
            &conn,
            &id,
            &json!({"favorite":true,"rating":4,"comment":"保留记录"}),
            true,
        )
        .unwrap();
        conn.execute("INSERT INTO resources(id,source_id,path,size,modified_at,position_seconds,duration_seconds,updated_at)VALUES(?1,'s',?2,5,'v1',40,100,?3)",params![format!("r{n}"),file.to_string_lossy(),store::now()]).unwrap();
        conn.execute(
            "INSERT INTO resource_links(resource_id,item_id)VALUES(?1,?2)",
            params![format!("r{n}"), id],
        )
        .unwrap();
        ids.push(id);
    }
    // 未识别文件使所属文件夹保持可进入；相似路径的其他目录不互相污染。
    let unknown = root.join("Movie00/Unknown.mkv");
    fs::write(&unknown, b"unknown").unwrap();
    let id = store::new_item(&conn, &json!({"title":"Unknown.mkv"})).unwrap();
    conn.execute(
        "INSERT INTO resources(id,source_id,path,updated_at)VALUES('unknown','s',?1,?2)",
        params![unknown.to_string_lossy(), store::now()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO resource_links(resource_id,item_id)VALUES('unknown',?1)",
        [id],
    )
    .unwrap();
    for n in 0..300 {
        store::new_item(&conn, &json!({"title":format!("无关{n}")})).unwrap();
    }
    conn.execute_batch("PRAGMA query_only=ON").unwrap();
    let response = browse_directory(&conn, &json!({"sourceId":"s"})).unwrap();
    let entries = response["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 30);
    assert_eq!(entries[0]["name"], "Movie00");
    assert_eq!(entries[0]["kind"], "folder");
    for entry in &entries[1..] {
        assert_eq!(entry["kind"], "item");
        assert_eq!(entry["directory"], true);
        assert_eq!(
            entry["item"],
            store::item(&conn, store::text(&entry["item"], "id")).unwrap()
        );
    }
    for smart in [true, false] {
        if !smart {
            conn.execute_batch(
                "PRAGMA query_only=OFF; UPDATE sources SET smart_folders=0; PRAGMA query_only=ON",
            )
            .unwrap();
        }
        let folder = root.join("Movie01");
        let response = browse_directory(&conn, &json!({"sourceId":"s","path":folder})).unwrap();
        let entry = &response["entries"][0];
        assert_eq!(entry["kind"], "item");
        assert_eq!(entry["directory"], false);
        assert_eq!(entry["item"], store::item(&conn, &ids[1]).unwrap());
        assert_eq!(entry["resource"]["positionSeconds"], 40.0);
    }
}
