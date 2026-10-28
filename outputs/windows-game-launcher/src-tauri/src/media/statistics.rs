use super::{store, State};
use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use tauri::Manager;

fn day(value: &str) -> Option<NaiveDate> {
    if value.len() == 10 {
        return NaiveDate::parse_from_str(value, "%Y-%m-%d").ok();
    }
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.with_timezone(&Local).date_naive())
}

fn read(conn: &Connection) -> Result<(Vec<Value>, Vec<Value>), String> {
    let mut statement = conn.prepare(
        "SELECT i.id,CASE WHEN i.kind='season' AND parent.title IS NOT NULL THEN parent.title || ' · ' || i.title ELSE i.title END,i.kind,i.genres_json,COALESCE(p.watched,0),p.rating,COALESCE(p.comment,''),
         COALESCE(p.marked_at,''),COALESCE(NULLIF(p.record_source,''),'local'),COALESCE(p.last_played,'')
         FROM media_items i LEFT JOIN personal p ON p.item_id=i.id LEFT JOIN media_items parent ON parent.id=i.parent_id"
    ).map_err(|_| "无法读取影视观影资料")?;
    let rows = statement.query_map([], |row| Ok(json!({
        "itemId":row.get::<_,String>(0)?,
        "title":row.get::<_,String>(1)?,
        "kind":row.get::<_,String>(2)?,
        "genres":serde_json::from_str::<Value>(&row.get::<_,String>(3)?).unwrap_or(json!([])),
        "watched":row.get::<_,i64>(4)?!=0,
        "rating":row.get::<_,Option<f64>>(5)?,
        "comment":row.get::<_,String>(6)?,
        "markedAt":row.get::<_,String>(7)?,
        "source":row.get::<_,String>(8)?,
        "lastPlayed":row.get::<_,String>(9)?,
    }))).map_err(|_| "无法查询影视观影资料")?
        .collect::<Result<Vec<_>,_>>().map_err(|_| "无法解析影视观影资料")?;
    let mut statement = conn.prepare("SELECT v.id,v.item_id,v.resource_id,v.started_at,v.ended_at,v.seconds,v.position_seconds,i.title,i.kind FROM view_sessions v JOIN media_items i ON i.id=v.item_id ORDER BY v.started_at DESC")
        .map_err(|_| "无法读取影视播放记录")?;
    let sessions = statement
        .query_map([], |row| {
            Ok(json!({
                "id":row.get::<_,String>(0)?,
                "itemId":row.get::<_,String>(1)?,
                "resourceId":row.get::<_,String>(2)?,
                "startedAt":row.get::<_,String>(3)?,
                "endedAt":row.get::<_,String>(4)?,
                "seconds":row.get::<_,f64>(5)?,
                "positionSeconds":row.get::<_,f64>(6)?,
                "title":row.get::<_,String>(7)?,
                "kind":row.get::<_,String>(8)?,
            }))
        })
        .map_err(|_| "无法查询影视播放记录")?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "无法解析影视播放记录")?;
    Ok((rows, sessions))
}

fn kind_matches(kind: &str, selected: &str) -> bool {
    selected == "all" || selected == kind || selected == "series" && matches!(kind, "season" | "episode")
}

