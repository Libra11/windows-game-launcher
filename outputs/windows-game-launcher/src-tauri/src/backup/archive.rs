use super::{model::*, schema};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::{BTreeMap, HashSet}, fs::File, io::{Read, Write}, path::Path};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

pub struct Loaded {
    pub directory: tempfile::TempDir,
    pub manifest: Manifest,
    pub data: Dataset,
    pub preferences: Preferences,
}
fn err(error: impl std::fmt::Display) -> String { format!("备份文件处理失败：{error}") }
fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
pub fn file_hash(path: &Path) -> Result<String, String> {
    file_hash_with(path, || Ok(()))
}
pub fn file_hash_with(path: &Path, mut check: impl FnMut() -> Result<(), String>) -> Result<String, String> {
    let meta = std::fs::symlink_metadata(path).map_err(err)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_TOTAL + 16 * 1024 * 1024 { return Err("备份文件类型或大小无效".into()); }
    let mut file = File::open(path).map_err(err)?;
    let mut hash = Sha256::new(); let mut buffer = [0; 64 * 1024];
    loop { check()?; let read = file.read(&mut buffer).map_err(err)?; if read == 0 { break; } hash.update(&buffer[..read]); }
    Ok(format!("{:x}", hash.finalize()))
}
fn valid_name(name: &str) -> bool {
    ["manifest.json","data.json","preferences.json"].contains(&name)
        || name.strip_prefix("covers/").is_some_and(schema::cover_name)
}
fn limit(name: &str) -> u64 {
    match name { "manifest.json" => 1024 * 1024, "preferences.json" => 64 * 1024, "data.json" => MAX_DATA, _ => MAX_COVER }
}
fn cover(bytes: &[u8], name: &str) -> Result<(), String> {
    if crate::cover_download::extension(bytes) != name.rsplit_once('.').map(|(_, ext)| ext) {
        return Err("备份包含无效封面文件".into());
    }
    Ok(())
}

pub fn write(
    destination: &Path, mut data: Dataset, preferences: Preferences, covers: &Path,
    version: String, mut progress: impl FnMut(&str, usize, usize) -> Result<(), String>,
) -> Result<Manifest, String> {
    schema::validate(&data, &preferences)?;
    let parent = destination.parent().filter(|path| path.is_dir()).ok_or("备份目标目录不存在")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(err)?;
    let mut records = BTreeMap::new(); let mut missing = Vec::new(); let mut total = 0u64;
    let cover_keys: Vec<_> = data.settings.keys().filter(|key| key.starts_with("cover_cache:")).cloned().collect();
    let mut seen = HashSet::new();
    {
        let mut writer = ZipWriter::new(temporary.as_file_mut());
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored).large_file(true);
        for (index, key) in cover_keys.iter().enumerate() {
            progress("covers", index, cover_keys.len())?;
            let info: Value = serde_json::from_str(&data.settings[key]).map_err(err)?;
            let name = info["file"].as_str().ok_or("封面索引缺少文件名")?;
            let path = covers.join(name);
            let usable = std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= MAX_COVER);
            if !usable { missing.push(key.clone()); data.settings.remove(key); continue; }
            let bytes = std::fs::read(path).map_err(err)?;
            if bytes.len() as u64 > MAX_COVER { return Err("封面在备份期间发生变化或超过大小限制".into()); }
            if cover(&bytes, name).is_err() { missing.push(key.clone()); data.settings.remove(key); continue; }
            let entry = format!("covers/{name}");
            if !seen.insert(entry.clone()) { continue; }
            total = total.checked_add(bytes.len() as u64).filter(|size| *size <= MAX_TOTAL).ok_or("备份内容超过 10 GiB")?;
            writer.start_file(&entry, options).map_err(err)?; writer.write_all(&bytes).map_err(err)?;
            records.insert(entry, FileRecord { size:bytes.len() as u64, sha256:digest(&bytes) });
        }
        progress("data", 0, 2)?;
        for (name, bytes) in [("data.json", serde_json::to_vec(&data).map_err(err)?), ("preferences.json", serde_json::to_vec(&preferences).map_err(err)?)] {
            if bytes.len() as u64 > limit(name) { return Err("备份数据超过格式限制".into()); }
            total = total.checked_add(bytes.len() as u64).filter(|size| *size <= MAX_TOTAL).ok_or("备份内容超过 10 GiB")?;
            writer.start_file(name, options.compression_method(CompressionMethod::Deflated)).map_err(err)?;
            writer.write_all(&bytes).map_err(err)?;
            records.insert(name.into(), FileRecord { size:bytes.len() as u64, sha256:digest(&bytes) });
        }
        let manifest = Manifest { format:"youji-backup".into(), format_version:2, app_version:version,
            created_at:data.snapshot_at.clone(), counts:schema::counts(&data, seen.len()), files:records, missing_covers:missing };
        let bytes = serde_json::to_vec(&manifest).map_err(err)?;
        if bytes.len() > 1024 * 1024 || manifest.files.len() + 1 > MAX_ENTRIES { return Err("备份清单过大".into()); }
        writer.start_file("manifest.json", options).map_err(err)?; writer.write_all(&bytes).map_err(err)?;
        writer.finish().map_err(err)?.sync_all().map_err(err)?;
    }
    progress("verify", 0, 1)?;
    let verified = read_with_progress(temporary.path(), |done, total| progress("verify", done, total))?;
    let manifest = verified.manifest.clone();
    drop(verified);
    progress("commit", 0, 1)?;
    temporary.persist(destination).map_err(|error| err(error.error))?;
    let _ = progress("complete", 1, 1);
    Ok(manifest)
}

