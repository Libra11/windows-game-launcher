//! 本地文件的候选核对：精确名称／官方别名、具体季年份与真实分集共同约束。
use super::{
    api, candidate, details_typed as details, matching, model, parse_id, search_candidates, store,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

struct Matched {
    value: Value,
    evidence: usize,
}

fn year_matches(value: &Value, field: &str, year: Option<u16>) -> bool {
    year.is_none_or(|year| store::text(value, field).get(..4) == Some(year.to_string().as_str()))
}

fn local_seasons(raw: &Value, parsed: &model::ParsedName) -> Vec<Value> {
    raw["seasons"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|season| {
            season["season_number"].as_u64().is_some_and(|number| {
                parsed
                    .season
                    .map(|wanted| number == wanted as u64)
                    .unwrap_or(number > 0)
            }) && (year_matches(season, "air_date", parsed.year)
                || parsed.season.is_some() && year_matches(raw, "first_air_date", parsed.year))
        })
        .cloned()
        .collect()
}

fn direct_name(row: &Value, names: &[String]) -> bool {
    ["title", "originalTitle", "englishTitle"]
        .iter()
        .any(|field| {
            let normalized = model::normalize_title(store::text(row, field));
            !normalized.is_empty()
                && names
                    .iter()
                    .any(|name| model::normalize_title(name) == normalized)
        })
}

async fn inspect(
    dir: &Path,
    rows: Vec<Value>,
    names: &[String],
    parsed: &model::ParsedName,
    seen: &mut HashSet<(String, String)>,
) -> Result<Vec<Matched>, super::retry::Failure> {
    let mut choices = Vec::new();
    let mut inspected = 0;
    // 显示名已经精确的候选先检查；仍会检查同名同年候选，不以排序代替消歧。
    let mut rows = rows;
    rows.sort_by_key(|row| !direct_name(row, names));
    for row in rows {
        let (kind, id) = parse_id(&row)?;
        if !seen.insert((kind.clone(), id.clone())) {
            continue;
        }
        if kind == "tv" && parsed.episodes.is_empty() {
            continue;
        }
        if kind == "movie" && !year_matches(&row, "year", parsed.year) {
            continue;
        }
        if kind == "tv" {
            if let (Some(year), Some(first)) =
                (parsed.year, store::text(&row, "year").parse::<u16>().ok())
            {
                if parsed.season != Some(0)
                    && (first > year
                        || parsed.season == Some(1) && first != year && !direct_name(&row, names))
                {
                    continue;
                }
            }
        }
        // 中英文首屏合并后至多核对40个身份；不因首屏重复或无关年份挤掉明确目标。
        if inspected >= 40 {
            return Err(super::retry::Failure::unmatched(
                "候选过多，请缩小作品名称或手动识别",
                false,
            ));
        }
        inspected += 1;
        super::diagnostics::candidate(false);
        let raw = details(dir, &kind, &id).await?;
        let evidence: Vec<bool> = names
            .iter()
            .map(|name| {
                direct_name(&row, std::slice::from_ref(name))
                    || matching::work_title_matches(&raw, std::slice::from_ref(name))
            })
            .collect();
        let work_matches = evidence.iter().any(|matched| *matched);
        if kind == "movie" {
            if parsed.season.is_none()
                && parsed.episodes.is_empty()
                && work_matches
                && year_matches(&raw, "release_date", parsed.year)
            {
                choices.push(Matched {
                    value: candidate(&raw, "movie"),
                    evidence: evidence.iter().filter(|matched| **matched).count(),
                });
            }
        } else {
            if !raw["seasons"].is_array() {
                return Err("TMDB 季列表不完整".into());
            }
            for season in local_seasons(&raw, parsed) {
                if work_matches || matching::exact_name(store::text(&season, "name"), names) {
                    let count = names
                        .iter()
                        .zip(&evidence)
                        .filter(|(name, work)| {
                            **work
                                || matching::exact_name(
                                    store::text(&season, "name"),
                                    std::slice::from_ref(*name),
                                )
                        })
                        .count();
                    choices.push(Matched {
                        value: matching::season_candidate(&raw, &season),
                        evidence: count,
                    });
                }
            }
        }
    }
    Ok(choices)
}

async fn search_rows(dir: &Path, names: &[String], kind: &str) -> Result<Vec<Value>, String> {
    let mut rows: Vec<Value> = Vec::new();
    for name in names.iter().take(2) {
        rows.extend(search_candidates(dir, name, kind).await?);
        if !name.chars().any(|c| c.is_ascii_alphabetic()) {
            continue;
        }
        let endpoint = if kind == "tv" { "tv" } else { "multi" };
        let raw = api(
            dir,
            &format!("/search/{endpoint}"),
            &[
                ("query", name.clone()),
                ("language", "en-US".into()),
                ("include_adult", "false".into()),
            ],
        )
        .await?;
        for raw in raw["results"]
            .as_array()
            .ok_or("TMDB 搜索响应不完整")?
            .iter()
            .take(20)
        {
            let kind = if endpoint == "tv" {
                "tv"
            } else {
                store::text(raw, "media_type")
            };
            if !matches!(kind, "movie" | "tv") {
                continue;
            }
            let english = candidate(raw, kind);
            if let Some(existing) = rows.iter_mut().find(|row| {
                row["providerType"] == english["providerType"]
                    && row["externalId"] == english["externalId"]
            }) {
                existing["englishTitle"] = english["title"].clone();
            } else {
                rows.push(english);
            }
        }
    }
    Ok(rows)
}

pub(super) async fn automatic(
    dir: &Path,
    path: &Path,
    parsed: &model::ParsedName,
) -> Result<Value, super::retry::Failure> {
    let numbered_title = !parsed.title.is_empty()
        && parsed.title.chars().all(|c| c.is_ascii_digit())
        && (parsed.season.is_some() || !parsed.episodes.is_empty());
    if parsed.title.trim().is_empty()
        || (parsed.year.is_none() && !model::meaningful_title(&parsed.title) && !numbered_title)
    {
        return Err(super::retry::Failure::unmatched(
            "文件没有可靠作品名称，请手动识别",
            false,
        ));
    }
    let names = model::path_titles(path, parsed);
    let television = parsed.season.is_some() || !parsed.episodes.is_empty();
    let kind = if television { "tv" } else { "all" };
    let rows = search_rows(dir, &names, kind).await?;
    let mut seen = HashSet::new();
    let mut choices = inspect(dir, rows, &names, parsed, &mut seen).await?;
    if choices.is_empty() && !television && parsed.year.is_some() {
        // multi 首屏可能被同名电视剧／人物淹没；限定电影年份再搜索一次。
        let query = [
            ("query", parsed.title.clone()),
            ("include_adult", "false".into()),
            ("primary_release_year", parsed.year.unwrap().to_string()),
        ];
        let raw = api(dir, "/search/movie", &query).await?;
        let rows = raw["results"]
            .as_array()
            .ok_or("TMDB 搜索响应不完整")?
            .iter()
            .take(20)
            .map(|row| candidate(row, "movie"))
            .collect();
        choices.extend(inspect(dir, rows, &names, parsed, &mut seen).await?);
    }
    let mut identities: HashMap<(String, String, Option<u64>), Matched> = HashMap::new();
    for choice in choices {
        let key = (
            store::text(&choice.value, "providerType").to_string(),
            store::text(&choice.value, "externalId").to_string(),
            choice.value["seasonNumber"].as_u64(),
        );
        match identities.get_mut(&key) {
            Some(existing) if choice.evidence > existing.evidence => *existing = choice,
            Some(_) => {}
            None => {
                identities.insert(key, choice);
            }
        }
    }
    let mut choices: Vec<_> = identities.into_values().collect();
    let best = choices.iter().map(|row| row.evidence).max().unwrap_or(0);
    // 同一文件中的双语名称是独立证据；只优先同时精确证明两种名称的作品。
    if names.len() > 1 && best > 1 {
        choices.retain(|row| row.evidence == best);
    }
    if choices.len() != 1 {
        return Err(super::retry::Failure::unmatched(
            if choices.is_empty() {
                "未找到名称、年份和季集一致的可靠候选"
            } else {
                "存在多个同名或季候选，请手动识别"
            },
            false,
        ));
    }
    let selected = choices.remove(0).value;
    // selected_data 会再次验证实际 season/episode 列表；这里只返回唯一且精确匹配的作品／季。
    Ok(selected)
}

pub(super) async fn explicit(
    dir: &Path,
    parsed: &model::ParsedName,
    kind: &str,
    id: &str,
) -> Result<Value, super::retry::Failure> {
    let mut value = json!({"providerType":kind,"externalId":id});
    if kind == "tv" && parsed.season.is_none() && !parsed.episodes.is_empty() {
        let raw = details(dir, kind, id).await?;
        if !raw["seasons"].is_array() {
            return Err("TMDB 季列表不完整".into());
        }
        let seasons = local_seasons(&raw, parsed);
        if seasons.len() != 1 {
            return Err(super::retry::Failure::unmatched(
                "编号已确认作品，但无法唯一确定具体季，请手动识别",
                false,
            ));
        }
        value["seasonNumber"] = seasons[0]["season_number"].clone();
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_year_belongs_to_selected_season_and_unknown_seasons_remain_ambiguous() {
        let raw = json!({"first_air_date":"2002-01-01","seasons":[{"season_number":0,"air_date":"2004-01-01"},{"season_number":1,"air_date":"2002-01-01"},{"season_number":3,"air_date":"2004-01-01"}]});
        let selected = model::parse_name("The.Wire.S03E12.2004.mkv");
        assert_eq!(local_seasons(&raw, &selected)[0]["season_number"], 3);
        let unknown = model::parse_name("The.Wire.E01.mkv");
        assert_eq!(local_seasons(&raw, &unknown).len(), 2);
        let dated = model::parse_name("The.Wire.E01.2004.mkv");
        assert_eq!(local_seasons(&raw, &dated)[0]["season_number"], 3);
        let series_year = model::parse_name("The.Wire.S03E12.2002.mkv");
        assert_eq!(local_seasons(&raw, &series_year)[0]["season_number"], 3);
        let no_season = model::parse_name("The.Wire.E01.2002.mkv");
        assert_eq!(local_seasons(&raw, &no_season)[0]["season_number"], 1);
        let later_only = json!({"first_air_date":"2002-01-01","seasons":[{"season_number":3,"air_date":"2004-01-01"}]});
        assert!(local_seasons(&later_only, &no_season).is_empty());
    }
}
