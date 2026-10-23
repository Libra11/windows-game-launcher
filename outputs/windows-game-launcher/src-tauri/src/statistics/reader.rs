use rusqlite::{Connection, OpenFlags};
use std::{path::Path, sync::atomic::Ordering};
use tauri::Manager;

pub(super) fn read<T>(
    app: &tauri::AppHandle,
    query: impl FnOnce(&Connection) -> Result<T, String>,
) -> Result<T, String> {
    let state = app.state::<crate::AppState>();
    let check = || {
        if state.maintenance.load(Ordering::Acquire) {
            Err("游迹正在准备更新或恢复，请等待操作完成".to_string())
        } else {
            Ok(())
        }
    };
    check()?;
    let path = app.path().app_data_dir().map_err(|e| e.to_string())?.join("games.sqlite");
    let result = read_path(&path, query)?;
    check()?;
    Ok(result)
}

pub(super) fn read_path<T>(
    path: &Path,
    query: impl FnOnce(&Connection) -> Result<T, String>,
) -> Result<T, String> {
    // WAL 独立只读连接：不持有游戏库共享锁，各条查询使用同一数据库快照。
    // 不能调用 db::open；统计读取不应初始化表、设置或云同步触发器。
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    query(&tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_snapshot_allows_writes_without_changing_its_view() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("games.sqlite");
        let writer = crate::db::open(&path).unwrap();
        crate::db::set_setting(&writer, "test", "before").unwrap();
        read_path(&path, |reader| {
            assert_eq!(crate::db::setting(reader, "test")?, "before");
            crate::db::set_setting(&writer, "test", "after")?;
            assert_eq!(crate::db::setting(reader, "test")?, "before");
            assert!(crate::db::set_setting(reader, "test", "invalid").is_err());
            Ok(())
        }).unwrap();
        assert_eq!(crate::db::setting(&writer, "test").unwrap(), "after");
    }

    #[test]
    fn missing_database_is_not_created_by_statistics() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.sqlite");
        assert!(read_path(&path, |_| Ok(())).is_err());
        assert!(!path.exists());
    }
}
