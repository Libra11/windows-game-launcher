# Windows 掌机连接与操作

交接日期：2026-10-01。本文供后续会话读取，记录已使用的连接方式和部署流程。

## 1. 设备与文件位置

| 项目 | 已知值 |
| --- | --- |
| 掌机系统 | Windows 11，x64 |
| SSH 用户与地址 | `lixin@192.168.10.96`，TCP 22 |
| 认证方式 | 本地 Ed25519 私钥，无需在聊天中提供密码 |
| 完整工作目录 | `/Volumes/T9/Projects/LibraSpace/windows-game-launcher` |
| 实际源码 | 工作目录下的 `outputs/windows-game-launcher/` |
| 私钥 | 工作目录下的 `work/handheld-ssh/id_ed25519`，已核对权限为 `0600` |
| 已信任主机记录 | 工作目录下的 `work/handheld-ssh/known_hosts` |
| 掌机文件传输目录 | `C:/Users/lixin/Downloads/` |
| 应用进程 | `local_achievement_launcher.exe` |
| 应用标识 | `dev.local.achievementlauncher` |
| 数据库 | `C:/Users/lixin/AppData/Roaming/dev.local.achievementlauncher/games.sqlite` |
| 历史可见桌面启动任务 | `CodexLauncherStart20261001` |

地址由局域网 DHCP 分配，可能变化。2026-10-01 写本文时，对 `192.168.10.96:22` 的一次检查超时；此轮未重新确认掌机在线、安装路径或运行进程。之前连接和部署成功的历史不等于当前实时状态。

SSH 密钥内容、Steam API Key、Xbox 令牌不得写入文档或输出到会话。数据库和备份也可能包含凭据。

## 2. Mac 端连接与 PowerShell 执行

在 Mac 的 zsh 中先运行以下初始化。后续命令示例依赖这些变量和函数，均使用 T9 上的真实路径：

```zsh
LAUNCHER_ROOT="/Volumes/T9/Projects/LibraSpace/windows-game-launcher"
LAUNCHER_REMOTE="lixin@192.168.10.96"
LAUNCHER_SSH_OPTIONS=(
  -i "$LAUNCHER_ROOT/work/handheld-ssh/id_ed25519"
  -o IdentitiesOnly=yes
  -o BatchMode=yes
  -o ConnectTimeout=8
  -o StrictHostKeyChecking=yes
  -o "UserKnownHostsFile=$LAUNCHER_ROOT/work/handheld-ssh/known_hosts"
)

handheld_ps() {
  local task_encoded
  task_encoded="$(python3 -c 'import base64,sys; print(base64.b64encode(sys.stdin.read().encode("utf-16-le")).decode("ascii"))')" || return
  ssh "${LAUNCHER_SSH_OPTIONS[@]}" "$LAUNCHER_REMOTE" \
    powershell.exe -NoProfile -NonInteractive -EncodedCommand "$task_encoded"
}
```

UTF-16LE `EncodedCommand` 用于避免 Mac shell、Windows 默认 SSH shell 和 PowerShell 之间的引号、中文路径及 `$` 转义问题；它不是加密。不要把密码或 API Key 放进编码命令。

连接检查只读取用户名、计算机名和 SSH 服务状态：

```zsh
handheld_ps <<'PS'
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Write-Output ("USER=" + $env:USERNAME)
Write-Output ("COMPUTER=" + $env:COMPUTERNAME)
Get-Service -Name sshd | Select-Object Name,Status
PS
```

需要交互终端时：

```zsh
ssh "${LAUNCHER_SSH_OPTIONS[@]}" "$LAUNCHER_REMOTE"
```

常见问题：

