//! 未识别本地视频的按需截图。图片属于可重建缓存，不改写业务图片或个人记录。
use super::super::{ignored, model, store, tools, State};
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use tauri::Manager;
use tokio::sync::Semaphore;

pub(in crate::media) fn key(id: &str, size: i64, modified: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("frame-v1\0{id}\0{size}\0{modified}"))
    )
}

fn resource_key(resource: &Value) -> String {
    key(
        store::text(resource, "id"),
        resource["size"].as_i64().unwrap_or(0),
        store::text(resource, "modifiedAt"),
    )
}

pub(in crate::media) fn supported(path: &Path, technical: &Value) -> bool {
    model::is_video(path)
        && !path.extension().and_then(|v| v.to_str())
            .is_some_and(|v| v.eq_ignore_ascii_case("iso") || v.eq_ignore_ascii_case("img"))
        && !(technical["state"] == "ready"
            && technical["video"].as_array().is_some_and(Vec::is_empty))
}

fn candidate(conn: &Connection, item_id: &str, expected: &str) -> Result<Option<Value>, String> {
    let recognized: Option<bool> = conn
        .query_row(
            "SELECT recognized FROM media_items WHERE id=?1",
            [item_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if recognized != Some(false) {
        return Ok(None);
    }
    Ok(store::resources_for(conn, item_id)?
        .into_iter()
        .find(|resource| {
            let path = Path::new(store::text(resource, "path"));
            resource["available"] == true
                && resource["missing"] != true
                && resource["ignored"] != true
                && resource_key(resource) == expected
                && supported(path, &resource["technical"])
        }))
}

fn same_file(resource: &Value) -> bool {
    fs::metadata(store::text(resource, "path")).is_ok_and(|meta| {
        let modified = meta.modified().ok().map(|time| {
            chrono::DateTime::<chrono::Utc>::from(time)
                .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
        });
        meta.is_file()
            && Some(meta.len()) == resource["size"].as_u64()
            && modified.as_deref() == Some(store::text(resource, "modifiedAt"))
    })
}

fn cached(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() || !(512..=2 * 1024 * 1024).contains(&meta.len()) {
        return false;
    }
    fs::read(path)
        .is_ok_and(|bytes| bytes.starts_with(&[0xff, 0xd8, 0xff]) && bytes.ends_with(&[0xff, 0xd9]))
}

fn positions(duration: f64) -> Vec<f64> {
    if duration.is_finite() && duration > 0.0 {
        vec![
            (duration * 0.1).min(600.0),
            (duration * 0.3).min(1200.0),
            0.0,
        ]
    } else {
        vec![10.0, 0.0]
    }
}

fn extract(program: &Path, resource: &Value, output: &Path) -> Result<(), String> {
    let started = Instant::now();
    for position in positions(resource["durationSeconds"].as_f64().unwrap_or(0.0)) {
        if started.elapsed() >= Duration::from_secs(35) {
            break;
        }
        let mut command = Command::new(program);
        command
            .args([
                "-nostdin",
                "-hide_banner",
                "-loglevel",
                "error",
                "-threads",
                "1",
                "-filter_threads",
                "1",
                "-ss",
            ])
            .arg(format!("{position:.3}"))
            .arg("-i")
            .arg(store::text(resource, "path"))
            .args([
                "-map",
                "0:V:0",
                "-an",
                "-sn",
                "-dn",
                "-frames:v",
                "1",
                "-vf",
                "scale=w='min(1280,iw)':h='min(720,ih)':force_original_aspect_ratio=decrease:force_divisible_by=2,thumbnail=24",
                "-threads",
                "1",
                "-c:v",
                "mjpeg",
                "-q:v",
                "3",
                "-f",
                "image2",
                "-update",
                "1",
                "-y",
            ])
            .arg(output)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .spawn()
            .map_err(|_| "无法启动 ffmpeg，请检查视频截图程序路径")?;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() < Duration::from_secs(35) => {
                    std::thread::sleep(Duration::from_millis(60))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("视频截图超时或进程不可用，保留默认图片".into());
                }
            }
        };
        if status.success() && cached(output) {
            return Ok(());
        }
        let _ = fs::remove_file(output);
    }
    Err("视频截图失败，保留默认图片".into())
}

