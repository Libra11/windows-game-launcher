use super::*;
use std::{
    sync::{mpsc, Arc, Barrier},
    thread,
    time::Duration,
};

#[test]
fn page_waits_for_metadata_writer_then_imports_without_losing_cursor_or_records() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    store::set_setting(&conn, &cursor_key("a"), &json!("40")).unwrap();
    let metadata = store::job_start(&conn, "metadata", 5).unwrap();
    let mut other = store::open(dir.path()).unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let writer = thread::spawn(move || {
        let tx = other
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        store::job_update(&tx, &metadata, "running", 1, 5, "并行元数据写入", "").unwrap();
        ready_tx.send(()).unwrap();
        thread::sleep(Duration::from_millis(150));
        tx.commit().unwrap();
    });
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let rows = [
        json!({"subjectId":"42","title":"新记录","stars":4,"markedAt":"2024-01-02","comment":"来源短评"}),
    ];
    let result = import_page(&conn, "a", &rows);
    writer.join().unwrap();
    assert_eq!(result.unwrap(), 1);
    assert!(conn.is_autocommit());
    assert_eq!(store::setting(&conn, &cursor_key("a")).unwrap(), "40");
    let item = store::items(&conn).unwrap().remove(0);
    assert_eq!(item["rating"], 4.0);
    assert_eq!(item["markedAt"], "2024-01-02");
    assert_eq!(item["comment"], "来源短评");
    assert_eq!(import_page(&conn, "a", &rows).unwrap(), 0);
    let mut conn = conn;
    let job = store::job_start(&conn, "douban-sync", 1).unwrap();
    finish_import(&mut conn, "a", Some("42"), &job, 1, 1).unwrap();
    assert_eq!(store::setting(&conn, &cursor_key("a")).unwrap(), "42");
}

#[test]
fn competing_page_imports_recheck_identity_after_writer_commits() {
    let dir = tempfile::tempdir().unwrap();
    store::open(dir.path()).unwrap();
    let start = Arc::new(Barrier::new(2));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let conn = store::open(dir.path()).unwrap();
        let start = start.clone();
        workers.push(thread::spawn(move || {
            start.wait();
            import_page(
                &conn,
                "a",
                &[json!({"subjectId":"42","title":"同一新条目","stars":4})],
            )
        }));
    }
    let imported: usize = workers
        .into_iter()
        .map(|t| t.join().unwrap().unwrap())
        .sum();
    assert_eq!(imported, 1);
    let conn = store::open(dir.path()).unwrap();
    assert_eq!(store::items(&conn).unwrap().len(), 1);
    for table in ["personal", "douban_records", "douban_source_data"] {
        assert_eq!(
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
}

#[test]
fn rejected_page_rolls_back_and_releases_writer_for_other_connections() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    assert!(import_page(
        &conn,
        "a",
        &[
            json!({"subjectId":"42","title":"第一页"}),
            json!({"subjectId":"invalid","title":"无效"})
        ]
    )
    .is_err());
    assert!(conn.is_autocommit());
    let other = store::open(dir.path()).unwrap();
    assert_eq!(
        import_page(&other, "a", &[json!({"subjectId":"42","title":"重试页"})]).unwrap(),
        1
    );
    assert_eq!(store::items(&conn).unwrap()[0]["title"], "重试页");
}