- 连接超时：确认掌机开机且未休眠、同一局域网、当前 IP 和 SSH 服务。超时不能证明程序或成就读取有问题。
- 在掌机运行 `ipconfig` 查看当前 IPv4；确认新地址后更新 `LAUNCHER_REMOTE`。不能仅凭端口开放就认定另一台设备是掌机。
- `Permission denied (publickey)`：核对账户和授权公钥文件。既有安装使用 `work/handheld-transfer/authorize-codex.ps1` 配置管理员或普通用户的公钥位置；不要直接重写现有授权文件。
- 主机密钥变化：由用户在掌机核对主机身份后再处理，不能关闭主机校验或直接清空 `known_hosts`。
- `work/handheld-transfer/setup-openssh.ps1` 是初次安装遗留脚本，写死了当时 Mac 的 HTTP 地址。现有 SSH 已安装，不应重复执行；重建环境前先审阅脚本、核对地址和授权。

## 3. 传输文件

此掌机之前的 SFTP 传输失败，已验证可用的是旧 SCP 协议：**每次使用 `scp -O`**，并使用相同密钥和主机校验。

上传一个脚本示例：

```zsh
scp -O "${LAUNCHER_SSH_OPTIONS[@]}" \
  "$LAUNCHER_ROOT/work/update-handheld.ps1" \
  "${LAUNCHER_REMOTE}:C:/Users/lixin/Downloads/update-handheld.ps1"
```

下载一个日志示例：

```zsh
scp -O "${LAUNCHER_SSH_OPTIONS[@]}" \
  "${LAUNCHER_REMOTE}:C:/Users/lixin/Downloads/launcher-diagnostic.log" \
  "$LAUNCHER_ROOT/work/diagnostics/launcher-diagnostic.log"
```

传输前确认远程文件存在。中文路径可先在远程 PowerShell 中复制到 Downloads 的英文文件名，再通过 SCP 下载。不要通过 HTTP 公开完整工作目录，里面含私钥和数据库。

## 4. 远程查看与操作桌面程序

SSH 可以执行 PowerShell、读日志和传文件，但不是远程桌面，不能凭命令成功就认定用户已经看见窗口。

### 查看安装位置与运行状态

安装位置从用户卸载注册表读取，避免猜测 Program Files 或 AppData 下的路径：

```zsh
handheld_ps <<'PS'
$ErrorActionPreference = 'Stop'
$entry = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue |
  Where-Object { $_.MainBinaryName -eq 'local_achievement_launcher.exe' } |
  Select-Object -First 1
if (-not $entry) { throw 'Launcher installation entry missing.' }
$entry | Select-Object DisplayName,DisplayVersion,InstallLocation
Get-Process local_achievement_launcher,XboxLocalProbe -ErrorAction SilentlyContinue |
  Select-Object Id,ProcessName,SessionId,Path
Get-Process explorer -ErrorAction SilentlyContinue | Select-Object Id,SessionId
PS
```

### 启动到掌机可见桌面

直接在 SSH 会话 `Start-Process` 不可靠：进程可能不在已登录用户的可见桌面。已验证方法是使用 **Interactive 用户登录类型的计划任务**。用户 `lixin` 需要已登录桌面。

已有任务的启动命令：

```zsh
handheld_ps <<'PS'
$ErrorActionPreference = 'Stop'
if (Get-Process local_achievement_launcher -ErrorAction SilentlyContinue) {
  Write-Output 'Launcher already running.'
  return
}
Start-ScheduledTask -TaskName 'CodexLauncherStart20261001'
Get-ScheduledTaskInfo -TaskName 'CodexLauncherStart20261001' |
  Select-Object LastRunTime,LastTaskResult
PS
```

随后单独查询应用进程和 SessionId；窗口是否显示由用户确认。任务需要允许电池启动且不因切到电池而停止；此前掌机电池模式阻止启动的问题已修复。

- `work/update-handheld.ps1` 安装后会创建此任务，使用 `Interactive`、`Limited`，设置电池选项，并等待 8 秒检查应用进程。
- `work/start-launcher-on-battery.ps1` 修正已有任务的电池设置并启动。
- `work/launch-handheld.ps1` 是较早验证脚本，使用另一任务名且没有电池选项；不要作为默认启动流程。

### 退出与重启

