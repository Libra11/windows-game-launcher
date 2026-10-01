use super::{detection::client_at, Info, Status};
use std::{io::ErrorKind, path::PathBuf};
use steamlocate::SteamDir;
use winreg::{
    enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ},
    RegKey,
};

pub(super) fn locate() -> Result<SteamDir, Info> {
    let mut uncertain = None;
    let mut absent = None;
    if let Ok(steam) = steamlocate::locate() {
        match client_at(Some(steam.path())) {
            Ok(steam) => return Ok(steam),
            Err(info) if info.state == Status::Unknown => uncertain = Some(info),
            Err(info) => absent = Some(info),
        }
    }
    // steamlocate 的定位错误未公开 I/O 类型；用只读注册表检查区分缺失与权限错误。
    // 同时处理仅当前用户注册的 Steam 安装。
    for (hive, key, value) in [
        (HKEY_CURRENT_USER, "SOFTWARE\\Valve\\Steam", "SteamPath"),
        (
            HKEY_LOCAL_MACHINE,
            "SOFTWARE\\Wow6432Node\\Valve\\Steam",
            "InstallPath",
        ),
        (HKEY_LOCAL_MACHINE, "SOFTWARE\\Valve\\Steam", "InstallPath"),
    ] {
        let path = RegKey::predef(hive)
            .open_subkey_with_flags(key, KEY_READ)
            .and_then(|key| key.get_value::<String, _>(value));
        match path {
            Ok(path) if !path.trim().is_empty() => match client_at(Some(&PathBuf::from(path))) {
                Ok(steam) => return Ok(steam),
                Err(info) if info.state == Status::Unknown => uncertain = Some(info),
                Err(info) => absent = Some(info),
            },
            Err(error) if error.kind() != ErrorKind::NotFound => {
                uncertain = Some(Info::unknown(format!(
                    "无法读取 Steam 安装注册信息：{error}"
                )))
            }
            _ => {}
        }
    }
    Err(uncertain
        .or(absent)
        .unwrap_or_else(|| client_at(None).unwrap_err()))
}
