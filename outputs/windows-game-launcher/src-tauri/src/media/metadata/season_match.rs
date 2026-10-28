//! 单季目录匹配：预览只读，确认仅重绑所选资源，不合并作品与个人记录。
mod files;
mod records;
#[cfg(test)]
mod tests;
use super::{selected_data, store};
use crate::media::{playback, State};
use rusqlite::{params, Connection, TransactionBehavior};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

#[derive(Clone)]
struct Plan {
    dir: PathBuf,
    input: Value,
    files: Vec<Value>,
    main: Value,
    season: Value,
    external: String,
    created: Instant,
    review: Option<records::Review>,
}
fn plans() -> &'static Mutex<HashMap<String, Plan>> {
    static PLANS: OnceLock<Mutex<HashMap<String, Plan>>> = OnceLock::new();
    PLANS.get_or_init(Default::default)
}
fn remember(plan: Plan) -> Result<String, String> {
    let mut plans = plans().lock().map_err(|_| "匹配预览暂不可用")?;
    plans.retain(|_, plan| plan.created.elapsed() < Duration::from_secs(900));
    if plans.len() >= 8 {
        if let Some(oldest) = plans
            .iter()
            .min_by_key(|(_, plan)| plan.created)
            .map(|(id, _)| id.clone())
        {
            plans.remove(&oldest);
        }
    }
    let token = uuid::Uuid::new_v4().to_string();
    plans.insert(token.clone(), plan);
    Ok(token)
}
fn take_plan(dir: &Path, token: &str) -> Result<Plan, String> {
    let mut plans = plans().lock().map_err(|_| "匹配预览暂不可用")?;
    let plan = plans
        .get(token)
        .filter(|plan| plan.dir == dir && plan.created.elapsed() < Duration::from_secs(900))
        .cloned()
        .ok_or("预览已过期，请重新预览")?;
    plans.remove(token);
    Ok(plan)
}
fn suggestion(file: &Value, season: &Value) -> Value {
    let parsed = crate::media::model::parse_path(Path::new(store::text(file, "path")));
    let number = season["seasonNumber"].as_u64().unwrap();
    let known: HashSet<u64> = season["episodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["episodeNumber"].as_u64())
        .collect();
    let mut episodes: Vec<u64> = parsed.episodes.iter().map(|n| *n as u64).collect();
    // 纯数字文件名仅在用户已明确选季之后作为集号线索，年份和分辨率不作集号。
    if episodes.is_empty() {
        let stem = Path::new(store::text(file, "path"))
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let stem = stem.trim();
        if !stem.is_empty() && stem.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(n) = stem.parse::<u64>() {
                if (1..1900).contains(&n) {
                    episodes.push(n);
                }
            }
        }
    }
    let reason = if !store::text(file, "error").is_empty() {
        store::text(file, "error").to_string()
    } else if parsed.ambiguous_season || parsed.season.is_some_and(|n| n as u64 != number) {
        "文件季号与所选季不同，请手动确认集号".into()
    } else if let Some(reason) = file["suggestionWarning"].as_str() {
        reason.to_string()
    } else if episodes.len() > 1 {
        "一个文件包含多集，请手动确认集号范围".into()
    } else if episodes.is_empty() {
        "未解析出明确集号，请手动选择".into()
    } else if !episodes.iter().all(|n| known.contains(n)) {
        "所选季不存在该集号，可调整偏移或手动选择".into()
    } else {
        String::new()
    };
    let can_suggest = store::text(file, "error").is_empty()
        && !parsed.ambiguous_season
        && parsed.season.is_none_or(|n| n as u64 == number)
        && file["suggestionWarning"].is_null()
        && episodes.len() == 1;
    let can_override_season = store::text(file, "error").is_empty()
        && !parsed.ambiguous_season
        && parsed.season.is_some_and(|n| n as u64 != number)
        && file["suggestionWarning"].is_null()
        && episodes.len() == 1;
    json!({"resourceId":file["id"],"path":file["path"],"name":file["name"],"current":file["links"],"parsedEpisodes":episodes,"canSuggest":can_suggest,"canOverrideSeason":can_override_season,
        "suggestedEpisodes":if reason.is_empty() { episodes.clone() } else { vec![] },"warning":reason,"disabled":!store::text(file,"error").is_empty()})
}
fn response(plan: &Plan, token: &str) -> Value {
    json!({"token":token,"title":plan.main["title"],"seasonTitle":plan.season["title"],"seasonNumber":plan.season["seasonNumber"],
        "episodes":plan.season["episodes"],"files":plan.files.iter().map(|file|suggestion(file,&plan.season)).collect::<Vec<_>>()})
}

