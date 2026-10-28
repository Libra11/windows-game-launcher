use super::*;
use crate::media::store;
use rusqlite::params;

#[test]
fn recent_games_are_played_sorted_capped_and_exclude_archived_or_removed() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE games(id TEXT PRIMARY KEY,title TEXT);CREATE TABLE game_activity(game_id TEXT,last_played TEXT);CREATE TABLE cloud_archives(game_id TEXT,archived INTEGER);").unwrap();
    for index in 1..=8 {
        conn.execute("INSERT INTO games VALUES(?1,?1)", [format!("g{index}")])
            .unwrap();
        conn.execute(
            "INSERT INTO game_activity VALUES(?1,?2)",
            params![
                format!("g{index}"),
                format!("2026-10-{index:02}T12:00:00+08:00")
            ],
        )
        .unwrap();
    }
    conn.execute("INSERT INTO games VALUES('unplayed','未玩')", [])
        .unwrap();
    conn.execute("INSERT INTO cloud_archives VALUES('g8',1)", [])
        .unwrap();
    conn.execute("DELETE FROM games WHERE id='g7'", []).unwrap();
    let rows = games(&conn).unwrap();
    assert_eq!(rows.len(), 5);
    assert_eq!(
        rows.iter()
            .map(|entry| entry.title.as_str())
            .collect::<Vec<_>>(),
        vec!["g6", "g5", "g4", "g3", "g2"]
    );
    assert_eq!(rows[0].action, Action::Game("g6".into()));
}

fn session(conn: &Connection, item: &str, resource: &str, day: u32) {
    conn.execute(
        "INSERT OR IGNORE INTO resources(id,source_id,path,updated_at)VALUES(?1,'s',?2,'now')",
        params![resource, format!("C:/test/{resource}.mkv")],
    )
    .unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO resource_links(resource_id,item_id)VALUES(?1,?2)",
        params![resource, item],
    )
    .unwrap();
    conn.execute("INSERT INTO view_sessions(id,item_id,resource_id,started_at,seconds)VALUES(?1,?2,?3,?4,10)", params![format!("v{day}"),item,resource,format!("2026-10-{day:02}T12:00:00+08:00")]).unwrap();
}

#[test]
fn recent_media_group_episodes_by_season_keep_latest_file_and_exclude_invalid_links() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    conn.execute(
        "INSERT INTO sources(id,path,label,created_at)VALUES('s','C:/test','测试','now')",
        [],
    )
    .unwrap();
    let show = store::new_item(&conn, &json!({"title":"剧名","kind":"series"})).unwrap();
    let season = store::new_item(
        &conn,
        &json!({"title":"第 1 季","kind":"season","parentId":show}),
    )
    .unwrap();
    let first = store::new_item(
        &conn,
        &json!({"title":"第一集","kind":"episode","parentId":season}),
    )
    .unwrap();
    let second = store::new_item(
        &conn,
        &json!({"title":"第二集","kind":"episode","parentId":season}),
    )
    .unwrap();
    session(&conn, &first, "first", 1);
    session(&conn, &second, "second", 2);
    let movie = store::new_item(&conn, &json!({"title":"电影","kind":"movie"})).unwrap();
    session(&conn, &movie, "movie", 3);
    let unavailable = store::new_item(&conn, &json!({"title":"离线","kind":"movie"})).unwrap();
    session(&conn, &unavailable, "offline", 4);
    conn.execute("UPDATE resources SET available=0 WHERE id='offline'", [])
        .unwrap();
    let rows = media(&conn).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].title, "电影");
    assert_eq!(rows[1].title, "剧名 · 第 1 季");
    assert_eq!(
        rows[1].action,
        Action::Media {
            item: second.clone(),
            resource: "second".into()
        }
    );
    conn.execute("DELETE FROM resource_links WHERE resource_id='second'", [])
        .unwrap();
    assert_eq!(
        media(&conn).unwrap()[1].action,
        Action::Media {
            item: first,
            resource: "first".into()
        }
    );
    for day in 5..=10 {
        let movie =
            store::new_item(&conn, &json!({"title":format!("电影{day}"),"kind":"movie"})).unwrap();
        session(&conn, &movie, &format!("movie{day}"), day);
    }
    assert_eq!(media(&conn).unwrap().len(), 5);
    assert_eq!(media(&conn).unwrap()[0].title, "电影10");
    conn.execute("UPDATE sources SET available=0", []).unwrap();
    assert!(media(&conn).unwrap().is_empty());
}

#[test]
fn menu_ids_cannot_collide_or_change_when_reordered() {
    let a = Entry {
        title: "同名".into(),
        action: Action::Media {
            item: "a:b".into(),
            resource: "c".into(),
        },
    };
    let b = Entry {
        title: "同名".into(),
        action: Action::Media {
            item: "a".into(),
            resource: "b:c".into(),
        },
    };
    assert_ne!(a.key(), b.key());
    let mut rows = vec![a.clone(), b];
    rows.reverse();
    assert_eq!(rows[1].key(), a.key());
}

#[test]
fn popup_height_reserves_empty_sections_and_grows_with_visible_rows() {
    let empty = Entries::default();
    assert_eq!(Snapshot::from(&empty).height(), 266.0);
    let entries = Entries {
        games: vec![Entry {
            title: "游戏".into(),
            action: Action::Game("g".into()),
        }],
        media: (0..5)
            .map(|index| Entry {
                title: format!("影视{index}"),
                action: Action::Media {
                    item: format!("i{index}"),
                    resource: format!("r{index}"),
                },
            })
            .collect(),
    };
    assert_eq!(Snapshot::from(&entries).height(), 418.0);
}
