//! 识别失败的持久化调度状态。
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Unmatched,
    Transient,
}

#[derive(Debug)]
pub(super) struct Failure {
    pub kind: Kind,
    pub message: String,
    pub needs_details: bool,
}
impl Failure {
    pub fn unmatched(message: impl Into<String>, needs_details: bool) -> Self {
        Self {
            kind: Kind::Unmatched,
            message: message.into(),
            needs_details,
        }
    }
}
impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self {
            kind: Kind::Transient,
            message,
            needs_details: false,
        }
    }
}
impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}
impl From<Failure> for String {
    fn from(failure: Failure) -> Self {
        failure.message
    }
}

pub(in crate::media) fn initialize(conn: &Connection) -> Result<(), String> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS media_match_retry(item_id TEXT PRIMARY KEY REFERENCES media_items(id) ON DELETE CASCADE, kind TEXT NOT NULL, attempts INTEGER NOT NULL, next_at TEXT NOT NULL, fingerprint TEXT NOT NULL); CREATE TRIGGER IF NOT EXISTS media_match_retry_clear AFTER DELETE ON media_match_errors BEGIN DELETE FROM media_match_retry WHERE item_id=OLD.item_id; END;").map_err(|e| e.to_string())
}

// 只包含匹配使用的线索，排除文件大小、修改时间、播放和个人记录。
pub(super) fn fingerprint(conn: &Connection, id: &str) -> Result<String, String> {
    let item: (String, String, String, Option<i64>, Option<i64>) = conn
        .query_row(
            "SELECT title,kind,year,season_number,episode_number FROM media_items WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .map_err(|e| e.to_string())?;
    let mut paths = conn.prepare("WITH RECURSIVE children(id) AS (SELECT ?1 UNION SELECT m.id FROM media_items m JOIN children c ON m.parent_id=c.id) SELECT DISTINCT r.path FROM resources r JOIN resource_links l ON l.resource_id=r.id JOIN children c ON c.id=l.item_id WHERE r.missing=0 ORDER BY r.path COLLATE NOCASE").map_err(|e|e.to_string())?;
    let paths = paths
        .query_map([id], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut sources = conn.prepare("SELECT d.raw_json FROM douban_source_data d JOIN external_ids e ON e.provider='douban' AND e.provider_type='subject' AND e.external_id=d.subject_id WHERE e.item_id=?1 ORDER BY d.account_key,d.subject_id").map_err(|e|e.to_string())?;
    let sources = sources
        .query_map([id], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let sources: Vec<_> = sources
        .iter()
        .map(|raw| {
            let v: serde_json::Value = serde_json::from_str(raw).unwrap_or_default();
            serde_json::json!([
                v["title"],
                v["year"],
                v["detail"]["imdbId"],
                v["detail"]["seasonNumber"],
                v["detail"]["kind"]
            ])
        })
        .collect();
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(item, paths, sources)).map_err(|e| e.to_string())?)
    ))
}

