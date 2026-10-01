use rusqlite::Connection;

// 撤回旧版黯井微光存档推断，仅匹配它的记录来源和七个推断标识。
// 手动记录、Steam 官方记录及其他游戏的解锁不受影响。
pub fn remove_inferred_unlocks(conn: &mut Connection) -> Result<usize, String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let count = tx.execute(
        "DELETE FROM unlocks WHERE source='local'
         AND game_id IN (SELECT id FROM games WHERE source='local' AND appid='3699590')
         AND replace(evidence, char(92), '/') LIKE '%/SaveGames/root/WELLDWELLER/blob.sav'
         AND api_name IN ('20_shop','21_trinket','35_vessel','38_100enemies','39_500enemies','40_10slots','41_50slots')",
        [],
    ).map_err(|e| e.to_string())?;
    tx.execute("UPDATE games SET scan_status='',source_file='' WHERE source='local' AND appid='3699590' AND scan_status LIKE '已读取黯井微光存档（部分检测：%'", []).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removes_only_inferred_records_and_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE games(id TEXT,source TEXT,appid TEXT,scan_status TEXT,source_file TEXT);
            CREATE TABLE unlocks(game_id TEXT,api_name TEXT,source TEXT,evidence TEXT);
            INSERT INTO games VALUES ('local','local','3699590','',''),('steam','steam','3699590','','');").unwrap();
        for (game, name, source, file) in [
            (
                "local",
                "20_shop",
                "local",
                "C:/SaveGames/root/WELLDWELLER/blob.sav",
            ),
            (
                "local",
                "21_trinket",
                "local",
                "C:/SaveGames/root/WELLDWELLER/blob.sav",
            ),
            (
                "local",
                "38_100enemies",
                "manual",
                "C:/SaveGames/root/WELLDWELLER/blob.sav",
            ),
            ("local", "39_500enemies", "local", "C:/achievements.json"),
            (
                "steam",
                "20_shop",
                "steam",
                "C:/SaveGames/root/WELLDWELLER/blob.sav",
            ),
        ] {
            conn.execute(
                "INSERT INTO unlocks VALUES (?1,?2,?3,?4)",
                rusqlite::params![game, name, source, file],
            )
            .unwrap();
        }
        assert_eq!(remove_inferred_unlocks(&mut conn).unwrap(), 2);
        assert_eq!(remove_inferred_unlocks(&mut conn).unwrap(), 0);
        assert_eq!(
            conn.query_row("SELECT count(*) FROM unlocks", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            3
        );
    }
}
