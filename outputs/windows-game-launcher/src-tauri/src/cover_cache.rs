use crate::{db, lock_db, model::Game, AppState};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::{Path, PathBuf}, sync::{Arc, Mutex, Weak}};
use tauri::Manager;
use tokio::sync::{Mutex as AsyncMutex, Semaphore};

pub(crate) struct CoverCache {
    downloads: Semaphore,
    locks: Mutex<HashMap<String, Weak<AsyncMutex<()>>>>,
}

impl Default for CoverCache {
    fn default() -> Self {
        Self { downloads: Semaphore::new(4), locks: Mutex::new(HashMap::new()) }
    }
}

#[derive(Deserialize, Serialize)]
struct Entry {
    appid: String,
    source: String,
    file: String,
}

fn key(game: &Game, wide: bool) -> String {
    format!("cover_cache:{}:{}", game.id, if wide { "hero" } else { "portrait" })
}

fn directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_cache_dir().map_err(|error| error.to_string())?.join("covers"))
}

pub(crate) fn cached_path(conn: &Connection, root: &Path, game: &Game, wide: bool) -> Option<PathBuf> {
    let entry: Entry = serde_json::from_str(&db::setting(conn, &key(game, wide)).ok()?).ok()?;
    // 关联变更后不使用另一款游戏的旧封面；缓存记录只允许本目录的单个文件名。
    if entry.appid != game.appid || entry.source != game.source
        || !entry.file.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.'))
        || entry.file.is_empty()
    {
        return None;
    }
    let path = root.join(entry.file);
    path.is_file().then_some(path)
}

pub(crate) fn decorate(conn: &Connection, app: &tauri::AppHandle, game: &mut Game) {
    let Ok(root) = directory(app) else { return };
    let Ok(mut metadata) = serde_json::from_str::<serde_json::Value>(&game.metadata_json) else { return };
    if !metadata.is_object() { return; }
    metadata["localArtwork"] = serde_json::json!({
        "portrait": cached_path(conn, &root, game, false),
        "hero": cached_path(conn, &root, game, true),
    });
    game.metadata_json = metadata.to_string();
}

fn sources(game: &Game, wide: bool) -> Vec<String> {
    let info: serde_json::Value = serde_json::from_str(&game.metadata_json).unwrap_or_default();
    let mut urls = Vec::new();
    if let Some(items) = info[if wide { "libraryHeroes" } else { "libraryCovers" }].as_array() {
        urls.extend(items.iter().filter_map(|item| item.as_str()).map(str::to_owned));
    }
    for field in ["cover", "icon"] {
        if let Some(url) = info[field].as_str() { urls.push(url.into()); }
    }
    let mut seen = std::collections::HashSet::new();
    urls.retain(|url| !url.trim().is_empty() && seen.insert(url.clone()));
    urls
}

pub(crate) async fn resolve(app: &tauri::AppHandle, game_id: &str, wide: bool, refresh: bool) -> Result<Option<String>, String> {
    let cache = app.state::<CoverCache>();
    let slot = format!("{game_id}:{wide}");
    let lock = {
        let mut locks = cache.locks.lock().map_err(|_| "封面缓存暂时不可用")?;
        locks.retain(|_, lock| lock.strong_count() > 0);
        let lock = locks.get(&slot).and_then(Weak::upgrade).unwrap_or_else(|| Arc::new(AsyncMutex::new(())));
        locks.insert(slot, Arc::downgrade(&lock));
        lock
    };
    let _guard = lock.lock().await;
    let state = app.state::<AppState>();
    let root = directory(app)?;
    let (game, previous) = {
        let conn = lock_db(&state)?;
        let game = db::game(&conn, game_id)?.ok_or("游戏不存在")?;
        let previous = cached_path(&conn, &root, &game, wide);
        (game, previous)
    };
    if !refresh && previous.is_some() {
        return Ok(previous.map(|path| path.to_string_lossy().into_owned()));
    }
    let _permit = cache.downloads.acquire().await.map_err(|_| "封面下载已停止")?;
    for url in sources(&game, wide) {
        let Ok((bytes, extension)) = crate::cover_download::fetch(&url, refresh).await else { continue };
        // 内容相同保持原文件名，避免刷新资料时重播图片过渡。
        if let Some(path) = &previous {
            if std::fs::read(path).is_ok_and(|old| old == bytes) {
                return Ok(Some(path.to_string_lossy().into_owned()));
            }
        }
        let file = format!("{}.{}", uuid::Uuid::new_v4(), extension);
        let path = root.join(&file);
        let write_root = root.clone();
        let write_path = path.clone();
        tauri::async_runtime::spawn_blocking(move || {
            std::fs::create_dir_all(write_root)?;
            std::fs::write(write_path, bytes)
        }).await.map_err(|error| error.to_string())?.map_err(|error| error.to_string())?;
        let conn = lock_db(&state)?;
        let current = db::game(&conn, game_id)?.ok_or("游戏不存在")?;
        if current.appid != game.appid || current.source != game.source
            || current.metadata_json != game.metadata_json
        {
            return Err("游戏资料已变更，已丢弃旧封面下载结果".into());
        }
        let entry = Entry { appid: game.appid.clone(), source: game.source.clone(), file };
        // 完整文件写入后才提交索引，下载失败不会使旧封面失效。
        db::set_setting(&conn, &key(&game, wide), &serde_json::to_string(&entry).map_err(|error| error.to_string())?)?;
        return Ok(Some(path.to_string_lossy().into_owned()));
    }
    Ok(previous.map(|path| path.to_string_lossy().into_owned()))
}

pub(crate) async fn refresh(app: &tauri::AppHandle, game_id: &str) {
    let _ = tokio::join!(resolve(app, game_id, false, true), resolve(app, game_id, true, true));
}

#[tauri::command]
pub(crate) async fn get_cached_cover(app: tauri::AppHandle, game_id: String, wide: bool) -> Result<Option<String>, String> {
    resolve(&app, &game_id, wide, false).await
}
