use super::{candidate, details_typed as details, model, parse_id, search_candidates, store};
use serde_json::{json, Value};
use std::{collections::HashSet, path::Path};

pub(super) struct Hints {
    pub names: Vec<String>,
    pub year: Option<u16>,
    pub season: Option<u32>,
    pub imdb_id: String,
    pub kind: String,
}

const NO_MATCH: &str = "未找到作品名称、年份和季信息一致的可靠资料";
const AMBIGUOUS: &str = "存在多个合理候选，暂未识别；可稍后重试或搜索选择";
pub(super) fn needs_details(error: &str) -> bool {
    matches!(error, NO_MATCH | AMBIGUOUS)
}

fn subject_name(name: &str) -> model::ParsedName {
    let mut parsed = model::parse_name(name);
    // 文件名解析器已有中文与阿拉伯数字规则，额外接受明确的 Season 罗马数字。
    let lower = name.to_ascii_lowercase();
    if parsed.season.is_none() {
        if let Some(index) = lower.rfind("season ") {
            let roman = lower[index + 7..].trim();
            let numbers = ["i", "ii", "iii", "iv", "v", "vi", "vii", "viii", "ix", "x"];
            if let Some(number) = numbers.iter().position(|value| *value == roman) {
                parsed.season = Some(number as u32 + 1);
                parsed.title = name[..index]
                    .trim_matches(|c: char| !c.is_alphanumeric())
                    .into();
            }
        }
    }
    parsed
}

pub(super) fn hints(dir: &Path, item: &Value) -> Result<Hints, String> {
    let conn = store::open(dir)?;
    let source = store::douban_source::source(&conn, store::text(item, "id"))?
        .map(|source| source.raw)
        .unwrap_or(Value::Null);
    Ok(source_hints(item, &source))
}

