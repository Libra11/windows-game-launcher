# 游迹云服务

独立 Rust 服务，不依赖 Tauri。Postgres 保存用户私有元数据、按用户提交顺序排列的同步日志与存档索引；阿里云 OSS 保存经后台校验的私有存档归档。客户端仍可离线使用。

## 运行配置

需要 Postgres 14 或更新版本，以及应用专用数据库账号。以下仅是配置名称，不包含可用凭据。邮件变量按 SMTP 或 HTTP 模式选择：

| 环境变量 | 用途 |
|---|---|
| `DATABASE_URL` | 专用 Postgres 连接地址 |
| `YOUJI_TOKEN_KEY` | 至少 32 字节随机服务端密钥，用于 token 和验证码摘要 |
| `YOUJI_MAIL_TRANSPORT` | `smtp` 或 `http`，必须显式配置 |
| `SMTP_HOST/PORT/SECURE/USER/PASS/FROM` | SMTP 模式的独立发件账户配置 |
| `YOUJI_MAIL_URL` | HTTPS 邮件适配接口 |
| `YOUJI_MAIL_TOKEN` | 邮件适配接口的 Bearer 凭据 |
| `YOUJI_BIND` | 默认 `127.0.0.1:8090`，通过 HTTPS 反向代理访问 |
| `OSS_ENDPOINT` | OSS 公共地域 HTTPS 根地址，不包含桶名和额外路径；用于客户端预签名 URL |
| `OSS_INTERNAL_ENDPOINT` | 可选，仅服务端同地域内网可达时使用 |
| `OSS_BUCKET`、`OSS_REGION` | 应用专用私有桶及地域，例如 `cn-hangzhou` |
| `OSS_ACCESS_KEY_ID`、`OSS_ACCESS_KEY_SECRET` | 仅服务端使用的 OSS 访问凭据 |
| `OSS_SECURITY_TOKEN` | 可选，STS 临时凭据的安全令牌 |
| `USER_QUOTA_BYTES` | 默认每用户 10 GiB，上传预留计入配额 |
| `YOUJI_SAVE_WORKERS` | 后台校验并发数，默认 2，由服务端限制上界 |
| `YOUJI_SAVE_WORK_DIR` | 默认系统临时目录；Compose 使用持久工作卷 |

支持独立 SMTP 或外部 HTTPS 邮件适配器。HTTP 模式的适配器接收 `POST` JSON：`{"to":"邮箱","subject":"标题","text":"正文"}`，成功返回 2xx。验证码不输出到终端，不提供开发万能码。未受邀邮箱和受邀邮箱的请求响应形状一致；邮件发送失败不能用于绕过登录。

初始化与邀请由管理员显式执行：

```sh
cargo run --manifest-path "server/Cargo.toml" -- migrate
cargo run --manifest-path "server/Cargo.toml" -- invite "player@example.com"
cargo run --manifest-path "server/Cargo.toml"
# 在独立进程中运行后台存档校验。
cargo run --manifest-path "server/Cargo.toml" -- worker
```

服务启动不自动迁移生产数据库。先配置数据库、运行迁移，再创建邀请、启动服务。HTTPS 反向代理应限制登录端点的请求速率和正文大小，不记录 Authorization、验证码正文或存档预签名 URL。客户端拒绝 HTTP 云服务地址。

`GET /health` 仅表示 HTTP 服务可响应，不表示邮件、数据库和对象存储已经验收。未配置对象存储时账号和元数据同步仍可运行，存档上传明确返回不可用。

## 存储边界

[OSS 配置与最小 RAM 权限](OSS.md) 说明桶私有、阻止公开访问、策略与 SSE 检查、静态 STS 凭据、端点分离及请求超时；配额变量统一为 `USER_QUOTA_BYTES`。

存储使用 OSS 原生接口。桶必须私有并启用服务端加密；安全配置检查失败时不得降级为公开或未加密存储。公共 endpoint 与可选内网 endpoint 分离，客户端不会收到服务端内网地址。此配置模板不再使用旧 `S3_*` 变量。

API 接收上传完成通知后将校验任务持久化到 Postgres 队列，由独立 `worker` 下载并校验归档。接收任务不代表校验通过；验证完成后才发布可下载的快照索引。worker 停止会导致任务积压，重启恢复以数据库队列和租约状态为准。

