# 河源部署交接（2026-10-21 更新：服务已上线并通过真实验收）

本文件优先于 PROJECT-HANDOFF.md 中较早的「尚未连接／部署」记录。本轮已在服务器完成镜像加载、数据库迁移、API/worker 启动，并用真实邮箱验证码完成登录与存档闭环验收。当前结论：**game.penlibra.xin 已可访问，登录邮件与 OSS 存档链路真实可用**；剩余只有 Windows 实机双设备验收和桌面安装包。

## 用户要求与边界

- 部署独立项目 libra-diary 到河源 ECS，域名 game.penlibra.xin，HTTPS 使用现有 Certbot。
- OSS 使用本项目**专用桶 libra-game**（北京地域，私有 + 阻止公开访问 + AES256 默认加密），**对象前缀为 games/**，不写桶根目录。
- 验证真实账号登录、验证码邮件、OSS 上传下载以及后台校验。
- 项目独立，只参考 libra-space 的运维方法，不复用其他项目数据库、SMTP 账户、应用密钥、数据卷，不修改其他项目。
- 不添加 CPU、内存、pids 限制，不强制单 worker；默认后台并发 2。
- 不执行 Git 提交、推送、分支操作；已有大量未提交源码必须保留。
- 真实账号 `libra085925@gmail.com` 只做登录/设备/刷新验证；存档闭环用用户提供的独立验收账号 `97220040@qq.com`，避免消耗真实资料库的首次历史导入基线。
- 未调用真实游戏存档恢复，未修改桌面资料库路径。

## 目录与连接

本机工作根目录：`/Volumes/T9/Projects/LibraSpace/windows-game-launcher`。
实际源码：`outputs/windows-game-launcher/`。不要编辑 `work/windows-build/project/` 构建副本。

服务器：

| 项目 | 值 |
|---|---|
| ECS 地域／实例 | cn-heyuan / i-f8z4cfpxgbgf6kbcvpok |
| 公网 IP | 47.121.140.196 |
| 主机名 | iZf8z4cfpxgbgf6kbcvpokZ |
| 部署目录 | /opt/apps/libra-diary |
| Compose 项目 | libra-diary |
| API 宿主机绑定 | 127.0.0.1:18090 |
| 数据库名／用户 | libra_diary |

从本机工作根目录连接：

```sh
ssh -i "work/cloud-deploy/deploy_key" -o IdentitiesOnly=yes -o ConnectTimeout=8 root@47.121.140.196
```

专用部署公钥通过阿里云 Cloud Assistant 添加至 root authorized_keys，注释 `libra-diary-deploy`，限制选项为 `restrict,port-forwarding`。没有修改其他已有公钥。私钥仅在被 Git 忽略的 work 目录内，禁止输出或提交。

## 当前运行状态（2026-10-21 实测）

- 镜像 `libra-diary:local`，ID `sha256:f45042be922b52af2df204052e25602346054076d1b79c8576b1ee32ab4e71cc`，amd64/linux，已 `docker load`。
- 三个容器：`libra-diary-api-1`（healthy，127.0.0.1:18090→8090）、`libra-diary-worker-1`（UID/GID 10001，`YOUJI_SAVE_WORKERS=2`）、`libra-diary-postgres-1`（healthy，不发布端口）。
- 迁移 0001–0004 已执行，`_sqlx_migrations` + 19 张业务表，owner `libra_diary`。
- `http://127.0.0.1:18090/health` 与 `https://game.penlibra.xin/health` 均返回 `ok`，HTTP 200，证书校验通过。
- 证书 `/etc/letsencrypt/live/game.penlibra.xin/` 有效至 2027-01-19，本站 dry-run 续期已通过。
- OSS `libra-game` 桶（本项目专用）：ACL private、BlockPublicAccess true、PolicyStatus IsPublic false、默认 SSE AES256、未启用版本控制；所有对象在 `games/` 下，匿名 GET 返回 403。

## 本次执行步骤（已运行验证）

服务器侧：

```sh
cd "/opt/apps/libra-diary"
sha256sum "libra-diary-image.tar.gz"        # 0ebda6a6…c929c，与本机一致
docker load -i "libra-diary-image.tar.gz"   # f45042be… amd64/linux
docker compose config --quiet
docker compose --profile tools run --rm --no-deps migrate
docker compose up -d --no-build api worker
curl --fail --silent --show-error "http://127.0.0.1:18090/health"
```

注意：`migrate` 属于 `tools` profile，必须带 `--profile tools`；不带会报服务不存在。不要运行全局 prune、其他项目 compose 命令或启用 Caddy profile。服务器只有 Compose 文件、配置、镜像包与 Nginx 模板，**不要在这里 build**。

本机验收：

```sh
python3 "work/cloud-deploy/auth_check.py" challenge          # 真实账号验证码
printf '<8位验证码>\n' | python3 "work/cloud-deploy/auth_check.py" verify
python3 "work/cloud-deploy/auth_check.py" refresh
python3 "work/cloud-deploy/smoke_login.py" challenge 97220040@qq.com
printf '<8位验证码>\n' | python3 "work/cloud-deploy/smoke_login.py" verify
python3 "work/cloud-deploy/smoke.py" --base "https://game.penlibra.xin" --session "work/cloud-deploy/smoke-session.json"
python3 "work/cloud-deploy/negative_check.py"                # 损坏/缺失对象负例
python3 "work/cloud-deploy/oss_list.py" "games/"             # 只读列举前缀
python3 "work/cloud-deploy/oss_control.py"                   # 桶级安全设置复查
```

## 验收结果

真实账号 `libra085925@gmail.com`：

- 8 位验证码登录通过，挑战只消费一次（`consumed=t`、`attempts=1`）。
- `/v1/devices` 读取通过；refresh 刷新通过，`session.json` 0600。
- 库内状态：`user_sync_state.initialized=false`、`sequence=0`，0 档案、0 快照、0 上传——**首次导入基线未被消耗**。

独立验收账号 `97220040@qq.com`（完成首次导入，`initialized=true`）：

- 正向闭环：创建测试游戏 → 创建档案 → `POST /v1/save-uploads` 取签名头 → OSS PUT（423 字节 ZIP）→ `complete` 202 → worker 首次尝试即 `completed` → `POST /v1/save-snapshots/{id}/download` → 核对 ZIP 大小、`manifest.json`、文件 SHA-256 全部一致。
- 对象路径：`games/objects/<userId>/<objectId>`（正式）与 `games/staging/<userId>/<uploadId>`（暂存，桶为 libra-game）；桶根未写入本项目对象。
- 负向：损坏归档（同长度改字节）与缺失对象两种情况下，worker 均判定 `failed`，快照列表无该快照，下载接口返回 404/409，未产生可下载完成快照。
- 结果文件：`work/cloud-deploy/smoke-result-b128e213-620d-4f60-afca-72363b3942eb.json`（0600，`status=passed`）。
- **换桶后在 `libra-game` 重跑同一闭环同样全部通过**：结果 `work/cloud-deploy/smoke-result-1775ed44-93c7-480a-873c-72f21c28f1fd.json`，对象 `games/objects/44b4b3e6…/84c26a42…`，匿名 GET 403，桶设置复核为 private + BlockPublicAccess true + AES256。

设备与会话：

- 本轮两个验收设备（真实账号「部署验收设备」、验收账号「部署验收独立设备」）均已 `DELETE /v1/devices/{id}`，`devices.revoked=t`，`auth_sessions.revoked=t`（共 2 条）。没有操作任何其他设备。

## 换桶记录（2026-10-21 晚）

用户确认原 `libra-diary` 桶里的 62 个 `client-releases/` 对象属于其另一项目（Knora-One 客户端下载），应保持原样。处理方式：

1. 新建专用桶 `libra-game`（cn-beijing），配置 private ACL、阻止公开访问、AES256 默认加密；已用原生 V4 查询复核。
2. 本机与服务器 `.env` 的 `OSS_BUCKET` 改为 `libra-game`，`docker compose up -d --no-build --force-recreate api worker`，API 的桶级安全自检通过、`/health` 返回 ok。
3. 删除旧桶 `games/` 下本项目 3 个测试对象，旧桶 `games/` 已为空。
4. 旧桶关闭本轮添加的阻止公开访问与默认加密，恢复到部署前状态；62 个 `client-releases/` 对象逐一核对仍在，匿名访问仍为 403（其桶 ACL 一直是 private，客户端下载依赖签名 URL，未被本次操作打断）。
5. 换桶后用 `97220040@qq.com` 重新登录并重跑 `smoke.py`，五步全过（worker 首次 `completed`，下载哈希一致），随后再次撤销该验收设备。

运维脚本：`work/cloud-deploy/oss_admin.py`（create/verify/relax/delete-prefix）。注意 OSS V4 的 PUT 必须使用 `UNSIGNED-PAYLOAD` 且把所有实际发送的 header（含 `content-type`、`x-oss-acl`）纳入签名；桶加密接口的 XML 根元素是 `ServerSideEncryptionRule`，写成 `...Configuration` 会返回 MalformedXML。

## 验收遗留数据（只存在于独立验收账号）

- 4 个测试游戏：1 个正例（`[部署验收测试] b128e213…`）、1 个探针（`[probe2]`）、2 个负例（`[部署负例测试] …`）。
- 3 个档案、1 个快照、3 条上传记录（其中 2 条为失败的上传尝试）。
- OSS：换桶时旧桶里的 1 个正式对象与 2 个 staging 对象已删除；数据库保留 3 条上传记录与 2 个完成快照，其中 1 条指向旧桶 key（仅验收账号，下载会失败，可忽略或按序删除）。新桶现有 1 个正式对象 + 1 个 staging 对象，staging 已进 GC 队列。
- 如需彻底清空，应通过 `DELETE /v1/save-snapshots/{id}` 与档案接口按序删除，或直接丢弃该验收账号；不要清理真实账号（该账号当前没有任何数据）。

## 已知问题与风险

- **原 `libra-diary` 桶属他人项目**：该桶实为 Knora-One 客户端发布用（62 个 `client-releases/` 下的 macOS DMG）。本项目已迁出并在 `libra-game` 上运行；旧桶恢复为部署前状态（仅保留原有 private ACL，关闭了本轮加的阻止公开访问与默认加密），`games/` 下对象已全部删除，62 个发布文件完好。不要再往 `libra-diary` 桶写本项目数据，也不要对该桶做桶级策略变更。
- API/worker 默认不输出运行日志（`docker compose logs` 为 0 行），排障需查数据库状态或临时提高日志级别；不要为此修改生产配置。
- 验收脚本会创建真实数据，只对专用验收账号运行；真实账号只能做登录验证。

## 2026-10-21 晚：预览导入 413 事故与修复

- 现象：新用户（首个真实外部账号）能登录，但点「预览资料导入」提示「云端响应格式错误」，重试多次都失败，云端只留下 6 个空快照。
- 根因：本站 TLS 前端是**系统 nginx**（`/etc/nginx/conf.d/libra-diary.conf`），容器 Caddy 未启用；该站点当时保留 `client_max_body_size 1m`，而导入预览会一次性上传整个资料库（含成就定义缓存）。超过 1 MB 时 nginx 返回 413 HTML 页面，客户端按 JSON 解析失败，于是显示「云端响应格式错误」。
- 证据：`/var/log/nginx/libra-diary-access.log` 中 `POST /v1/imports/prepare` 连续 413（UA `GameCollection/0.2.1`）；抬高限制后 2 MB、8 MB 请求均返回 JSON。
- 修复：站点 `client_max_body_size` 改为 `64m`，`proxy_read_timeout` 放宽到 `120s`；`nginx -t` 通过后 `nginx -s reload`。回滚备份在服务器 `/root/libra-diary.conf.bak-*`。
- 配置副本：`server/deploy/nginx-site.conf`（与实际文件一致）；`nginx-heyuan.conf` 为本轮早期的临时片段，已被实际站点文件取代。
- 遗留改进：客户端把非 JSON 响应一律报成「云端响应格式错误」，应改为带上 HTTP 状态码与来源，便于下次直接定位；列入 0.2.16。

## 2026-10-21 20:46 存档自动化服务已更新

- 已完成迁移 0005 并更新 API/worker，API healthy；Postgres 保持原容器和数据卷。实际镜像 ID `684dadfc483b92910c142dbec0feed59b53dfeeb7cf22344b5665c68d4dc9620`，标签 `libra-diary:local` 与 `libra-diary:save-auto-20261021`。
- 更新前备份 `/opt/apps/libra-diary/backups/save-auto-20261021T124258Z`；包含 `database.dump`（pg_restore -l 已验证）、配置和旧镜像标签 `image.txt`。回滚应用可恢复该标签，新增数据库列保持兼容；不要直接回灌旧数据库覆盖后续数据。
- 网络构建改为本地校验官方下载的 Rust 1.99.1 Linux 组件与 Cargo vendor，再传至独立目录 `build-save-auto-20261021` 离线编译，运行镜像复用原 Debian 镜像。最终二进制 SHA256 `6efa78478d837e5d62700969f5239828c9015d4b58a9203ccba02eca43cd444d`。
- 首次迁移因 CRLF/LF 导致 VersionMismatch(1)，当时立即恢复旧镜像标签且没有更新服务。核实四个旧 SQL 的 LF SHA384 与生产记录全部相同后规范为 LF，重新构建通过；源码 `.gitattributes` 已固定 SQL 换行。不得通过修改 `_sqlx_migrations` 校验值绕过检查。
- 独立测试账号完成实际客户端归档 ZIP → OSS 上传 → worker 校验 → 幂等重试 → pin/prune 保护 → 下载哈希一致 → Windows 实际恢复代码闭环。测试设备已撤销，测试版本保留供审计；未改真实用户存档。证据 `work/deploy-save-automation/live-result.json`。自动退出调度仅有此前单元测试，双设备和完整 UI 游戏退出闭环仍待实机验证。

## 凭据位置（不要打印内容）

- 本机 `outputs/windows-game-launcher/server/deploy/.env`：完整部署配置，0600、Git 忽略。
- 本机同目录 `.env.smtp`：独立 SMTP 密码，0600、Git 忽略。
- 服务器 `/opt/apps/libra-diary/.env`：已安全传输的相同配置，0600。
- 本机 `work/cloud-deploy/deploy_key`：专用 SSH 私钥。
- 本机 `work/cloud-deploy/session.json`、`smoke-session.json`：已失效的验收会话（设备已撤销）。
- `.env` 不在镜像或构建上下文内。只运行 `docker compose config --quiet`，普通 config 会输出秘密。
- `work/cloud-deploy/cloud_ops.py` 为阿里云 CLI 管理辅助函数，读取参考项目的管理凭据并仅通过子进程环境传递。不要把管理凭据复制进服务运行配置。正常续部署无需再调用管理 API。

## 项目隔离复核（本轮已复核）

- 旧五个 Nginx 站点 `sha256sum -c /opt/apps/libra-diary/nginx-before.sha256` 全部 OK（api/default/livekit/stock-api/tool）。
- 旧容器保持部署前状态：libra-stock-api 等仍为 Exited，本轮未启动、停止或重建；当前仅本项目三个容器运行。
- 数据库端口未发布，仅 Compose 网络可达；数据卷 `libra-diary_postgres-data`、`libra-diary_worker-temp` 独立。
- 没有提交、推送、重置任何 Git 内容；桌面端未生成安装包。

## 剩余事项

1. Windows 掌机实机验收：双设备、真实存档备份/恢复、桌面资料库路径选择、自动更新。本机是 macOS，无法替代。
2. ~~确认桶归属与策略~~：已确认属于 Knora-One 客户端发布，已换桶并将旧桶还原，此项完成。
3. 按需生成 Windows 安装包。

## 本轮源码改动与验证

- OSS_PREFIX 默认、Compose、示例及 OSS 权限文档为 games；相关 6 项 storage 单元测试通过。
- 新增 `server/deploy/nginx-heyuan.conf`，更新 `HEYUAN.md`。
- Dockerfile 运行阶段改从同 Debian bookworm 构建镜像提取 curl、CA 及其依赖，不再联网 apt install；容器内 `curl --version` 与 `/health` 健康检查均已在真实容器验证通过。
- 先前完整 server 24 项测试通过；Linux amd64 Release 构建通过。
- 所有已有未提交变更保留，不要 git reset 或覆盖其他开发内容。

## 网络情况与构建复现

构建期间 Docker Hub 曾 EOF/TLS timeout，crates.io 及 apt 下载超时；SSH、域名 HTTPS、OSS 原生 V4 请求正常。已用固定镜像摘要和本机 Cargo 缓存绕过。当前无需要继续等待的构建或传输任务。

确需源码变更重建时，本机 `work/cloud-deploy/Dockerfile.cached` 与当前源 Dockerfile 对应，额外固定 base digest 并挂载本机 registry 进行离线构建：

```sh
docker build --platform linux/amd64 \
  --build-context "cargo-registry=$HOME/.cargo/registry" \
  -f "work/cloud-deploy/Dockerfile.cached" \
  -t libra-diary:local "outputs/windows-game-launcher"
```

只挂载 registry 缓存，不挂载整个 .cargo（避免带入认证配置）。`oss_control.py` 与 `oss_list.py` 读取 .env 生成 V4 请求，只打印安全字段；不要把安装 oss2 当作部署前置条件。