fn generate(
    dir: &Path,
    root: &Path,
    item_id: &str,
    expected: &str,
    cache_only: bool,
) -> Result<Option<PathBuf>, String> {
    let conn = store::open(dir)?;
    let Some(resource) = candidate(&conn, item_id, expected)? else {
        return Ok(None);
    };
    if !same_file(&resource) {
        return Ok(None);
    }
    let destination = root.join(format!("media-frame-{expected}.jpg"));
    if cached(&destination) {
        if let Ok(file) = fs::File::options().write(true).open(&destination) {
            let _ = file.set_times(fs::FileTimes::new().set_modified(std::time::SystemTime::now()));
        }
        return Ok(Some(destination));
    }
    if cache_only {
        return Ok(None);
    }
    let configured = store::setting(&conn, "ffmpegPath")?;
    let companion = store::setting(&conn, "ffprobePath")?;
    let Some(program) = tools::discover(
        "ffmpeg.exe",
        configured.as_str().unwrap_or(""),
        companion.as_str().unwrap_or(""),
    ) else {
        return Ok(None);
    };
    drop(conn);
    // 失败短暂退避，避免刷新或两种图片角色反复启动损坏／不支持的文件。
    static FAILURES: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    let failures = FAILURES.get_or_init(|| Mutex::new(HashMap::new()));
    let attempt = format!("{expected}:{}", program.display());
    if failures
        .lock()
        .map_err(|_| "截图状态不可用")?
        .get(&attempt)
        .is_some_and(|when| when.elapsed() < Duration::from_secs(900))
    {
        return Ok(None);
    }
    fs::create_dir_all(root).map_err(|_| "无法创建视频截图缓存")?;
    let temporary = tempfile::Builder::new()
        .prefix("media-frame-")
        .suffix(".jpg")
        .tempfile_in(root)
        .map_err(|_| "无法创建视频截图文件")?
        .into_temp_path();
    if let Err(error) = extract(&program, &resource, &temporary) {
        let mut failures = failures.lock().map_err(|_| "截图状态不可用")?;
        if failures.len() >= 256 {
            failures.retain(|_, when| when.elapsed() < Duration::from_secs(900));
        }
        if failures.len() >= 256 {
            failures.clear();
        }
        failures.insert(attempt, Instant::now());
        return Err(error);
    }
    // 抽帧期间可能已更换文件、停用源、加入 ignore 或识别成功，旧结果不能再提供。
    let conn = store::open(dir)?;
    if candidate(&conn, item_id, expected)?.is_none()
        || !same_file(&resource)
        || ignored::contains(
            &conn,
            store::text(&resource, "sourceId"),
            store::text(&resource, "path"),
        )?
    {
        return Ok(None);
    }
    fs::rename(&temporary, &destination).map_err(|_| "无法保存视频截图缓存")?;
    super::frame_cache::maintain(root, &destination);
    Ok(Some(destination))
}

pub(super) async fn resolve(
    app: tauri::AppHandle,
    item_id: String,
    expected: String,
    cache_only: bool,
) -> Result<Option<PathBuf>, String> {
    let _operation = super::super::operation(&app).await?;
    let dir = app.state::<State>().dir.clone();
    let root = app
        .path()
        .app_cache_dir()
        .map_err(|_| "无法访问图片缓存目录")?
        .join("covers");
    // 命中缓存不排在慢速解码后面；缓存校验及文件读取也不占用异步运行线程。
    let check = (dir.clone(), root.clone(), item_id.clone(), expected.clone());
    let hit = tauri::async_runtime::spawn_blocking(move || generate(&check.0, &check.1, &check.2, &check.3, true))
        .await.map_err(|_| "视频截图任务失败")??;
    if hit.is_some() || cache_only { return Ok(hit); }
    static WORKERS: OnceLock<Semaphore> = OnceLock::new();
    let _permit = WORKERS.get_or_init(|| Semaphore::new(1)).acquire().await
        .map_err(|_| "视频截图已停止")?;
    tauri::async_runtime::spawn_blocking(move || generate(&dir, &root, &item_id, &expected, false))
        .await
        .map_err(|_| "视频截图任务失败")?
}

#[cfg(test)]
mod tests;