fn source_hints(item: &Value, source: &Value) -> Hints {
    let title = source["title"]
        .as_str()
        .unwrap_or(store::text(item, "title"));
    let mut names = Vec::new();
    let mut season = None;
    for name in title.split(" / ") {
        let parsed = subject_name(name);
        if let Some(number) = parsed.season {
            // 不同别名季号相互矛盾时不丢弃其中一个线索。
            if season.is_some_and(|v| v != number) {
                season = Some(u32::MAX);
            } else {
                season = Some(number);
            }
        }
        if !parsed.title.is_empty() && !names.contains(&parsed.title) {
            names.push(parsed.title);
        }
    }
    if let Some(number) = source["detail"]["seasonNumber"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
    {
        season = Some(if season.is_some_and(|n| n != number) {
            u32::MAX
        } else {
            number
        });
    }
    Hints {
        names,
        year: source["year"]
            .as_str()
            .or(item["year"].as_str())
            .and_then(|v| v.parse().ok()),
        season,
        imdb_id: store::douban_source::imdb_id(source).into(),
        kind: store::text(&source["detail"], "kind").into(),
    }
}

pub(super) fn matching_seasons(raw: &Value, hints: &Hints) -> Vec<Value> {
    let seasons = raw["seasons"].as_array().cloned().unwrap_or_default();
    let mut eligible: Vec<Value> = seasons
        .into_iter()
        .filter(|season| {
            season["season_number"]
                .as_u64()
                .is_some_and(|n| n > 0 && hints.season.is_none_or(|wanted| n == wanted as u64))
                && hints.year.is_none_or(|year| {
                    store::text(season, "air_date").get(..4) == Some(year.to_string().as_str())
                })
        })
        .collect();
    if hints.season.is_none() && eligible.len() > 1 {
        // 同年多季时，只有具体季的名称能进一步消除歧义。
        eligible.retain(|season| {
            hints.names.iter().any(|name| {
                model::normalize_title(name) == model::normalize_title(store::text(season, "name"))
            })
        });
    }
    eligible
}

pub(super) fn title_matches(raw: &Value, names: &[String]) -> bool {
    if work_title_matches(raw, names) {
        return true;
    }
    raw["seasons"].as_array().is_some_and(|seasons| {
        seasons
            .iter()
            .any(|row| exact_name(store::text(row, "name"), names))
    })
}

pub(super) fn exact_name(title: &str, names: &[String]) -> bool {
    let title = model::normalize_title(title);
    !title.is_empty()
        && names
            .iter()
            .any(|name| model::normalize_title(name) == title)
}

pub(super) fn work_title_matches(raw: &Value, names: &[String]) -> bool {
    let mut titles: Vec<&str> = ["title", "name", "original_title", "original_name"]
        .iter()
        .filter_map(|key| raw[key].as_str())
        .collect();
    for key in ["titles", "results"] {
        if let Some(aliases) = raw["alternative_titles"][key].as_array() {
            titles.extend(aliases.iter().filter_map(|row| row["title"].as_str()));
        }
    }
    titles.iter().any(|title| exact_name(title, names))
}

// 不把大小写／标点不同的重复别名计作多份证据，也不按别名搜索命中次数加分。
fn direct_evidence(raw: &Value, names: &[String]) -> usize {
    let titles: HashSet<_> = ["title", "name", "original_title", "original_name"]
        .iter()
        .map(|key| model::normalize_title(store::text(raw, key)))
        .filter(|title| !title.is_empty())
        .collect();
    names
        .iter()
        .map(|name| model::normalize_title(name))
        .filter(|name| titles.contains(name))
        .collect::<HashSet<_>>()
        .len()
}

pub(super) fn season_candidate(raw: &Value, season: &Value) -> Value {
    let mut value = candidate(raw, "tv");
    let number = season["season_number"].as_u64().unwrap();
    value["seriesTitle"] = value["title"].clone();
    value["seasonTitle"] = json!(season["name"].as_str().filter(|s| !s.is_empty()).map(str::to_owned).unwrap_or_else(|| format!("第 {number} 季")));
    value["kind"] = json!("season");
    value["seasonNumber"] = json!(number);
    value["title"] = json!(format!(
        "{} · {}",
        store::text(&value, "title"),
        season["name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("第 {number} 季"))
    ));
    value["year"] = json!(store::text(season, "air_date").get(..4).unwrap_or(""));
    value["overview"] = season["overview"].clone();
    let poster = super::image(&season["poster_path"], "w500");
    if !poster.is_empty() {
        value["posterUrl"] = json!(poster);
    }
    value
}

pub(super) async fn automatic(dir: &Path, item: &Value) -> Result<Value, super::retry::Failure> {
    let hints = hints(dir, item)?;
    if !hints.imdb_id.is_empty() {
        if let Some(value) = super::imdb::automatic(dir, &hints).await? {
            return Ok(value);
        }
    }
    let kind = if hints.season.is_some()
        || hints.kind == "series"
        || matches!(store::text(item, "kind"), "series" | "season")
    {
        "tv"
    } else if item["kind"] == "movie" || hints.kind == "movie" {
        "movie"
    } else {
        "all"
    };
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    for name in &hints.names {
        for row in search_candidates(dir, name, kind).await? {
            let (kind, id) = parse_id(&row)?;
            if !seen.insert((kind.clone(), id.clone())) {
                continue;
            }
            if super::prefilter::movie_year_differs(&row, hints.year) {
                super::diagnostics::candidate(true);
                continue;
            }
            rows.push(row);
        }
    }
    // 有年份时先核对显示名／原名精确的候选；别名搜索结果共同参与消歧。
    // 没有年份时证据不足，仍核对全部官方别名，不能仅凭唯一显示名提前确定。
    let (exact, fallback): (Vec<_>, Vec<_>) = rows.into_iter().partition(|row| {
        hints.year.is_some()
            && ["title", "originalTitle"]
                .iter()
                .any(|key| exact_name(store::text(row, key), &hints.names))
    });
    let mut choices = Vec::new();
    for (index, tier) in [&exact, &fallback].into_iter().enumerate() {
        for row in tier {
            let (kind, id) = parse_id(row)?;
            super::diagnostics::candidate(false);
            let raw = details(dir, &kind, &id).await?;
            if !title_matches(&raw, &hints.names) {
                continue;
            }
            let evidence = direct_evidence(&raw, &hints.names);
            if kind == "tv" {
                if !raw["seasons"].is_array() {
                    return Err("TMDB 季列表不完整".into());
                }
                // 有明确季号时只核对该季；否则必须唯一确定一个正常季。
                for season in matching_seasons(&raw, &hints) {
                    choices.push((season_candidate(&raw, &season), evidence));
                }
            } else {
                let value = candidate(&raw, &kind);
                if hints
                    .year
                    .is_none_or(|year| value["year"] == year.to_string())
                {
                    choices.push((value, evidence));
                }
            }
            if index == 1 && choices.len() > 1 {
                super::diagnostics::ambiguity_stop();
                return Err(super::retry::Failure::unmatched(AMBIGUOUS, true));
            }
        }
        if index == 0 && !choices.is_empty() {
            // 必须核对完精确层；后面的双语候选可能比先遇到的单名称候选更明确。
            let best = choices.iter().map(|(_, score)| *score).max().unwrap();
            choices.retain(|(_, score)| *score == best);
            if choices.len() > 1 {
                super::diagnostics::ambiguity_stop();
                return Err(super::retry::Failure::unmatched(AMBIGUOUS, true));
            }
            for _ in &fallback {
                super::diagnostics::candidate(true);
            }
            return Ok(choices.remove(0).0);
        }
    }
    if choices.len() != 1 {
        return Err(super::retry::Failure::unmatched(
            if choices.is_empty() {
                NO_MATCH
            } else {
                AMBIGUOUS
            },
            true,
        ));
    }
    Ok(choices.remove(0).0)
}

/// 手动搜索直接给出最终的季，仍然只需搜索并选择一项。
pub(super) async fn search_targets(
    dir: &Path,
    item_id: &str,
    query: &str,
    kind: &str,
) -> Result<Vec<Value>, String> {
    if !item_id.is_empty() {
        store::item(&store::open(dir)?, item_id)?;
    }
    let parsed = subject_name(query);
    let rows = search_candidates(
        dir,
        &parsed.title,
        if parsed.season.is_some() { "tv" } else { kind },
    )
    .await?;
    let mut result = Vec::new();
    for row in rows {
        let (kind, id) = parse_id(&row)?;
        if kind == "movie" {
            result.push(row);
            continue;
        }
        let raw = details(dir, "tv", &id).await?;
        if let Some(seasons) = raw["seasons"].as_array() {
            for season in seasons {
                if season["season_number"].as_u64().is_some_and(|n| {
                    (n > 0 || parsed.season == Some(0))
                        && parsed.season.is_none_or(|wanted| n == wanted as u64)
                }) {
                    result.push(season_candidate(&raw, season));
                }
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn season_rules_use_season_year_exclude_specials_and_preserve_ambiguity() {
        let raw = json!({"seasons":[{"season_number":0,"air_date":"2024-01-01"},{"season_number":1,"air_date":"2020-01-01"},{"season_number":2,"air_date":"2024-01-01"}]});
        let hints = source_hints(
            &json!({}),
            &json!({"title":"某剧 第二季 / Some Show Season II","year":"2024"}),
        );
        assert_eq!(hints.season, Some(2));
        assert_eq!(hints.names, vec!["某剧", "Some Show"]);
        assert_eq!(matching_seasons(&raw, &hints)[0]["season_number"], 2);
        let hints = source_hints(&json!({"title":"某剧","year":"2024"}), &Value::Null);
        assert_eq!(matching_seasons(&raw, &hints)[0]["season_number"], 2);
        let hints = source_hints(&json!({"title":"某剧"}), &Value::Null);
        assert!(matching_seasons(&raw, &hints).is_empty());
        let one = json!({"seasons":[{"season_number":0},{"season_number":1}]});
        assert_eq!(matching_seasons(&one, &hints).len(), 1);
        let wrong = source_hints(&json!({"title":"某剧 第三季"}), &Value::Null);
        assert!(matching_seasons(&one, &wrong).is_empty());
    }
}
