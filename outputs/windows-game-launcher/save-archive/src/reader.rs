use crate::*;
use std::{collections::HashSet, fs::{self, File, OpenOptions}, io::{Read, Write}, path::Path};
use zip::ZipArchive;
fn process(path: &Path, destination: Option<&Path>) -> Result<Manifest, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_BYTES { return Err("归档超过 2 GiB".into()); }
    let mut zip = ZipArchive::new(file).map_err(|e| e.to_string())?;
    if zip.len() > MAX_FILES + 1 { return Err("归档条目过多".into()); }
    let manifest: Manifest = {
        let entry = zip.by_name(MANIFEST).map_err(|_| "缺少存档清单")?;
        if entry.size() > MANIFEST_LIMIT { return Err("存档清单过大".into()); }
        let mut bytes = Vec::new();
        entry.take(MANIFEST_LIMIT + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MANIFEST_LIMIT { return Err("存档清单过大".into()); }
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?
    };
    youji_cloud_protocol::saves::validate(&manifest)?;
    if zip.len() != manifest.files.len() + 1 { return Err("归档条目与清单不一致".into()); }
    let mut seen = HashSet::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().map_err(|e| e.to_string())?.into_owned();
        if !seen.insert(name.clone()) || entry.is_dir() || entry.encrypted() { return Err("归档包含重复目录或加密条目".into()); }
        if let Some(mode) = entry.unix_mode() {
            if mode & 0o170000 != 0 && mode & 0o170000 != 0o100000 { return Err("归档包含非普通文件".into()); }
        }
        if name == MANIFEST { continue; }
        let relative = name.strip_prefix("files/").ok_or("无效归档条目")?;
        let expected = manifest.files.get(relative).ok_or("归档包含清单之外的文件")?;
        if entry.size() != expected.bytes { return Err("文件大小与清单不符".into()); }
        let mut output = if let Some(root) = destination {
            let target = root.join(relative);
            let parent = target.parent().ok_or("无效目标路径")?;
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            check_tree_path(parent)?;
            Some(OpenOptions::new().write(true).create_new(true).open(target).map_err(|e| e.to_string())?)
        } else { None };
        let mut hash = Sha256::new();
        let mut total = 0u64;
        let mut buf = [0; 65536];
        loop {
            let count = entry.read(&mut buf).map_err(|e| e.to_string())?;
            if count == 0 { break; }
            total += count as u64;
            if total > expected.bytes { return Err("文件解压大小超限".into()); }
            hash.update(&buf[..count]);
            if let Some(file) = &mut output { file.write_all(&buf[..count]).map_err(|e| e.to_string())?; }
        }
        if total != expected.bytes || format!("{:x}", hash.finalize()) != expected.sha256 { return Err("存档文件校验失败".into()); }
        if let Some(file) = output { file.sync_all().map_err(|e| e.to_string())?; }
    }
    Ok(manifest)
}
pub fn verify(path: &Path) -> Result<Manifest, String> { process(path, None) }
pub fn extract(path: &Path, destination: &Path) -> Result<Manifest, String> {
    // 不向已有目录解压，避免链接攻击或与旧文件混合。
    verify(path)?;
    check_tree_path(destination.parent().ok_or("解压目标缺少父目录")?)?;
    fs::create_dir(destination).map_err(|e| e.to_string())?;
    process(path, Some(destination))
}