优先让用户从启动器界面正常退出。更新脚本要求启动器已退出，避免安装覆盖正在运行的程序。

`work/prepare-launcher-update.ps1` 会先请求窗口关闭，6 秒后仍未退出则结束启动器进程；它只针对启动器，但不能当作无影响的查看命令。执行前检查游玩与捕获状态，并确认本轮任务允许关闭程序。

Xbox 捕获附加时不要直接强杀 `XboxLocalProbe.exe`。正常退出启动器会通过控制文件请求捕获组件恢复自身调试寄存器并解除附加；必须区分启动器、捕获组件和游戏进程。无需为了连接或读取日志关闭游戏。

## 5. 构建 Windows 安装包

主源码和构建副本分开：先修改 `outputs/windows-game-launcher/`，再同步到构建副本。不能只改 `/app` 或 `work/windows-build/project/`。

| 项目 | 值 |
| --- | --- |
| 已迁移容器名称 | `codex-launcher-windows-t9` |
| 保留工具环境的镜像 | `codex-tauri-windows-builder:migrated-20261001` |
| 容器工作目录 | `/app` |
| `/app` 实际数据 | T9 的 `work/windows-build/project/` |
| 构建工具目录 | `/build-tools`，对应 T9 的 `work/windows-build/` |
| Rust 目标 | `x86_64-pc-windows-msvc` |
| 构建方式 | `cargo-xwin`，NSIS 安装包 |

2026-10-01 本轮本地检查：该容器处于 running。使用稳定容器名称，不使用旧容器 ID；迁移前的旧容器已删除。

用户要求打包时才执行：

```zsh
docker inspect --format '{{.Name}} {{.Config.Image}} {{.State.Status}}' \
  "codex-launcher-windows-t9"

tar -C "$LAUNCHER_ROOT/outputs/windows-game-launcher" \
  --exclude=node_modules --exclude=target --exclude=dist --exclude=gen \
  -cf - . | docker exec -i "codex-launcher-windows-t9" tar -xf - -C /app

docker exec "codex-launcher-windows-t9" npm run tauri -- build \
  --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis
```

上述同步适用于增改文件，不会删除构建副本中已从源码移除的文件。存在源码文件删除时应处理对应旧文件，不能直接清空包含工具链缓存的 `/app`。

当前 0.2.1 安装包路径：

```text
work/windows-build/project/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/本地游戏启动器_0.2.1_x64-setup.exe
```

`src-tauri/resources/XboxLocalProbe.exe` 是必要资源。修改捕获 C# 源码后需在 Windows 按 `src-tauri/probe/build.ps1` 重新编译资源，再同步和打包；仅打包 Rust 不会重新编译 C#。不要回退已修复的捕获组件。

## 6. 安装或更新流程

前提：用户要求安装或已明确授权本次更新；先确认掌机身份、游戏和启动器状态。默认保留游戏库、游玩时长、资料与解锁记录。

1. 生成安装包，计算本次产物的 SHA-256，不能复用旧版本的散列。
2. 上传安装包到 `C:/Users/lixin/Downloads/windows-game-launcher-latest-x64-setup.exe`。
3. 上传 `work/update-handheld.ps1`。需要关闭启动器时先按上一节处理。
4. 使用更新脚本的 `ExpectedHash` 参数执行。脚本先校验散列和应用已退出状态，再备份 Roaming、Local 应用数据，静默安装并启动到桌面。
5. 确认 `INSTALL_EXIT=0`、安装位置、实际 EXE 的散列、进程 SessionId，并让用户确认可见窗口。

上传与散列示例：

