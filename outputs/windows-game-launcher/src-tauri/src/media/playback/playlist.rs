use super::{player_path, resource_by_id, Resource};
use crate::media::store;
use rusqlite::Connection;
use std::{collections::HashMap, path::PathBuf};

pub(super) struct Queue {
    pub paths: Vec<String>,
    pub start_index: usize,
}
// 与片库保持相同版本优先级：最近播放的可用资源优先，然后按路径稳定选择。
fn resources(conn: &Connection, item: &str) -> Result<Vec<Resource>, String> {
    let mut query = conn.prepare("SELECT r.id FROM resources r JOIN resource_links l ON l.resource_id=r.id WHERE l.item_id=?1 AND r.available=1 AND r.missing=0 ORDER BY COALESCE((SELECT MAX(v.started_at) FROM view_sessions v WHERE v.resource_id=r.id),'') DESC,r.path COLLATE NOCASE,r.id").map_err(|e| e.to_string())?;
    let ids = query
        .query_map([item], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(ids
        .into_iter()
        .filter_map(|id| resource_by_id(conn, &id, Some(item)).ok())
        .filter(|resource| std::path::Path::new(&resource.path).is_file())
        .collect())
}

pub(super) fn paths(conn: &Connection, initial: &Resource) -> Result<Queue, String> {
    let single = || Queue {
        paths: vec![player_path(&initial.path)],
        start_index: 0,
    };
    let items = store::items(conn)?;
    let by_id: HashMap<_, _> = items.iter().map(|v| (store::text(v, "id"), v)).collect();
    let Some(item) = by_id.get(initial.item_id.as_str()) else {
        return Ok(single());
    };
    if !matches!(store::text(item, "kind"), "episode" | "season" | "series") {
        return Ok(single());
    }
    let root = |id: &str| {
        let mut current = id.to_string();
        for _ in 0..32 {
            let Some(parent) = by_id
                .get(current.as_str())
                .and_then(|v| v["parentId"].as_str())
                .filter(|v| !v.is_empty())
            else {
                break;
            };
            current = parent.to_string();
        }
        current
    };
    let root_id = root(&initial.item_id);
    let order = |v: &serde_json::Value| {
        let season = v["seasonNumber"]
            .as_i64()
            .or_else(|| {
                v["parentId"]
                    .as_str()
                    .and_then(|id| by_id.get(id))
                    .and_then(|v| v["seasonNumber"].as_i64())
            })
            .unwrap_or(1);
        (season, v["episodeNumber"].as_i64().unwrap_or(0))
    };
    let mut entries = Vec::new();
    for value in &items {
        let id = store::text(value, "id");
        if root(id) != root_id
            || !matches!(store::text(value, "kind"), "episode" | "season" | "series")
        {
            continue;
        }
        let mut available = resources(conn, id)?;
        if value["kind"] == "episode" {
            // 当前集保留用户本次选的版本；其他集只加入默认版本。
            if id == initial.item_id {
                available.clear();
                entries.push((order(value), player_path(&initial.path)));
            } else if let Some(resource) = available.first() {
                entries.push((order(value), player_path(&resource.path)));
            }
        } else {
            // 没有分集层级的豆瓣季仍可拥有多个直接关联的文件。
            entries.extend(
                available
                    .into_iter()
                    .map(|resource| (order(value), player_path(&resource.path))),
            );
        }
    }
    entries.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
    });
    let mut seen = std::collections::HashSet::new();
    let paths: Vec<_> = entries
        .into_iter()
        .map(|(_, path)| path)
        .filter(|path| seen.insert(path.to_lowercase()))
        .collect();
    let initial_path = player_path(&initial.path);
    let Some(start_index) = paths
        .iter()
        .position(|path| path.eq_ignore_ascii_case(&initial_path))
    else {
        return Ok(single());
    };
    Ok(Queue { paths, start_index })
}

