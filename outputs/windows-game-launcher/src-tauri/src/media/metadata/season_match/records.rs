//! 更正身份时迁移业务记录；不搬运旧 TMDB 身份或旧分集树。
use super::*;
use rusqlite::OptionalExtension;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub(super) struct Review {
    pub mappings: Value,
    pub choices: Value,
    pub baseline: Value,
    pub output: Value,
    migrations: Vec<(String, String)>,
    states: BTreeMap<String, Value>,
    targets: BTreeMap<String, Target>,
}
#[derive(Clone)]
struct Target {
    kind: String,
    external: String,
    title: String,
    id: Option<String>,
}
fn personal(conn: &Connection, id: &str) -> Result<Value, String> {
    let value=conn.query_row("SELECT watched,favorite,rating,comment,marked_at,last_played,record_source,manual_fields_json FROM personal WHERE item_id=?1",[id],|r|{
        let raw:String=r.get(7)?;
        Ok(json!({"watched":r.get::<_,bool>(0)?,"favorite":r.get::<_,bool>(1)?,"rating":r.get::<_,Option<f64>>(2)?,"comment":r.get::<_,String>(3)?,"markedAt":r.get::<_,String>(4)?,"lastPlayed":r.get::<_,String>(5)?,"recordSource":r.get::<_,String>(6)?,"manualFields":serde_json::from_str::<Value>(&raw).unwrap_or(json!([]))}))
    }).optional().map_err(|e|e.to_string())?;
    Ok(value.unwrap_or(json!({"watched":false,"favorite":false,"rating":null,"comment":"","markedAt":"","lastPlayed":"","recordSource":"","manualFields":[]})))
}
fn strings(conn: &Connection, sql: &str, id: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let result = stmt
        .query_map([id], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string());
    result
}
fn tree(conn: &Connection, id: &str) -> Result<Vec<String>, String> {
    strings(conn,"WITH RECURSIVE t(id)AS(SELECT ?1 UNION SELECT m.id FROM media_items m JOIN t ON m.parent_id=t.id)SELECT id FROM t ORDER BY id",id)
}
fn resources(conn: &Connection, id: &str) -> Result<Vec<String>, String> {
    strings(conn,"WITH RECURSIVE t(id)AS(SELECT ?1 UNION SELECT m.id FROM media_items m JOIN t ON m.parent_id=t.id)SELECT DISTINCT l.resource_id FROM resource_links l JOIN t ON t.id=l.item_id ORDER BY l.resource_id",id)
}
fn state(conn: &Connection, id: &str) -> Result<Value, String> {
    let mut value=conn.query_row("SELECT kind,parent_id,title,independent,episode_number FROM media_items WHERE id=?1",[id],|r|Ok(json!({"kind":r.get::<_,String>(0)?,"parentId":r.get::<_,Option<String>>(1)?,"title":r.get::<_,String>(2)?,"independent":r.get::<_,bool>(3)?,"episodeNumber":r.get::<_,Option<i64>>(4)?}))).map_err(|e|e.to_string())?;
    value["personal"] = personal(conn, id)?;
    value["resources"] = json!(resources(conn, id)?);
    value["collections"] = json!(strings(
        conn,
        "SELECT collection_id FROM collection_items WHERE item_id=?1 ORDER BY collection_id",
        id
    )?);
    value["tags"] = json!(strings(
        conn,
        "SELECT tag_id FROM media_item_tags WHERE item_id=?1 ORDER BY tag_id",
        id
    )?);
    value["sessions"] = json!(strings(
        conn,
        "SELECT id FROM view_sessions WHERE item_id=?1 ORDER BY id",
        id
    )?);
    value["doubanRecords"]=json!(strings(conn,"SELECT account_key||':'||subject_id FROM douban_records WHERE item_id=?1 ORDER BY account_key,subject_id",id)?);
    let mut stmt=conn.prepare("SELECT provider,provider_type,external_id FROM external_ids WHERE item_id=?1 ORDER BY provider,provider_type,external_id").map_err(|e|e.to_string())?;
    value["externalIds"]=json!(stmt.query_map([id],|r|Ok(json!({"provider":r.get::<_,String>(0)?,"providerType":r.get::<_,String>(1)?,"externalId":r.get::<_,String>(2)?}))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?);
    Ok(value)
}
fn has_records(value: &Value) -> bool {
    let p = &value["personal"];
    value["independent"] == true
        || p["watched"] == true
        || p["favorite"] == true
        || !p["rating"].is_null()
        || ["comment", "markedAt", "lastPlayed", "recordSource"]
            .iter()
            .any(|key| !store::text(p, key).is_empty())
        || ["collections", "tags", "sessions", "doubanRecords"]
            .iter()
            .any(|key| value[*key].as_array().is_some_and(|v| !v.is_empty()))
        || p["manualFields"].as_array().is_some_and(|v| !v.is_empty())
        || value["externalIds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["provider"] != "tmdb")
}
fn targets(conn: &Connection, plan: &Plan) -> Result<BTreeMap<String, Target>, String> {
    let season_no = plan.season["seasonNumber"].as_u64().unwrap();
    let mut values = vec![
        (
            "tv".to_string(),
            plan.external.clone(),
            store::text(&plan.main, "title").to_string(),
        ),
        (
            "season".into(),
            format!("{}:{season_no}", plan.external),
            format!(
                "{} · {}",
                store::text(&plan.main, "title"),
                store::text(&plan.season, "title")
            ),
        ),
    ];
    for row in plan.season["episodes"].as_array().unwrap() {
        values.push((
            "episode".into(),
            format!("{}:{season_no}:{}", plan.external, row["episodeNumber"]),
            format!(
                "第 {} 集 · {}",
                row["episodeNumber"],
                store::text(row, "title")
            ),
        ));
    }
    let mut result = BTreeMap::new();
    for (kind, external, title) in values {
        let id=conn.query_row("SELECT item_id FROM external_ids WHERE provider='tmdb' AND provider_type=?1 AND external_id=?2",params![kind,external],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
        result.insert(
            format!("{kind}:{external}"),
            Target {
                kind,
                external,
                title,
                id,
            },
        );
    }
    Ok(result)
}
fn candidates<'a>(review: &'a Review, key: &str, field: &str) -> Vec<(String, String, Value)> {
    let mut result = Vec::new();
    let target = &review.targets[key];
    if let Some(id) = &target.id {
        result.push((
            id.clone(),
            format!("目标：{}", target.title),
            review.states[id]["personal"][field].clone(),
        ));
    }
    for (source, to) in &review.migrations {
        if to == key {
            result.push((
                source.clone(),
                format!("原条目：{}", store::text(&review.states[source], "title")),
                review.states[source]["personal"][field].clone(),
            ));
        }
    }
    let mut values = HashSet::new();
    result
        .into_iter()
        .filter(|(_, _, v)| !v.is_null() && v.as_str() != Some("") && values.insert(v.to_string()))
        .collect()
}

pub(super) fn review(
    conn: &Connection,
    plan: &Plan,
    mappings: &Value,
    choices: &Value,
) -> Result<Review, String> {
    let chosen = validate_mappings(plan, mappings)?;
    let selected: HashSet<String> = chosen.iter().map(|(id, _)| id.clone()).collect();
    let targets = targets(conn, plan)?;
    let target_ids: HashSet<String> = targets.values().filter_map(|v| v.id.clone()).collect();
    let mut sources = BTreeSet::new();
    for file in &plan.files {
        if !selected.contains(store::text(file, "id")) {
            continue;
        }
        for link in file["links"].as_array().unwrap() {
            let mut id = store::text(link, "itemId").to_string();
            let mut seen = HashSet::new();
            while seen.insert(id.clone()) {
                sources.insert(id.clone());
                let parent: Option<String> = conn
                    .query_row(
                        "SELECT parent_id FROM media_items WHERE id=?1",
                        [&id],
                        |r| r.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                let Some(parent) = parent else {
                    break;
                };
                id = parent;
            }
        }
    }
    for id in sources.clone() {
        let kind: String = conn
            .query_row("SELECT kind FROM media_items WHERE id=?1", [&id], |r| {
                r.get(0)
            })
            .map_err(|e| e.to_string())?;
        if kind == "season" && resources(conn, &id)?.iter().all(|id| selected.contains(id)) {
            sources.extend(tree(conn, &id)?);
        }
    }
    let mut states = BTreeMap::new();
    for id in sources.iter().chain(target_ids.iter()) {
        if !states.contains_key(id) {
            states.insert(id.clone(), state(conn, id)?);
        }
    }
    let mut effects = Vec::new();
    let mut migrations = Vec::new();
    let season_key = format!("season:{}:{}", plan.external, plan.season["seasonNumber"]);
    for id in sources {
        let old = &states[&id];
        let kind = store::text(old, "kind");
        let covered = old["resources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| selected.contains(r.as_str().unwrap()));
        let allowed: Vec<_> = targets
            .iter()
            .filter(|(_, v)| match kind {
                "series" => v.kind == "tv",
                "season" => v.kind == "season",
                _ => v.kind == "episode",
            })
            .map(|(key, value)| json!({"key":key,"title":value.title}))
            .collect();
        let mut numbers = BTreeSet::<u64>::new();
        for file in &plan.files {
            if file["links"]
                .as_array()
                .unwrap()
                .iter()
                .any(|link| link["itemId"] == id)
            {
                if let Some((_, ns)) = chosen.iter().find(|(key, _)| file["id"] == *key) {
                    numbers.extend(ns.iter().copied());
                }
            }
        }
        let inferred = match kind {
            "series" => Some(format!("tv:{}", plan.external)),
            "season" => Some(season_key.clone()),
            _ => {
                let old_n = old["episodeNumber"].as_u64();
                let number = if numbers.len() == 1 {
                    numbers.first().copied()
                } else if old_n.is_some_and(|n| numbers.is_empty() || numbers.contains(&n)) {
                    old_n
                } else {
                    None
                };
                number.map(|n| {
                    format!(
                        "episode:{}:{}:{n}",
                        plan.external, plan.season["seasonNumber"]
                    )
                })
            }
        }
        .filter(|key| targets.contains_key(key));
        let mut to = choices[&id]
            .as_str()
            .map(str::to_owned)
            .or(inferred)
            .filter(|key| key != "keep");
        if to
            .as_ref()
            .is_some_and(|key| !allowed.iter().any(|v| v["key"] == *key))
        {
            return Err("记录迁移的目标不在所选季中".into());
        }
        let mut reason = String::new();
        let mut can_choose = covered;
        if !covered {
            to = None;
            reason = "仍有本次范围外或跳过的文件，原条目及其记录保留".into();
        }
        if target_ids.contains(&id) {
            to = None;
            can_choose = false;
            reason = "已是目标作品的条目，仅调整文件关联，记录保留".into();
        }
        if let Some(key) = &to {
            if let Some(target_id) = &targets[key].id {
                if old["externalIds"].as_array().unwrap().iter().any(|left| {
                    left["provider"] != "tmdb"
                        && states[target_id]["externalIds"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|right| {
                                left["provider"] == right["provider"]
                                    && left["providerType"] == right["providerType"]
                                    && left["externalId"] != right["externalId"]
                            })
                }) {
                    to = None;
                    can_choose = false;
                    reason = "原条目与目标的来源编号冲突，保留原条目，请单独核对".into();
                }
            }
        }
        if reason.is_empty() {
            reason = if choices[&id] == "keep" {
                "按你的选择保留原条目及记录"
            } else if to.is_none() && covered && !has_records(old) {
                can_choose = false;
                "无个人记录且没有其他文件，确认后清理旧空条目"
            } else if to.is_some() {
                "记录迁移到目标；确认后清理无其他文件支撑的旧条目"
            } else {
                "记录暂留原条目；可指定记录归属后一起更正"
            }
            .into();
        }
        if let Some(key) = &to {
            migrations.push((id.clone(), key.clone()));
        }
        effects.push(json!({"itemId":id,"title":old["title"],"targetKey":to,"reason":reason,"hasRecords":has_records(old),"options":if can_choose{allowed}else{vec![]}}));
    }
    // 多个旧条目汇入同一目标时，也不能把互相冲突的来源身份一起搬过去。
    let mut provenance = BTreeMap::<(String, String, String), HashSet<String>>::new();
    for (source, to) in &migrations {
        for ext in states[source]["externalIds"].as_array().unwrap() {
            if ext["provider"] != "tmdb" {
                provenance
                    .entry((
                        to.clone(),
                        store::text(ext, "provider").into(),
                        store::text(ext, "providerType").into(),
                    ))
                    .or_default()
                    .insert(store::text(ext, "externalId").into());
            }
        }
    }
    let blocked: HashSet<String> = migrations
        .iter()
        .filter(|(source, to)| {
            states[source]["externalIds"]
                .as_array()
                .unwrap()
                .iter()
                .any(|ext| {
                    ext["provider"] != "tmdb"
                        && provenance
                            .get(&(
                                to.clone(),
                                store::text(ext, "provider").into(),
                                store::text(ext, "providerType").into(),
                            ))
                            .is_some_and(|ids| ids.len() > 1)
                })
        })
        .map(|(source, _)| source.clone())
        .collect();
    migrations.retain(|(source, _)| !blocked.contains(source));
    for effect in &mut effects {
        if blocked.contains(store::text(effect, "itemId")) {
            effect["targetKey"] = Value::Null;
            effect["reason"] =
                json!("多个原条目的来源编号互相冲突，暂保留；请选择保留其中的来源后重新预览");
        }
    }
    let baseline = json!({"states":states,"targetIds":targets.iter().map(|(key,target)|json!([key,target.id])).collect::<Vec<_>>()});
    let mut review = Review {
        mappings: mappings.clone(),
        choices: choices.clone(),
        baseline,
        output: Value::Null,
        migrations,
        states,
        targets,
    };
    let mut conflicts = Vec::new();
    for key in review.targets.keys() {
        if !review.migrations.iter().any(|(_, to)| to == key) {
            continue;
        }
        for (field, label) in [
            ("rating", "评分"),
            ("comment", "短评"),
            ("markedAt", "观看日期"),
        ] {
            let values = candidates(&review, key, field);
            if values.len() > 1 {
                conflicts.push(json!({"key":format!("{key}/{field}"),"title":review.targets[key].title,"label":label,"choices":values.into_iter().map(|(id,title,value)|json!({"id":id,"title":title,"value":value})).collect::<Vec<_>>() }));
            }
        }
    }
    review.output = json!({"reviewId":uuid::Uuid::new_v4().to_string(),"effects":effects,"conflicts":conflicts});
    Ok(review)
}

pub(super) fn transfer(
    conn: &Connection,
    review: &Review,
    resolutions: &Value,
) -> Result<(), String> {
    // 在清空任何来源之前合并所有目标，支持多个来源汇入同一个分集并逐字段处理冲突。
    let mut merged = BTreeMap::new();
    for (key, target) in &review.targets {
        let sources: Vec<_> = review
            .migrations
            .iter()
            .filter(|(_, to)| to == key)
            .map(|(id, _)| id)
            .collect();
        if sources.is_empty() {
            continue;
        }
        let id:String=conn.query_row("SELECT item_id FROM external_ids WHERE provider='tmdb' AND provider_type=?1 AND external_id=?2",params![target.kind,target.external],|r|r.get(0)).map_err(|e|e.to_string())?;
        let mut personal = personal(conn, &id)?;
        let mut manual: BTreeSet<String> = personal["manualFields"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        for source in &sources {
            let p = &review.states[*source]["personal"];
            for field in ["watched", "favorite"] {
                personal[field] = json!(personal[field] == true || p[field] == true);
            }
            if store::text(p, "lastPlayed") > store::text(&personal, "lastPlayed") {
                personal["lastPlayed"] = p["lastPlayed"].clone();
            }
            if store::text(&personal, "recordSource").is_empty() {
                personal["recordSource"] = p["recordSource"].clone();
            }
            manual.extend(
                p["manualFields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            );
        }
        for field in ["rating", "comment", "markedAt"] {
            let values = candidates(review, key, field);
            if values.len() == 1 {
                personal[field] = values[0].2.clone();
            } else if values.len() > 1 {
                let selected = store::text(resolutions, &format!("{key}/{field}"));
                personal[field] = values
                    .iter()
                    .find(|(id, _, _)| id == selected)
                    .ok_or("请先明确选择冲突记录，未保存任何更改")?
                    .2
                    .clone();
            }
        }
        personal["manualFields"] = json!(manual);
        merged.insert(key.clone(), (id, personal));
    }
    for (key, (target, p)) in &merged {
        conn.execute("UPDATE personal SET watched=?2,favorite=?3,rating=?4,comment=?5,marked_at=?6,last_played=?7,record_source=?8,manual_fields_json=?9 WHERE item_id=?1",params![target,p["watched"].as_bool().unwrap(),p["favorite"].as_bool().unwrap(),p["rating"].as_f64(),store::text(p,"comment"),store::text(p,"markedAt"),store::text(p,"lastPlayed"),store::text(p,"recordSource"),p["manualFields"].to_string()]).map_err(|e|e.to_string())?;
        for (source, to) in &review.migrations {
            if to != key {
                continue;
            }
            conn.execute("INSERT OR IGNORE INTO collection_items(collection_id,item_id)SELECT collection_id,?2 FROM collection_items WHERE item_id=?1",params![source,target]).map_err(|e|e.to_string())?;
            conn.execute("INSERT OR IGNORE INTO media_item_tags(item_id,tag_id)SELECT ?2,tag_id FROM media_item_tags WHERE item_id=?1",params![source,target]).map_err(|e|e.to_string())?;
            conn.execute(
                "UPDATE view_sessions SET item_id=?2 WHERE item_id=?1",
                params![source, target],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE douban_records SET item_id=?2 WHERE item_id=?1",
                params![source, target],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE external_ids SET item_id=?2 WHERE item_id=?1 AND provider<>'tmdb'",
                params![source, target],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE media_items SET independent=MAX(independent,?2) WHERE id=?1",
                params![
                    target,
                    review.states[source]["independent"].as_bool().unwrap()
                ],
            )
            .map_err(|e| e.to_string())?;
            for table in ["personal", "collection_items", "media_item_tags"] {
                conn.execute(&format!("DELETE FROM {table} WHERE item_id=?1"), [source])
                    .map_err(|e| e.to_string())?;
            }
            conn.execute("UPDATE media_items SET independent=0 WHERE id=?1", [source])
                .map_err(|e| e.to_string())?;
        }
    }
    // 显式选择保留空的旧条目，也尊重该选择；有剩余资源者无需更改。
    for effect in review.output["effects"].as_array().unwrap() {
        if review.choices[store::text(effect, "itemId")] == "keep" {
            conn.execute(
                "UPDATE media_items SET independent=1 WHERE id=?1",
                [store::text(effect, "itemId")],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
