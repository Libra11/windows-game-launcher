mod client;
mod local;
pub use client::api;
pub(super) mod diagnostics;
mod imdb;
mod matching;
mod prefilter;
mod retry;
mod seasons;
mod reidentify;
pub(super) mod season_match;
mod worker;
use super::{model, store, State};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Duration,
};
use tauri::{Emitter, Manager};

pub(super) use retry::initialize as initialize_retry;

fn validate_details(raw: &Value, id: &str, kind: &str) -> Result<(), String> {
    let key = if kind == "tv" { "name" } else { "title" };
    if raw["id"].as_u64().map(|n| n.to_string()).as_deref() != Some(id)
        || raw[key].as_str().is_none_or(|s| s.trim().is_empty())
    {
        return Err("TMDB 作品资料不完整，已保留原条目".into());
    }
    Ok(())
}
async fn details(dir: &Path, kind: &str, id: &str) -> Result<Value, String> {
    details_typed(dir, kind, id).await.map_err(Into::into)
}
async fn details_typed(dir: &Path, kind: &str, id: &str) -> Result<Value, retry::Failure> {
    let raw = client::api_typed(
        dir,
        &format!("/{kind}/{id}"),
        &[("append_to_response", "alternative_titles".into())],
    )
    .await?;
    validate_details(&raw, id, kind)?;
    Ok(raw)
}
fn image(value: &Value, size: &str) -> String {
    value
        .as_str()
        .filter(|path| {
            path.starts_with('/') && !path.contains("..") && !path.contains(['\r', '\n'])
        })
        .map(|path| format!("https://image.tmdb.org/t/p/{size}{path}"))
        .unwrap_or_default()
}
fn candidate(raw: &Value, kind: &str) -> Value {
    let kind = if kind == "tv" { "tv" } else { "movie" };
    let title = if kind == "tv" {
        raw["name"].as_str()
    } else {
        raw["title"].as_str()
    }
    .unwrap_or("");
    let original = if kind == "tv" {
        raw["original_name"].as_str()
    } else {
        raw["original_title"].as_str()
    }
    .unwrap_or("");
    let date = if kind == "tv" {
        raw["first_air_date"].as_str()
    } else {
        raw["release_date"].as_str()
    }
    .unwrap_or("");
    json!({
        "provider":"tmdb",
        "providerType":kind,
        "id":raw["id"],
        "externalId":raw["id"].as_i64().map(|v|v.to_string()).unwrap_or_default(),
        "kind":if kind=="tv"{ "series"} else{ "movie"},
        "title":title,
        "originalTitle":original,
        "year":date.get(..4).unwrap_or(""),
        "overview":raw["overview"],
        "posterUrl":image(&raw["poster_path"], "w500"),
        "backdropUrl":image(&raw["backdrop_path"], "w1280"),
    })
}
pub async fn search_candidates(dir: &Path, query: &str, kind: &str) -> Result<Vec<Value>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    if query.chars().count() > 200 {
        return Err("搜索名称过长".into());
    }
    let endpoint = match kind {
        "movie" => "movie",
        "tv" | "series" | "season" | "episode" => "tv",
        _ => "multi",
    };
    let raw = api(
        dir,
        &format!("/search/{endpoint}"),
        &[
            ("query", query.trim().into()),
            ("include_adult", "false".into()),
        ],
    )
    .await?;
    let rows = raw["results"]
        .as_array()
        .cloned()
        .ok_or("TMDB 搜索响应不完整")?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let kind = if endpoint == "multi" {
                row["media_type"].as_str().unwrap_or("")
            } else {
                endpoint
            };
            matches!(kind, "movie" | "tv").then(|| candidate(&row, kind))
        })
        .take(20)
        .collect())
}
pub async fn search(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let _operation = super::operation(&app).await?;
    let dir = app.state::<State>().dir.clone();
    let query = store::text(&input, "query");
    let kind = store::text(&input, "kind");
    let result = matching::search_targets(&dir, store::text(&input, "itemId"), query, kind).await?;
    Ok(json!(result))
}
fn data(raw: &Value, kind: &str) -> Value {
    let mut value = candidate(raw, kind);
    if kind == "tv" {
        value["seasonCount"] = raw["number_of_seasons"].clone();
    }
    value["genres"] = json!(raw["genres"]
        .as_array()
        .map(|values| values
            .iter()
            .filter_map(|v| v["name"].as_str())
            .collect::<Vec<_>>())
        .unwrap_or_default());
    value
}
fn parse_id(candidate: &Value) -> Result<(String, String), String> {
    let kind = candidate["providerType"].as_str().ok_or("资料类型无效")?;
    let kind = match kind {
        "movie" => "movie",
        "tv" => "tv",
        _ => return Err("资料类型无效".into()),
    };
    let id = candidate["externalId"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or("资料编号无效")?;
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return Err("TMDB 编号无效".into());
    }
    Ok((kind.into(), id))
}
async fn selected_data(
    dir: &Path,
    candidate: &Value,
    parsed: Option<&model::ParsedName>,
) -> Result<(Value, Option<Value>, Option<Value>, String, String), retry::Failure> {
    let (kind, id) = parse_id(candidate)?;
    let raw = details_typed(dir, &kind, &id).await?;
    let main = data(&raw, &kind);
    let mut selection = parsed.cloned();
    if let Some(number) = candidate.get("seasonNumber") {
        if kind != "tv" {
            return Err("电影资料不能指定季".into());
        }
        let number = number
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or("季号无效")?;
        let value = selection.get_or_insert(model::ParsedName {
            title: String::new(),
            year: None,
            season: None,
            episodes: vec![],
            explicit_tmdb: None,
            ambiguous_season: false,
        });
        if !value.episodes.is_empty() && value.season.is_some_and(|n| n != number) {
            return Err("所选季与本地文件季号不一致".into());
        }
        value.season = Some(number);
    }
    if let Some(number) = candidate.get("episodeNumber") {
        if kind != "tv" || selection.as_ref().and_then(|p| p.season).is_none() {
            return Err("单集资料需要明确的电视剧和季号".into());
        }
        let number = number
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n > 0)
            .ok_or("集号无效")?;
        let parsed = selection.as_mut().unwrap();
        if !parsed.episodes.is_empty() && parsed.episodes != [number] {
            return Err("所选集与本地文件集号不一致".into());
        }
        parsed.episodes = vec![number];
    }
    let parsed = selection.as_ref();
    if kind == "tv" {
        if let Some(parsed) = parsed {
            if let Some(season) = parsed.season {
                let seasons = raw["seasons"].as_array().ok_or("TMDB 季列表不完整")?;
                if !seasons
                    .iter()
                    .any(|v| v["season_number"].as_u64() == Some(season as u64))
                {
                    return Err(retry::Failure::unmatched(
                        format!("资料源没有第 {season} 季，已保留为待识别，不按文件名强行关联"),
                        false,
                    ));
                }
                let season_raw =
                    client::api_typed(dir, &format!("/tv/{id}/season/{season}"), &[]).await?;
                if season_raw["season_number"].as_u64() != Some(season as u64)
                    || season_raw["id"].as_u64().is_none_or(|n| n == 0)
                    || season_raw["name"].as_str().is_none()
                {
                    return Err("TMDB 季资料不完整，已保留待识别".into());
                }
                let mut season_data = json!({
                    "title":season_raw["name"],
                    "kind":"season",
                    "seasonNumber":season,
                    "overview":season_raw["overview"],
                    "posterUrl":image(&season_raw["poster_path"], "w500"),
                    "year":store::text(&season_raw, "air_date").get(..4).unwrap_or(""),
                    "backdropUrl":main["backdropUrl"],
                    "genres":main["genres"],
                    "episodes":seasons::episodes(&season_raw,season as u64)?,
                });

                if season_data["title"].as_str().is_none_or(str::is_empty) {
                    season_data["title"] = json!(format!("第 {season} 季"));
                }
                if parsed.episodes.len() > 1 {
                    return Err(retry::Failure::unmatched(
                        "文件包含多集，请在资源详情中手动确认分集关联",
                        false,
                    ));
                }
                if let Some(episode) = parsed.episodes.first() {
                    let episode_raw = season_raw["episodes"]
                        .as_array()
                        .and_then(|list| {
                            list.iter()
                                .find(|v| v["episode_number"].as_u64() == Some(*episode as u64))
                        })
                        .ok_or_else(|| {
                            retry::Failure::unmatched(
                                format!("资料源没有 S{season:02}E{episode:02}，已保留为待识别"),
                                false,
                            )
                        })?;
                    if let Some(expected) = candidate.get("episodeTmdbId") {
                        if expected.as_str().is_none_or(str::is_empty)
                            || episode_raw["id"].as_u64().map(|n| n.to_string()).as_deref()
                                != expected.as_str()
                        {
                            return Err(retry::Failure::unmatched(
                                "TMDB 季中单集与 IMDb 查询身份不一致，已保留待识别",
                                false,
                            ));
                        }
                    }
                    let mut episode_data = json!({
                        "title":episode_raw["name"],
                        "kind":"episode",
                        "seasonNumber":season,
                        "episodeNumber":episode,
                        "overview":episode_raw["overview"],
                        "posterUrl":image(&episode_raw["still_path"], "w500"),
                        "year":store::text(episode_raw, "air_date").get(..4).unwrap_or(""),
                    });

                    if episode_data["title"].as_str().is_none_or(str::is_empty) {
                        episode_data["title"] = json!(format!("第 {episode} 集"));
                    }
                    return Ok((main, Some(season_data), Some(episode_data), kind, id));
                }
                return Ok((main, Some(season_data), None, kind, id));
            }
        }
    }
    if kind == "tv" {
        return Err(retry::Failure::unmatched(
            "电视剧需要对应到具体季，请选择季后再识别",
            false,
        ));
    }
    Ok((main, None, None, kind, id))
}
pub async fn identify_item(
    dir: &Path,
    item_id: &str,
    candidate: &Value,
    manual: bool,
) -> Result<Value, String> {
    identify_item_typed(dir, item_id, candidate, manual)
        .await
        .map_err(Into::into)
}
async fn identify_item_typed(
    dir: &Path,
    item_id: &str,
    candidate: &Value,
    manual: bool,
) -> Result<Value, retry::Failure> {
    let conn = store::open(dir)?;
    let original = store::item(&conn, item_id)?;
    let files: Vec<_> = store::resources_for(&conn, item_id)?
        .into_iter()
        .filter(|resource| resource["ignored"] != true)
        .collect();
    let parsed = files
        .first()
        .filter(|_| !manual || !matches!(store::text(&original,"kind"), "season" | "series"))
        .map(|resource| model::parse_path(Path::new(store::text(resource, "path"))));
    drop(conn);
    let (main, season, episode, kind, external_id) =
        selected_data(dir, candidate, parsed.as_ref()).await?;
    if kind == "tv"
        && season.is_none()
        && original["externalIds"]
            .as_array()
            .is_some_and(|ids| ids.iter().any(|id| id["provider"] == "douban"))
    {
        return Err(retry::Failure::unmatched(
            "豆瓣剧集需要对应到具体季，请在搜索结果中选择季",
            false,
        ));
    }
    let mut conn = store::open(dir)?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let latest = store::item(&tx, item_id)?;
    // 网络请求期间可能已完成手动豆瓣识别，后台结果不能覆盖人工确认。
    if !manual && latest["recognized"] == true {
        return Ok(latest);
    }
    if manual && latest["externalIds"] != original["externalIds"] {
        return Err("作品身份已变化，请重新打开识别窗口".into());
    }
    if !manual && !super::ignored::metadata_allowed(&tx, item_id)? {
        return Err("条目已移除或资源已忽略，已取消资料识别".into());
    }
    let ext = original["externalIds"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if ext.iter().any(|v| {
        v["provider"] == "tmdb"
            && (v["providerType"] != kind || v["externalId"] != external_id)
            && !matches!(v["providerType"].as_str(), Some("season" | "episode"))
    }) {
        return Err("条目已有不同的 TMDB 关联，请先解除资料识别再重新选择".into());
    }
    let selection_identity = if let Some(ref season) = season {
        if episode.is_some() { ("episode", format!("{external_id}:{}:{}",season["seasonNumber"],episode.as_ref().unwrap()["episodeNumber"])) }
        else { ("season",format!("{external_id}:{}",season["seasonNumber"])) }
    } else { (kind.as_str(), external_id.clone()) };
    reidentify::validate_children(&tx,item_id,selection_identity.0,&selection_identity.1)?;
    let target = save_selection(
        &tx,
        &main,
        season,
        episode,
        &kind,
        &external_id,
        original["independent"].as_bool().unwrap_or(false),
    )?;
    if original["recognized"] == true && target != item_id {
        return Err("条目已识别，请先解除识别再选择其他作品".into());
    }
    store::merge_items(&tx, item_id, &target)?;
    if manual {
        tx.execute(
            "UPDATE resource_links SET method='manual',locked=1 WHERE item_id=?1",
            [&target],
        )
        .map_err(|e| e.to_string())?;
    } else {
        tx.execute(
            "UPDATE resource_links SET method='auto' WHERE item_id=?1 AND locked=0",
            [&target],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    store::item(&conn, &target).map_err(Into::into)
}

fn save_selection(
    conn: &rusqlite::Connection,
    main: &Value,
    season: Option<Value>,
    episode: Option<Value>,
    kind: &str,
    external_id: &str,
    independent: bool,
) -> Result<String, String> {
    let mut target =
        store::upsert_provider_item(conn, "tmdb", kind, external_id, main, independent)?;
    if let Some(mut season) = season {
        season["parentId"] = json!(&target);
        let number = season["seasonNumber"].as_u64().unwrap();
        target = store::upsert_provider_item(
            conn,
            "tmdb",
            "season",
            &format!("{external_id}:{number}"),
            &season,
            false,
        )?;
        seasons::save_episodes(
            conn,
            &target,
            &format!("{external_id}:{number}"),
            season["episodes"].as_array().unwrap(),
        )?;
        if let Some(mut episode) = episode {
            episode["parentId"] = json!(&target);
            let episode_number = episode["episodeNumber"].as_u64().unwrap();
            target = store::upsert_provider_item(
                conn,
                "tmdb",
                "episode",
                &format!("{external_id}:{number}:{episode_number}"),
                &episode,
                false,
            )?;
        }
    }
    Ok(target)
}

pub async fn create_item(dir: &Path, candidate: &Value) -> Result<Value, String> {
    if candidate["provider"] != "tmdb"
        || !matches!(candidate["kind"].as_str(), Some("movie" | "season"))
    {
        return Err("请搜索并选择 TMDB 电影或具体季后添加".into());
    }
    let (kind, id) = parse_id(candidate)?;
    let (provider_type, external_id) = match (candidate["kind"].as_str(), kind.as_str()) {
        (Some("movie"), "movie") if candidate.get("seasonNumber").is_none() => ("movie", id),
        (Some("season"), "tv") => {
            let number = candidate["seasonNumber"]
                .as_u64()
                .filter(|number| u32::try_from(*number).is_ok())
                .ok_or("请选择具体季后添加")?;
            ("season", format!("{id}:{number}"))
        }
        _ => return Err("请选择电影或具体季后添加".into()),
    };
    {
        let mut conn = store::open(dir)?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let existing: Option<String> = tx.query_row(
            "SELECT item_id FROM external_ids WHERE provider='tmdb' AND provider_type=?1 AND external_id=?2",
            params![provider_type, external_id], |row| row.get(0),
        ).optional().map_err(|e| e.to_string())?;
        if let Some(target) = existing {
            tx.execute(
                "UPDATE media_items SET independent=1 WHERE id=?1",
                [&target],
            )
            .map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())?;
            return store::item(&conn, &target);
        }
    }
    let (main, season, episode, kind, external_id) = selected_data(dir, candidate, None).await?;
    let mut conn = store::open(dir)?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let target = save_selection(&tx, &main, season, episode, &kind, &external_id, false)?;
    // 独立收录具体电影或季；没有本地文件也不会被后续清理删除。
    tx.execute(
        "UPDATE media_items SET independent=1 WHERE id=?1",
        [&target],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    store::item(&conn, &target)
}
fn unlink_metadata(conn: &rusqlite::Connection, id: &str) -> Result<(), String> {
    // Imported subject identity is provenance, so a later source update must reuse it.
    conn.execute(
        "DELETE FROM external_ids WHERE item_id=?1 AND provider<>'douban'",
        [id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE media_items SET recognized=0,updated_at=?2 WHERE id=?1",
        params![id, store::now()],
    )
    .map_err(|e| e.to_string())?;

    conn.execute(
        "UPDATE resource_links SET method='manual',locked=1 WHERE item_id=?1",
        [id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
fn unlink_resource(
    conn: &rusqlite::Connection,
    item_id: &str,
    resource_id: &str,
) -> Result<String, String> {
    let resource = store::resource(conn, resource_id)?;
    let linked: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM resource_links WHERE resource_id=?1 AND item_id=?2)",
            params![resource_id, item_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !linked {
        return Err("该资源不属于当前条目".into());
    }
    let old_linked_ids = vec![item_id.to_string()];
    let new_id = store::new_item(conn, &json!({"title":resource["name"]}))?;
    conn.execute(
        "DELETE FROM resource_links WHERE resource_id=?1 AND item_id=?2",
        params![resource_id, item_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO resource_links(resource_id,item_id,method,locked)VALUES(?1,?2,'manual',1)",
        params![resource_id, new_id],
    )
    .map_err(|e| e.to_string())?;
    super::removal::orphaned(conn, &old_linked_ids)?;
    Ok(new_id)
}

pub async fn identify(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let _operation = super::operation(&app).await?;
    let dir = app.state::<State>().dir.clone();
    let item_id = store::text(&input, "itemId");
    let result = if input["unlink"] == true {
        let input = input.clone();

        tauri::async_runtime::spawn_blocking(move || {
            let _operation = _operation;
            let mut conn = store::open(&dir)?;
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            let id = store::text(&input, "itemId");
            store::item(&tx, id)?;
            let target = if let Some(resource_id) = input["resourceId"].as_str() {
                unlink_resource(&tx, id, resource_id)?
            } else {
                unlink_metadata(&tx, id)?;
                id.to_string()
            };
            tx.commit().map_err(|e| e.to_string())?;
            store::item(&conn, &target)
        })
        .await
        .map_err(|_| "解除关联失败")??
    } else if let Some(target) = input["targetItemId"].as_str() {
        let target = target.to_string();
        let source = item_id.to_string();
        tauri::async_runtime::spawn_blocking(move || {
            let _operation = _operation;
            let mut conn = store::open(&dir)?;
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            let original = store::item(&tx, &source)?;
            let destination = store::item(&tx, &target)?;
            validate_manual_merge(&original, &destination)?;
            reidentify::validate_merge_children(&tx,&source,&destination)?;
            if original["externalIds"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|id| id["provider"] == "douban"))
                && matches!(store::text(&destination, "kind"), "series" | "episode")
            {
                return Err("豆瓣记录请关联电影或具体季".into());
            }
            store::merge_items(&tx, &source, &target)?;
            tx.execute(
                "UPDATE resource_links SET locked=1,method='manual' WHERE item_id=?1",
                [&target],
            )
            .map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())?;
            store::item(&conn, &target)
        })
        .await
        .map_err(|_| "手动关联失败")??
    } else {
        identify_item(&dir, item_id, &input["candidate"], true).await?
    };
    let _ = app.emit("media-changed", ());
    Ok(result)
}
fn validate_manual_merge(source: &Value, target: &Value) -> Result<(), String> {
    if source["recognized"] == true || target["recognized"] != true {
        return Err("仅允许将未识别条目合并到已识别作品".into());
    }
    for left in source["externalIds"].as_array().into_iter().flatten() {
        let conflict = target["externalIds"].as_array().into_iter().flatten().any(|right|
            left["provider"] == right["provider"] && left["providerType"] == right["providerType"]
            && left["externalId"] != right["externalId"]);
        if conflict { return Err("双方已有冲突的来源身份，不能合并".into()); }
    }
    Ok(())
}
pub async fn auto_identify(dir: &Path, item_id: &str) -> Result<Value, String> {
    auto_identify_typed(dir, item_id)
        .await
        .map_err(|e| e.message)
}
async fn auto_identify_typed(dir: &Path, item_id: &str) -> Result<Value, retry::Failure> {
    let conn = store::open(dir)?;
    let item = store::item(&conn, item_id)?;
    if !super::ignored::metadata_allowed(&conn, item_id)? {
        return Err("条目已移除或资源已忽略，已取消资料识别".into());
    }
    if item["recognized"] == true {
        return Ok(item);
    }
    let locked: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM resource_links WHERE item_id=?1 AND locked=1)",
            [item_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if locked {
        return Err("人工关联受保护".into());
    }
    let resources: Vec<_> = store::resources_for(&conn, item_id)?
        .into_iter()
        .filter(|resource| resource["ignored"] != true)
        .collect();
    let Some(first) = resources.first() else {
        drop(conn);
        let candidate = matching::automatic(dir, &item).await?;
        return identify_item_typed(dir, item_id, &candidate, false).await;
    };
    if let Some(reason) = super::file_filter::automatic_rejection(first)? {
        return Err(retry::Failure::unmatched(reason, false));
    }
    let path = Path::new(store::text(first, "path"));
    let parsed = model::parse_path(path);
    if parsed.ambiguous_season {
        return Err(retry::Failure::unmatched(
            "文件或目录包含多个季，缺少可靠的当前季信息，请手动识别",
            false,
        ));
    }
    if parsed.season.is_some() && parsed.episodes.is_empty() {
        return Err(retry::Failure::unmatched(
            "文件包含季号但缺少明确集号，请手动确认对应剧集",
            false,
        ));
    }
    drop(conn);
    let hints = matching::hints(dir, &item)?;
    if !hints.imdb_id.is_empty() {
        if let Some(candidate) = imdb::automatic(dir, &hints).await? {
            return identify_item_typed(dir, item_id, &candidate, false).await;
        }
    }
    let candidate = if let Some((kind, id)) = &parsed.explicit_tmdb {
        local::explicit(dir, &parsed, kind, id).await?
    } else {
        local::automatic(dir, path, &parsed).await?
    };
    identify_item_typed(dir, item_id, &candidate, false).await
}

static REFRESHING: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
fn legacy_retry_due(conn: &rusqlite::Connection, id: &str) -> bool {
    let checked: Result<String, _> = conn.query_row(
        "SELECT checked_at FROM media_match_errors WHERE item_id=?1",
        [id],
        |r| r.get(0),
    );
    checked
        .ok()
        .and_then(|v| chrono::DateTime::parse_from_rfc3339(&v).ok())
        .is_none_or(|time| chrono::Utc::now().signed_duration_since(time).num_seconds() >= 900)
}
fn retry_due(conn: &rusqlite::Connection, id: &str) -> bool {
    retry::due(conn, id)
}

pub fn start(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(15)).await;
        loop {
            let _ = refresh(app.clone(), json!({"automatic":true})).await;
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}
async fn refresh_known(dir: &Path, item_id: &str) -> Result<Value, String> {
    let original = {
        let conn = store::open(dir)?;
        store::item(&conn, item_id)?
    };
    let ids = original["externalIds"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let Some(external) = ids.iter().find(|v| v["provider"] == "tmdb") else {
        return Ok(original);
    };
    let kind = store::text(external, "providerType");
    let id = store::text(external, "externalId");
    let mut updated = match kind {
        "movie" | "tv" => {
            let raw = details(dir, kind, id).await?;
            data(&raw, kind)
        }
        "season" | "episode" => {
            let components: Vec<&str> = id.split(':').collect();
            if components.len() < 2
                || components
                    .iter()
                    .any(|v| v.is_empty() || !v.chars().all(|c| c.is_ascii_digit()))
            {
                return Err("季集资料编号无效".into());
            }
            let path = if kind == "season" {
                format!("/tv/{}/season/{}", components[0], components[1])
            } else {
                if components.len() != 3 {
                    return Err("集资料编号无效".into());
                }
                format!(
                    "/tv/{}/season/{}/episode/{}",
                    components[0], components[1], components[2]
                )
            };
            let raw = api(dir, &path, &[]).await?;
            if raw["id"].as_u64().is_none_or(|n| n == 0)
                || raw["name"].as_str().is_none()
                || raw["season_number"]
                    .as_u64()
                    .map(|n| n.to_string())
                    .as_deref()
                    != Some(components[1])
                || (kind == "episode"
                    && raw["episode_number"]
                        .as_u64()
                        .map(|n| n.to_string())
                        .as_deref()
                        != Some(components[2]))
            {
                return Err("TMDB 季集资料不完整，保留原有资料".into());
            }
            let mut value = json!({
                "title":raw["name"],
                "kind":kind,
                "parentId":original["parentId"],
                "seasonNumber":original["seasonNumber"],
                "episodeNumber":original["episodeNumber"],
                "overview":raw["overview"],
                "posterUrl":image(if kind=="season"{ &raw["poster_path"]} else{ &raw["still_path"]} , "w500"),
                "year":store::text(&raw, "air_date").get(..4).unwrap_or(""),
                "airDate":raw["air_date"],
            });
            if kind == "season" {
                value["episodes"] = json!(seasons::episodes(
                    &raw,
                    raw["season_number"].as_u64().unwrap()
                )?);
            }
            value
        }
        _ => return Ok(original),
    };
    updated["independent"] = original["independent"].clone();
    let mut conn = store::open(dir)?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let current = store::item(&tx, item_id)?;
    if current["recognized"] != true || current["externalIds"] != original["externalIds"] {
        return Ok(current);
    }
    if !super::ignored::metadata_allowed(&tx, item_id)? {
        return Err("条目已移除或资源已忽略，已取消资料刷新".into());
    }
    store::upsert_provider_item(
        &tx,
        "tmdb",
        kind,
        id,
        &updated,
        original["independent"].as_bool().unwrap_or(false),
    )?;
    if let Some(rows) = updated["episodes"].as_array() {
        seasons::save_episodes(&tx, item_id, id, rows)?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    store::item(&conn, item_id)
}
pub async fn refresh(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let operation = super::operation(&app).await?;
    let dir = app.state::<State>().dir.clone();
    if store::credential(&dir, "tmdbReadAccessToken")?.is_empty()
        && store::credential(&dir, "tmdbApiKey")?.is_empty()
    {
        return Err("请先配置 TMDB 凭据，再获取影视资料".into());
    }
    let conn = store::open(&dir)?;
    let items = if input["automatic"] == true && input["itemIds"].is_array() {
        let ids = input["itemIds"].as_array().unwrap().iter()
            .filter_map(Value::as_str).map(str::to_owned).collect::<Vec<_>>();
        store::items_by_ids(&conn, &ids)?
    } else {
        store::items(&conn)?
    };
    if !REFRESHING
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "任务状态不可用")?
        .insert(dir.clone())
    {
        return Err("已有影视资料任务正在执行".into());
    }
    let wanted = input["itemId"].as_str();
    if wanted.is_some() {
        client::clear_cache();
    }
    let ids: Vec<String> = items
        .iter()
        .filter(|item| {
            wanted.map(|id| item["id"] == id).unwrap_or(
                item["recognized"] == false
                    && (item["resourceCount"].as_u64().unwrap_or(0) > 0
                        || item["independent"] == true),
            )
        })
        .filter(|item| input["automatic"] != true || retry_due(&conn, store::text(item, "id")))
        .filter(|item| {
            super::ignored::metadata_allowed(&conn, store::text(item, "id")).unwrap_or(false)
        })
        .map(|v| store::text(v, "id").to_string())
        .collect();
    if ids.is_empty() {
        if let Ok(mut set) = REFRESHING.get_or_init(Default::default).lock() {
            set.remove(&dir);
        }
        return Ok(json!({"queued":false}));
    }
    let id = match store::job_start(&conn, "metadata", ids.len() as i64) {
        Ok(id) => id,
        Err(error) => {
            if let Ok(mut set) = REFRESHING.get_or_init(Default::default).lock() {
                set.remove(&dir);
            }
            return Err(error);
        }
    };
    let job = id.clone();
    drop(conn);
    tauri::async_runtime::spawn(async move {
        let _operation = operation;
        let mut count = 0;
        let mut failures = 0;
        for item in &ids {
            if !store::open(&dir)
                .and_then(|conn| super::ignored::metadata_allowed(&conn, item))
                .unwrap_or(false)
            {
                continue;
            }
            let Ok(fingerprint) =
                store::open(&dir).and_then(|conn| retry::fingerprint(&conn, item))
            else {
                continue;
            };
            let mut failure = None;
            let result = diagnostics::track(&dir, item, async {
                worker::identify(&app, &dir, item).await.map_err(|error| {
                    let message = error.message.clone();
                    failure = Some(error);
                    message
                })
            })
            .await;
            if result.is_err() {
                if let Ok(conn) = store::open(&dir) {
                    if !super::ignored::metadata_allowed(&conn, item).unwrap_or(false)
                        || store::item(&conn, item).is_ok_and(|value| value["recognized"] == true) {
                        continue;
                    }
                    failures += 1;
                    if let Some(failure) = failure.as_ref() {
                        let _ = retry::record(&conn, item, failure, &fingerprint);
                    }
                }
            } else if let Ok(conn) = store::open(&dir) {
                let _ = conn.execute("DELETE FROM media_match_retry WHERE item_id=?1", [item]);
            }
            count += 1;
            if let Ok(conn) = store::open(&dir) {
                let _ = store::job_update(
                    &conn,
                    &job,
                    "running",
                    count,
                    ids.len() as i64,
                    "正在识别影视资料",
                    "",
                );
            }
            let _ = app.emit("media-job-changed", ());
        }
        if let Ok(conn) = store::open(&dir) {
            let _ = store::job_update(
                &conn,
                &job,
                "completed",
                count,
                ids.len() as i64,
                &format!("已处理 {count} 条，{failures} 条暂未识别，可稍后重试"),
                "",
            );
        }
        if let Ok(mut set) = REFRESHING.get_or_init(Default::default).lock() {
            set.remove(&dir);
        }
        let _ = app.emit("media-changed", ());
    });
    Ok(json!({"jobId":id}))
}

#[cfg(test)]
mod tests;
