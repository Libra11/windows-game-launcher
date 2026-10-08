// 发布版是图形应用，避免额外终端窗口干扰游戏焦点。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    local_achievement_launcher_lib::run();
}
