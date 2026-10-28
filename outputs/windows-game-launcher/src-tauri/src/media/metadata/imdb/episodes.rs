use super::{matching_seasons, season_candidate, Hints};
use serde_json::{json, Value};

pub(super) fn season(raw: &Value, hints: &Hints, number: u64) -> Vec<Value> {
    if number == 0 {
        return Vec::new();
    }
    if hints.season.is_some_and(|n| n as u64 != number) && hints.year.is_none() {
        return Vec::new();
    }
    let explicit = Hints {
        names: hints.names.clone(),
        year: hints.year,
        season: Some(number as u32),
        imdb_id: hints.imdb_id.clone(),
        kind: hints.kind.clone(),
    };
    matching_seasons(raw, &explicit)
}

pub(super) fn standalone(
    raw: &Value,
    season: u64,
    episode: u64,
    episode_id: &str,
) -> Result<Value, super::super::retry::Failure> {
    let summary = raw["seasons"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["season_number"].as_u64() == Some(season))
        })
        .ok_or_else(|| {
            super::super::retry::Failure::unmatched("IMDb 单集所属季不存在，已保留待识别", false)
        })?;
    // 最终selected_data从完整季资料核对集号及IMDb返回的单集ID，复用同一次季请求。
    let mut value = season_candidate(raw, summary);
    value["kind"] = json!("episode");
    value["episodeNumber"] = json!(episode);
    value["episodeTmdbId"] = json!(episode_id);
    Ok(value)
}