客户端只能拿到特定暂存对象的短期上传签名。服务端下载归档并验证大小、SHA-256、文件清单与 Windows 路径，再发布到另外的对象键；客户端不能改写已发布对象。存档相对路径不会被当作本机绝对路径执行。

单归档与解压内容各限 2 GiB、最多 50,000 个文件。上传任务有效 24 小时，预留空间计入配额；未完成任务与未引用对象有延后回收队列。用户历史快照不自动清理。当前版本及被子快照引用的父版本不能直接删除。

存档使用服务端可解密的加密边界，不是端到端加密。公开游戏封面不占用私有存档对象配额。

## 容器配置

[单机 Compose 说明](deploy/README.md) 包含 API、Postgres、独立 worker、显式迁移工具和可选 Caddy。默认不启动反代，不自动迁移，不暴露数据库端口；API 只发布宿主机回环 8090 端口。[环境变量示例](deploy/.env.example) 的凭据均为占位值。

这套配置仍需运行验收。本次只做本地 `docker compose config --quiet` 校验，没有构建镜像、部署或连接真实 OSS。构建上下文仅包含服务端及共享 Rust 库，不打包环境文件、桌面资料库或密钥。

后台状态、租约恢复与对象容量预留规则见 [存档后台校验](SAVE-JOBS.md)。

## 同步约定

- 所有私有查询由登录会话取得 `user_id`，设备身份也来自会话。
- 用户同步状态行是提交锁；业务状态、变更日志与操作幂等结果在同一事务提交。
- 操作 ID 只能对应一种请求内容。重试返回第一次的结果，内容变更会被拒绝。
- 拉取使用提交顺序序号和固定高水位；日志保存当次提交的完整资源状态。
- 本机路径、平台 token、代理和原始成就证据不属于共享 DTO，未知字段被拒绝。
- 计时以唯一会话累计检查点为事实；整体累计来自历史基线与会话求和。
- 人工编辑使用资源 revision；不同成员关系分别版本化，过时编辑不按上传时间强制覆盖。
- 官方快照需重新获取后处理冲突；自动解锁与可撤销的手动状态分开保存。
- 首次导入先准备预览，确认时再次检查本地摘要和云端序号。其他设备不重复导入无会话历史差额。
- 首次导入总请求上限 64 MiB、10,000 条资源，单资源仍不超过 1 MiB；普通 push 保持 100 操作、1 MiB。增量和全量分页同时限制条数与约 4 MiB 内容预算。
- 同名分类需要明确关联或改名，已有可变状态差异需要选择本机或云端；存在未解决项时不能提交导入。
- 服务器日志、操作幂等记录和删除标记首版不进行压缩，需监控数据库容量。

## 验证

协议和归档测试不依赖在线服务：

```sh
cargo test --manifest-path "cloud-protocol/Cargo.toml"
cargo test --manifest-path "save-archive/Cargo.toml" --features http
```

服务端集成测试仅使用 `YOUJI_TEST_DATABASE_URL`，不会回退到 `DATABASE_URL`。必须指向一次性的隔离测试库，测试会初始化表并生成测试数据；不设置时数据库用例跳过。

```sh
cargo test --manifest-path "server/Cargo.toml"
```

正式使用前还需配置真实邮件与私有 OSS，验证两台 Windows 设备上的登录、离线重连、存档上传和恢复。单元测试与临时数据库测试不能替代这些环境验收。


## 独立 SMTP 邮件

默认部署配置使用 `YOUJI_MAIL_TRANSPORT=smtp`，直接发送验证码，无需单独邮件适配器。配置 `SMTP_HOST/PORT/SECURE/USER/PASS/FROM`；`SMTP_SECURE=true` 使用隐式 TLS（通常 465），false 强制 STARTTLS，不允许明文降级。SMTP 密码与 OSS AccessKey 是不同凭据，不能混用。

使用独立发件域名、发件地址及 SMTP 密码，不复制其他项目的发件账户。河源配置使用 `noreply@mail.game.penlibra.xin`，需要阿里云验证发件域名后才可用。应用不发送部署测试邮件，实际验证码仅由用户请求触发。

保留 `YOUJI_MAIL_TRANSPORT=http` 作为可选模式，此时才需要 `YOUJI_MAIL_URL` 和 `YOUJI_MAIL_TOKEN`。两种模式都限制发送总时限，发送失败使该验证码失效，不记录验证码或邮件凭据。
