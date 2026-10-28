//! IMDb 结果逐候选核对；来源类型用于选择范围，不让另一类别否决已确认身份。
mod episodes;
use super::{
    api, candidate, details_typed as details,
    matching::{matching_seasons, season_candidate, Hints},
    store,
};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

fn id(row: &Value, field: &str) -> Result<String, String> {
    row[field]
        .as_u64()
        .filter(|n| *n > 0)
        .map(|n| n.to_string())
        .ok_or_else(|| "IMDb 查询返回无效的作品编号".into())
}

fn unique(
    targets: BTreeMap<(String, u64, u64), Value>,
) -> Result<Option<Value>, super::retry::Failure> {
    if targets.len() != 1 {
        return Err(super::retry::Failure::unmatched(
            if targets.is_empty() {
                "IMDb 作品已找到，但无法确定对应的作品或季，已保留待识别"
            } else {
                "IMDb 对应多个作品或季，已保留待识别"
            },
            false,
        ));
    }
    Ok(targets.into_values().next())
}

pub(super) async fn automatic(
    dir: &Path,
    hints: &Hints,
) -> Result<Option<Value>, super::retry::Failure> {
    if !store::douban_source::valid_imdb_id(&hints.imdb_id) {
        return Err(super::retry::Failure::unmatched(
            "IMDb 编号无效，已保留待识别",
            false,
        ));
    }
    if hints.season == Some(u32::MAX) {
        return Err(super::retry::Failure::unmatched(
            "豆瓣来源季号相互冲突，已保留待识别",
            false,
        ));
    }
    let found = api(
        dir,
        &format!("/find/{}", hints.imdb_id),
        &[("external_source", "imdb_id".into())],
    )
    .await?;
    let mut groups = Vec::new();
    for key in ["movie_results", "tv_results", "tv_episode_results"] {
        groups.push(
            found[key]
                .as_array()
                .ok_or("IMDb 查询响应不完整，已保留待识别")?,
        );
    }
    if groups.iter().all(|rows| rows.is_empty()) {
        return Ok(None);
    }
    let mut targets = BTreeMap::new();
    // 明确电影且找到电影身份时，不再让同编号的剧集／单集副本使整个任务失败。
    if hints.kind != "series" && hints.season.is_none() {
        for row in groups[0] {
            let id = id(row, "id")?;
            let raw = details(dir, "movie", &id).await?;
            targets.insert((format!("movie:{id}"), 0, 0), candidate(&raw, "movie"));
        }
        if hints.kind == "movie" && !targets.is_empty() {
            return unique(targets);
        }
    }
    if hints.kind != "movie" {
        for row in groups[1] {
            let id = id(row, "id")?;
            let raw = details(dir, "tv", &id).await?;
            if !raw["seasons"].is_array() {
                return Err("TMDB 季列表不完整".into());
            }
            for season in series_seasons(&raw, hints) {
                let number = season["season_number"].as_u64().unwrap();
                targets.insert(
                    (format!("tv:{id}"), number, 0),
                    season_candidate(&raw, &season),
                );
            }
        }
    }
    let mut conflict = None;
    for row in groups[2] {
        let show = id(row, "show_id")?;
        let episode_id = id(row, "id")?;
        let season = row["season_number"]
            .as_u64()
            .filter(|n| u32::try_from(*n).is_ok())
            .ok_or("IMDb 单集缺少季号，已保留待识别")?;
        let episode = row["episode_number"]
            .as_u64()
            .filter(|n| *n > 0 && u32::try_from(*n).is_ok())
            .ok_or("IMDb 单集编号无效，已保留待识别")?;
        let raw = details(dir, "tv", &show).await?;
        if !raw["seasons"].is_array() {
            return Err("TMDB 季列表不完整".into());
        }
        if hints.kind == "movie" && hints.season.is_none() {
            // 独立电影／OVA被TMDB归为单集时，只转移到这一集，不标记整季已看。
            let value = episodes::standalone(&raw, season, episode, &episode_id)?;
            targets.insert((format!("tv:{show}"), season, episode), value);
            continue;
        }
        let mapped = episodes::season(&raw, hints, season);
        if mapped.is_empty() {
            conflict = Some("IMDb 已定位剧集，但豆瓣分季与 TMDB 季范围冲突，已保留待识别");
        }
        for matched in mapped {
            targets.insert(
                (format!("tv:{show}"), season, 0),
                season_candidate(&raw, &matched),
            );
        }
    }
    if targets.is_empty() {
        if let Some(error) = conflict {
            return Err(super::retry::Failure::unmatched(error, false));
        }
    }
    unique(targets)
}

fn series_seasons(raw: &Value, hints: &Hints) -> Vec<Value> {
    let strict = matching_seasons(raw, hints);
    if !strict.is_empty() || hints.year.is_none() {
        return strict;
    }
    // IMDb已经确定整剧身份：唯一季年份可纠正来源站与TMDB不同的季编号。
    let by_year = Hints {
        names: hints.names.clone(),
        year: hints.year,
        season: None,
        imdb_id: hints.imdb_id.clone(),
        kind: hints.kind.clone(),
    };
    matching_seasons(raw, &by_year)
}
