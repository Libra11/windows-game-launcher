use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;
pub const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_FILES: usize = 50_000;
pub const DEFAULT_QUOTA: u64 = 10 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileRecord { pub bytes: u64, pub sha256: String }
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest { pub version: u32, pub files: BTreeMap<String, FileRecord> }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UploadRequest {
    pub snapshot_id: Uuid, pub profile_id: Uuid, pub parent_ids: Vec<Uuid>,
    pub base_revision: i64, pub manifest_hash: String, pub archive_hash: String, pub bytes: u64,
}

pub fn valid_hash(s: &str) -> bool { s.len()==64 && s.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) }
pub fn valid_path(path: &str) -> bool {
    !path.is_empty() && path.len() <= 2048 && !path.contains('\\') && path.split('/').all(|part| {
        !part.is_empty() && part != "." && part != ".." && !part.ends_with([' ', '.']) && !part.chars().any(|c| c.is_control() || "<>:\"|?*".contains(c))
            && !matches!(part.split('.').next().unwrap_or("").to_uppercase().as_str(), "CON"|"PRN"|"AUX"|"NUL"|"COM1"|"COM2"|"COM3"|"COM4"|"COM5"|"COM6"|"COM7"|"COM8"|"COM9"|"LPT1"|"LPT2"|"LPT3"|"LPT4"|"LPT5"|"LPT6"|"LPT7"|"LPT8"|"LPT9")
    })
}
pub fn validate(manifest: &Manifest) -> Result<u64,String> {
    if manifest.version!=1 || manifest.files.len()>MAX_FILES { return Err("存档格式或文件数量无效".into()); }
    let mut names=std::collections::HashSet::new(); let mut total=0u64;
    for (path,file) in &manifest.files {
        if !valid_path(path) || !valid_hash(&file.sha256) || !names.insert(path.to_lowercase()) { return Err("存档包含无效路径、大小写冲突或校验值".into()); }
        total=total.checked_add(file.bytes).filter(|n| *n<=MAX_BYTES).ok_or("存档超过 2 GiB")?;
    }
    for path in &names {
        let mut parent=path.as_str();
        while let Some((prefix,_))=parent.rsplit_once('/') { if names.contains(prefix) { return Err("存档文件与目录冲突".into()); } parent=prefix; }
    }
    Ok(total)
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn windows_paths_are_strict() {
        for bad in ["../a","C:/a","a\\b","/a","a:stream","CON.txt","a/../b","a./b"] { assert!(!valid_path(bad),"{bad}"); }
        assert!(valid_path("存档/profile/save.dat"));
    }
    #[test] fn file_directory_collisions_rejected() {
        let record=FileRecord{bytes:0,sha256:"a".repeat(64)};
        let m=Manifest{version:1,files:[("A".into(),record.clone()),("a/b".into(),record)].into()};
        assert!(validate(&m).is_err());
    }
}
