# 应用内更新与签名发布

使用入口：设置 → 关于与更新。首版仅支持 Windows x64 NSIS 正式安装版；Mac 开发运行和浏览器预览不发起下载或安装。当前应用版本为 0.2.6，首个包含此功能的正式版本需要先手动安装一次。

## 用户操作

- 默认在原生进程启动、主窗口就绪后延迟八秒检查一次。可以关闭启动检查；手动检查始终可用。后台检查失败只记录在设置状态中。
- 只检查正式版，发现更新在设置入口显示圆点。阅读中文更新说明后手动下载，下载可取消，离开设置后继续。
- 只有完整下载并通过 Minisign 签名和版本绑定校验的包才写入缓存。缓存位于应用缓存目录 `app-updates/`，单包上限 512 MiB。
- 重启后需要重新检查；版本、地址和签名一致且重新验签后才复用。离线时只能显示缓存版本，不直接安装。首版不做断点续传。
- 游戏或捕获运行时可以下载，安装会被拒绝。确认安装后保存计时检查点，阻止新的启动和捕获，等待已有写入完成，再启动 NSIS 简洁安装进度界面。必要时 Windows 会要求系统授权。
- 不自动备份，更新保留应用数据及账号连接。检查点保存失败或正在备份／恢复时不得启动安装器。
- 安装器启动成功后插件直接退出进程，不依赖普通退出事件再次保存。安装器负责重新打开应用。上次安装是否成功根据当前实际版本确认；拒绝授权或中途退出后，再次打开旧版可检查并重试。
- 启动检查开关随应用数据备份保存；更新缓存、准备 ID、安装尝试记录及签名密钥不随备份迁移。

## 首次配置 GitHub Actions Secrets

正式更新密钥已经生成，私钥没有进入仓库。维护者本机文件：

- 私钥：`/Users/libra/.local/share/youji-release-signing/updater.key`
- 密码：`/Users/libra/.local/share/youji-release-signing/password.txt`
- 公钥：`/Users/libra/.local/share/youji-release-signing/updater.key.pub`，已写入 Tauri 配置。

在仓库的 Settings → Secrets and variables → Actions 配置：

- `TAURI_SIGNING_PRIVATE_KEY`：私钥文件完整内容。
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：密码文件完整内容。

也可在已登录且有仓库权限的终端手动执行以下命令；它们从文件读取，不将值打印到终端：

```sh
gh secret set TAURI_SIGNING_PRIVATE_KEY --repo Libra11/windows-game-launcher < "$HOME/.local/share/youji-release-signing/updater.key"
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo Libra11/windows-game-launcher < "$HOME/.local/share/youji-release-signing/password.txt"
```

本轮没有替用户上传 Secrets。缺少任一项时工作流明确失败，不发布未签名安装包。单独保护并备份私钥及密码；不要提交、发到公开服务或替换现有签名公钥。丢失原私钥后无法给已安装的客户端提供同公钥签名的更新。

## 版本发布

1. 统一五处应用版本配置，并新增 `release-notes/<版本号>.md`。中文说明不得为空，最多 256 KiB；此文件同时用于应用内与 GitHub Release。
2. 版本变更合入 main 后，现有工作流校验版本与签名配置，使用锁定的依赖构建 Windows x64 NSIS。
3. `createUpdaterArtifacts=true` 生成 `.exe` 与对应 `.exe.sig`。安装包保持 ASCII 文件名 `youji_<版本>_x64-setup.exe`，签名文件同步命名。
4. 发布脚本验证签名格式和可信注释内的版本，生成 `latest.json`，只包含 `windows-x86_64` 平台；发布脚本同时使用 Node 原生 Ed25519／BLAKE2b 验证安装包与可信版本注释，客户端下载与安装前还会重新验签。
5. 所有产物先上传草稿 Release，再公开发布。正式版标记 latest，预发布不成为 latest；已公开版本不覆盖，同名标签不得移到另一个提交。

固定检查入口：`https://github.com/Libra11/windows-game-launcher/releases/latest/download/latest.json`。只接受游迹仓库内 HTTPS NSIS 下载地址，重定向必须保持 HTTPS。使用内置公钥，开启 `requireSignedVersion`；不能关闭 TLS 或签名校验。

当前依赖为 `tauri-plugin-updater=2.13.2`。独立启用该插件使用的 reqwest 0.13 SOCKS 功能，遵循现有系统／直连／自定义 HTTP 或 SOCKS5 代理。检查超时二十秒，连接十秒；下载独立设置三十分钟总超时及三十秒读取超时。代理变化用于下一次操作，不切换正在下载的连接。

## 状态与维护

自定义命令为 `get_app_update_status`、`check_app_update`、`download_app_update`、`cancel_app_update_download`、`install_app_update`。前端不接收可修改的下载地址、公钥或程序路径，只提交后端准备 ID。主窗口事件为 `app-update-progress`，带操作 ID、递增 revision、阶段及字节进度；前端拒绝迟到的旧 revision。

更新状态由应用进程管理，设置页仅订阅；重复检查等待同一检查结果，其他冲突操作拒绝。下载进度通知最多约每 100 ms 一次，完成和失败立即通知。只有完整验签后才原子发布缓存索引；保存失败不替换之前的有效缓存。

更新和备份恢复共享维护标记，游戏启动与捕获使用同一闸门。维护失败自动解除标记，已封存的恢复任务保持标记等待重启。不会启动或结束用户游戏，也不会恢复下载过程或安装器的进程状态。

安装更新时先检查游戏均已退出，再进入维护状态阻止新的捕获，通过 `control.txt` 请求现有捕获组件正常停止，最多等待 10 秒并确认组件进程退出后才启动安装器。等待游戏的常驻组件也会正常停止；游戏仍在运行、控制文件写入失败、进程状态读取失败或停止超时均拒绝安装，不强杀组件。安装准备失败会解除维护状态，由后续后台扫描重新建立捕获。此流程需要包含修复的新桌面程序，旧安装版不会因下载新版安装包而获得修复。

## 回归与实机验收

本轮仅编写用例，未主动运行测试或构建。用例涵盖受控说明渲染、进度与页面订阅、迟到状态、稳定版本与下载来源、签名篡改、缓存版本绑定及写入失败、维护竞争、实际版本确认和发布产物规则。

Windows 上仍需实机验证：首次旧版手动升级、正常在线更新、离线缓存提示、HTTP／SOCKS5／系统代理、下载取消与断网、磁盘不足、游戏及捕获阻止安装、安装器启动失败、UAC 取消和安装中断后的旧版重试。发布工作流需要 Secrets 配置后，通过实际版本发布验证草稿上传失败不影响已发布版本的更新入口。

技术依据：Tauri 官方 Updater 文档及锁定版本插件源码；不要套用旧版 `downloadAndInstall` 在 Windows 返回后再重启的示例。
