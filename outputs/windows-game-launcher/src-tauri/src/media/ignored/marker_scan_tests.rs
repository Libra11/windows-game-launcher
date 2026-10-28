use super::*;

#[test]
fn marker_added_after_enumeration_rejects_the_stale_batch_and_browse_hides_it() {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(media.path()).unwrap();
    let backup = root.join("Backup");
    fs::create_dir(&backup).unwrap();
    fs::write(backup.join("Movie.2024.mkv"), b"sample").unwrap();
    let mut conn = store::open(data.path()).unwrap();
    conn.execute(
        "INSERT INTO sources(id,path,label,created_at) VALUES('s',?1,'示例','')",
        [root.to_string_lossy().as_ref()],
    )
    .unwrap();
    let job = store::job_start(&conn, "scan", 0).unwrap();
    let (files, complete) = collect(&root, &conn, "s").unwrap();
    assert!(complete);
    assert_eq!(files.len(), 1);
    fs::write(backup.join(ignored::MARKER_FILE), b"").unwrap();
    let result = apply_scan(&mut conn, "s", &job, &files, complete).unwrap();
    assert_eq!(result["files"], 0);
    assert!(store::items(&conn).unwrap().is_empty());
    let browse = browse_directory(&conn, &json!({"sourceId":"s"})).unwrap();
    assert!(browse["entries"].as_array().unwrap().is_empty());
}

#[test]
fn marker_added_after_enumeration_deactivates_existing_resources_without_resetting_progress() {
    let data = tempfile::tempdir().unwrap();
    let media = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(media.path()).unwrap();
    let backup = root.join("Backup");
    fs::create_dir(&backup).unwrap();
    fs::write(backup.join("Movie.2024.mkv"), b"sample").unwrap();
    let mut conn = store::open(data.path()).unwrap();
    conn.execute(
        "INSERT INTO sources(id,path,label,created_at) VALUES('s',?1,'示例','')",
        [root.to_string_lossy().as_ref()],
    )
    .unwrap();
    let job = store::job_start(&conn, "scan", 0).unwrap();
    let (files, complete) = collect(&root, &conn, "s").unwrap();
    apply_scan(&mut conn, "s", &job, &files, complete).unwrap();
    conn.execute(
        "UPDATE resources SET position_seconds=77,duration_seconds=1000",
        [],
    )
    .unwrap();
    let (files, complete) = collect(&root, &conn, "s").unwrap();
    assert_eq!(files.len(), 1);
    let marker = backup.join(ignored::MARKER_FILE);
    fs::write(&marker, b"").unwrap();
    apply_scan(&mut conn, "s", &job, &files, complete).unwrap();
    let state = conn
        .query_row(
            "SELECT available,missing,position_seconds FROM resources",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(state, (0, 1, 77.0));
    fs::remove_file(marker).unwrap();
    let (files, complete) = collect(&root, &conn, "s").unwrap();
    let restored = apply_scan(&mut conn, "s", &job, &files, complete).unwrap();
    assert_eq!(restored["changedResourceIds"], json!([]));
    let state = conn
        .query_row(
            "SELECT available,missing,position_seconds FROM resources",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(state, (1, 0, 77.0));
}
