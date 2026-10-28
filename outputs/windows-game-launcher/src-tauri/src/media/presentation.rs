//! 可重新获取的 TMDB 展示资料，独立缓存，不进入业务数据库或备份。
use super::{metadata, store, State};
use serde_json::{json, Value};
use std::{collections::HashSet, path::Path, time::Duration};
use tauri::Manager;

pub(super) fn image(value: &Value, size: &str) -> String {
    value
        .as_str()
        .filter(|path| {
            path.starts_with('/')
                && !path.contains("..")
                && !path.contains(['?', '#', '\\', '\r', '\n'])
        })
        .map(|path| format!("https://image.tmdb.org/t/p/{size}{path}"))
        .unwrap_or_default()
}

pub(super) fn normalize(raw: &Value, kind: &str) -> Value {
    let mut logos = raw["images"]["logos"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    logos.retain(|logo| {
        store::text(logo, "file_path").ends_with(".png")
            && logo["aspect_ratio"].as_f64().unwrap_or(0.0) >= 1.2
    });
    logos.sort_by(|a, b| {
        let rank = |value: &Value| match value["iso_639_1"].as_str() {
            Some("zh") => 0,
            Some("en") => 1,
            None => 2,
            _ => 3,
        };
        let fit =
            |value: &Value| !(1.4..=3.2).contains(&value["aspect_ratio"].as_f64().unwrap_or(0.0));
        rank(a)
            .cmp(&rank(b))
            .then_with(|| fit(a).cmp(&fit(b)))
            .then_with(|| {
                b["vote_count"]
                    .as_i64()
                    .unwrap_or(0)
                    .cmp(&a["vote_count"].as_i64().unwrap_or(0))
            })
    });
    let credits = if matches!(kind, "tv" | "season") {
        &raw["aggregate_credits"]
    } else {
        &raw["credits"]
    };
    let mut people = Vec::new();
    let mut seen = HashSet::new();
    for person in credits["cast"].as_array().into_iter().flatten().take(12) {
        let Some(id) = person["id"].as_u64() else {
            continue;
        };
        if !seen.insert(id) || store::text(person, "name").is_empty() {
            continue;
        }
        let character = person["character"]
            .as_str()
            .or_else(|| person["roles"].as_array()?.first()?["character"].as_str())
            .unwrap_or("");
        people.push(json!({
            "id":id,
            "name":person["name"],
            "role":character,
            "department":"cast",
            "profileUrl":image(&person["profile_path"],"w185"),
        }));
    }
    let jobs = [
        ("Director", "导演"),
        ("Screenplay", "编剧"),
        ("Writer", "编剧"),
        ("Executive Producer", "执行制片"),
    ];
    for (job, label) in jobs {
        for person in credits["crew"].as_array().into_iter().flatten() {
            let Some(id) = person["id"].as_u64() else {
                continue;
            };
            let matches = person["job"] == job
                || person["jobs"]
                    .as_array()
                    .is_some_and(|list| list.iter().any(|value| value["job"] == job));
            if !matches || seen.contains(&id) || store::text(person, "name").is_empty() {
                continue;
            }
            let crew_count = people
                .iter()
                .filter(|value| value["department"] == "crew")
                .count();
            if crew_count >= 4 {
                break;
            }
            seen.insert(id);
            people.push(json!({
                "id":id,
                "name":person["name"],
                "role":label,
                "department":"crew",
                "profileUrl":image(&person["profile_path"],"w185"),
            }));
            break; // 每类主创保留一位，避免多季导演名单占满整条横排。
        }
    }
    json!({
        "logoUrl":logos.first().map(|logo|image(&logo["file_path"],"w500")).unwrap_or_default(),
        "people":people,
    })
}

fn identity(item: &Value) -> Option<(String, String)> {
    let external = item["externalIds"].as_array()?.iter().find(|value| {
        value["provider"] == "tmdb"
            && matches!(
                value["providerType"].as_str(),
                Some("movie" | "tv" | "season")
            )
    })?;
    let kind = external["providerType"].as_str()?;
    let id = external["externalId"].as_str()?;
    super::people::path(kind, id)
        .is_ok()
        .then(|| (kind.into(), id.into()))
}

fn read(path: &Path) -> Option<Value> {
    if path.metadata().ok()?.len() > 256 * 1024 {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

pub(super) fn cached_for(app: &tauri::AppHandle, item: &Value) -> Option<Value> {
    let (kind, id) = identity(item)?;
    read(
        &app.path()
            .app_cache_dir()
            .ok()?
            .join("media-metadata")
            .join(format!("v1-{kind}-{}.json", id.replace(':', "-"))),
    )
}

pub async fn get(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let _operation = super::operation(&app).await?;
    let dir = app.state::<State>().dir.clone();
    let id = store::text(&input, "itemId").to_owned();
    let item = tauri::async_runtime::spawn_blocking(move || store::item(&store::open(&dir)?, &id))
        .await
        .map_err(|_| "读取演职员资料失败")??;
    let Some((kind, id)) = identity(&item) else {
        return Ok(json!({"logoUrl":"","people":[]}));
    };
    let root = app
        .path()
        .app_cache_dir()
        .map_err(|_| "无法访问资料缓存")?
        .join("media-metadata");
    let file = root.join(format!("v1-{kind}-{}.json", id.replace(':', "-")));
    let cached = read(&file);
    if input["refresh"] != true
        && file
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|time| time.elapsed().ok())
            .is_some_and(|age| age < Duration::from_secs(7 * 86400))
    {
        if let Some(value) = cached.clone() {
            return Ok(value);
        }
    }
    let dir = app.state::<State>().dir.clone();
    let raw = metadata::api(
        &dir,
        &super::people::path(&kind, &id)?,
        &[
            (
                "append_to_response",
                if matches!(kind.as_str(), "tv" | "season") {
                    "images,aggregate_credits"
                } else {
                    "images,credits"
                }
                .into(),
            ),
            ("include_image_language", "zh,en,null".into()),
        ],
    )
    .await;
    let value = match raw {
        Ok(raw) => {
            let credits = if matches!(kind.as_str(), "tv" | "season") {
                &raw["aggregate_credits"]
            } else {
                &raw["credits"]
            };
            if let Ok(conn) = store::open(&app.state::<State>().dir) {
                let _ = super::people::save(&conn, &kind, &id, credits);
            }
            normalize(&raw, &kind)
        }
        Err(error) => return cached.ok_or(error),
    };
    // 缓存写入失败不阻断观看；临时文件原子替换，避免读到半份 JSON。
    if std::fs::create_dir_all(&root).is_ok() {
        let temporary = root.join(format!("{}.part", uuid::Uuid::new_v4()));
        if std::fs::write(&temporary, serde_json::to_vec(&value).unwrap_or_default()).is_ok() {
            if std::fs::rename(&temporary, &file).is_err() {
                let _ = std::fs::remove_file(&temporary);
            }
        }
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logo_language_and_aggregate_roles_are_preserved() {
        let value = normalize(
            &json!({
                "images":{ "logos":[{ "file_path":"/en.png", "iso_639_1":"en", "aspect_ratio":3.0, "vote_count":20} , { "file_path":"/zh.png", "iso_639_1":"zh", "aspect_ratio":2.0} , { "file_path":"/unsafe.svg", "iso_639_1":"zh", "aspect_ratio":3.0} ]},
                "aggregate_credits":{ "cast":[{ "id":1, "name":"演员", "roles":[{ "character":"角色"} ], "profile_path":"/person.jpg"} ], "crew":[{ "id":2, "name":"导演", "jobs":[{ "job":"Director"} ]} ]},
            }),
            "tv",
        );
        assert!(value["logoUrl"].as_str().unwrap().ends_with("/zh.png"));
        assert_eq!(value["people"][0]["role"], "角色");
        assert_eq!(value["people"][1]["role"], "导演");
    }
    #[test]
    fn movie_people_are_deduplicated_and_paths_cannot_escape() {
        let value = normalize(
            &json!({
                "credits":{"cast":[{"id":1,"name":"演员","character":"角色","profile_path":"/../secret"},{"id":1,"name":"重复"}],"crew":[{"id":1,"name":"演员","job":"Director"}]},
            }),
            "movie",
        );
        assert_eq!(value["people"].as_array().unwrap().len(), 1);
        assert_eq!(value["people"][0]["profileUrl"], "");
        assert_eq!(value["logoUrl"], "");
        assert!(identity(&json!({"externalIds":[{"provider":"tmdb","providerType":"tv","externalId":"../../file"}]})).is_none());
    }
}
