# 项目交接状态

记录日期：2026-10-01。此文记录已经完成的工作，连接和部署步骤见 `HANDHELD-OPERATIONS.md`。新会话应以源码、实时日志和掌机状态核实后续问题，不重复从零搭建环境。

## 目录与技术栈

- 完整工作目录：`/Volumes/T9/Projects/LibraSpace/windows-game-launcher`。
- 主源码：`outputs/windows-game-launcher/`；构建副本：`work/windows-build/project/`。
- Tauri 2、Rust、Vite 8.3.1、原生 JavaScript、SQLite，当前应用版本 `0.2.1`。
- Windows 优先，中文优先；用户希望开发时不要自动生成安装包。
- 旧目录 `/Users/libra/Documents/Codex/2026-09-28/wo-x` 只保留 `outputs`、`work` 指向 T9 的链接。迁移已逐文件校验：53,845 个文件，57,104 个条目。数据与依赖均在 T9，当前会话的项目归属不会因文件搬迁自动改变。
- 构建容器使用名称 `codex-launcher-windows-t9`；迁移元数据在 `work/windows-build/migration.json`。Docker inspect 的 `/app` Source 曾显示旧路径，但已用仅在 T9 创建的文件验证真实挂载是 T9；不因显示旧路径而重复迁移。

## 已实现的主要功能

- Steam 游戏库通过官方 API Key 导入；本机目录仅用于 Windows 安装状态判断。
- 本地 `.exe` 添加、名称模糊搜索与资料关联、Steam／Xbox 平台身份检查及明确确认。
- 中文资料与封面、详情页、收藏、最近游玩、亮暗主题、大屏多行网格及手柄操作。
- Windows 自定义可拖拽标题栏、启动与重复启动保护、本地时长记录、Steam 时长读取、本地游戏移除。
- Steam 官方成就与本地解锁分开；历史导入不补发通知，真实新事件才通知；网络失败保留离线缓存。
- GSE／Goldberg JSON 与 RUNE／CODEX INI 通用读取；GRIME 存档读取是独立游戏适配，不能宣称适用于所有游戏。
- Xbox 公开定义无需 Azure 注册或登录；Xbox 本地事件捕获目前只验证指定版本《黯井微光》，没有实现所有 Xbox 游戏通用自动解锁。

## 最近修复：GRIME II 本地 INI 成就

用户游戏目录为 `C:/Games/GRIME/GRIME II`。正确 Steam 资料 AppID 是 **2529790**，本地运行环境配置编号为 **2456740**。用户已明确确认资料关联，未修改游戏程序或 `steam_emu.ini`。

真实记录路径：

```text
C:/Users/Public/Documents/Steam/RUNE/2456740/achievements.ini
```

已缓存 40 项中文成就，最后实机确认 **1/40**：`APPETIZER`，中文「开胃菜」，条件「破除杂草丛生的屏障」，记录时间为 2026-10-01 12:03:48（北京时间）。这是游戏写出的 `Achieved=1` 和 `UnlockTime`，不是推算存档。

通用修复文件：`src-tauri/src/record_paths.rs`、`runtime_record.rs`、`scanner.rs`、`detection.rs` 和 `scanner_tests.rs`，真实样例在 `src-tauri/fixtures/runtime-rune.ini`。

- INI 仅采纳成就分区 `Achieved=1`，忽略进度数字和 `SteamAchievements` 索引。
- INI 导入需完整成就定义，并验证真实解锁标识属于当前游戏。
- 资料 AppID 与运行环境编号不同，仅在已确认关联且当前配置仍一致时查找备用路径；错误游戏记录会拒绝导入。
- 检测状态变化会发送 `library-changed`，读取器和游戏记录不会改写游戏配置。
- 上轮 Rust 测试 48 项通过；首次历史导入、重复写入、新解锁通知去重、迁移路径和错误定义已覆盖。
- 已构建并安装到掌机，实机首次导入成功。**仍需用户游玩产生下一条新记录，以验证真实游玩中的系统弹窗；不能把单元测试或首次导入说成实机新解锁弹窗已成功。**

## 必须保留的既有修复与数据

### 黯井微光 / Well Dweller

- 游戏 ID：`local-37c3b9d5-e2ed-436b-a753-69a68d01df56`。
- 程序：`C:/Games/Well Dweller/WellDweller.exe`。
- Steam AppID `3699590` 仅用于封面和简介；成就来源是 Xbox。
- Xbox Title ID `64439fe5`（十进制 1682153445），Store ID `9PLH7R84GJWP`。
- 缓存 41 项 Xbox 公开定义，中文文本按 Xbox 编号和条件核对显示；使用 `xbox:64439fe5:<id>`，不和 Steam 解锁混合。
- 历史实机记录已有 9 项解锁，含 #38「虫群麻烦」。它们来自接口请求与成功回调证据，不来自杀敌数推算。
- `src-tauri/probe/WellInterfaceProbe.cs`、`ProbeHost.cs` 和资源 `src-tauri/resources/XboxLocalProbe.exe` 已修复固定一小时退出问题，监听持续到游戏退出或明确停止。
- 只捕获匹配 SHA-256 的模块版本；请求进度 100% 且完成回调 HRESULT 为 0 才保存解锁。每次附加后前 60 秒静默保存，避免读档重放通知。
- 解除附加先暂停线程、恢复本组件拥有的断点与调试寄存器再恢复线程；拒绝覆盖外部调试器断点。不要强杀捕获组件来替代正常停止，不回退已打包资源。
- 捕获输出是本地记录，不表示 Xbox 官方账号已解锁。其他 Xbox 游戏取得定义不等于能自动捕获事件。

### GRIME / 尘埃异变一代

- 本地游戏 ID：`local-79d27737-3a41-4738-b3a7-4ca306aa6c5b`，Steam AppID `1123050`。
- 用户要求补录接近 20 小时，已将本地累计时长补为 **72000 秒**，之后继续自动累计，不要重置。
- 已修复 Unity `*_Data/Plugins` 嵌套目录的平台与 AppID 识别。

### 数据与网络

- 上次部署保留 84 条游戏记录、已有成就、收藏和时长；后续可能因用户操作变化，不能把这个数字视为实时值。
- 用户此前明确授权开启掌机现有 `127.0.0.1:7890` 的系统代理并安装网络修复版。系统代理开关、代理服务和 Steam 访问是否可用需实时检查；不要未经请求重设系统代理。
- SQLite 及历史诊断副本可能含 Steam API Key。读取必要字段，禁止把整个设置表输出到聊天。

## 上次成功部署证据（历史记录）

- 安装包 SHA-256：`48a01ef8cdd1123bfff4bdfd18138b6e40e53eb89c5ee0e6cf98be896d50f12c`。
- 已安装 EXE SHA-256：`dc6f54172356231ea60a2ab542dc332a6225a19f28689ff5e45aea37c928ced6`。
- 数据备份：`C:/Users/lixin/Downloads/launcher-backup-20261001-121927`。
- 安装后数据库副本：`work/diagnostics/public-verification/grime2-after/games.sqlite`。
- 后续重新构建必须计算新散列，不能把上面的历史值作为新产物校验值。

本文编写时掌机 SSH 超时，未安装新版本、未卸载、未修改掌机或游戏文件。后续先恢复连接，再读取实时状态。
