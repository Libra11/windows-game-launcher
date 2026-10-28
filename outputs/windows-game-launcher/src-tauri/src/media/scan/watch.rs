//! Windows 递归变动通知只作扫描提示，仍由完整扫描确认资源与缺失。
use super::{catalog, normalized, scan, store, State};
use serde_json::json;
use std::{
    collections::HashMap,
    os::windows::ffi::OsStrExt,
    path::Path,
    sync::mpsc,
    time::{Duration, Instant},
};
use tauri::Manager;
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
        Storage::FileSystem::{
            FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification,
            FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
            FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
        },
        System::Threading::WaitForSingleObject,
    },
};

#[derive(Default)]
struct Pending {
    first: Option<Instant>,
    last: Option<Instant>,
}
impl Pending {
    fn changed(&mut self, now: Instant) {
        self.first.get_or_insert(now);
        self.last = Some(now);
    }
    fn take_due(&mut self, now: Instant) -> bool {
        let due = self
            .last
            .is_some_and(|at| now.duration_since(at) >= Duration::from_secs(2))
            || self
                .first
                .is_some_and(|at| now.duration_since(at) >= Duration::from_secs(10));
        if due {
            *self = Self::default();
        }
        due
    }
}

struct Watch {
    handle: HANDLE,
    pending: Pending,
}
impl Watch {
    fn open(path: &str) -> Option<Self> {
        let root = normalized(Path::new(path)).ok()?;
        let wide: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
        // 不订阅读取／访问时间，扫描与探测本身不会反复触发。
        let handle = unsafe {
            FindFirstChangeNotificationW(
                PCWSTR(wide.as_ptr()),
                true,
                FILE_NOTIFY_CHANGE_DIR_NAME
                    | FILE_NOTIFY_CHANGE_FILE_NAME
                    | FILE_NOTIFY_CHANGE_LAST_WRITE
                    | FILE_NOTIFY_CHANGE_SIZE,
            )
        }
        .ok()?;
        Some(Self {
            handle,
            pending: Pending::default(),
        })
    }
    fn poll(&mut self, now: Instant) -> Result<bool, ()> {
        match unsafe { WaitForSingleObject(self.handle, 0) } {
            WAIT_OBJECT_0 => {
                unsafe { FindNextChangeNotification(self.handle) }.map_err(|_| ())?;
                self.pending.changed(now);
            }
            WAIT_TIMEOUT => {}
            _ => return Err(()),
        }
        Ok(self.pending.take_due(now))
    }
}
impl Drop for Watch {
    fn drop(&mut self) {
        let _ = unsafe { FindCloseChangeNotification(self.handle) };
    }
}

pub(super) fn start(app: tauri::AppHandle) {
    let (config_tx, config_rx) = mpsc::channel::<Vec<(String, String)>>();
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    // HANDLE 仅在此线程创建、轮询和关闭，不跨 async await，也无每源常驻线程。
    std::thread::spawn(move || {
        let mut watches: HashMap<String, (String, Option<Watch>, Instant)> = HashMap::new();
        loop {
            match config_rx.recv_timeout(Duration::from_millis(250)) {
                Ok(config) => {
                    watches.retain(|id, (path, _, _)| {
                        config.iter().any(|(i, p)| i == id && p == path)
                    });
                    for (id, path) in config {
                        watches.entry(id).or_insert_with(|| {
                            let watch = Watch::open(&path);
                            (path, watch, Instant::now())
                        });
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            let now = Instant::now();
            for (id, (path, watch, attempted)) in &mut watches {
                if watch.is_none() && now.duration_since(*attempted) >= Duration::from_secs(30) {
                    *watch = Watch::open(path);
                    *attempted = now;
                    // 离线重连期间可能错过变动，主动补扫。
                    if watch.is_some() {
                        let _ = event_tx.send(id.clone());
                    }
                }
                if let Some(active) = watch {
                    match active.poll(now) {
                        Ok(true) => {
                            let _ = event_tx.send(id.clone());
                        }
                        Ok(false) => {}
                        Err(()) => {
                            *watch = None;
                            *attempted = now;
                        }
                    }
                }
            }
        }
    });
    tauri::async_runtime::spawn(async move {
        loop {
            // 及时同步新增／停用来源及自动扫描开关，关闭后丢弃尚未发出的变动。
            let config = if let Ok(operation) = super::super::operation(&app).await {
                let config = (|| {
                    let conn = store::open(&app.state::<State>().dir)?;
                    if store::setting(&conn, "autoScanIntervalMinutes")?
                        .as_u64()
                        .unwrap_or(30)
                        == 0
                    {
                        return Ok::<_, String>(Vec::new());
                    }
                    Ok(catalog::source_list(&conn)?
                        .into_iter()
                        .map(|source| {
                            (
                                store::text(&source, "id").to_owned(),
                                store::text(&source, "path").to_owned(),
                            )
                        })
                        .collect::<Vec<_>>())
                })()
                .unwrap_or_default();
                drop(operation);
                config
            } else {
                Vec::new()
            };
            if config_tx.send(config.clone()).is_err() {
                break;
            }
            while let Ok(id) = event_rx.try_recv() {
                if config.iter().any(|(source, _)| source == &id) {
                    // 扫描中的变动必须补扫；沿用已有同源去重与停用校验。
                    let _ = scan(app.clone(), json!({"sourceId":id,"afterCurrent":true})).await;
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn debounce_merges_bursts_and_bounds_continuous_writes() {
        let now = Instant::now();
        let mut pending = Pending::default();
        assert!(!pending.take_due(now));
        pending.changed(now);
        pending.changed(now + Duration::from_secs(1));
        assert!(!pending.take_due(now + Duration::from_secs(2)));
        assert!(pending.take_due(now + Duration::from_secs(3)));
        assert!(!pending.take_due(now + Duration::from_secs(20)));
        for seconds in 0..10 {
            pending.changed(now + Duration::from_secs(seconds));
        }
        assert!(pending.take_due(now + Duration::from_secs(10)));
    }
    #[test]
    fn native_watch_detects_nested_add_rename_write_and_remove() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("season");
        std::fs::create_dir(&child).unwrap();
        let mut watch = Watch::open(dir.path().to_str().unwrap()).unwrap();
        let file = child.join("one.mkv");
        let renamed = child.join("two.mkv");
        let changes: Vec<Box<dyn FnOnce()>> = vec![
            Box::new(|| std::fs::write(&file, b"video").unwrap()),
            Box::new(|| std::fs::rename(&file, &renamed).unwrap()),
            Box::new(|| std::fs::write(&renamed, b"changed video").unwrap()),
            Box::new(|| std::fs::remove_file(&renamed).unwrap()),
        ];
        for change in changes {
            change();
            let deadline = Instant::now() + Duration::from_secs(5);
            while unsafe { WaitForSingleObject(watch.handle, 0) } != WAIT_OBJECT_0 {
                assert!(Instant::now() < deadline, "未收到递归目录通知");
                std::thread::sleep(Duration::from_millis(10));
            }
            let now = Instant::now();
            assert!(!watch.poll(now).unwrap());
            assert!(watch.pending.take_due(now + Duration::from_secs(2)));
        }
    }
}