fn calculate(
    mut rows: Vec<Value>,
    sessions: Vec<Value>,
    input: &Value,
    today: NaiveDate,
) -> Result<Value, String> {
    let source = input["source"].as_str().unwrap_or("all");
    let selected_kind = input["kind"].as_str().unwrap_or("all");
    if !["all", "douban", "local"].contains(&source) {
        return Err("观影来源无效".into());
    }
    let period = input["period"].as_str().unwrap_or("month");
    let to = input["to"].as_str().and_then(day).unwrap_or(today);
    let from = if let Some(from) = input["from"].as_str().and_then(day) {
        from
    } else {
        match period {
            "week" => to - Duration::days(6),
            "month" => to - Duration::days(29),
            "year" => to - Duration::days(364),
            "all" => rows
                .iter()
                .filter_map(|row| day(row["markedAt"].as_str().unwrap_or("")))
                .chain(
                    sessions
                        .iter()
                        .filter_map(|row| day(row["startedAt"].as_str().unwrap_or(""))),
                )
                .min()
                .unwrap_or(to),
            _ => return Err("观影周期无效".into()),
        }
    };
    if from > to || to.signed_duration_since(from).num_days() > 36525 {
        return Err("观影日期范围无效".into());
    }
    let within = |date: NaiveDate| date >= from && date <= to;
    let sessions: Vec<Value> = sessions
        .into_iter()
        .filter(|session| {
            source != "douban"
                && kind_matches(session["kind"].as_str().unwrap_or("other"), selected_kind)
                && day(session["startedAt"].as_str().unwrap_or("")).is_some_and(&within)
        })
        .map(|mut session| {
            session["activityDate"] =
                json!(day(store::text(&session, "startedAt")).unwrap().to_string());
            session
        })
        .collect();
    let mut seconds_by_item: BTreeMap<String, f64> = BTreeMap::new();
    let mut latest_session: BTreeMap<String, String> = BTreeMap::new();
    let mut activity_dates: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut daily: BTreeMap<NaiveDate, (usize, f64)> = BTreeMap::new();
    let mut active = HashSet::new();
    for session in &sessions {
        let seconds = session["seconds"].as_f64().unwrap_or(0.0).max(0.0);
        *seconds_by_item
            .entry(session["itemId"].as_str().unwrap_or("").to_owned())
            .or_default() += seconds;
        let latest = latest_session
            .entry(session["itemId"].as_str().unwrap_or("").to_owned())
            .or_default();
        let started = session["startedAt"].as_str().unwrap_or("");
        if started > latest.as_str() {
            *latest = started.to_owned();
        }
        if seconds > 0.0 {
            if let Some(date) = day(session["startedAt"].as_str().unwrap_or("")) {
                daily.entry(date).or_default().1 += seconds;
                active.insert(date);
                activity_dates
                    .entry(store::text(session, "itemId").to_owned())
                    .or_default()
                    .insert(date.to_string());
            }
        }
    }
    rows.retain_mut(|row| {
        let imported = row["source"] == "douban";
        let marked = day(row["markedAt"].as_str().unwrap_or(""));
        let played = day(row["lastPlayed"].as_str().unwrap_or(""));
        let session = latest_session.get(row["itemId"].as_str().unwrap_or(""));
        let include_mark = source != "local" || !imported;
        let marked_in_range = include_mark && marked.is_some_and(&within);
        let mut dates = activity_dates
            .get(store::text(row, "itemId"))
            .cloned()
            .unwrap_or_default();
        if marked_in_range {
            dates.insert(marked.unwrap().to_string());
        }
        if source != "douban" && played.is_some_and(&within) {
            dates.insert(played.unwrap().to_string());
        }
        row["activityDates"] = json!(dates);
        let unknown = marked.is_none() && played.is_none();
        row["activityAt"] = json!(if marked_in_range {
            row["markedAt"].as_str().unwrap_or("")
        } else if let Some(session) = session {
            session
        } else {
            row["lastPlayed"].as_str().unwrap_or("")
        });
        row["watchedInRange"] = json!(
            include_mark
                && row["watched"] == true
                && (marked_in_range || period == "all" && marked.is_none())
        );
        let has_record = row["watched"] == true
            || row["rating"].is_number()
            || !row["comment"].as_str().unwrap_or("").is_empty()
            || !row["lastPlayed"].as_str().unwrap_or("").is_empty();
        has_record
            && kind_matches(row["kind"].as_str().unwrap_or("other"), selected_kind)
            && (source == "all"
                || source == "douban" && imported
                || source == "local" && (!imported || session.is_some()))
            && (marked_in_range
                || source != "douban" && (played.is_some_and(&within) || session.is_some())
                || period == "all" && unknown)
    });
    let mut rating_sum = 0.0;
    let mut rating_count = 0usize;
    let mut rating_distribution: BTreeMap<i64, usize> = BTreeMap::new();
    let mut genres: BTreeMap<String, usize> = BTreeMap::new();
    for row in &mut rows {
        row["playedSeconds"] = json!(seconds_by_item
            .get(row["itemId"].as_str().unwrap_or(""))
            .copied()
            .unwrap_or(0.0));
        if let Some(rating) = row["rating"].as_f64().filter(|rating| {
            (1.0..=5.0).contains(rating) && !(source == "local" && row["source"] == "douban")
        }) {
            rating_sum += rating;
            rating_count += 1;
            *rating_distribution
                .entry(rating.round() as i64)
                .or_default() += 1;
        }
        if row["watchedInRange"] == true {
            if let Some(date) =
                day(row["markedAt"].as_str().unwrap_or("")).filter(|date| within(*date))
            {
                daily.entry(date).or_default().0 += 1;
                active.insert(date);
            }
            if let Some(values) = row["genres"].as_array() {
                for genre in values.iter().filter_map(Value::as_str) {
                    *genres.entry(genre.to_owned()).or_default() += 1;
                }
            }
        }
    }
    rows.sort_by(|a, b| {
        b["activityAt"]
            .as_str()
            .unwrap_or("")
            .cmp(a["activityAt"].as_str().unwrap_or(""))
    });
    let watched_count = |kind: &str| {
        rows.iter()
            .filter(|row| row["watchedInRange"] == true && row["kind"] == kind)
            .count()
    };
    let played_seconds: f64 = sessions
        .iter()
        .map(|row| row["seconds"].as_f64().unwrap_or(0.0).max(0.0))
        .sum::<f64>()
        .max(0.0);
    let now = Utc::now().to_rfc3339();
    Ok(json!({
        "from":from.to_string(),
        "to":to.to_string(),
        "generatedAt":now,
        "summary":{ "watchedMovies":watched_count("movie"), "watchedSeries":watched_count("series"), "watchedSeasons":watched_count("season"), "watchedEpisodes":watched_count("episode"), "recordCount":rows.len(), "activeDays":active.len(), "averageRating":if rating_count>0 { Some(rating_sum/rating_count as f64)} else{ None} , "playedSeconds":played_seconds},
        "daily":daily.into_iter().map(|(date,(count,seconds))|json!({"date":date.to_string(),"count":count,"seconds":seconds})).collect::<Vec<_>>(),
        "ratingDistribution":(1..=5).map(|rating|json!({"rating":rating,"count":rating_distribution.get(&rating).copied().unwrap_or(0)})).collect::<Vec<_>>(),
        "genreDistribution":genres.into_iter().map(|(name,count)|json!({"name":name,"count":count})).collect::<Vec<_>>(),
        "records":rows,
        "sessions":sessions,
        "notes":["豆瓣日期为来源标记日期；没有播放数据时不补算观看时长。","仅有季或整剧已看标记时，不生成逐集观看记录。","本机播放时长按实际有效播放累计；日分布按播放会话开始日归属。"],
    }))
}

