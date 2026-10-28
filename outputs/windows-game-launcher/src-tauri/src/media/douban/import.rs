use super::store;
use rusqlite::{params, Connection, Transaction, TransactionBehavior};
use serde_json::{json, Value};

pub(super) fn display_title(title: &str) -> &str {
    let Some((primary, aliases)) = title.split_once(" / ") else {
        return title;
    };
    let chinese = primary
        .chars()
        .any(|c| ('\u{3400}'..='\u{9fff}').contains(&c));
    let foreign = aliases.chars().any(|c| {
        c.is_ascii_alphabetic()
            || ('\u{3040}'..='\u{30ff}').contains(&c)
            || ('\u{ac00}'..='\u{d7af}').contains(&c)
    });
    if chinese && foreign {
        primary.trim()
    } else {
        title
    }
}

/// 边界只取上次成功轮次的首条；本轮已写入的重复记录不能充当停止条件。
pub(super) fn before_boundary<'a>(rows: &'a [Value], boundary: &str) -> (&'a [Value], bool) {
    let index = rows
        .iter()
        .position(|row| !boundary.is_empty() && row["subjectId"] == boundary);
    match index {
        Some(index) => (&rows[..index], true),
        None => (rows, false),
    }
}

pub(super) fn cursor_key(account: &str) -> String {
    format!("doubanImportCursor.{account}")
}

pub(super) fn finish_import(
    conn: &mut Connection,
    account: &str,
    head: Option<&str>,
    job: &str,
    checked: usize,
    imported: usize,
) -> Result<(), String> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    // 空列表没有新首条，保留旧边界。
    if let Some(head) = head {
        store::set_setting(&tx, &cursor_key(account), &json!(head))?;
    }
    store::job_update(
        &tx,
        job,
        "done",
        checked as i64,
        checked as i64,
        &format!("豆瓣导入完成，检查 {checked} 条，新增 {imported} 条"),
        "",
    )?;
    tx.commit().map_err(|e| e.to_string())
}

