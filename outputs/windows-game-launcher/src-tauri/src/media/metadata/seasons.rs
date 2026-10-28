use super::{image, store};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::HashSet;

pub(super) fn episodes(raw: &Value, season: u64) -> Result<Vec<Value>, String> {
    let rows = raw["episodes"]
        .as_array()
        .ok_or("TMDB 季资料缺少分集列表，已保留原资料")?;
    let mut seen = HashSet::new();
    rows.iter().map(|row| {
        let number = row["episode_number"].as_u64().filter(|n| *n > 0)
            .ok_or("TMDB 分集编号无效，已保留原资料")?;
        if !seen.insert(number) || row["season_number"].as_u64().is_some_and(|n| n != season) {
            return Err("TMDB 分集列表与所选季不一致，已保留原资料".into());
        }
        Ok(json!({
            "title":row["name"].as_str().filter(|v| !v.is_empty()).map(str::to_owned).unwrap_or_else(|| format!("第 {number} 集")),
            "kind":"episode","seasonNumber":season,"episodeNumber":number,
            "overview":row["overview"],"posterUrl":image(&row["still_path"],"w500"),
            "year":store::text(row,"air_date").get(..4).unwrap_or(""),"airDate":row["air_date"],
        }))
    }).collect()
}

pub(super) fn save_episodes(
    conn: &Connection,
    season_id: &str,
    external_id: &str,
    rows: &[Value],
) -> Result<Vec<String>, String> {
    rows.iter()
        .map(|row| {
            let mut row = row.clone();
            row["parentId"] = json!(season_id);
            store::upsert_provider_item(
                conn,
                "tmdb",
                "episode",
                &format!("{external_id}:{}", row["episodeNumber"].as_u64().unwrap()),
                &row,
                false,
            )
        })
        .collect()
}
