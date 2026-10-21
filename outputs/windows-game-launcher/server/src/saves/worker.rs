use super::*;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

// 心跳和校验必须持续独立推进：最终事务锁住上传行时，续租可能等待它提交。
async fn supervise(
    task: impl std::future::Future<Output = Result<()>>,
    lease: impl std::future::Future<Output = Result<()>>,
    timeout: Duration,
) -> Result<()> {
    tokio::select! {
        result=task=>result,
        result=lease=>result,
        _=tokio::time::sleep(timeout)=>Err(ApiError(axum::http::StatusCode::SERVICE_UNAVAILABLE,"存档校验超过时限".into())),
    }
}
async fn execute(
    app: &App,
    job: &jobs::Job,
    dir: &Path,
    slots: Arc<tokio::sync::Semaphore>,
) -> Result<()> {
    let task = async { verification::verify(app, job, dir, slots).await.map(|_| ()) };
    let lease = async {
        let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
        loop {
            heartbeat.tick().await;
            tokio::time::timeout(Duration::from_secs(15), jobs::renew(app, job))
                .await
                .map_err(|_| internal())??;
        }
    };
    supervise(task, lease, Duration::from_secs(45 * 60)).await
}
// 只清理本组件创建且超过最长任务时限的临时目录，不读取或删除用户存档。
fn clean_temporary(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("youji-job-")
            && entry.file_type()?.is_dir()
            && meta
                .modified()?
                .elapsed()
                .is_ok_and(|age| age > Duration::from_secs(24 * 3600))
        {
            std::fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}
pub async fn run_workers(app: App) -> Result<()> {
    storage(&app)?;
    let concurrency = std::env::var("YOUJI_SAVE_WORKERS")
        .unwrap_or_else(|_| "2".into())
        .parse::<usize>()
        .ok()
        .filter(|n| (1..=8).contains(n))
        .ok_or_else(|| ApiError::bad("YOUJI_SAVE_WORKERS 必须为 1–8"))?;
    let dir = std::env::var_os("YOUJI_SAVE_WORK_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("youji-save-jobs"));
    std::fs::create_dir_all(&dir).map_err(|_| internal())?;
    clean_temporary(&dir).map_err(|_| internal())?;
    let mut tasks = tokio::task::JoinSet::new();
    let slots = Arc::new(tokio::sync::Semaphore::new(concurrency));
    for _ in 0..concurrency {
        let slots = slots.clone();
        let app = app.clone();
        let dir = dir.clone();
        tasks.spawn(async move {
            loop {
                match jobs::claim(&app).await {
                    Ok(Some(job)) => {
                        if let Err(error) = execute(&app, &job, &dir, slots.clone()).await {
                            if jobs::fail(&app, &job, error).await.is_err() {
                                eprintln!("存档任务状态保存失败，将由租约恢复");
                            }
                        }
                    }
                    Ok(None) => tokio::time::sleep(Duration::from_secs(2)).await,
                    Err(_) => {
                        eprintln!("存档任务领取失败，稍后重试");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        });
    }
    let cleanup = app.clone();
    tasks.spawn(async move {
        loop {
            if uploads::cleanup(&cleanup).await.is_err() {
                eprintln!("存档暂存对象清理失败，保留队列");
            }
            if clean_temporary(&dir).is_err() {
                eprintln!("存档临时目录清理失败");
            }
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });
    // 任一 worker 意外退出则整体退出，由进程管理器重启，未提交任务依租约恢复。
    tokio::select! {
        _=tasks.join_next()=>return Err(internal()),
        _=shutdown()=>{},
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    Ok(())
}
async fn shutdown() {
    #[cfg(unix)]
    {
        if let Ok(mut term) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! {_=tokio::signal::ctrl_c()=>{},_=term.recv()=>{}};
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn heartbeat_waiting_on_final_transaction_does_not_stop_commit() {
        let row = tokio::sync::Mutex::new(());
        let locked = tokio::sync::Notify::new();
        let committed = std::sync::atomic::AtomicBool::new(false);
        let task = async {
            let _guard = row.lock().await;
            locked.notify_one();
            tokio::time::sleep(Duration::from_millis(20)).await;
            committed.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        };
        let heartbeat = async {
            locked.notified().await;
            let _guard = row.lock().await;
            std::future::pending::<Result<()>>().await
        };
        assert!(supervise(task, heartbeat, Duration::from_secs(1))
            .await
            .is_ok());
        assert!(committed.load(std::sync::atomic::Ordering::SeqCst));
    }
}