pub fn read(path: &Path) -> Result<Loaded, String> {
    read_with_progress(path, |_, _| Ok(()))
}
pub fn read_with_progress(path: &Path, mut progress: impl FnMut(usize, usize) -> Result<(), String>) -> Result<Loaded, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(err)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_TOTAL + 16 * 1024 * 1024 { return Err("备份文件类型或大小无效".into()); }
    let mut zip = ZipArchive::new(File::open(path).map_err(err)?).map_err(err)?;
    if zip.len() > MAX_ENTRIES { return Err("备份文件条目过多".into()); }
    let directory = tempfile::tempdir().map_err(err)?;
    std::fs::create_dir(directory.path().join("covers")).map_err(err)?;
    let mut records = BTreeMap::new(); let mut names = HashSet::new(); let mut total = 0u64;
    for index in 0..zip.len() {
        progress(index, zip.len())?;
        let mut file = zip.by_index(index).map_err(err)?;
        let name = file.name().map_err(err)?.into_owned();
        let kind = file.unix_mode().unwrap_or(0) & 0o170000;
        if !valid_name(&name) || !names.insert(name.clone()) || file.is_dir() || ![0, 0o100000].contains(&kind) {
            return Err("备份包含越界路径、重复文件、链接或不支持的条目".into());
        }
        let size = file.size();
        total = total.checked_add(size).filter(|size| *size <= MAX_TOTAL).ok_or("备份解压内容超过 10 GiB")?;
        if size > limit(&name) { return Err("备份条目超过大小限制".into()); }
        let mut bytes = Vec::new();
        (&mut file).take(limit(&name) + 1).read_to_end(&mut bytes).map_err(err)?;
        if bytes.len() as u64 != size { return Err("备份条目大小不一致".into()); }
        if name.starts_with("covers/") { cover(&bytes, &name)?; }
        records.insert(name.clone(), FileRecord { size, sha256:digest(&bytes) });
        std::fs::write(directory.path().join(&name), bytes).map_err(err)?;
    }
    let manifest: Manifest = serde_json::from_reader(File::open(directory.path().join("manifest.json")).map_err(err)?).map_err(err)?;
    if manifest.format != "youji-backup" || manifest.format_version != 2 { return Err("不支持此备份格式版本".into()); }
    records.remove("manifest.json");
    if manifest.files.len() != records.len() || manifest.files.iter().any(|(name, expected)| {
        records.get(name).is_none_or(|actual| actual.size != expected.size || actual.sha256 != expected.sha256)
    }) { return Err("备份校验失败，文件可能损坏或被修改".into()); }
    let data: Dataset = serde_json::from_reader(File::open(directory.path().join("data.json")).map_err(err)?).map_err(err)?;
    let preferences: Preferences = serde_json::from_reader(File::open(directory.path().join("preferences.json")).map_err(err)?).map_err(err)?;
    schema::validate(&data, &preferences)?;
    if manifest.created_at != data.snapshot_at { return Err("备份快照时间与清单不一致".into()); }
    for (key, text) in &data.settings {
        if key.starts_with("cover_cache:") {
            let info: Value = serde_json::from_str(text).map_err(err)?;
            if !manifest.files.contains_key(&format!("covers/{}", info["file"].as_str().unwrap())) { return Err("封面索引引用了备份中不存在的文件".into()); }
        }
    }
    let actual = schema::counts(&data, records.keys().filter(|name| name.starts_with("covers/")).count());
    if serde_json::to_value(&actual).map_err(err)? != serde_json::to_value(&manifest.counts).map_err(err)? { return Err("备份统计清单与内容不一致".into()); }
    Ok(Loaded { directory, manifest, data, preferences })
}

pub fn atomic_json(path: &Path, value: &impl serde::Serialize) -> Result<(), String> {
    let parent = path.parent().ok_or("备份工作目录无效")?;
    std::fs::create_dir_all(parent).map_err(err)?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(err)?;
    serde_json::to_writer(file.as_file_mut(), value).map_err(err)?;
    file.as_file().sync_all().map_err(err)?;
    file.persist(path).map_err(|error| err(error.error))?;
    Ok(())
}

pub fn copy_verified(source: &Path, destination: &Path, expected: &str) -> Result<(), String> {
    let parent = destination.parent().ok_or("恢复目录无效")?;
    std::fs::create_dir_all(parent).map_err(err)?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(err)?;
    std::io::copy(&mut File::open(source).map_err(err)?, file.as_file_mut()).map_err(err)?;
    file.as_file().sync_all().map_err(err)?;
    if file_hash(file.path())? != expected { return Err("备份文件已变更，请重新选择并预览".into()); }
    file.persist(destination).map_err(|error| err(error.error))?;
    Ok(())
}
