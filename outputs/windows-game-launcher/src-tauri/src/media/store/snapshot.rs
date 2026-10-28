//! 列表与单条共用快照映射；单条查询只遍历目标及其后代资源。
use super::*;
#[cfg(test)]
mod tests;
const ATTACHED_ALL:&str="WITH RECURSIVE attached(item_id,resource_id)AS(SELECT item_id,resource_id FROM resource_links UNION SELECT m.parent_id,a.resource_id FROM attached a JOIN media_items m ON m.id=a.item_id WHERE m.parent_id IS NOT NULL)";
const ATTACHED_ONE:&str="WITH RECURSIVE children(root_id,id)AS(SELECT id,id FROM media_items WHERE id IN(SELECT value FROM json_each(?1)) UNION SELECT c.root_id,m.id FROM media_items m JOIN children c ON m.parent_id=c.id),attached(item_id,resource_id)AS(SELECT DISTINCT c.root_id,l.resource_id FROM resource_links l JOIN children c ON c.id=l.item_id)";
fn filtered(sql: &str, condition: &str, id: Option<&str>) -> String {
    if id.is_some() {
        format!("{sql} WHERE {}", condition.replace("=?1", " IN(SELECT value FROM json_each(?1))"))
    } else {
        sql.into()
    }
}
pub(super) fn read(conn: &Connection, id: Option<&str>) -> Result<Vec<Value>, String> {
    let selected = id.map(|id| json!([id]).to_string());
    read_selected(conn, selected.as_deref())
}
pub(super) fn read_many(conn: &Connection, ids: &[String]) -> Result<Vec<Value>, String> {
    read_selected(conn, Some(&json!(ids).to_string()))
}
fn read_selected(conn: &Connection, id: Option<&str>) -> Result<Vec<Value>, String> {
    let base="SELECT m.id,m.kind,m.parent_id,m.title,m.year,m.overview,m.poster_url,m.backdrop_url,m.genres_json,m.recognized,m.independent,m.season_number,m.episode_number,m.created_at,m.updated_at,COALESCE(p.watched,0),COALESCE(p.favorite,0),p.rating,COALESCE(p.comment,''),COALESCE(p.marked_at,''),COALESCE(p.last_played,''),COALESCE(p.record_source,'') FROM media_items m LEFT JOIN personal p ON p.item_id=m.id ORDER BY m.title COLLATE NOCASE";
    let sql = if id.is_some() {
        base.replace(" ORDER BY", " WHERE m.id IN(SELECT value FROM json_each(?1)) ORDER BY")
    } else {
        base.into()
    };
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

    let mut values = stmt
        .query_map(rusqlite::params_from_iter(id), |r| {
            let genres: String = r.get(8)?;
            Ok(json!({
                "id":r.get::<_, String>(0)?,
                "kind":r.get::<_, String>(1)?,
                "parentId":r.get::<_, Option<String>>(2)?,
                "title":r.get::<_, String>(3)?,
                "year":r.get::<_, String>(4)?,
                "overview":r.get::<_, String>(5)?,
                "posterUrl":r.get::<_, String>(6)?,
                "backdropUrl":r.get::<_, String>(7)?,
                "genres":serde_json::from_str::<Value>(&genres).unwrap_or(json!([])),
                "recognized":r.get::<_, bool>(9)?,
                "recognitionStatus":if r.get::<_, bool>(9)? { "recognized" } else { "pending" },
                "independent":r.get::<_, bool>(10)?,
                "seasonNumber":r.get::<_, Option<i64>>(11)?,
                "episodeNumber":r.get::<_, Option<i64>>(12)?,
                "createdAt":r.get::<_, String>(13)?,
                "updatedAt":r.get::<_, String>(14)?,
                "watched":r.get::<_, bool>(15)?,
                "favorite":r.get::<_, bool>(16)?,
                "rating":r.get::<_, Option<f64>>(17)?,
                "comment":r.get::<_, String>(18)?,
                "markedAt":r.get::<_, String>(19)?,
                "lastPlayed":r.get::<_, String>(20)?,
                "recordSource":r.get::<_, String>(21)?,
                "externalIds":[],
                "tags":[],
                "collectionIds":[],
                "playable":false,
                "resourceCount":0,
                "positionSeconds":0.0,
                "durationSeconds":0.0,
            }))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    if values.is_empty() {
        return Ok(values);
    }
    let positions: HashMap<String, usize> = values
        .iter()
        .enumerate()
        .map(|(i, v)| (text(v, "id").to_string(), i))
        .collect();
    let mut metadata = conn
        .prepare(&filtered(
            "SELECT item_id,air_date,season_count FROM media_item_metadata",
            "item_id=?1",
            id,
        ))
        .map_err(|e| e.to_string())?;
    for row in metadata
        .query_map(rusqlite::params_from_iter(id), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
    {
        let (id, date, count) = row.map_err(|e| e.to_string())?;
        if let Some(i) = positions.get(&id) {
            values[*i]["airDate"] = json!(date);
            values[*i]["seasonCount"] = json!(count);
        }
    }
    let mut errors = conn
        .prepare(&filtered(
            "SELECT e.item_id,e.error,r.kind FROM media_match_errors e LEFT JOIN media_match_retry r ON r.item_id=e.item_id",
            "e.item_id=?1",
            id,
        ))
        .map_err(|e| e.to_string())?;
    for row in errors
        .query_map(rusqlite::params_from_iter(id), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))
        })
        .map_err(|e| e.to_string())?
    {
        let (id, error, kind) = row.map_err(|e| e.to_string())?;
        if let Some(i) = positions.get(&id) {
            values[*i]["recognitionError"] = json!(error);
            if values[*i]["recognized"] != true {
                values[*i]["recognitionStatus"] = json!(match kind.as_deref() {
                    Some("unmatched") => "unmatched",
                    Some("transient") => "failed",
                    _ => "pending",
                });
            }
        }
    }
    let mut external = conn
        .prepare(&filtered(
            "SELECT item_id,provider,provider_type,external_id FROM external_ids",
            "item_id=?1",
            id,
        ))
        .map_err(|e| e.to_string())?;
    for row in external
        .query_map(rusqlite::params_from_iter(id), |r| {
            Ok((
                r.get::<_, String>(0)?,
                json!({
                    "provider":r.get::<_, String>(1)?,
                    "providerType":r.get::<_, String>(2)?,
                    "externalId":r.get::<_, String>(3)?,
                }),
            ))
        })
        .map_err(|e| e.to_string())?
    {
        let (id, value) = row.map_err(|e| e.to_string())?;
        if let Some(i) = positions.get(&id) {
            values[*i]["externalIds"]
                .as_array_mut()
                .unwrap()
                .push(value);
        }
    }

    douban_source::attach_imdb_ids(conn, &mut values, id)?;
    let mut collected = conn
        .prepare(&filtered(
            "SELECT item_id,collection_id FROM collection_items",
            "item_id=?1",
            id,
        ))
        .map_err(|e| e.to_string())?;
    for row in collected
        .query_map(rusqlite::params_from_iter(id), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?
    {
        let (id, value) = row.map_err(|e| e.to_string())?;
        if let Some(i) = positions.get(&id) {
            values[*i]["collectionIds"]
                .as_array_mut()
                .unwrap()
                .push(json!(value));
        }
    }
    let mut tags=conn.prepare(&filtered("SELECT x.item_id,t.id,t.name FROM media_item_tags x JOIN media_tags t ON t.id=x.tag_id", "x.item_id=?1", id)).map_err(|e|e.to_string())?;
    for row in tags
        .query_map(rusqlite::params_from_iter(id), |r| {
            Ok((
                r.get::<_, String>(0)?,
                json!({"id":r.get::<_,String>(1)?,"name":r.get::<_,String>(2)?}),
            ))
        })
        .map_err(|e| e.to_string())?
    {
        let (id, value) = row.map_err(|e| e.to_string())?;
        if let Some(i) = positions.get(&id) {
            values[*i]["tags"].as_array_mut().unwrap().push(value);
        }
    }
    let attached = if id.is_some() {
        ATTACHED_ONE
    } else {
        ATTACHED_ALL
    };
    let resource_filter = if id.is_some() {
        "WHERE l.resource_id IN(SELECT resource_id FROM attached)"
    } else {
        ""
    };
    let resource_sql = format!("{attached},episode_order(resource_id,season_number,episode_number)AS(SELECT l.resource_id,MIN(COALESCE(m.season_number,p.season_number,2147483647)),MIN(COALESCE(m.episode_number,2147483647)) FROM resource_links l JOIN media_items m ON m.id=l.item_id LEFT JOIN media_items p ON p.id=m.parent_id {resource_filter} GROUP BY l.resource_id) SELECT a.item_id,r.id,r.available,r.missing,r.position_seconds,r.duration_seconds,COALESCE((SELECT MAX(s.started_at) FROM view_sessions s WHERE s.resource_id=r.id),''),r.size,r.modified_at,r.path,r.technical_json FROM attached a JOIN resources r ON r.id=a.resource_id LEFT JOIN episode_order eo ON eo.resource_id=r.id ORDER BY (r.available=1 AND r.missing=0) DESC,7 DESC,eo.season_number,eo.episode_number,r.path COLLATE NOCASE,r.id");
    let mut resources = conn.prepare(&resource_sql).map_err(|e| e.to_string())?;

    let mut chosen = HashSet::new();
    let mut artwork_paths: HashMap<String, String> = HashMap::new();
    for row in resources
        .query_map(rusqlite::params_from_iter(id), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
                r.get::<_, bool>(3)?,
                r.get::<_, f64>(4)?,
                r.get::<_, f64>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, String>(9)?,
                r.get::<_, String>(10)?,
            ))
        })
        .map_err(|e| e.to_string())?
    {
        let (id, resource_id, available, missing, pos, duration, last, size, modified, path, technical) =
            row.map_err(|e| e.to_string())?;
        if let Some(i) = positions.get(&id) {
            if !missing {
                values[*i]["resourceCount"] =
                    json!(values[*i]["resourceCount"].as_u64().unwrap_or(0) + 1);
            }
            if available && !missing {
                values[*i]["playable"] = json!(true);
                if values[*i]["recognized"] == false
                    && crate::media::artwork::frame::supported(Path::new(&path), &serde_json::from_str::<Value>(&technical).unwrap_or_default())
                    && artwork_paths.get(&id).is_none_or(|previous| path < *previous)
                {
                    artwork_paths.insert(id.clone(), path);
                    values[*i]["localArtworkKey"] =
                        json!(crate::media::artwork::frame::key(&resource_id, size, &modified));
                }
            }
            if chosen.insert(id.clone()) {
                values[*i]["playbackCompleted"] = json!(playback_complete(pos, duration));
                values[*i]["positionSeconds"] = json!(pos);
                values[*i]["durationSeconds"] = json!(duration);
                values[*i]["resumeResourceId"] = if available && !missing {
                    json!(resource_id)
                } else {
                    Value::Null
                };
            }
            if last.as_str() > text(&values[*i], "lastPlayed") {
                values[*i]["lastPlayed"] = json!(last);
            }
        }
    }
    let roots = if id.is_some() {
        "SELECT id,id FROM media_items WHERE id IN(SELECT value FROM json_each(?1))"
    } else {
        "SELECT id,id FROM media_items"
    };
    let mut played=conn.prepare(&format!("WITH RECURSIVE descendants(root_id,id)AS({roots} UNION SELECT d.root_id,m.id FROM descendants d JOIN media_items m ON m.parent_id=d.id)SELECT d.root_id,MAX(p.last_played) FROM descendants d JOIN personal p ON p.item_id=d.id GROUP BY d.root_id")).map_err(|e|e.to_string())?;

    for row in played
        .query_map(rusqlite::params_from_iter(id), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?
    {
        let (id, last) = row.map_err(|e| e.to_string())?;
        if let Some(i) = positions.get(&id) {
            if last.as_str() > text(&values[*i], "lastPlayed") {
                values[*i]["lastPlayed"] = json!(last);
            }
        }
    }
    Ok(values)
}
