use crate::{db, lock_db, AppState};
use serde::Serialize;
use std::path::Path;
use tauri::Manager;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProgramImport {
    pub exe_path: String,
    pub title: String,
    pub existing_game_id: Option<String>,
}

pub(crate) fn same_program(left: &str, right: &str) -> bool {
    left.replace('/', "\\")
        .eq_ignore_ascii_case(&right.replace('/', "\\"))
}

pub(crate) fn resolve(path: &Path) -> Result<ProgramImport, String> {
    if !path.is_file() {
        return Err("文件不存在或无法访问，请拖入游戏程序或快捷方式".into());
    }
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let target = if extension.eq_ignore_ascii_case("lnk") {
        shortcut(path)?
    } else if extension.eq_ignore_ascii_case("exe") {
        path.to_string_lossy().into_owned()
    } else {
        return Err("目前支持 .exe 游戏程序和 .lnk 快捷方式".into());
    };
    let target_path = Path::new(&target);
    if !target_path
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("exe"))
        || !target_path.is_file()
    {
        return Err("快捷方式没有指向可访问的 .exe 程序，请拖入实际游戏程序".into());
    }
    let title = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .trim_end_matches(" - 快捷方式")
        .trim_end_matches(".exe")
        .to_owned();
    Ok(ProgramImport {
        exe_path: target,
        title,
        existing_game_id: None,
    })
}

#[cfg(windows)]
fn shortcut(path: &Path) -> Result<String, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::{Interface, PCWSTR},
        Win32::{
            Foundation::RPC_E_CHANGED_MODE,
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile,
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, STGM_READ,
            },
            UI::Shell::{IShellLinkW, ShellLink},
        },
    };
    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() }
            }
        }
    }
    let decode = |buffer: &[u16]| {
        String::from_utf16_lossy(
            &buffer[..buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len())],
        )
    };
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_MULTITHREADED);
        // WebView 的 UI 线程可能已有 STA；复用其 COM 环境，不解除其他组件的初始化。
        let _guard = if initialized == RPC_E_CHANGED_MODE {
            ComGuard(false)
        } else {
            initialized
                .ok()
                .map_err(|e| format!("无法读取快捷方式：{e}"))?;
            ComGuard(true)
        };
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| format!("无法读取快捷方式：{e}"))?;
        let file: IPersistFile = link.cast().map_err(|e| e.to_string())?;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        file.Load(PCWSTR(wide.as_ptr()), STGM_READ)
            .map_err(|_| "快捷方式无法解析，请拖入实际游戏程序")?;
        let mut target = vec![0; 32768];
        link.GetPath(&mut target, std::ptr::null_mut(), 0)
            .map_err(|e| e.to_string())?;
        Ok(decode(&target))
    }
}

#[cfg(not(windows))]
fn shortcut(_: &Path) -> Result<String, String> {
    Err("Windows 快捷方式仅能在 Windows 桌面应用中导入".into())
}

#[tauri::command]
pub(crate) async fn prepare_local_import(
    app: tauri::AppHandle,
    path: String,
) -> Result<ProgramImport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut candidate = resolve(Path::new(&path))?;
        let state = app.state::<AppState>();
        candidate.existing_game_id = db::games(&*lock_db(&state)?)?
            .into_iter()
            .find(|game| {
                game.source == "local" && same_program(&game.exe_path, &candidate.exe_path)
            })
            .map(|game| game.id);
        Ok(candidate)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("program-import-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn imports_exe_without_running_it_and_rejects_other_files() {
        let fixture = Fixture::new();
        let exe = fixture.0.join("游戏.EXE");
        std::fs::write(&exe, b"not executed").unwrap();
        let item = resolve(&exe).unwrap();
        assert_eq!(item.exe_path, exe.to_string_lossy());
        assert_eq!(item.title, "游戏");
        assert!(resolve(&fixture.0).is_err());
        let text = fixture.0.join("game.url");
        std::fs::write(&text, b"[InternetShortcut]").unwrap();
        assert!(resolve(&text).is_err());
        assert!(resolve(&fixture.0.join("missing.exe")).is_err());
        assert!(same_program("C:/Games/Game.exe", "c:\\games\\GAME.EXE"));
    }

    #[cfg(windows)]
    #[test]
    fn resolves_unicode_shortcut_to_exe_without_retaining_launch_settings() {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            core::{Interface, PCWSTR},
            Win32::{
                System::Com::{
                    CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile,
                    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
                },
                UI::Shell::{IShellLinkW, ShellLink},
            },
        };
        let fixture = Fixture::new();
        let exe = fixture.0.join("实际 游戏.exe");
        std::fs::write(&exe, b"not executed").unwrap();
        let shortcut = fixture.0.join("测试游戏.lnk");
        let wide = |path: &Path| {
            path.as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>()
        };
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok().unwrap();
            {
                let link: IShellLinkW =
                    CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
                link.SetPath(PCWSTR(wide(&exe).as_ptr())).unwrap();
                let args: Vec<u16> = "--profile \"中文 空格\""
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                link.SetArguments(PCWSTR(args.as_ptr())).unwrap();
                link.SetWorkingDirectory(PCWSTR(wide(&fixture.0).as_ptr()))
                    .unwrap();
                let persist: IPersistFile = link.cast().unwrap();
                persist
                    .Save(PCWSTR(wide(&shortcut).as_ptr()), true)
                    .unwrap();
            }
            CoUninitialize();
        }
        let imported = resolve(&shortcut).unwrap();
        assert!(same_program(&imported.exe_path, &exe.to_string_lossy()));
        assert_eq!(imported.title, "测试游戏");
        let payload = serde_json::to_value(&imported).unwrap();
        assert!(payload.get("shortcutPath").is_none());
        assert!(payload.get("arguments").is_none());
        assert!(payload.get("workingDirectory").is_none());
        let sta_shortcut = shortcut.clone();
        std::thread::spawn(move || unsafe {
            use windows::Win32::System::Com::COINIT_APARTMENTTHREADED;
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
            assert!(resolve(&sta_shortcut).is_ok());
            CoUninitialize();
        })
        .join()
        .unwrap();
        std::fs::remove_file(&exe).unwrap();
        assert!(resolve(&shortcut).is_err());
        std::fs::write(&exe, b"not executed").unwrap();
        std::fs::remove_file(&shortcut).unwrap();
        assert_eq!(
            resolve(Path::new(&imported.exe_path)).unwrap().exe_path,
            imported.exe_path
        );
        std::fs::write(&shortcut, b"broken link").unwrap();
        assert!(resolve(&shortcut).is_err());
    }
}
