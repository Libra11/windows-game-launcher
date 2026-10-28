//! 手动豆瓣元数据：先预览后确认，原地更新条目，不合并或删除作品。
use super::{active, browser, connection, dir, navigate, runtime, Runtime};
use crate::media::store;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};

pub(super) struct Preview {
    token: String,
    item: String,
    identity: Value,
    detail: Value,
    created: Instant,
}

struct Lease(Arc<Runtime>);
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}

// 图片只能来自本机已获取的有效预览，调用方不能传任意下载地址。
pub(in crate::media) fn preview_poster(
    root: &std::path::Path,
    item: &str,
    token: &str,
) -> Result<String, String> {
    let rt = runtime(root);
    let preview = rt.preview.lock().unwrap_or_else(|e| e.into_inner());
    let preview = preview
        .as_ref()
        .filter(|p| !token.is_empty() && p.token == token && p.item == item
            && p.created.elapsed() <= Duration::from_secs(900))
        .ok_or("豆瓣预览已失效，请重新获取")?;
    Ok(store::text(&preview.detail, "posterUrl").to_string())
}

fn subject_id(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    let digits = |value: &str| {
        !value.is_empty()
            && value.len() <= 20
            && value.bytes().all(|c| c.is_ascii_digit())
            && value.bytes().any(|c| c != b'0')
    };
    if digits(raw) {
        return Ok(raw.into());
    }
    let url: tauri::Url = raw.parse().map_err(|_| "请填写豆瓣电影作品链接或编号")?;
    if url.scheme() != "https"
        || url.host_str() != Some("movie.douban.com")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("请使用 https://movie.douban.com/subject/编号/ 作品链接".into());
    }
    let id = url
        .path()
        .strip_prefix("/subject/")
        .unwrap_or("")
        .trim_end_matches('/');
    if !digits(id) {
        return Err("豆瓣作品编号无效".into());
    }
    Ok(id.into())
}

fn normalize(page: &Value, subject: &str) -> Result<Value, String> {
    if page["restricted"] == true || page["login"] == true {
        return Err("豆瓣需要登录或安全验证，请在影视设置中打开豆瓣窗口后重试".into());
    }
    let raw = &page["detail"];
    let title = store::text(raw, "title").trim();
    let kind = match store::text(raw, "kind") {
        "movie" => "movie",
        "series" => "season",
        _ => return Err("无法确定豆瓣作品类型，原条目未修改".into()),
    };
    if raw["subjectId"] != subject || title.is_empty() || title.chars().count() > 500 {
        return Err("豆瓣作品资料不完整，原条目未修改".into());
    }
    let year = store::text(raw, "year");
    if !year.is_empty() && (year.len() != 4 || !year.bytes().all(|c| c.is_ascii_digit())) {
        return Err("豆瓣作品年份无效".into());
    }
    let season = if kind == "season" && !raw["seasonNumber"].is_null() {
        Some(
            raw["seasonNumber"]
                .as_u64()
                .filter(|n| *n <= u32::MAX as u64)
                .ok_or("豆瓣季号无效")?,
        )
    } else {
        None
    };
    let imdb = store::text(raw, "imdbId");
    if !imdb.is_empty() && !store::douban_source::valid_imdb_id(imdb) {
        return Err("豆瓣 IMDb 编号无效".into());
    }
    // 只接受豆瓣图片域名，页面缺少可用图片时保持无图。
    let poster = store::text(raw, "posterUrl")
        .parse::<tauri::Url>()
        .ok()
        .filter(|url| {
            url.scheme() == "https"
                && url.port_or_known_default() == Some(443)
                && url.username().is_empty()
                && url.password().is_none()
                && url
                    .host_str()
                    .is_some_and(|host| host == "doubanio.com" || host.ends_with(".doubanio.com"))
        })
        .map(|url| url.to_string())
        .unwrap_or_default();
    let genres: Vec<_> = raw["genres"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty() && v.chars().count() <= 80)
        .take(30)
        .collect();
    Ok(
        json!({"subjectId":subject,"title":title,"kind":kind,"seasonNumber":season,
        "year":year,"overview":store::text(raw,"overview").chars().take(20000).collect::<String>(),
        "posterUrl":poster,"genres":genres,"imdbId":imdb}),
    )
}

fn identity(item: &Value) -> Value {
    json!([
        item["kind"],
        item["parentId"],
        item["recognized"],
        item["externalIds"],
        item["title"],
        item["year"],
        item["overview"],
        item["posterUrl"],
        item["seasonNumber"]
    ])
}