pub(super) struct PlaylistFile(pub PathBuf);
impl PlaylistFile {
    pub fn new(paths: &[String], start_index: usize) -> Result<Self, String> {
        let selected = paths.get(start_index).ok_or("剧集列表起始位置无效")?;
        if paths.iter().any(|path| path.contains(['\r', '\n'])) {
            return Err("媒体路径包含无效换行符".into());
        }
        let path = std::env::temp_dir().join(format!("youji-media-{}.dpl", uuid::Uuid::new_v4()));
        let mut content = format!("\u{feff}DAUMPLAYLIST\r\nplayname={selected}\r\ntopindex={start_index}\r\nsaveplaypos=0\r\n");
        for (index, file) in paths.iter().enumerate() {
            content.push_str(&format!("{}*file*{}\r\n", index + 1, file));
        }
        std::fs::write(&path, content).map_err(|_| "无法准备剧集播放列表")?;
        Ok(Self(path))
    }
}
impl Drop for PlaylistFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use serde_json::json;
    #[test]
    fn potplayer_file_contains_full_order_and_selected_file_and_is_removed_on_drop() {
        let paths = vec![
            "C:\\剧集 & 测试\\E01.mkv".into(),
            "C:\\剧集 & 测试\\E02.mkv".into(),
        ];
        let file = PlaylistFile::new(&paths, 1).unwrap();
        let path = file.0.clone();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("\u{feff}DAUMPLAYLIST\r\n"));
        assert!(content.contains("playname=C:\\剧集 & 测试\\E02.mkv\r\n"));
        assert!(content
            .contains("1*file*C:\\剧集 & 测试\\E01.mkv\r\n2*file*C:\\剧集 & 测试\\E02.mkv\r\n"));
        drop(file);
        assert!(!path.exists());
        assert!(PlaylistFile::new(&paths, 2).is_err());
        assert!(PlaylistFile::new(&["C:/bad\nfile".into()], 0).is_err());
    }
    #[test]
    fn queue_loads_all_seasons_and_specials_skips_gaps_and_preserves_selected_version() {
        let dir = tempfile::tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        conn.execute(
            "INSERT INTO sources(id,path,label,created_at)VALUES('s',?1,'test','now')",
            [dir.path().to_string_lossy()],
        )
        .unwrap();
        let series = store::new_item(&conn, &json!({"title":"剧","kind":"series"})).unwrap();
        let mut episodes: Vec<String> = Vec::new();
        for (season, episode, version, available) in [
            (1, 1, 1, true),
            (1, 2, 1, true),
            (1, 2, 2, true),
            (1, 10, 1, false),
            (2, 1, 1, true),
            (0, 1, 1, true),
        ] {
            let key = format!("{season}-{episode}");
            let item = if version == 2 {
                episodes.last().unwrap().clone()
            } else {
                let id=store::new_item(&conn,&json!({"title":key,"kind":"episode","parentId":series,"seasonNumber":season,"episodeNumber":episode})).unwrap();
                episodes.push(id.clone());
                id
            };
            let id = format!("{key}-{version}");
            let path = dir.path().join(format!("{id}.mkv"));
            std::fs::write(&path, b"fixture").unwrap();
            conn.execute("INSERT INTO resources(id,source_id,path,available,updated_at)VALUES(?1,'s',?2,?3,'now')",params![id,path.to_string_lossy(),available]).unwrap();
            conn.execute(
                "INSERT INTO resource_links(resource_id,item_id)VALUES(?1,?2)",
                params![id, item],
            )
            .unwrap();
        }
        let initial = resource_by_id(&conn, "1-1-1", None).unwrap();
        let queue = paths(&conn, &initial).unwrap();
        assert_eq!(queue.paths.len(), 4);
        assert_eq!(queue.start_index, 1);
        assert!(queue.paths[1].ends_with("1-1-1.mkv"));
        assert!(queue.paths[2].ends_with("1-2-1.mkv"));
        conn.execute("INSERT INTO view_sessions(id,item_id,resource_id,started_at,seconds)VALUES('recent',?1,'1-2-2','2026-10-28',10)", [&episodes[1]]).unwrap();
        let recent = paths(&conn, &initial).unwrap();
        assert!(recent.paths[2].ends_with("1-2-2.mkv"));
        let middle = resource_by_id(&conn, "1-2-2", None).unwrap();
        let queue = paths(&conn, &middle).unwrap();
        assert_eq!(queue.start_index, 2);
        assert!(queue.paths[2].ends_with("1-2-2.mkv"));
        let special = resource_by_id(&conn, "0-1-1", None).unwrap();
        assert_eq!(paths(&conn, &special).unwrap().paths.len(), 4);
        assert!(queue.paths[3].ends_with("2-1-1.mkv"));
        // 首选版本文件已丢失、但扫描尚未更新时，可使用同集的另一可用版本。
        std::fs::remove_file(dir.path().join("1-2-2.mkv")).unwrap();
        assert!(paths(&conn, &initial).unwrap().paths[2].ends_with("1-2-1.mkv"));
    }

    #[test]
    fn standalone_season_loads_all_direct_files_and_combined_episodes_share_one_path() {
        let dir = tempfile::tempdir().unwrap();
        let conn = store::open(dir.path()).unwrap();
        conn.execute(
            "INSERT INTO sources(id,path,label,created_at)VALUES('s',?1,'test','now')",
            [dir.path().to_string_lossy()],
        )
        .unwrap();
        let season = store::new_item(&conn, &json!({"title":"豆瓣季","kind":"season"})).unwrap();
        for index in 1..=3 {
            let path = dir.path().join(format!("E{index:02}.mkv"));
            std::fs::write(&path, b"fixture").unwrap();
            conn.execute(
                "INSERT INTO resources(id,source_id,path,updated_at)VALUES(?1,'s',?2,'now')",
                params![format!("r{index}"), path.to_string_lossy()],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO resource_links(resource_id,item_id)VALUES(?1,?2)",
                params![format!("r{index}"), season],
            )
            .unwrap();
        }
        let initial = resource_by_id(&conn, "r2", Some(&season)).unwrap();
        let queue = paths(&conn, &initial).unwrap();
        assert_eq!(queue.paths.len(), 3);
        assert_eq!(queue.start_index, 1);
        for index in 1..=2 {
            let episode = store::new_item(
                &conn,
                &json!({"title":"合并集","kind":"episode","parentId":season,"episodeNumber":index}),
            )
            .unwrap();
            conn.execute(
                "INSERT INTO resource_links(resource_id,item_id)VALUES('r1',?1)",
                [&episode],
            )
            .unwrap();
        }
        let queue = paths(&conn, &initial).unwrap();
        assert_eq!(
            queue.paths.len(),
            3,
            "合并文件及父级重复关联在播放列表中去重"
        );
    }
}
