//! 更正季身份前检查整个旧子树，防止保留下来的集目被跨作品搬运。
use super::store;
use rusqlite::Connection;
use serde_json::Value;

pub(super) fn validate_children(
    conn: &Connection,
    source: &str,
    kind: &str,
    external: &str,
) -> Result<(), String> {
    let prefix = format!("{external}:");
    let mut stmt = conn.prepare("WITH RECURSIVE tree(id) AS(SELECT id FROM media_items WHERE parent_id=?1 UNION SELECT m.id FROM media_items m JOIN tree t ON m.parent_id=t.id) SELECT m.kind,e.provider_type,e.external_id FROM tree t JOIN media_items m ON m.id=t.id LEFT JOIN external_ids e ON e.item_id=m.id AND e.provider='tmdb'").map_err(|e|e.to_string())?;
    let rows = stmt
        .query_map([source], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (child_kind, provider_type, id) = row.map_err(|e| e.to_string())?;
        let valid = match kind {
            "tv" => {
                matches!(child_kind.as_str(), "season" | "episode")
                    && provider_type.as_deref() == Some(child_kind.as_str())
            }
            "season" => child_kind == "episode" && provider_type.as_deref() == Some("episode"),
            _ => false,
        } && id.is_some_and(|id| id.starts_with(&prefix));
        if !valid {
            return Err(
                "当前条目保留的分集不属于所选作品或季，不能直接合并；请先分离错误分集".into(),
            );
        }
    }
    Ok(())
}

pub(super) fn validate_merge_children(
    conn: &Connection,
    source: &str,
    target: &Value,
) -> Result<(), String> {
    let has_children: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM media_items WHERE parent_id=?1)",
            [source],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !has_children {
        return Ok(());
    }
    let tmdb: Vec<_> = target["externalIds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|id| id["provider"] == "tmdb")
        .collect();
    if tmdb.len() != 1 {
        return Err("原条目含分集，目标缺少唯一的 TMDB 身份，不能直接合并".into());
    }
    validate_children(
        conn,
        source,
        store::text(tmdb[0], "providerType"),
        store::text(tmdb[0], "externalId"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use serde_json::json;
    #[test]
    fn old_episodes_only_follow_their_exact_series_and_season() {
        let dir = tempfile::tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        let season = store::new_item(&conn, &json!({"kind":"season","title":"第2季"})).unwrap();
        let child=store::upsert_provider_item(&conn,"tmdb","episode","100:2:1",&json!({"kind":"episode","title":"第一集","parentId":season,"seasonNumber":2,"episodeNumber":1}),false).unwrap();
        assert!(validate_children(&conn, &season, "season", "100:2").is_ok());
        for (kind, id) in [("season", "200:2"), ("season", "100:1"), ("movie", "100")] {
            assert!(validate_children(&conn, &season, kind, id).is_err());
        }
        assert_eq!(store::item(&conn, &child).unwrap()["parentId"], season);
        conn.execute(
            "INSERT INTO external_ids VALUES('tmdb','episode','200:2:1',?1)",
            params![child],
        )
        .unwrap();
        assert!(validate_children(&conn, &season, "season", "100:2").is_err());
    }
}