```zsh
LAUNCHER_INSTALLER="$LAUNCHER_ROOT/work/windows-build/project/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/本地游戏启动器_0.2.1_x64-setup.exe"
shasum -a 256 "$LAUNCHER_INSTALLER"
scp -O "${LAUNCHER_SSH_OPTIONS[@]}" "$LAUNCHER_INSTALLER" \
  "${LAUNCHER_REMOTE}:C:/Users/lixin/Downloads/windows-game-launcher-latest-x64-setup.exe"
scp -O "${LAUNCHER_SSH_OPTIONS[@]}" "$LAUNCHER_ROOT/work/update-handheld.ps1" \
  "${LAUNCHER_REMOTE}:C:/Users/lixin/Downloads/update-handheld.ps1"
```

将下面占位文字替换成刚计算的 64 位十六进制散列：

```zsh
handheld_ps <<'PS'
$ErrorActionPreference = 'Stop'
& (Join-Path $env:USERPROFILE 'Downloads\update-handheld.ps1') -ExpectedHash '本次安装包的SHA256'
PS
```

脚本是 Windows PowerShell 文件；遇到执行策略阻止时先查原因，不修改全局执行策略。此前传输脚本使用单次进程的 `-ExecutionPolicy Bypass`，仅在授权执行已审阅脚本时使用。

默认备份位置：`C:/Users/lixin/Downloads/launcher-backup-<日期时间>/`，含 Roaming、Local 子目录。不要把旧 `work/install-handheld.ps1` 用于更新，它写死了 0.1.1 的文件名和散列。

## 7. 数据和成就诊断

| 类型 | 路径或检查点 |
| --- | --- |
| 主数据 | `%APPDATA%/dev.local.achievementlauncher/games.sqlite` |
| Xbox 本地捕获 | `%APPDATA%/dev.local.achievementlauncher/xbox-local/64439fe5/<game-id>/` |
| Xbox 事件 | 上述目录的 `interface-events.log` |
| Xbox 监听状态 | 上述目录的 `interface-ready.txt`，正常附加包含 `ARMED` |
| Xbox 控制文件 | 上述目录的 `control.txt`；不要无目的改写 |
| GSE／Goldberg | `%APPDATA%/GSE Saves/<AppID>/achievements.json` 等 |
| RUNE／CODEX | `%PUBLIC%/Documents/Steam/RUNE/<运行环境AppID>/achievements.ini` 等 |
| 本机历史诊断 | 工作目录下的 `work/diagnostics/`，属于历史副本 |

读取事件和状态前先确认真实目录及当前游戏进程。日志已有旧行，不代表这次游玩新触发；要核对时间、进程和新的文件内容。不要上传全部设置表或打印 API Key。

数据库使用 SQLite，运行时可能有 `games.sqlite-wal` 和 `games.sqlite-shm`。直接下载一个运行中的主文件可能漏掉最近写入；一致备份优先正常退出后复制整个应用数据目录，或使用 SQLite 的备份机制。历史诊断副本不能替代掌机实时数据库。

具体游戏、最近验证的结果与边界见 `PROJECT-HANDOFF.md`；通用成就机制见源码 `README.md` 和 `XBOX-ACHIEVEMENTS.md`。

## 8. 卸载说明（仅在用户明确要求时）

写下卸载步骤不表示现在执行卸载。先备份应用数据，正常退出启动器并确认捕获解除，再选择下面一种方式：

- 在掌机「设置 → 应用 → 已安装的应用」中找到「本地游戏启动器」，选择卸载。
- 远程卸载时按第 4 节获取真实 `InstallLocation`，确认该目录的 `uninstall.exe` 存在，使用 `Start-Process -FilePath $uninstaller -ArgumentList '/S' -Wait -PassThru`，再检查退出码、注册表和程序文件。

当前 NSIS 脚本确实生成 `uninstall.exe` 并在用户卸载注册表登记。卸载后的数据保留行为应在当次使用的安装器中确认，不能依赖猜测。不要把卸载程序、清空数据库、删除游戏文件混为一个操作。

部署创建的 `CodexLauncherStart20261001` 任务可能仍在；如用户要求彻底卸载，核对任务的目标 EXE 后一并清理。删除应用数据、备份、密钥或撤销 SSH 访问均需对应的明确请求，不能随普通软件更新或卸载自动执行。