pub(in crate::media) fn import_page(
    conn: &Connection,
    account: &str,
    records: &[Value],
) -> Result<usize, String> {
    // 先取得写锁，再读取去重身份。延迟 SAVEPOINT 的读快照无法在并行元数据写入后
    // 安全升级为写事务，SQLite 会立即返回 BUSY/BUSY_SNAPSHOT，绕过 busy_timeout。
    // 这里只包住本页数据库操作，不持锁等待豆瓣或 TMDB 网络请求。
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let conn = &tx;
    let mut imported = 0;
    for row in records {
        let subject = row["subjectId"]
            .as_str()
            .filter(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
            .ok_or("豆瓣条目编号无效")?;
        // 只导入新来源身份；已合并到 TMDB 的记录同样通过此身份去重。
        let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM external_ids WHERE provider='douban' AND provider_type='subject' AND external_id=?1)", [subject], |r| r.get(0)).map_err(|e| e.to_string())?;
        if exists {
            continue;
        }
        let title = row["title"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or("豆瓣条目名称缺失")?;
        let poster = row["posterUrl"]
            .as_str()
            .filter(|s| s.starts_with("https://"))
            .unwrap_or("");
        let item = store::upsert_provider_item(
            conn,
            "douban",
            "subject",
            subject,
            &json!({"kind":"other","title":display_title(title),"year":row["year"],"posterUrl":poster}),
            true,
        )?;
        let date = row["markedAt"]
            .as_str()
            .filter(|v| chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").is_ok())
            .unwrap_or("");
        let stars = row["stars"].as_f64().filter(|v| (1.0..=5.0).contains(v));
        let comment = row["comment"].as_str().unwrap_or("");
        store::update_record(
            conn,
            &item,
            &json!({"watched":true,"rating":stars,"comment":comment,"markedAt":date,"recordSource":"douban"}),
            false,
        )?;
        conn.execute(
            "INSERT INTO douban_source_data(account_key,subject_id,raw_json) VALUES(?1,?2,?3)",
            params![account, subject, row.to_string()],
        )
        .map_err(|e| e.to_string())?;
        conn.execute("INSERT INTO douban_records(account_key,subject_id,item_id,stars,marked_at,comment,tags_json,imported_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![account,subject,item,stars,date,comment,row["tags"].to_string(),store::now()]).map_err(|e| e.to_string())?;
        imported += 1;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(imported)
}

#[cfg(test)]
#[path = "import/concurrency_tests.rs"]
mod concurrency_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_records_and_merged_identities_are_never_reimported() {
        let dir = tempfile::tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        let original = json!({"subjectId":"42","title":"测试电影 / Test Movie","stars":4,"markedAt":"2020-01-02","comment":"来源短评","tags":["测试"]});
        assert_eq!(import_page(&conn, "a", &[original.clone()]).unwrap(), 1);
        let item = store::items(&conn).unwrap().remove(0);
        assert_eq!(item["recognized"], false);
        assert_eq!(item["title"], "测试电影");
        let id = item["id"].as_str().unwrap();
        store::update_record(
            &conn,
            id,
            &json!({"watched":false,"rating":5,"comment":"本机修改"}),
            true,
        )
        .unwrap();
        let target = store::upsert_provider_item(
            &conn,
            "tmdb",
            "movie",
            "7",
            &json!({"title":"TMDB片名","kind":"movie"}),
            true,
        )
        .unwrap();
        store::merge_items(&conn, id, &target).unwrap();
        let before = store::item(&conn, &target).unwrap();
        let mut edited = original.clone();
        edited["stars"] = json!(2);
        edited["comment"] = json!("修改后的来源");
        assert_eq!(import_page(&conn, "a", &[edited.clone()]).unwrap(), 0);
        assert_eq!(import_page(&conn, "b", &[edited]).unwrap(), 0);
        assert_eq!(store::item(&conn, &target).unwrap(), before);
        let raw: String = conn
            .query_row("SELECT raw_json FROM douban_source_data", [], |r| r.get(0))
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(), original);
    }
    #[test]
    fn failure_keeps_boundary_and_retry_passes_already_imported_rows() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = store::open(dir.path()).unwrap();
        store::set_setting(&conn, &cursor_key("a"), &json!("10")).unwrap();
        let job = store::job_start(&conn, "douban-sync", 0).unwrap();
        let first = json!({"subjectId":"13","title":"新增13"});
        import_page(&conn, "a", &[first.clone()]).unwrap();
        assert_eq!(store::setting(&conn, &cursor_key("a")).unwrap(), "10");
        let rows = vec![
            json!({"subjectId":"14","title":"新增14"}),
            first,
            json!({"subjectId":"12","title":"新增12"}),
            json!({"subjectId":"10"}),
            json!({"subjectId":"9"}),
        ];
        let (pending, found) = before_boundary(&rows, "10");
        assert!(found);
        assert_eq!(pending.len(), 3);
        assert_eq!(import_page(&conn, "a", pending).unwrap(), 2);
        finish_import(&mut conn, "a", Some("14"), &job, 3, 2).unwrap();
        assert_eq!(store::setting(&conn, &cursor_key("a")).unwrap(), "14");
        assert!(store::setting(&conn, &cursor_key("b")).unwrap().is_null());
        assert_eq!(before_boundary(&rows, "deleted").0.len(), rows.len());
        assert!(!before_boundary(&rows, "deleted").1);
    }
    #[test]
    fn completion_failure_does_not_advance_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = store::open(dir.path()).unwrap();
        store::set_setting(&conn, &cursor_key("a"), &json!("10")).unwrap();
        let job = store::job_start(&conn, "douban-sync", 0).unwrap();
        conn.execute_batch("CREATE TRIGGER fail_finish BEFORE UPDATE ON jobs BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
        assert!(finish_import(&mut conn, "a", Some("14"), &job, 4, 4).is_err());
        assert_eq!(store::setting(&conn, &cursor_key("a")).unwrap(), "10");
        conn.execute_batch("DROP TRIGGER fail_finish;").unwrap();
        finish_import(&mut conn, "a", None, &job, 0, 0).unwrap();
        assert_eq!(store::setting(&conn, &cursor_key("a")).unwrap(), "10");
    }
    #[test]
    fn invalid_page_rolls_back_all_new_records() {
        let dir = tempfile::tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        assert!(import_page(
            &conn,
            "a",
            &[
                json!({"subjectId":"1","title":"有效"}),
                json!({"subjectId":"bad","title":"无效"})
            ]
        )
        .is_err());
        assert!(store::items(&conn).unwrap().is_empty());
    }
}
