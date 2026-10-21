# 单机 Compose 配置

这是部署模板，本次仅开发和静态校验；没有构建镜像、连接服务器或部署。下文启动命令仅供管理员获准部署后执行。

## 组件与持久化

默认启动 API、Postgres 16、独立 worker。API 和 worker 使用相同镜像；worker 执行 `youji-cloud-server worker`，通过 Postgres 队列领取后台校验任务，不依赖 API 进程内存，不提供 HTTP 服务。收到上传完成通知不代表归档已校验通过；停止 worker 后任务可能积压。

| 卷 | 用途 |
|---|---|
| `postgres-data` | 数据库、队列、上传配额预留和校验状态 |
| `worker-temp` | 挂到 `/var/lib/youji-worker` 的下载和归档校验工作区 |
| `caddy-data`、`caddy-config` | 可选反代的证书和配置 |

工作目录由镜像预设 UID/GID 10001 权限。重建容器保留工作卷，但该卷不替代数据库和 OSS 备份。归档对象存放在私有 OSS，Compose 不启动 MinIO。`USER_QUOTA_BYTES` 默认每用户 10 GiB，上传预留计入配额。为每个并发任务预留下载、验证文件等磁盘空间，不能只按压缩包大小估算。

API 仅映射宿主机 `127.0.0.1:8090`（端口可配置）；数据库与 worker 不发布端口。`/health` 只证明 HTTP 存活，`pg_isready` 也不能证明迁移、邮件、OSS 和任务消费均已验收。

构建上下文是实际源码根目录。根 `.dockerignore` 仅允许三个 Rust crate 的源码、manifest、服务端锁文件和迁移进入构建。Dockerfile 不读取环境文件、不接受凭据构建参数，只把最终二进制复制到运行镜像。运行环境由 Compose 注入；Docker 管理员仍能读取容器环境。

## 配置与静态验证

以下命令均以实际源码根目录 `outputs/windows-game-launcher/` 为工作目录。复制 `server/deploy/.env.example` 为同目录 `.env`，替换全部占位值；局部 `.gitignore` 已排除真实环境文件。

- `DATABASE_URL` 主机名用 `postgres`，用户、数据库、密码与 `POSTGRES_*` 一致；密码中的 URL 特殊字符须百分号编码。随机十六进制密码可简化编码。
- 邮件默认使用独立 SMTP 账户：`YOUJI_MAIL_TRANSPORT=smtp`，配置 `SMTP_HOST/PORT/SECURE/USER/PASS/FROM`。隐式 TLS 或强制 STARTTLS，不允许明文降级。也可选择 `http` 模式使用原 HTTPS 适配器；此时才配置 `YOUJI_MAIL_URL/TOKEN`。
- `OSS_ENDPOINT` 是公共地域根地址，如 `https://oss-cn-hangzhou.aliyuncs.com`，不含桶名、路径或查询；`OSS_REGION` 对应 `cn-hangzhou`。客户端预签名地址使用公共 endpoint。
- `OSS_INTERNAL_ENDPOINT` 可留空，仅在服务端确实能访问同地域内网时填写，不能把内网地址发送给桌面客户端。
- 桶须私有、启用服务端加密，并授予实现所需的桶安全检查及对象读写权限。长期凭据不填 `OSS_SECURITY_TOKEN`；STS 临时凭据须带 token，过期前更新运行配置。
- `YOUJI_SAVE_WORKERS` 默认 2，由服务端校验并发上界。裸进程的 `YOUJI_SAVE_WORK_DIR` 默认系统临时目录；Compose 固定为 `/var/lib/youji-worker`。更改目录时须同时修改挂载与权限。

仅解析配置，不访问 Docker daemon，不构建、不启动：

```sh
docker compose --env-file "server/deploy/.env.example" -f "server/deploy/compose.yaml" config --quiet
docker compose --env-file "server/deploy/.env.example" -f "server/deploy/compose.yaml" --profile tools --profile proxy config --quiet
```

占位凭据和域名不可用于实际服务。解析成功不证明 Linux 镜像可构建、OSS/邮件可达、worker 和迁移已通过运行验收。不要公开真实环境的完整 `config` 输出，它包含插值后的凭据；使用 `--quiet`。

## 明确执行的部署顺序

下列命令本次未执行。正式环境应将镜像固定为经过验证的版本或 digest，默认滚动标签仅用于模板。

```sh
# 构建共享镜像。
docker compose --env-file "server/deploy/.env" -f "server/deploy/compose.yaml" build api

# 先启动数据库，再显式执行一次迁移。迁移失败则停止后续步骤。
docker compose --env-file "server/deploy/.env" -f "server/deploy/compose.yaml" up -d postgres
docker compose --env-file "server/deploy/.env" -f "server/deploy/compose.yaml" --profile tools run --rm migrate

# 创建邀请：覆盖工具服务命令。
docker compose --env-file "server/deploy/.env" -f "server/deploy/compose.yaml" --profile tools run --rm migrate invite "player@example.com"

# 已有 HTTPS 反代时启动默认应用组件。
docker compose --env-file "server/deploy/.env" -f "server/deploy/compose.yaml" up -d api worker
```

API/worker 不依赖迁移服务，普通 `up` 不自动迁移。不要用 `--profile tools up` 代替单次迁移；启用该 profile 后直接 `up` 会包含工具服务。修改 `POSTGRES_PASSWORD` 环境变量不会自动更改已有数据库密码，需独立轮换。

更新前先备份数据库，明确迁移和回滚策略，再停写、迁移、切换 API/worker 镜像。OSS 对象和数据库索引需协调备份；不要删除卷来升级或排错。

## 可选 Caddy

默认关闭。已有宿主机 HTTPS 反代转发至 `127.0.0.1:8090`；若反代也在容器中，应连接合适的网络并访问 `api:8090`，容器回环地址不是宿主机。

使用模板 Caddy 前先配置真实域名、ACME 邮箱、DNS 和 80/443 入站，然后显式启用：

```sh
docker compose --env-file "server/deploy/.env" -f "server/deploy/compose.yaml" --profile proxy up -d api worker caddy
```

Caddy 管理 HTTPS 并反代至 `api:8090`，不替代登录端点限流与防火墙策略。模板不启用访问日志；现有反代也不应记录 Authorization、验证码正文或预签名 URL。API 普通 push 上限 1 MiB，导入上限 64 MiB；已有反代须允许导入体积。大存档直接传 OSS，不经过反代。

## 官方语法依据

- [Docker 多阶段构建](https://docs.docker.com/build/building/multi-stage/)
- [构建上下文和 .dockerignore](https://docs.docker.com/build/concepts/context/#dockerignore-files)
- [Compose profiles 与显式运行服务](https://docs.docker.com/compose/how-tos/profiles/)
- [Compose 服务、健康依赖、卷和端口](https://docs.docker.com/reference/compose-file/services/)
- [Compose 变量插值](https://docs.docker.com/compose/how-tos/environment-variables/variable-interpolation/)

河源目标与项目隔离状态见 [HEYUAN.md](HEYUAN.md)。该主机已有 Nginx，使用独立站点配置，不启用 Caddy profile。
