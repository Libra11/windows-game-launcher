use crate::*;
use std::{collections::BTreeMap, fs, io::{Read, Write}, path::{Path, PathBuf}};
use zip::{write::SimpleFileOptions, ZipWriter};
fn scan(root: &Path) -> Result<Manifest, String> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    let mut visited = 0usize;
    let mut total = 0u64;
    while let Some(dir) = pending.pop() {
        check_tree_path(&dir)?;
        for item in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let path = item.map_err(|e| e.to_string())?.path();
            visited += 1;
            if visited > MAX_FILES * 2 { return Err("目录条目过多".into()); }
            check_tree_path(&path)?;
            let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if meta.is_dir() { pending.push(path); continue; }
            if !meta.is_file() { return Err("存档中包含非普通文件".into()); }
            total = total.checked_add(meta.len()).filter(|n| *n <= MAX_BYTES).ok_or("存档超过 2 GiB")?;
            if files.len() >= MAX_FILES { return Err("存档文件过多".into()); }
            let name = path.strip_prefix(root).map_err(|e| e.to_string())?.to_str().ok_or("存档路径不是 UTF-8")?.replace('\\', "/");
            files.insert(name, FileRecord { bytes: meta.len(), sha256: file_hash(&path)? });
        }
    }
    let manifest = Manifest { version: 1, files };
    youji_cloud_protocol::saves::validate(&manifest)?;
    Ok(manifest)
}
pub fn create(root: &Path, destination: &Path) -> Result<Manifest, String> {
    let root = validate_root(root)?;
    let parent = destination.parent().ok_or("归档目标缺少父目录")?;
    check_tree_path(parent)?;
    let parent = parent.canonicalize().map_err(|e| e.to_string())?;
    if parent.starts_with(&root) { return Err("归档不能写入被备份的目录".into()); }
    if destination.exists() { return Err("归档目标已存在".into()); }
    // 每次重试均使用独立临时文件；只有两次源扫描与归档校验一致才发布。
    let mut failure = String::new();
    for _ in 0..3 {
        match attempt(&root, &parent, destination) {
            Ok(manifest) => return Ok(manifest),
            Err(error) => failure = error,
        }
    }
    Err(format!("存档采集失败，请退出游戏及其他存档工具后重试：{failure}"))
}
fn attempt(root: &Path, parent: &Path, destination: &Path) -> Result<Manifest,String> {
        let before = scan(root)?;
        let mut temp = tempfile::NamedTempFile::new_in(&parent).map_err(|e| e.to_string())?;
        {
            let mut zip = ZipWriter::new(temp.as_file_mut());
            let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).unix_permissions(0o600);
            zip.start_file(MANIFEST, options).map_err(|e| e.to_string())?;
            zip.write_all(&serde_json::to_vec(&before).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            for (name, record) in &before.files {
                let source: PathBuf = root.join(name);
                check_tree_path(&source)?;
                zip.start_file(format!("files/{name}"), options).map_err(|e| e.to_string())?;
                let mut file = File::open(source).map_err(|e| e.to_string())?.take(record.bytes + 1);
                let copied = std::io::copy(&mut file, &mut zip).map_err(|e| e.to_string())?;
                if copied > record.bytes { break; }
            }
            zip.finish().map_err(|e| e.to_string())?;
        }
        if temp.as_file().metadata().map_err(|e| e.to_string())?.len() > MAX_BYTES { return Err("压缩归档超过 2 GiB".into()); }
        if verify(temp.path()).ok().as_ref() == Some(&before) && scan(&root)? == before {
            temp.as_file_mut().sync_all().map_err(|e| e.to_string())?;
            temp.persist_noclobber(destination).map_err(|e| e.to_string())?;
            return Ok(before);
        }
    Err("存档在读取期间发生变化".into())
}
