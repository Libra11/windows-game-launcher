pub(in crate::media) mod frame;
mod frame_cache;
use super::{presentation, store, State};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::OnceLock};
use tauri::Manager;
use tokio::sync::Semaphore;

fn cached(root: &std::path::Path, key: &str) -> Option<PathBuf> {
    ["jpg", "png", "webp", "gif", "avif"]
        .iter()
        .map(|extension| root.join(format!("media-{key}.{extension}")))
        .find(|path| {
            path.metadata()
                .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
        })
}

pub async fn resolve(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let dir = app.state::<State>().dir.clone();
    let item_id = input["itemId"].as_str().ok_or("请选择影视条目")?.to_owned();
    let role = input["role"].as_str().unwrap_or("poster");
    if !matches!(role, "poster" | "backdrop" | "logo" | "profile") {
        return Err("图片类型无效".into());
    }
    let field = if role == "backdrop" {
        "backdropUrl"
    } else {
        "posterUrl"
    };
    let item = store::item(&store::open(&dir)?, &item_id)?;
    let extra = if matches!(role, "logo" | "profile") {
        presentation::cached_for(&app, &item)
    } else {
        None
    };
    let indexed_profile = if role == "profile" {
        super::people::profile(
            &store::open(&dir)?,
            input["personId"].as_u64().unwrap_or(0),
            &item_id,
        )?
    } else {
        String::new()
    };
    let source = match role {
        "logo" => extra.as_ref().and_then(|value| value["logoUrl"].as_str()),
        "profile" if !indexed_profile.is_empty() => Some(indexed_profile.as_str()),
        "profile" => extra
            .as_ref()
            .and_then(|value| value["people"].as_array())
            .and_then(|people| {
                people
                    .iter()
                    .find(|person| person["id"] == input["personId"])
            })
            .and_then(|person| person["profileUrl"].as_str()),
        _ => item[field].as_str(),
    };
    let preview_url = if let Some(token) = input["previewToken"].as_str() {
        if role != "poster" {
            return Err("豆瓣预览仅支持海报".into());
        }
        Some(super::douban::preview_poster(&dir, &item_id, token)?)
    } else {
        None
    };
    let url = preview_url.as_deref().unwrap_or_else(|| source.unwrap_or(""));
    if url.is_empty() {
        if matches!(role, "poster" | "backdrop") && item["recognized"] == false {
            if let Some(key) = item["localArtworkKey"].as_str() {
                if input["localArtworkKey"].as_str().is_some_and(|expected| expected != key) {
                    return Ok(json!({"path":null}));
                }
                let path = frame::resolve(app, item_id, key.to_string(), input["cacheOnly"] == true).await?;
                return Ok(json!({"path":path}));
            }
        }
        return Ok(json!({"path":null}));
    }
    let parsed = reqwest::Url::parse(url).map_err(|_| "影视图片地址无效")?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.host_str().is_some_and(|host| {
            host == "image.tmdb.org"
                || host == "doubanio.com"
                || host.ends_with(".doubanio.com")
                || host == "img.douban.com"
        })
    {
        return Err("影视图片来源不受支持".into());
    }
    let key = format!("{:x}", Sha256::digest(url.as_bytes()));
    let root = app
        .path()
        .app_cache_dir()
        .map_err(|_| "无法访问图片缓存目录")?
        .join("covers");
    if let Some(path) = cached(&root, &key) {
        return Ok(json!({"path":path}));
    }
    static DOWNLOADS: OnceLock<Semaphore> = OnceLock::new();
    let _permit = DOWNLOADS
        .get_or_init(|| Semaphore::new(4))
        .acquire()
        .await
        .map_err(|_| "图片下载已停止")?;
    if let Some(path) = cached(&root, &key) {
        return Ok(json!({"path":path}));
    }
    let referer = parsed
        .host_str()
        .filter(|host| {
            host.ends_with(".doubanio.com") || *host == "doubanio.com" || *host == "img.douban.com"
        })
        .map(|_| "https://movie.douban.com/");
    let (bytes, extension) = crate::cover_download::fetch_with_referer(url, false, referer).await?;
    std::fs::create_dir_all(&root).map_err(|_| "无法创建图片缓存")?;
    let destination = root.join(format!("media-{key}.{extension}"));
    let temporary = root.join(format!("media-{}.part", uuid::Uuid::new_v4()));
    std::fs::write(&temporary, bytes).map_err(|_| "无法保存影视图片")?;
    if let Err(error) = std::fs::rename(&temporary, &destination) {
        let _ = std::fs::remove_file(&temporary);
        if !destination.is_file() {
            let _ = error;
            return Err("无法完成影视图片缓存".into());
        }
    }
    Ok(json!({"path":destination}))
}
