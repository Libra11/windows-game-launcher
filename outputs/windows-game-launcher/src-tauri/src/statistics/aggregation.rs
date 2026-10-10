use super::model::*;
use crate::{achievement_platform, activity, db, steam_playtime};
use chrono::{Days, Local, NaiveDate};
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

fn date(value: &str) -> Option<NaiveDate> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&Local).date_naive())
}

pub(super) fn validate(query: &Query) -> Result<(), String> {
    if !["all", "steam", "epic", "local"].contains(&query.source.as_str())
        || !["7", "30", "90", "365", "all"].contains(&query.period.as_str())
    {
        return Err("统计筛选条件无效".into());
    }
    if let Some(day) = &query.day {
        NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|_| "统计日期无效")?;
    }
    Ok(())
}

fn sessions(conn: &Connection, source: &str) -> Result<Vec<Session>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT p.id,p.game_id,g.title,g.source,p.started_at,p.ended_at,p.seconds
         FROM play_sessions p JOIN games g ON g.id=p.game_id
         WHERE ?1='all' OR g.source=?1 ORDER BY p.started_at DESC,p.id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([source], |row| {
            Ok(Session {
                id: row.get(0)?,
                game_id: row.get(1)?,
                game_title: row.get(2)?,
                source: row.get(3)?,
                started_at: row.get(4)?,
                ended_at: row.get(5)?,
                seconds: row.get::<_, i64>(6)?.max(0) as u64,
                daily_recorded: false,
                dates: Vec::new(),
            })
        })
        .map_err(|e| e.to_string())?;
    let mut items = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut day_stmt = conn
        .prepare(
            "SELECT d.session_id,d.date FROM daily_playtime d JOIN games g ON g.id=d.game_id
         WHERE ?1='all' OR g.source=?1 ORDER BY d.date",
        )
        .map_err(|e| e.to_string())?;
    let rows = day_stmt
        .query_map([source], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;
    let mut dates: HashMap<String, Vec<String>> = HashMap::new();
    for row in rows {
        let (id, day) = row.map_err(|e| e.to_string())?;
        dates.entry(id).or_default().push(day);
    }
    for item in &mut items {
        item.dates = dates.remove(&item.id).unwrap_or_default();
        item.daily_recorded = !item.dates.is_empty();
    }
    items.sort_by(|a, b| {
        chrono::DateTime::parse_from_rfc3339(&b.started_at)
            .ok()
            .cmp(&chrono::DateTime::parse_from_rfc3339(&a.started_at).ok())
            .then(a.id.cmp(&b.id))
    });
    Ok(items)
}

fn range(
    query: &Query,
    started: NaiveDate,
    items: &[Session],
    today: NaiveDate,
) -> (NaiveDate, NaiveDate) {
    let from = if query.period == "all" {
        items
            .iter()
            .filter_map(|item| date(&item.started_at))
            .min()
            .unwrap_or(started)
            .min(started)
            .min(today)
    } else {
        today
            .checked_sub_days(Days::new(query.period.parse::<u64>().unwrap() - 1))
            .unwrap()
    };
    (from, today)
}

fn start(conn: &Connection) -> Result<(String, NaiveDate), String> {
    let value = db::setting(conn, "statistics_started_at")?;
    let day = date(&value).ok_or("统计起始时间无效")?;
    Ok((value, day))
}

pub(super) fn session_page(
    conn: &Connection,
    query: &Query,
    offset: usize,
    limit: usize,
) -> Result<SessionPage, String> {
    validate(query)?;
    let (_, started) = start(conn)?;
    let mut items = sessions(conn, &query.source)?;
    let (from, to) = range(query, started, &items, Local::now().date_naive());
    items.retain(|item| {
        if let Some(day) = &query.day {
            return item.dates.contains(day)
                || (!item.daily_recorded
                    && date(&item.started_at).is_some_and(|d| d.to_string() == *day));
        }
        item.dates.iter().any(|d| {
            d.as_str() >= from.to_string().as_str() && d.as_str() <= to.to_string().as_str()
        }) || date(&item.started_at).is_some_and(|d| d >= from && d <= to)
    });
    let total = items.len();
    Ok(SessionPage {
        items: items
            .into_iter()
            .skip(offset)
            .take(limit.clamp(1, 100))
            .collect(),
        total,
    })
}

