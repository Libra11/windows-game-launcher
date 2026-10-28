//! 登录窗口只在标题匹配失败后读取详情；不参与列表导入或个人记录合并。
use super::{
    active, browser, changed, connection, dir, navigate, runtime, save_connection, Runtime,
};
use crate::media::store::douban_source as cache;
use crate::media::{ignored, store};
use serde_json::{json, Value};
use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

struct BrowserLease(Arc<Runtime>);
impl Drop for BrowserLease {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}

fn eligible(state: &Value) -> bool {
    state["phase"] != "loggingIn"
}

fn validate(page: &Value, subject: &str) -> Result<Value, String> {
    if page["restricted"] == true {
        return Err("豆瓣需要安全验证，请打开连接窗口完成验证后重试".into());
    }
    let detail = &page["detail"];
    if detail["subjectId"] != subject || store::text(detail, "title").is_empty() {
        return Err("豆瓣详情结构无法识别，已保留原条目".into());
    }
    let id = store::text(detail, "imdbId");
    if !id.is_empty() && !cache::valid_imdb_id(id) {
        return Err("豆瓣返回的 IMDb 编号无效".into());
    }
    if !detail["seasonNumber"].is_null()
        && detail["seasonNumber"]
            .as_u64()
            .is_none_or(|n| n > u32::MAX as u64)
    {
        return Err("豆瓣返回的季号无效".into());
    }
    Ok(
        json!({"subjectId":subject,"imdbId":id,"seasonNumber":detail["seasonNumber"],"kind":detail["kind"],"title":detail["title"]}),
    )
}

/// 调用方持有影视 operation 读锁；这里只持有短暂写锁，不嵌套申请 operation 锁。
pub(in crate::media) async fn enrich(
    app: &tauri::AppHandle,
    item_id: &str,
) -> Result<bool, String> {
    let root = dir(app);
    let Some(source) = cache::source(&store::open(&root)?, item_id)? else {
        return Ok(false);
    };
    let state = connection(app)?;
    if !eligible(&state) {
        return Err("豆瓣窗口正在登录，稍后重试详情".into());
    }
    if !cache::due(&source.raw) {
        if !store::text(&source.raw, "detailError").is_empty() {
            return Err("豆瓣详情暂未获取，稍后重试".into());
        }
        return Ok(false);
    }
    let rt = runtime(&root);
    if rt
        .busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("豆瓣窗口正忙，稍后重试详情".into());
    }
    let _lease = BrowserLease(rt.clone());
    let generation = rt.generation.load(Ordering::Acquire);
    // 用户取消后不自动恢复窗口访问；下次显式同步/连接会重置取消标志。
    active(&rt, generation)?;
    if !eligible(&connection(app)?) {
        return Err("豆瓣窗口正在登录，稍后重试详情".into());
    }
    let window = browser(app, false).await?;
    let url = format!("https://movie.douban.com/subject/{}/", source.subject)
        .parse()
        .map_err(|_| "豆瓣详情地址无效")?;
    let result = async {
        tokio::time::sleep(Duration::from_millis(1200)).await;
        let page = navigate(&window, &url, &rt, generation).await?;
        validate(&page, &source.subject)
    }
    .await;
    let _guard = rt.writes.lock().unwrap_or_else(|e| e.into_inner());
    active(&rt, generation)?;
    let current = connection(app)?;
    if !eligible(&current) {
        return Ok(false);
    }
    let mut conn = store::open(&root)?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    if !ignored::metadata_allowed(&tx, item_id)? || store::item(&tx, item_id)?["recognized"] == true
    {
        return Ok(false);
    }
    cache::save(&tx, item_id, &source, &result)?;
    tx.commit().map_err(|e| e.to_string())?;
    match result {
        Ok(detail) => Ok(!store::text(&detail, "imdbId").is_empty()),
        Err(error) => {
            if error.contains("登录") || error.contains("验证") {
                rt.cancel.store(true, Ordering::Release);
                let mut state = current;
                state["phase"] = json!("needsLogin");
                state["needsLogin"] = json!(true);
                state["message"] = json!(&error);
                save_connection(app, &state)?;
                changed(app);
            }
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests;
