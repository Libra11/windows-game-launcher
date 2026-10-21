// 使用真实客户端恢复与存储源码，在临时目录验证崩溃窗口；不启动 Tauri。
#[path = "../../src-tauri/src/saves/restore.rs"]
mod restore;
#[path = "../../src-tauri/src/saves/retention.rs"]
mod retention;
#[path = "../../src-tauri/src/saves/store.rs"]
mod store;
#[path = "../../src-tauri/src/saves/tasks.rs"]
mod tasks;
