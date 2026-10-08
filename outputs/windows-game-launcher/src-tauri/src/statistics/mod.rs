mod aggregation;
mod model;
pub(crate) mod store;
#[cfg(test)]
mod tests;
use crate::{lock_db, AppState};
use model::{Query, SessionPage, Snapshot};

#[tauri::command]
pub(crate) fn get_statistics(
    state: tauri::State<'_, AppState>,
    query: Query,
) -> Result<Snapshot, String> {
    aggregation::snapshot(&*lock_db(&state)?, &query)
}

#[tauri::command]
pub(crate) fn list_statistics_sessions(
    state: tauri::State<'_, AppState>,
    query: Query,
    offset: usize,
    limit: usize,
) -> Result<SessionPage, String> {
    aggregation::session_page(&*lock_db(&state)?, &query, offset, limit)
}
