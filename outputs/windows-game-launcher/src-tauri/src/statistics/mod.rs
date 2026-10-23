mod aggregation;
mod model;
mod reader;
pub(crate) mod store;
#[cfg(test)]
mod tests;
use model::{Query, SessionPage, Snapshot};

#[tauri::command(async)]
pub(crate) fn get_statistics(
    app: tauri::AppHandle,
    query: Query,
) -> Result<Snapshot, String> {
    aggregation::validate(&query)?;
    reader::read(&app, |conn| aggregation::snapshot(conn, &query))
}

#[tauri::command(async)]
pub(crate) fn list_statistics_sessions(
    app: tauri::AppHandle,
    query: Query,
    offset: usize,
    limit: usize,
) -> Result<SessionPage, String> {
    aggregation::validate(&query)?;
    reader::read(&app, |conn| aggregation::session_page(conn, &query, offset, limit))
}