pub async fn statistics(app: tauri::AppHandle, input: Value) -> Result<Value, String> {
    let dir = app.state::<State>().dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = store::open(&dir)?;
        let (rows, sessions) = read(&conn)?;
        calculate(rows, sessions, &input, Local::now().date_naive())
    })
    .await
    .map_err(|_| "影视统计读取任务失败".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calendar_records_keep_every_play_day_and_mark_date() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        let rows = vec![json!({
            "itemId":"m",
            "kind":"movie",
            "watched":true,
            "markedAt":"2026-10-24",
            "lastPlayed":"2026-10-26T10:00:00+08:00",
            "source":"douban",
            "rating":4,
        })];
        let started = ["2026-10-25T10:00:00+08:00", "2026-10-26T10:00:00+08:00"];
        let sessions = started
            .into_iter()
            .map(|value| json!({"itemId":"m","kind":"movie","startedAt":value,"seconds":10}))
            .collect::<Vec<_>>();
        let result = calculate(
            rows.clone(),
            sessions.clone(),
            &json!({"period":"week"}),
            today,
        )
        .unwrap();
        let dates = result["records"][0]["activityDates"].as_array().unwrap();
        for daily in result["daily"].as_array().unwrap() {
            assert!(dates.contains(&daily["date"]));
        }
        let local = calculate(
            rows.clone(),
            sessions.clone(),
            &json!({"period":"week","source":"local"}),
            today,
        )
        .unwrap();
        assert_eq!(local["records"].as_array().unwrap().len(), 1);
        assert_eq!(local["records"][0]["watchedInRange"], false);
        assert_eq!(local["summary"]["averageRating"], Value::Null);
        assert_eq!(
            local["records"][0]["activityDates"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let imported = calculate(
            rows,
            sessions,
            &json!({"period":"week","source":"douban"}),
            today,
        )
        .unwrap();
        assert_eq!(
            imported["records"][0]["activityDates"],
            json!(["2026-10-24"])
        );
        assert!(imported["sessions"].as_array().unwrap().is_empty());
    }
    #[test]
    fn imported_watches_never_fabricate_playtime_or_episodes() {
        let rows = vec![json!({
            "itemId":"season",
            "title":"剧第一季",
            "kind":"season",
            "watched":true,
            "rating":4,
            "markedAt":"2026-10-20",
            "source":"douban",
            "genres":["剧情"],
        })];
        let result = calculate(
            rows,
            vec![],
            &json!({"period":"all"}),
            NaiveDate::from_ymd_opt(2026, 10, 25).unwrap(),
        )
        .unwrap();
        assert_eq!(result["summary"]["playedSeconds"], 0.0);
        assert_eq!(result["summary"]["watchedEpisodes"], 0);
        assert_eq!(result["summary"]["watchedSeasons"], 1);
        assert_eq!(result["summary"]["averageRating"], 4.0);
    }
    #[test]
    fn date_source_and_kind_filters_keep_real_sessions_separate() {
        let rows = vec![
            json!({"itemId":"m","kind":"movie","watched":true,"markedAt":"2026-10-01","source":"douban"}),
            json!({
                "itemId":"e",
                "kind":"episode",
                "watched":false,
                "lastPlayed":"2026-10-25T10:00:00+08:00",
                "source":"local",
            }),
        ];
        let sessions = vec![
            json!({"itemId":"e","kind":"episode","startedAt":"2026-10-25T10:00:00+08:00","seconds":60}),
        ];
        let today = NaiveDate::from_ymd_opt(2026, 10, 25).unwrap();
        let result = calculate(
            rows.clone(),
            sessions.clone(),
            &json!({"period":"week","source":"local"}),
            today,
        )
        .unwrap();
        assert_eq!(result["summary"]["playedSeconds"], 60.0);
        assert_eq!(result["records"].as_array().unwrap().len(), 1);
        let result = calculate(
            rows,
            sessions,
            &json!({"period":"all","source":"douban"}),
            today,
        )
        .unwrap();
        assert_eq!(result["summary"]["playedSeconds"], 0.0);
        assert_eq!(result["summary"]["watchedMovies"], 1);
    }
    #[test]
    fn recent_replay_does_not_count_an_old_watched_mark_again() {
        let rows = vec![json!({
            "itemId":"m",
            "kind":"movie",
            "watched":true,
            "markedAt":"2025-01-01",
            "lastPlayed":"2026-10-25T10:00:00+08:00",
            "source":"local",
        })];
        let sessions = vec![
            json!({"itemId":"m","kind":"movie","startedAt":"2026-10-25T10:00:00+08:00","seconds":45.0}),
        ];
        let result = calculate(
            rows,
            sessions,
            &json!({"period":"week"}),
            NaiveDate::from_ymd_opt(2026, 10, 25).unwrap(),
        )
        .unwrap();
        assert_eq!(result["summary"]["watchedMovies"], 0);
        assert_eq!(result["summary"]["playedSeconds"], 45.0);
        assert_eq!(result["summary"]["recordCount"], 1);
        assert_eq!(result["daily"].as_array().unwrap().len(), 1);
        assert_eq!(
            result["records"][0]["activityAt"],
            "2026-10-25T10:00:00+08:00"
        );
    }
}
