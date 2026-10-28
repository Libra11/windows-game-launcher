//! 有界性能日志：不记录片名、查询词、文件路径、账号、凭据或完整错误文本。
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    future::Future,
    io::Write,
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Default, Clone)]
struct Metrics {
    item_id: Option<uuid::Uuid>,
    api_calls: u64,
    network_attempts: u64,
    cache_hits: u64,
    api_us: u128,
    database_reads: u64,
    database_us: u128,
    candidate_details: u64,
    candidates_skipped: u64,
    ambiguity_stops: u64,
}
tokio::task_local! {static CURRENT:RefCell<Metrics>;}
static LOG_LOCK: Mutex<()> = Mutex::new(());
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

fn update(f: impl FnOnce(&mut Metrics)) {
    let _ = CURRENT.try_with(|m| f(&mut m.borrow_mut()));
}
pub(in crate::media) fn database_read(elapsed: Duration) {
    update(|m| {
        m.database_reads += 1;
        m.database_us += elapsed.as_micros();
    });
}
pub(super) fn cache_hit() {
    update(|m| m.cache_hits += 1);
}
pub(super) fn network_attempt() {
    update(|m| m.network_attempts += 1);
}
pub(super) fn candidate(skipped: bool) {
    update(|m| {
        if skipped {
            m.candidates_skipped += 1
        } else {
            m.candidate_details += 1
        }
    });
}
pub(super) fn ambiguity_stop() {
    update(|m| m.ambiguity_stops += 1);
}

fn category(path: &str) -> &'static str {
    if path.starts_with("/search/") {
        "search"
    } else if path.starts_with("/find/") {
        "find"
    } else if path.ends_with("credits") {
        "credits"
    } else if path.contains("/season/") {
        "season"
    } else {
        "details"
    }
}
pub(super) fn api_finished(dir: &Path, path: &str, elapsed: Duration, success: bool) {
    update(|m| {
        m.api_calls += 1;
        m.api_us += elapsed.as_micros();
    });
    if !success || elapsed >= Duration::from_secs(2) {
        write(
            dir,
            json!({"event":"tmdb_call","category":category(path),"elapsedMs":elapsed.as_millis(),"success":success}),
        );
    }
}
pub(super) fn http_status(dir: &Path, path: &str, attempt: usize, status: Option<u16>) {
    if status.is_none_or(|code| code >= 400) {
        write(
            dir,
            json!({"event":"tmdb_attempt","category":category(path),"attempt":attempt+1,"httpStatus":status}),
        );
    }
}

pub(super) async fn track<F: Future<Output = Result<Value, String>>>(
    dir: &Path,
    item: &str,
    work: F,
) -> Result<Value, String> {
    CURRENT.scope(RefCell::new(Metrics{item_id:uuid::Uuid::parse_str(item).ok(),..Metrics::default()}),async{
        let start=Instant::now();
        let result=work.await;
        let metrics=CURRENT.with(|m|m.borrow().clone());
        write(dir,json!({"event":"metadata_item","itemId":uuid::Uuid::parse_str(item).ok().map(|id|id.to_string()),
            "elapsedMs":start.elapsed().as_millis(),"success":result.is_ok(),
            "failure":result.as_ref().err().map(|e|failure_kind(e)),
            "apiCalls":metrics.api_calls,"networkAttempts":metrics.network_attempts,"cacheHits":metrics.cache_hits,"apiMs":metrics.api_us as f64 / 1000.0,
            "databaseReads":metrics.database_reads,"databaseMs":metrics.database_us as f64 / 1000.0,
            "candidateDetails":metrics.candidate_details,"candidatesSkipped":metrics.candidates_skipped,"ambiguityStops":metrics.ambiguity_stops}));
        result
    }).await
}
fn failure_kind(error: &str) -> &'static str {
    if error.contains("locked") {
        "database_busy"
    } else if error.contains("登录") || error.contains("验证") {
        "douban_access"
    } else if error.contains("连接") || error.contains("超时") {
        "network"
    } else if error.contains("凭据") {
        "credentials"
    } else if error.contains("冲突") {
        "identity_conflict"
    } else if error.contains("多个") {
        "ambiguous"
    } else if error.contains("未找到") {
        "no_match"
    } else {
        "other"
    }
}
fn write(dir: &Path, mut event: Value) {
    let Ok(_guard) = LOG_LOCK.lock() else {
        return;
    };
    let folder = dir.join("logs");
    if std::fs::create_dir_all(&folder).is_err() {
        return;
    }
    event["time"] = json!(chrono::Utc::now().to_rfc3339());
    if event.get("itemId").is_none() {
        if let Ok(Some(id)) = CURRENT.try_with(|m| m.borrow().item_id.map(|id| id.to_string())) {
            event["itemId"] = json!(id);
        }
    }
    let Ok(mut line) = serde_json::to_vec(&event) else {
        return;
    };
    line.push(b'\n');
    let path = folder.join("media-metadata.jsonl");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() + line.len() as u64 > MAX_LOG_BYTES) {
        if std::fs::copy(&path, folder.join("media-metadata.previous.jsonl")).is_err() {
            return;
        }
        if std::fs::write(&path, []).is_err() {
            return;
        }
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
    {
        let _ = file.write_all(&line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn logs_counts_without_queries_paths_errors_or_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let secret = "SECRET_API_KEY_QUERY_TITLE_AND_LOCAL_PATH";
        let result = track(dir.path(), &id, async {
            database_read(Duration::from_micros(500));
            cache_hit();
            candidate(true);
            candidate(false);
            network_attempt();
            api_finished(
                dir.path(),
                &format!("/search/{secret}"),
                Duration::from_millis(9),
                false,
            );
            http_status(dir.path(), &format!("/{secret}"), 0, Some(429));
            Err(format!("连接失败 {secret}"))
        })
        .await;
        assert!(result.is_err());
        let text = std::fs::read_to_string(dir.path().join("logs/media-metadata.jsonl")).unwrap();
        assert!(!text.contains(secret));
        let row: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
        assert_eq!(row["databaseReads"], 1);
        assert_eq!(row["databaseMs"].as_f64(), Some(0.5));
        assert_eq!(row["apiMs"].as_f64(), Some(9.0));
        for line in text.lines() {
            assert_eq!(serde_json::from_str::<Value>(line).unwrap()["itemId"], id);
        }
        assert_eq!(row["apiCalls"], 1);
        assert_eq!(row["networkAttempts"], 1);
        assert_eq!(row["cacheHits"], 1);
        assert_eq!(row["failure"], "network");
    }
    #[test]
    fn rotates_known_logs_and_logging_failure_does_not_affect_work() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("logs");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(
            folder.join("media-metadata.jsonl"),
            vec![b' '; MAX_LOG_BYTES as usize],
        )
        .unwrap();
        write(dir.path(), json!({"event":"test"}));
        assert_eq!(
            std::fs::metadata(folder.join("media-metadata.previous.jsonl"))
                .unwrap()
                .len(),
            MAX_LOG_BYTES
        );
        assert!(
            std::fs::metadata(folder.join("media-metadata.jsonl"))
                .unwrap()
                .len()
                < 1024
        );
        let blocked = dir.path().join("file");
        std::fs::write(&blocked, b"file").unwrap();
        write(&blocked, json!({"event":"test"}));
    }
}
