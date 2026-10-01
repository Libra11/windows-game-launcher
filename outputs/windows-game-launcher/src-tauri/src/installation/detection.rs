use super::{Info, Status};
use crate::model::Game;
use std::{
    collections::HashMap,
    fs,
    io::ErrorKind,
    path::{Component, Path},
};
use steamlocate::{app::StateFlag, Library, SteamDir};

#[cfg(any(windows, test))]
pub(super) fn client_at(path: Option<&Path>) -> Result<SteamDir, Info> {
    let Some(path) = path else {
        return Err(Info::new(
            Status::ClientMissing,
            "此电脑尚未安装 Steam 客户端",
        ));
    };
    let exe = path.join("steam.exe");
    match fs::metadata(&exe) {
        Ok(metadata) if metadata.is_file() => SteamDir::from_dir(path)
            .map_err(|error| Info::unknown(format!("无法读取 Steam 安装目录：{error}"))),
        Ok(_) => Err(Info::unknown(format!(
            "Steam 客户端路径不是文件：{}",
            exe.display()
        ))),
        Err(error) if error.kind() == ErrorKind::NotFound => Err(Info::new(
            Status::ClientMissing,
            format!("未找到 Steam 客户端：{}", exe.display()),
        )),
        Err(error) => Err(Info::unknown(format!(
            "无法访问 Steam 客户端 {}：{error}",
            exe.display()
        ))),
    }
}

pub(super) fn scan(games: &[Game], steam: Result<SteamDir, Info>) -> HashMap<String, Info> {
    let games: Vec<_> = games.iter().filter(|game| game.source == "steam").collect();
    if games.is_empty() {
        return HashMap::new();
    }
    let libraries = steam.and_then(|steam| {
        let mut paths = steam
            .library_paths()
            .map_err(|error| Info::unknown(format!("无法读取 Steam 游戏库列表：{error}")))?;
        if !paths.iter().any(|path| path == steam.path()) {
            paths.push(steam.path().to_owned());
        }
        paths.sort();
        paths.dedup();
        Ok(paths
            .into_iter()
            .map(|path| {
                Library::from_dir(&path).map_err(|error| format!("无法访问 Steam 游戏库：{error}"))
            })
            .collect::<Vec<_>>())
    });
    games
        .into_iter()
        .map(|game| {
            let info = match &libraries {
                Ok(libraries) => detect_game(&game.appid, libraries),
                Err(info) => info.clone(),
            };
            (game.id.clone(), info)
        })
        .collect()
}

pub(super) fn detect_game(appid: &str, libraries: &[Result<Library, String>]) -> Info {
    let Ok(appid) = appid.parse::<u32>() else {
        return Info::unknown("Steam AppID 无效");
    };
    let mut uncertain = None;
    let mut absent = Info::new(Status::NotInstalled, "此游戏尚未在本机 Steam 游戏库中安装");
    for library in libraries {
        let library = match library {
            Ok(library) => library,
            Err(error) => {
                uncertain.get_or_insert_with(|| Info::unknown(error));
                continue;
            }
        };
        let Some(app) = library.app(appid) else {
            continue;
        };
        let app = match app {
            Ok(app) if app.app_id == appid => app,
            Ok(_) => {
                uncertain = Some(Info::unknown("安装清单的 AppID 与游戏不一致"));
                continue;
            }
            Err(error) => {
                uncertain = Some(Info::unknown(format!("无法读取游戏安装清单：{error}")));
                continue;
            }
        };
        let Some(flags) = app.state_flags else {
            uncertain = Some(Info::unknown("游戏安装清单缺少安装完成标记"));
            continue;
        };
        let flags: Vec<_> = flags.flags().collect();
        let incomplete = flags.iter().any(|flag| {
            matches!(
                flag,
                StateFlag::Uninstalled
                    | StateFlag::Encrypted
                    | StateFlag::Locked
                    | StateFlag::FilesMissing
                    | StateFlag::FilesCorrupt
                    | StateFlag::Uninstalling
                    | StateFlag::UpdateRunning
                    | StateFlag::UpdatePaused
                    | StateFlag::UpdateStarted
                    | StateFlag::Reconfiguring
                    | StateFlag::Validating
                    | StateFlag::AddingFiles
                    | StateFlag::Preallocating
                    | StateFlag::Downloading
                    | StateFlag::Staging
                    | StateFlag::Committing
                    | StateFlag::UpdateStopping
            )
        });
        if !flags.contains(&StateFlag::FullyInstalled) || incomplete {
            absent = Info::new(
                Status::NotInstalled,
                "安装尚未完成，请在 Steam 中完成安装或修复",
            );
            continue;
        }
        if app.install_dir.is_empty()
            || !Path::new(&app.install_dir)
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        {
            uncertain = Some(Info::unknown("游戏安装清单中的目录无效"));
            continue;
        }
        let path = library.resolve_app_dir(&app);
        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_dir() => match fs::read_dir(&path) {
                Ok(_) => {
                    return Info::new(Status::Installed, "已确认本机安装，可以通过 Steam 启动")
                }
                Err(error) => {
                    uncertain = Some(Info::unknown(format!(
                        "无法访问游戏目录 {}：{error}",
                        path.display()
                    )))
                }
            },
            Ok(_) => {
                uncertain = Some(Info::unknown(format!(
                    "游戏安装路径不是目录：{}",
                    path.display()
                )))
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                absent = Info::new(
                    Status::NotInstalled,
                    format!("游戏安装目录缺失：{}", path.display()),
                )
            }
            Err(error) => {
                uncertain = Some(Info::unknown(format!(
                    "无法访问游戏目录 {}：{error}",
                    path.display()
                )))
            }
        }
    }
    // 任何不可读的库都可能包含这款游戏，不能将未找到误判为未安装。
    uncertain.unwrap_or(absent)
}
