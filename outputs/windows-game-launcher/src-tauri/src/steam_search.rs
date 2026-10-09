use crate::steam_store::{self, SearchGame};

#[tauri::command]
pub(crate) async fn search_steam_games(query: String) -> Result<Vec<SearchGame>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    if query.chars().count() > 100 {
        return Err("搜索名称不能超过 100 个字符".into());
    }
    steam_store::search(query).await
}
