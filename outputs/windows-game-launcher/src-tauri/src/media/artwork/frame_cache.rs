//! 只回收可重建的抽帧缓存，不触及游戏封面、远程图片或业务文件。
use std::{fs, path::Path, sync::{Mutex, OnceLock}, time::{Duration, Instant}};

const BUDGET: u64 = 512 * 1024 * 1024;

pub(super) fn maintain(root: &Path, current: &Path) {
    static LAST: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    let Ok(mut last) = LAST.get_or_init(|| Mutex::new(None)).lock() else { return };
    if last.is_some_and(|when| when.elapsed() < Duration::from_secs(60)) { return; }
    *last = Some(Instant::now());
    trim(root, current, BUDGET);
}

fn trim(root: &Path, current: &Path, budget: u64) {
    let Ok(entries) = fs::read_dir(root) else { return };
    let mut files = Vec::new();
    let mut total = 0u64;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(key) = name.strip_prefix("media-frame-").and_then(|v| v.strip_suffix(".jpg")) else { continue };
        if key.len() != 64 || !key.bytes().all(|v| v.is_ascii_hexdigit()) { continue; }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() || meta.is_symlink() { continue; }
        total += meta.len();
        files.push((meta.modified().ok(), entry.path(), meta.len()));
    }
    files.sort_by_key(|entry| entry.0);
    for (_, path, size) in files {
        if total <= budget { break; }
        if path != current && fs::remove_file(path).is_ok() { total = total.saturating_sub(size); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trim_only_generated_frames_and_protect_current() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join(format!("media-frame-{}.jpg", "a".repeat(64)));
        let current = dir.path().join(format!("media-frame-{}.jpg", "b".repeat(64)));
        for path in [&old, &current] { fs::write(path, [0; 10]).unwrap(); }
        fs::write(dir.path().join("media-frame-temporary.jpg"), [0; 10]).unwrap();
        fs::write(dir.path().join("media-remote.jpg"), [0; 10]).unwrap();
        trim(dir.path(), &current, 10);
        assert!(!old.exists());
        assert!(current.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 3);
    }
}
