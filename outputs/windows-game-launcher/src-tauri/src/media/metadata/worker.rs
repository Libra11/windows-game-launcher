//! 单条元数据任务编排；匹配、豆瓣访问和写库各自留在原模块。
use super::{auto_identify_typed, refresh_known, retry::Failure, store};
use serde_json::Value;
use std::path::Path;

pub(super) async fn identify(
    app: &tauri::AppHandle,
    dir: &Path,
    item: &str,
) -> Result<Value, Failure> {
    let current = store::item(&store::open(dir)?, item)?;
    if current["recognized"] == true {
        return refresh_known(dir, item).await.map_err(Into::into);
    }
    let result = auto_identify_typed(dir, item).await;
    if result.as_ref().is_err_and(|e| e.needs_details) {
        match crate::media::douban::details::enrich(app, item).await {
            Ok(true) => auto_identify_typed(dir, item).await,
            Ok(false) => result,
            Err(error) => Err(error.into()),
        }
    } else {
        result
    }
}