pub(super) fn snapshot(conn: &Connection, query: &Query) -> Result<Snapshot, String> {
    validate(query)?;
    let (started_at, start_date) = start(conn)?;
    let items = sessions(conn, &query.source)?;
    let (from, to) = range(query, start_date, &items, Local::now().date_naive());
    let from_text = from.to_string();
    let to_text = to.to_string();
    let selected = db::games(conn)?
        .into_iter()
        .filter(|g| query.source == "all" || g.source == query.source)
        .collect::<Vec<_>>();
    let activities = activity::all(conn)?;
    let steam_id = db::setting(conn, "steam_id")?;
    let steam = steam_playtime::read(conn, &steam_id)?;
    let connected = !steam_id.is_empty() && !db::setting(conn, "steam_api_key")?.is_empty();
    let mut summary = Summary::default();
    let mut period = Period::default();
    let mut days: HashMap<String, (u64, usize)> = HashMap::new();
    let mut game_period: HashMap<String, u64> = HashMap::new();
    let mut session_ids = HashSet::new();
    let mut stmt = conn.prepare(
        "SELECT d.date,d.game_id,d.session_id,d.seconds FROM daily_playtime d
         JOIN games g ON g.id=d.game_id WHERE d.date>=?1 AND d.date<=?2 AND (?3='all' OR g.source=?3)"
    ).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![from_text, to_text, query.source], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?.max(0) as u64,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (day, game, session, seconds) = row.map_err(|e| e.to_string())?;
        days.entry(day).or_default().0 += seconds;
        *game_period.entry(game).or_default() += seconds;
        period.seconds += seconds;
        if seconds > 0 {
            session_ids.insert(session);
        }
    }
    period.sessions = session_ids.len();
    period.average_seconds = if period.sessions > 0 {
        period.seconds / period.sessions as u64
    } else {
        0
    };
    let mut games = Vec::new();
    let mut namespaces = HashMap::new();
    let mut eligible_total = 0;
    let mut eligible_unlocked = 0;
    for game in &selected {
        let local_seconds = activities
            .get(&game.id)
            .map(|a| a.played_seconds)
            .unwrap_or(0);
        let time = steam_playtime::info(game, local_seconds, &steam, connected);
        let achievements = db::achievements(conn, &game.id)?;
        let total = achievements.len();
        let unlocked = achievements
            .iter()
            .filter(|a| a.unlocked_at.is_some())
            .count();
        let manual = achievements
            .iter()
            .filter(|a| a.unlock_source.as_deref() == Some("manual"))
            .count();
        let definition_state = if game.schema_source.contains("暂无成就") {
            "none"
        } else if ["本地记录（仅已解锁）", "Xbox 本地事件（仅已确认）"]
            .contains(&game.schema_source.as_str())
        {
            "partial"
        } else if total > 0
            && [
                "Steam Web API",
                "本地定义文件",
                "Xbox 公开成就资料（Exophase）",
                "Epic 官方成就",
            ]
            .contains(&game.schema_source.as_str())
        {
            "complete"
        } else {
            "unknown"
        };
        let unavailable_epic_records = game.source == "epic"
            && serde_json::from_str::<serde_json::Value>(&game.metadata_json).ok()
                .is_some_and(|metadata| metadata.get("epicAchievementSyncError")
                    .and_then(serde_json::Value::as_str).is_some_and(|error| !error.is_empty()));
        let completion_rate = (definition_state == "complete" && !unavailable_epic_records)
            .then(|| unlocked as f64 / total as f64 * 100.0);
        if completion_rate.is_some() {
            eligible_total += total;
            eligible_unlocked += unlocked;
            if unlocked == total {
                summary.completed_games += 1;
            }
        }
        summary.local_seconds += local_seconds;
        summary.unlocked += unlocked;
        summary.manual_unlocked += manual;
        if game.source == "steam" {
            summary.steam_games += 1;
            if crate::steam_family::is_shared(game) { summary.steam_family_games += 1; }
            else { summary.steam_owned_games += 1; }
            if let Some(seconds) = time.seconds {
                summary.steam_known += 1;
                *summary.steam_seconds.get_or_insert(0) += seconds;
            }
            summary.steam_cached |= time.state == "cached";
            summary.steam_checked_at = time.checked_at.clone();
        }
        namespaces.insert(
            game.id.clone(),
            achievement_platform::namespace(conn, game)?,
        );
        games.push(GameStats {
            game_id: game.id.clone(),
            title: game.title.clone(),
            source: game.source.clone(),
            metadata_json: game.metadata_json.clone(),
            local_seconds,
            period_seconds: *game_period.get(&game.id).unwrap_or(&0),
            steam_seconds: (game.source == "steam").then_some(time.seconds).flatten(),
            steam_state: time.state,
            steam_checked_at: time.checked_at,
            total_achievements: total,
            unlocked,
            manual_unlocked: manual,
            definition_state: definition_state.into(),
            completion_rate,
        });
    }
    summary.game_count = games.len();
    summary.completion_rate =
        (eligible_total > 0).then(|| eligible_unlocked as f64 / eligible_total as f64 * 100.0);
    let mut events = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT e.game_id,g.title,e.namespace,e.api_name,a.name,a.icon,u.source,e.date,e.recorded_at
         FROM statistics_unlock_events e JOIN games g ON g.id=e.game_id
         JOIN unlocks u ON u.game_id=e.game_id AND u.api_name=e.api_name
         JOIN achievements a ON a.appid=e.namespace AND a.api_name=e.api_name
         WHERE u.source<>'manual' AND e.date>=?1 AND e.date<=?2 AND (?3='all' OR g.source=?3)
         ORDER BY e.recorded_at DESC,e.game_id,e.api_name"
    ).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![from_text, to_text, query.source], |row| {
            Ok((
                row.get::<_, String>(2)?,
                Event {
                    game_id: row.get(0)?,
                    game_title: row.get(1)?,
                    api_name: row.get(3)?,
                    name: row.get(4)?,
                    icon: row.get(5)?,
                    source: row.get(6)?,
                    date: row.get(7)?,
                    recorded_at: row.get(8)?,
                },
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (namespace, event) = row.map_err(|e| e.to_string())?;
        if namespaces.get(&event.game_id) != Some(&namespace) {
            continue;
        }
        days.entry(event.date.clone()).or_default().1 += 1;
        events.push(event);
    }
    period.new_unlocks = events.len();
    period.active_days = days.values().filter(|(seconds, _)| *seconds > 0).count();
    let daily = (0..=(to - from).num_days())
        .map(|index| {
            let day = from.checked_add_days(Days::new(index as u64)).unwrap();
            let (seconds, unlocks) = *days.get(&day.to_string()).unwrap_or(&(0, 0));
            Day {
                date: day.to_string(),
                recorded: day >= start_date,
                seconds,
                unlocks,
            }
        })
        .collect();
    let platforms = ["steam", "epic", "local"]
        .into_iter()
        .map(|source| Platform {
            source: source.into(),
            games: games.iter().filter(|g| g.source == source).count(),
            seconds: games
                .iter()
                .filter(|g| g.source == source)
                .map(|g| g.period_seconds)
                .sum(),
        })
        .collect();
    let day_unlocks = events
        .iter()
        .filter(|e| query.day.as_deref() == Some(&e.date))
        .cloned()
        .collect();
    Ok(Snapshot {
        started_at,
        start_date: start_date.to_string(),
        from: from_text,
        to: to_text,
        generated_at: Local::now().to_rfc3339(),
        summary,
        period,
        daily,
        platforms,
        games,
        recent_unlocks: events.into_iter().take(20).collect(),
        day_unlocks,
    })
}