pub(super) fn due(conn: &Connection, id: &str) -> bool {
    let row: Result<Option<(String, String, String)>, _> = conn
        .query_row(
            "SELECT kind,next_at,fingerprint FROM media_match_retry WHERE item_id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional();
    match row {
        Ok(Some((kind, next, saved))) => {
            if fingerprint(conn, id).is_ok_and(|current| current != saved) {
                return true;
            }
            if kind == "unmatched" {
                return false;
            }
            chrono::DateTime::parse_from_rfc3339(&next).is_ok_and(|at| chrono::Utc::now() >= at)
        }
        Ok(None) => super::legacy_retry_due(conn, id),
        Err(_) => false,
    }
}

pub(super) fn record(
    conn: &Connection,
    id: &str,
    failure: &Failure,
    fingerprint: &str,
) -> Result<(), String> {
    let previous: Option<(String, i64, String)> = conn
        .query_row(
            "SELECT kind,attempts,fingerprint FROM media_match_retry WHERE item_id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let attempts = previous
        .filter(|(kind, _, fp)| {
            kind == "transient" && fp == fingerprint && failure.kind == Kind::Transient
        })
        .map_or(1, |(_, n, _)| (n + 1).min(4));
    let delay = [900, 3600, 21600, 86400][(attempts - 1) as usize];
    let now = chrono::Utc::now();
    let kind = if failure.kind == Kind::Unmatched {
        "unmatched"
    } else {
        "transient"
    };
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO media_match_errors(item_id,error,checked_at) VALUES(?1,?2,?3) ON CONFLICT(item_id) DO UPDATE SET error=excluded.error,checked_at=excluded.checked_at",params![id,failure.message,now.to_rfc3339()]).map_err(|e|e.to_string())?;
    tx.execute("INSERT INTO media_match_retry(item_id,kind,attempts,next_at,fingerprint) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(item_id) DO UPDATE SET kind=excluded.kind,attempts=excluded.attempts,next_at=excluded.next_at,fingerprint=excluded.fingerprint",params![id,kind,attempts,(now+chrono::Duration::seconds(delay)).to_rfc3339(),fingerprint]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::store;
    use serde_json::json;

    #[test]
    fn unmatched_waits_for_new_clues_and_transient_retries_back_off() {
        let dir = tempfile::tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        let id = store::new_item(
            &conn,
            &json!({"title":"Film","kind":"other","independent":true}),
        )
        .unwrap();
        let fp = fingerprint(&conn, &id).unwrap();
        assert!(due(&conn, &id));
        record(&conn, &id, &Failure::unmatched("无匹配", false), &fp).unwrap();
        assert!(!due(&conn, &id));
        let reopened = store::open(dir.path()).unwrap();
        assert!(!due(&reopened, &id));
        drop(reopened);
        conn.execute(
            "UPDATE media_items SET overview='个人不相关变化' WHERE id=?1",
            [&id],
        )
        .unwrap();
        assert!(!due(&conn, &id));
        conn.execute(
            "UPDATE media_items SET title='Film 2024' WHERE id=?1",
            [&id],
        )
        .unwrap();
        assert!(due(&conn, &id));
        let changed = fingerprint(&conn, &id).unwrap();
        for (attempt, delay) in [(1, 900), (2, 3600), (3, 21600), (4, 86400), (4, 86400)] {
            let start = chrono::Utc::now();
            record(&conn, &id, &Failure::from("网络故障"), &changed).unwrap();
            let (count, next): (i64, String) = conn
                .query_row(
                    "SELECT attempts,next_at FROM media_match_retry WHERE item_id=?1",
                    [&id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(count, attempt);
            assert!(
                (chrono::DateTime::parse_from_rfc3339(&next)
                    .unwrap()
                    .with_timezone(&chrono::Utc)
                    - start)
                    .num_seconds()
                    >= delay - 1
            );
            assert!(!due(&conn, &id));
        }
        conn.execute(
            "UPDATE media_match_retry SET next_at='2000-01-01T00:00:00Z' WHERE item_id=?1",
            [&id],
        )
        .unwrap();
        assert!(due(&conn, &id));
        conn.execute("DELETE FROM media_match_errors WHERE item_id=?1", [&id])
            .unwrap();
        assert!(due(&conn, &id));
    }

    #[test]
    fn path_and_source_clues_requeue_but_file_and_detail_cache_updates_do_not() {
        let dir = tempfile::tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        let id = store::upsert_provider_item(
            &conn,
            "douban",
            "subject",
            "42",
            &json!({"title":"作品","year":"2024"}),
            true,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO douban_source_data(account_key,subject_id,raw_json) VALUES('a','42',?1)",
            [json!({"title":"作品","year":"2024"}).to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sources(id,path,label,created_at) VALUES('s','c:/media','s','')",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO resources(id,source_id,path,updated_at) VALUES('r','s','c:/media/Film.mkv','')",[]).unwrap();
        conn.execute(
            "INSERT INTO resource_links(resource_id,item_id) VALUES('r',?1)",
            [&id],
        )
        .unwrap();
        let fp = fingerprint(&conn, &id).unwrap();
        record(&conn, &id, &Failure::unmatched("无匹配", false), &fp).unwrap();
        conn.execute(
            "UPDATE resources SET size=99,modified_at='new' WHERE id='r'",
            [],
        )
        .unwrap();
        conn.execute("UPDATE douban_source_data SET raw_json=?1",[json!({"title":"作品","year":"2024","detailCheckedAt":"later","detailError":"network"}).to_string()]).unwrap();
        assert!(!due(&conn, &id));
        conn.execute(
            "UPDATE resources SET path='c:/media/Film 2024.mkv' WHERE id='r'",
            [],
        )
        .unwrap();
        assert!(due(&conn, &id));
        let fp = fingerprint(&conn, &id).unwrap();
        record(&conn, &id, &Failure::unmatched("无匹配", false), &fp).unwrap();
        conn.execute(
            "UPDATE douban_source_data SET raw_json=?1",
            [json!({"title":"作品","year":"2024","detail":{"imdbId":"tt1234567"}}).to_string()],
        )
        .unwrap();
        assert!(due(&conn, &id));
    }
}
