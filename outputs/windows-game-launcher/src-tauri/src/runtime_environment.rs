use crate::model::Game;
use std::path::Path;

pub fn missing_record_status(game: &Game) -> String {
    let Some(folder) = Path::new(&game.exe_path).parent() else {
        return "未找到解锁记录文件".into();
    };
    if folder.join("MicrosoftGame.Config").is_file() && folder.join("GDKExtension.dll").is_file() {
        let mut message = "当前无法自动检测：检测到 Xbox/GDK 游戏文件，但未找到受支持的真实成就事件记录；关联 Steam 仅补全资料，不会转换成就机制".to_string();
        if let Ok(id) = std::fs::read_to_string(folder.join("steam_settings/steam_appid.txt")) {
            let id = id.trim();
            if !game.appid.is_empty()
                && !id.is_empty()
                && id.bytes().all(|b| b.is_ascii_digit())
                && id != game.appid
            {
                message.push_str(&format!(
                    "。目录中的 Steam 标识 {id} 与资料 AppID {} 不一致",
                    game.appid
                ));
            }
        }
        message
    } else {
        "未找到解锁记录文件；只有游戏或运行环境写出受支持的真实记录，才能自动检测".into()
    }
}