fn validate_mappings(plan: &Plan, mappings: &Value) -> Result<Vec<(String, Vec<u64>)>, String> {
    let mappings = mappings.as_array().ok_or("分集对应关系无效")?;
    if mappings.is_empty() {
        return Err("请至少选择一个文件的目标分集".into());
    }
    let known: HashSet<u64> = plan.season["episodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["episodeNumber"].as_u64())
        .collect();
    let mut chosen = Vec::new();
    let mut seen = HashSet::new();
    for row in mappings {
        let id = store::text(row, "resourceId");
        if !seen.insert(id.to_string()) {
            return Err("同一个文件不能重复提交".into());
        }
        let file = plan
            .files
            .iter()
            .find(|file| file["id"] == id)
            .ok_or("文件不在本次预览范围内")?;
        if !store::text(file, "error").is_empty() {
            return Err("文件不可用，请重新扫描并预览".into());
        }
        let episodes = row["episodeNumbers"].as_array().ok_or("请选择有效分集")?;
        let mut numbers = Vec::new();
        for number in episodes {
            let n = number
                .as_u64()
                .filter(|n| known.contains(n))
                .ok_or("所选季不存在该分集")?;
            if numbers.contains(&n) {
                return Err("一个文件的分集不能重复".into());
            }
            numbers.push(n);
        }
        if numbers.is_empty() {
            return Err("请选择分集，跳过的文件不应提交".into());
        }
        chosen.push((id.to_string(), numbers));
    }
    Ok(chosen)
}
fn apply(conn: &mut Connection, plan: &Plan, resolutions: &Value) -> Result<Value, String> {
    let review = plan.review.as_ref().ok_or("请先预览更正影响")?;
    let chosen = validate_mappings(plan, &review.mappings)?;
    // 文件版本与关联的核对在同一写事务中；网络资料已在预览阶段取得。
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let current = files::collect(&tx, &plan.input)?;
    for (id, _) in &chosen {
        let original = plan.files.iter().find(|file| file["id"] == *id).unwrap();
        let latest = current
            .iter()
            .find(|file| file["id"] == *id)
            .ok_or("文件已移除、离线或被忽略，请重新预览")?;
        if original != latest {
            return Err("文件或原关联已变化，请重新预览后确认".into());
        }
    }
    let latest_review = records::review(&tx, plan, &review.mappings, &review.choices)?;
    if latest_review.baseline != review.baseline {
        return Err("作品记录或关联范围已变化，请重新预览更正影响".into());
    }
    let mut season = plan.season.clone();
    let series_id =
        store::upsert_provider_item(&tx, "tmdb", "tv", &plan.external, &plan.main, false)?;
    season["parentId"] = json!(series_id);
    let season_external = format!("{}:{}", plan.external, season["seasonNumber"]);
    let season_id =
        store::upsert_provider_item(&tx, "tmdb", "season", &season_external, &season, false)?;
    let episodes = season["episodes"].as_array().unwrap();
    let episode_ids = super::seasons::save_episodes(&tx, &season_id, &season_external, episodes)?;
    let targets: HashMap<u64, &String> = episodes
        .iter()
        .zip(episode_ids.iter())
        .map(|(row, id)| (row["episodeNumber"].as_u64().unwrap(), id))
        .collect();
    let mut old_ids = HashSet::new();
    for (id, numbers) in &chosen {
        let file = plan.files.iter().find(|file| file["id"] == *id).unwrap();
        for old in file["links"].as_array().unwrap() {
            old_ids.insert(store::text(old, "itemId").to_string());
        }
        tx.execute("DELETE FROM resource_links WHERE resource_id=?1", [id])
            .map_err(|e| e.to_string())?;
        for number in numbers {
            tx.execute("INSERT INTO resource_links(resource_id,item_id,method,locked)VALUES(?1,?2,'manual',1)",params![id,targets[number]]).map_err(|e|e.to_string())?;
        }
    }
    records::transfer(&tx, review, resolutions)?;
    crate::media::removal::orphaned_after_relink(&tx, &old_ids.into_iter().collect::<Vec<_>>())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(json!({"itemId":season_id,"matched":chosen.len()}))
}

pub async fn command(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let operation = crate::media::operation(&app).await?;
    let dir = app.state::<State>().dir.clone();
    match store::text(&input, "action") {
        "inspect" => tauri::async_runtime::spawn_blocking(move || {
            let _operation = operation;
            let files = files::collect(&store::open(&dir)?, &input)?;
            Ok(json!({"files":files,"count":files.len()}))
        })
        .await
        .map_err(|_| "读取单季目录失败")?,
        "preview" => {
            if input["candidate"]["providerType"] != "tv"
                || input["candidate"]["seasonNumber"].as_u64().is_none()
                || !input["candidate"]["episodeNumber"].is_null()
            {
                return Err("请选择电视剧的具体季".into());
            }
            let read_dir = dir.clone();
            let scope = input.clone();
            let files = tauri::async_runtime::spawn_blocking(move || {
                files::collect(&store::open(&read_dir)?, &scope)
            })
            .await
            .map_err(|_| "读取单季目录失败")??;
            if files.is_empty() {
                return Err("目录中没有已扫描的可用视频，请先扫描媒体目录".into());
            }
            let (main, season, episode, _, external) =
                selected_data(&dir, &input["candidate"], None)
                    .await
                    .map_err(String::from)?;
            if episode.is_some() {
                return Err("请选择整季资料".into());
            }
            let season = season.ok_or("请选择具体季")?;
            if season["episodes"].as_array().is_none_or(Vec::is_empty) {
                return Err("所选季暂无可用分集资料，请选择其他季或稍后重试".into());
            }
            let plan = Plan {
                dir,
                input,
                files,
                main,
                season,
                external,
                created: Instant::now(),
                review: None,
            };
            let token = remember(plan.clone())?;
            Ok(response(&plan, &token))
        }
        "review" => {
            let token = store::text(&input, "token").to_string();
            let plan = plans()
                .lock()
                .map_err(|_| "匹配预览暂不可用")?
                .get(&token)
                .filter(|plan| plan.dir == dir && plan.created.elapsed() < Duration::from_secs(900))
                .cloned()
                .ok_or("预览已过期，请重新选择季")?;
            let read_dir = dir.clone();
            let scope = input.clone();
            let review = tauri::async_runtime::spawn_blocking(move || {
                records::review(
                    &store::open(&read_dir)?,
                    &plan,
                    &scope["mappings"],
                    &scope["recordTargets"],
                )
            })
            .await
            .map_err(|_| "预览更正影响失败")??;
            let output = review.output.clone();
            let mut plans = plans().lock().map_err(|_| "匹配预览暂不可用")?;
            let current = plans
                .get_mut(&token)
                .filter(|plan| plan.dir == dir)
                .ok_or("预览已关闭，请重新选择季")?;
            current.review = Some(review);
            Ok(output)
        }
        "apply" => {
            if playback::is_active(&app) {
                return Err("请先停止影视播放，再确认分集关联".into());
            }
            let plan = take_plan(&dir, store::text(&input, "token"))?;
            if plan.review.as_ref().is_none_or(|review| {
                review.output["reviewId"] != input["reviewId"]
                    || input["reviewId"].as_str().is_none()
            }) {
                return Err("更正预览已变化，请重新预览更正影响".into());
            }
            let result = tauri::async_runtime::spawn_blocking(move || {
                let _operation = operation;
                apply(&mut store::open(&dir)?, &plan, &input["resolutions"])
            })
            .await
            .map_err(|_| "保存分集关联失败，请重新预览")??;
            let _ = app.emit("media-changed", ());
            Ok(result)
        }
        "discard" => {
            let token = store::text(&input, "token");
            let mut plans = plans().lock().map_err(|_| "匹配预览暂不可用")?;
            if plans.get(token).is_some_and(|plan| plan.dir == dir) {
                plans.remove(token);
            }
            Ok(json!({"ok":true}))
        }
        _ => Err("单季匹配操作无效".into()),
    }
}
