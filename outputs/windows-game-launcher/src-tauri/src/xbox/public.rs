use super::SCHEMA;
use crate::{achievement_platform, db, lock_db, model::AchievementDefinition, AppState};
use serde::Deserialize;
use std::collections::HashSet;
use tauri::{Emitter, Manager};

const WELL_SOURCE: &str = "https://www.exophase.com/game/well-dweller-xbox/achievements/";
#[derive(Deserialize)]
struct Entry {
    id: String,
    name: String,
    description: String,
    icon: String,
    hidden: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    title_id: String,
    store_id: String,
    source: String,
    retrieved_at: String,
    items: Vec<Entry>,
}
pub(crate) fn default_source(title_id: &str) -> &'static str {
    if title_id == "64439fe5" {
        WELL_SOURCE
    } else {
        ""
    }
}

pub(crate) fn validate_url(url: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(url).map_err(|_| "公开成就地址无效")?;
    let parts = url.path().trim_matches('/').split('/').collect::<Vec<_>>();
    if url.scheme() != "https"
        || url.host_str() != Some("www.exophase.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || parts.len() != 3
        || parts[0] != "game"
        || parts[2] != "achievements"
        || !parts[1].ends_with("-xbox")
        || !parts[1]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("请填写 Exophase 的 Xbox 成就列表地址：https://www.exophase.com/game/游戏名-xbox/achievements/".into());
    }
    Ok(())
}
fn selector(css: &str) -> scraper::Selector {
    scraper::Selector::parse(css).expect("固定 CSS 选择器有效")
}
fn parse_html(
    html: &str,
    store_id: &str,
    namespace: &str,
) -> Result<Vec<AchievementDefinition>, String> {
    if store_id.is_empty() {
        return Err("缺少 Microsoft Store ID，不能确认公开页面与本地游戏一致".into());
    }
    let page = scraper::Html::parse_document(html);
    if !page
        .select(&selector("[data-environment='xbox']"))
        .any(|_| true)
    {
        return Err("公开页面未返回 Xbox 资料，可能需要浏览器验证；已保留缓存".into());
    }
    let store_matches = page.select(&selector("a[href]")).any(|a| {
        a.value()
            .attr("href")
            .and_then(|href| reqwest::Url::parse(href).ok())
            .is_some_and(|url| {
                url.scheme() == "https"
                    && url.host_str() == Some("www.microsoft.com")
                    && url
                        .path()
                        .trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .is_some_and(|id| id.eq_ignore_ascii_case(store_id))
            })
    });
    if !store_matches {
        return Err("公开页面的 Microsoft Store ID 与本地游戏不一致，已拒绝合并".into());
    }
    let title = selector(".award-title a");
    let description = selector(".award-description");
    let image = selector("img.award-image");
    let mut ids = HashSet::new();
    let mut items = Vec::new();
    for row in page.select(&selector("li.award[data-award-id]")) {
        let id = row.value().attr("data-award-id").unwrap_or("");
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) || !ids.insert(id.to_owned()) {
            return Err("公开成就缺少唯一的平台编号，已拒绝按顺序匹配".into());
        }
        let name = row
            .select(&title)
            .next()
            .map(|n| n.text().collect::<String>().trim().to_owned())
            .filter(|s| !s.is_empty())
            .ok_or("公开成就缺少名称")?;
        let description = row
            .select(&description)
            .next()
            .map(|n| n.text().collect::<String>().trim().to_owned())
            .unwrap_or_default();
        let icon = row
            .select(&image)
            .next()
            .and_then(|n| n.value().attr("src"))
            .filter(|s| s.starts_with("https://"))
            .unwrap_or("")
            .to_owned();
        items.push(AchievementDefinition {
            api_name: format!("{namespace}:{id}"),
            name,
            description,
            icon,
            hidden: row.value().classes().any(|c| c == "secret"),
        });
    }
    let total = page.select(&selector("li")).find_map(|row| {
        let label = row
            .select(&selector("span"))
            .any(|n| n.text().collect::<String>().trim() == "Total Achievements");
        if label {
            row.select(&selector("strong"))
                .next()
                .and_then(|n| n.text().collect::<String>().trim().parse::<usize>().ok())
        } else {
            None
        }
    });
    if total != Some(items.len()) {
        return Err("公开成就列表不完整，已保留缓存".into());
    }
    if items.is_empty() {
        return Err("公开页面没有可解析的成就列表，已保留缓存".into());
    }
    super::localization::apply(namespace, &mut items)?;
    Ok(items)
}
fn bundled(
    title_id: &str,
    source: &str,
) -> Result<Option<(Vec<AchievementDefinition>, String)>, String> {
    if title_id != "64439fe5" || source != WELL_SOURCE {
        return Ok(None);
    }
    let data: Snapshot = serde_json::from_str(include_str!("../../fixtures/xbox-well-public.json"))
        .map_err(|_| "内置公开资料无效")?;
    if data.title_id != title_id
        || data.source != source
        || data.store_id != "9PLH7R84GJWP"
        || data.items.len() != 41
    {
        return Err("内置公开资料身份不一致".into());
    }
    let mut ids = HashSet::new();
    let mut items = data
        .items
        .into_iter()
        .map(|e| {
            if e.id.is_empty()
                || !e.id.bytes().all(|b| b.is_ascii_digit())
                || !ids.insert(e.id.clone())
            {
                return Err("内置公开资料的平台编号无效".into());
            }
            Ok(AchievementDefinition {
                api_name: format!("xbox:{title_id}:{}", e.id),
                name: e.name,
                description: e.description,
                icon: e.icon,
                hidden: e.hidden,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    super::localization::apply(&format!("xbox:{title_id}"), &mut items)?;
    Ok(Some((items, data.retrieved_at)))
}
async fn fetch(
    source: &str,
    store_id: &str,
    namespace: &str,
) -> Result<Vec<AchievementDefinition>, String> {
    validate_url(source)?;
    let response = crate::network::client(crate::network::Service::Public)?
        .get(source)
        .send()
        .await
        .map_err(|_| "公开成就网络请求失败")?;
    if !response.status().is_success() {
        return Err(format!(
            "公开成就查询失败（HTTP {}），已保留缓存",
            response.status().as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|n| n > 2 * 1024 * 1024)
    {
        return Err("公开页面超过读取限制".into());
    }
    let mut response = response;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "公开页面读取失败")? {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err("公开页面超过读取限制".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let html = String::from_utf8(bytes).map_err(|_| "公开页面编码无效")?;
    parse_html(&html, store_id, namespace)
}

pub(crate) async fn sync(app: &tauri::AppHandle, game_id: &str) -> Result<usize, String> {
    let state = app.state::<AppState>();
    let (game, profile) = {
        let conn = lock_db(&state)?;
        let game = db::game(&conn, game_id)?.ok_or("游戏不存在")?;
        let p = achievement_platform::for_game(&conn, &game)?;
        (game, p)
    };
    if profile.platform != "xbox" {
        return Err("此游戏未选择 Xbox 成就".into());
    }
    let source = &profile.public_source;
    if source.is_empty() {
        return Err("请在编辑游戏中填写对应的 Exophase Xbox 成就列表地址；无需登录".into());
    }
    validate_url(source)?;
    let namespace = format!("xbox:{}", profile.title_id);
    let fallback = bundled(&profile.title_id, source)?;
    // 现有完整缓存也补全中文，网络离线时立即生效，不更改解锁记录。
    if game.schema_source == SCHEMA {
        let mut conn = lock_db(&state)?;
        let mut cached = db::achievements(&conn, game_id)?
            .into_iter()
            .map(|a| AchievementDefinition {
                api_name: a.api_name,
                name: a.name,
                description: a.description,
                icon: a.icon,
                hidden: a.hidden,
            })
            .collect::<Vec<_>>();
        if super::localization::apply(&namespace, &mut cached)? {
            db::save_definitions(&mut conn, game_id, &namespace, &cached, SCHEMA)?;
            drop(conn);
            let _ = app.emit("library-changed", ());
        }
    }

    // 先提供已核对平台 ID 的离线资料，网络刷新失败时保持可用。
    if let Some((items, date)) = &fallback {
        if game.schema_source != SCHEMA {
            let mut conn = lock_db(&state)?;
            let current = db::game(&conn, game_id)?.ok_or("游戏已移除")?;
            if achievement_platform::namespace(&conn, &current)? != namespace
                || current.exe_path != game.exe_path
                || achievement_platform::for_game(&conn, &current)?.public_source != *source
            {
                return Err("成就平台已变化".into());
            }
            db::save_definitions(&mut conn, game_id, &namespace, items, SCHEMA)?;
            db::set_setting(
                &conn,
                &format!("achievement_public_source:{game_id}"),
                &format!("Exophase · 内置快照 {date} · {source}"),
            )?;
            drop(conn);
            let _ = app.emit("library-changed", ());
        }
    }
    let items = match fetch(source, &profile.store_id, &namespace).await {
        Ok(items) => items,
        Err(error) => {
            let conn = lock_db(&state)?;
            db::set_setting(
                &conn,
                &format!("achievement_definition_error:{game_id}"),
                &error,
            )?;
            let count = db::achievements(&conn, game_id)?.len();
            if count > 0 && (fallback.is_some() || game.schema_source == SCHEMA) {
                return Ok(count);
            }
            return Err(error);
        }
    };
    let mut conn = lock_db(&state)?;
    let current = db::game(&conn, game_id)?.ok_or("游戏已移除")?;
    let current_profile = achievement_platform::for_game(&conn, &current)?;
    if achievement_platform::namespace(&conn, &current)? != namespace
        || current_profile.public_source != *source
        || current.exe_path != game.exe_path
    {
        return Err("成就关联已变更，已丢弃旧资料".into());
    }
    db::save_definitions(&mut conn, game_id, &namespace, &items, SCHEMA)?;
    db::set_setting(
        &conn,
        &format!("achievement_definition_error:{game_id}"),
        "",
    )?;
    db::set_setting(
        &conn,
        &format!("achievement_public_source:{game_id}"),
        &format!(
            "Exophase · 更新于 {} · {source}",
            chrono::Utc::now().to_rfc3339()
        ),
    )?;
    drop(conn);
    let _ = app.emit("library-changed", ());
    Ok(items.len())
}

#[tauri::command]
pub(crate) async fn xbox_refresh_definitions(app: tauri::AppHandle) -> Result<String, String> {
    let state = app.state::<AppState>();
    let ids = {
        let conn = lock_db(&state)?;
        db::games(&conn)?
            .iter()
            .filter_map(|g| {
                achievement_platform::for_game(&conn, g)
                    .ok()
                    .filter(|p| p.platform == "xbox")
                    .map(|_| g.id.clone())
            })
            .collect::<Vec<_>>()
    };
    let mut total = 0;
    let mut errors = Vec::new();
    for id in ids {
        match sync(&app, &id).await {
            Ok(n) => total += n,
            Err(e) => {
                if let Ok(conn) = lock_db(&state) {
                    let _ =
                        db::set_setting(&conn, &format!("achievement_definition_error:{id}"), &e);
                }
                errors.push(e)
            }
        }
    }
    let _ = app.emit("library-changed", ());
    if errors.is_empty() {
        Ok(format!("已载入 {total} 项 Xbox 公开成就资料，无需登录"))
    } else {
        Err(format!("已载入 {total} 项；{}", errors.join("；")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uses_platform_ids_not_web_ids_or_list_order() {
        let html = r#"<div data-environment="xbox"></div><li><strong>1</strong><span>Total Achievements</span></li><a href="https://www.microsoft.com/store/apps/STORE123">Store</a><li class="award" data-award-id="4" data-master="7746928"><div class="award-title"><a>Warming Up</a></div><div class="award-description">Kill 100 enemies</div></li>"#;
        let items = parse_html(html, "STORE123", "xbox:64439fe5").unwrap();
        assert_eq!(items[0].api_name, "xbox:64439fe5:4");
        assert!(parse_html(html, "OTHER", "xbox:64439fe5").is_err());
        assert!(parse_html(
            &html.replace("data-award-id=\"4\"", "data-award-id=\"\""),
            "STORE123",
            "xbox:64439fe5"
        )
        .is_err());
        let (items, _) = bundled("64439fe5", WELL_SOURCE).unwrap().unwrap();
        assert_eq!(items.len(), 41);
        assert!(items.iter().all(|i| i.name.chars().any(|c| !c.is_ascii())
            && i.description.chars().any(|c| !c.is_ascii())));
        assert_eq!(
            items
                .iter()
                .find(|i| i.api_name.ends_with(":4"))
                .unwrap()
                .name,
            "热身"
        );
        assert!(bundled("480", WELL_SOURCE).unwrap().is_none());
    }
    #[test]
    fn public_definitions_replace_fallbacks_and_preserve_local_events_offline() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let file = root.join("games.sqlite");
        let mut conn = db::open(&file).unwrap();
        let game = crate::model::Game {
            id: "local-xbox".into(),
            source: "local".into(),
            appid: "3699590".into(),
            title: "Well Dweller".into(),
            metadata_json: "{}".into(),
            ..Default::default()
        };
        db::upsert_game(&conn, &game).unwrap();
        db::set_setting(
            &conn,
            "achievement_platform:local-xbox",
            &serde_json::to_string(&achievement_platform::Profile {
                platform: "xbox".into(),
                title_id: "64439fe5".into(),
                ..Default::default()
            })
            .unwrap(),
        )
        .unwrap();
        db::ensure_definition(&conn, "xbox:64439fe5", "xbox:64439fe5:4").unwrap();
        db::add_unlock(
            &conn,
            &game.id,
            "xbox:64439fe5:4",
            "xbox-local",
            "2026-10-01T02:21:41Z",
            "interface-events.log",
        )
        .unwrap();
        db::save_definitions(
            &mut conn,
            &game.id,
            "xbox:64439fe5",
            &bundled("64439fe5", WELL_SOURCE).unwrap().unwrap().0,
            SCHEMA,
        )
        .unwrap();
        drop(conn);
        let offline = db::open(&file).unwrap();
        let items = db::achievements(&offline, &game.id).unwrap();
        let local = items.iter().find(|i| i.api_name.ends_with(":4")).unwrap();
        assert_eq!(local.name, "热身");
        assert_eq!(local.unlock_source.as_deref(), Some("xbox-local"));
        assert!(items
            .iter()
            .find(|i| i.api_name.ends_with(":3"))
            .unwrap()
            .unlocked_at
            .is_none());
        assert_eq!(
            db::game(&offline, &game.id).unwrap().unwrap().schema_source,
            SCHEMA
        );
        drop(offline);
        std::fs::remove_dir_all(root).unwrap();
    }
}