fn check_item(conn: &Connection, item: &str, subject: &str) -> Result<Value, String> {
    let value = store::item(conn, item)?;
    if !matches!(store::text(&value, "kind"), "movie" | "season" | "other")
        || !value["parentId"].is_null() && value["parentId"].as_str().is_some_and(|s| !s.is_empty())
        || conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM media_items WHERE parent_id=?1)",
                [item],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|e| e.to_string())?
    {
        return Err("豆瓣仅支持独立电影或季；已有季集层级请先核对原有识别".into());
    }
    for id in value["externalIds"].as_array().into_iter().flatten() {
        if id["provider"] == "tmdb" {
            return Err("条目已有 TMDB 身份，请先解除识别再使用豆瓣资料".into());
        }
        if id["provider"] == "douban" && id["externalId"] != subject {
            return Err("条目已有不同的豆瓣来源身份，不能覆盖".into());
        }
    }
    let owner: Option<String> = conn.query_row("SELECT item_id FROM external_ids WHERE provider='douban' AND provider_type='subject' AND external_id=?1",[subject],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
    if owner.is_some_and(|owner| owner != item) {
        return Err("该豆瓣作品已在库内其他条目中，请核对并关联已有条目".into());
    }
    Ok(value)
}

fn save(conn: &Connection, item: &str, detail: &Value, expected: &Value) -> Result<Value, String> {
    let subject = store::text(detail, "subjectId");
    let current = check_item(conn, item, subject)?;
    if identity(&current) != *expected {
        return Err("作品资料已变化，请重新获取豆瓣预览".into());
    }
    conn.execute("INSERT OR IGNORE INTO external_ids(provider,provider_type,external_id,item_id)VALUES('douban','subject',?1,?2)",params![subject,item]).map_err(|e|e.to_string())?;
    conn.execute("UPDATE media_items SET kind=?2,title=?3,year=?4,overview=?5,poster_url=?6,backdrop_url='',genres_json=?7,recognized=1,season_number=?8,episode_number=NULL,updated_at=?9 WHERE id=?1",
        params![item,store::text(detail,"kind"),store::text(detail,"title"),store::text(detail,"year"),store::text(detail,"overview"),store::text(detail,"posterUrl"),detail["genres"].to_string(),detail["seasonNumber"].as_i64(),store::now()]).map_err(|e|e.to_string())?;
    // 空 account_key 专用于手动作品资料；不覆盖任何账号导入的原始来源或个人记录。
    let raw = json!({"detail":detail,"detailCheckedAt":store::now(),"detailError":""});
    conn.execute("INSERT INTO douban_source_data(account_key,subject_id,raw_json)VALUES('',?1,?2) ON CONFLICT(account_key,subject_id)DO UPDATE SET raw_json=excluded.raw_json",params![subject,raw.to_string()]).map_err(|e|e.to_string())?;
    conn.execute(
        "UPDATE resource_links SET method='manual',locked=1 WHERE item_id=?1",
        [item],
    )
    .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM media_match_errors WHERE item_id=?1", [item])
        .map_err(|e| e.to_string())?;
    store::item(conn, item)
}

pub async fn manual_metadata(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let _operation = crate::media::operation(&app).await?;
    let root = dir(&app);
    let rt = runtime(&root);
    let item = store::text(&input, "itemId");
    match store::text(&input, "action") {
        "preview" => {
            let subject = subject_id(store::text(&input, "subject"))?;
            let original = check_item(&store::open(&root)?, item, &subject)?;
            if connection(&app)?["phase"] == "loggingIn" {
                return Err("豆瓣窗口正在登录，请完成后重试".into());
            }
            rt.busy
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .map_err(|_| "豆瓣窗口正忙，请稍后重试")?;
            let _lease = Lease(rt.clone());
            rt.cancel.store(false, Ordering::Release);
            let generation = rt.generation.fetch_add(1, Ordering::AcqRel) + 1;
            let window = browser(&app, false).await?;
            let url = format!("https://movie.douban.com/subject/{subject}/")
                .parse()
                .map_err(|_| "豆瓣作品地址无效")?;
            let page = navigate(&window, &url, &rt, generation).await?;
            let detail = normalize(&page, &subject)?;
            active(&rt, generation)?;
            let token = uuid::Uuid::new_v4().to_string();
            let response = json!({"token":token,"detail":detail});
            *rt.preview.lock().unwrap_or_else(|e| e.into_inner()) = Some(Preview {
                token,
                item: item.into(),
                identity: identity(&original),
                detail,
                created: Instant::now(),
            });
            Ok(response)
        }
        "apply" => {
            let preview = rt.preview.lock().unwrap_or_else(|e| e.into_inner());
            let preview = preview
                .as_ref()
                .filter(|p| {
                    p.token == store::text(&input, "token")
                        && p.item == item
                        && p.created.elapsed() <= Duration::from_secs(900)
                })
                .ok_or("豆瓣预览已失效，请重新获取")?;
            let mut conn = store::open(&root)?;
            let tx = conn
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let result = save(&tx, item, &preview.detail, &preview.identity)?;
            tx.commit().map_err(|e| e.to_string())?;
            super::changed(&app);
            Ok(result)
        }
        _ => Err("豆瓣资料操作无效".into()),
    }
}

#[cfg(test)]
mod tests;
