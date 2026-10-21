# 河源独立项目配置

项目名：libra-diary。只参考 libra-space 的阿里云操作方法，不复用其数据库、运行配置、SMTP 账户或数据卷。

## 已确认资源

- ECS：cn-heyuan，i-f8z4cfpxgbgf6kbcvpok，公网 47.121.140.196。
- API 域名：game.penlibra.xin，A 记录已创建；本轮临时创建的 diary 记录已撤销。
- 远端部署目录：/opt/apps/libra-diary。
- API 回环端口：127.0.0.1:18090。
- Compose 项目：libra-diary；数据库 libra_diary，专用 PostgreSQL 容器、网络及命名卷。
- OSS：**libra-game 专用桶**，cn-beijing，公网 HTTPS endpoint，独立对象前缀 games/。桶为私有、已开启阻止公开访问及 AES256 默认加密，未配置跨地域内网 endpoint。原 `libra-diary` 桶实际存有 Knora-One 客户端安装包（client-releases/），已还给该用途：删除本项目测试对象并关闭本轮添加的桶级策略，仅保留其原有 private ACL。
- 邮件域：mail.game.penlibra.xin，阿里云 SPF、DKIM、DMARC、MX 已验证通过。
- 发件地址：noreply@mail.game.penlibra.xin；独立 SMTP 密码，不复用其他项目账户。
- SMTP：smtpdm.aliyun.com:465，隐式 TLS。已实际投递验证码邮件，两个验收邮箱均收到并成功登录。

SMTP 本地秘密配置在本目录的 .env.smtp，权限 0600，Git 已忽略。不要打印、提交或放入镜像。OSS AccessKey 不能代替 SMTP 密码。

## 隔离边界

独立 Compose 项目自动隔离网络、容器与数据卷；运行时凭据仅属于本项目。默认 worker 并发为 2，没有额外 CPU、内存或进程数限制，也不限制为单 worker。

共用的是用户指定的物理 ECS、其已有 Nginx 入口和阿里云账号。物理资源与账号级配额无法因 Compose 项目隔离而变成独占资源。此次没有修改已有项目服务、Nginx 配置或其他邮件账户。

## 部署状态

2026-10-21：**已部署并完成真实验收**。镜像已加载，迁移 0001–0004 已执行，api（healthy）与 worker（并发 2）运行中，`https://game.penlibra.xin/health` 返回 200。真实账号完成邮件验证码登录、设备列表与刷新；独立验收账号走通「创建游戏/档案 → OSS PUT → complete → worker 校验 → 下载核对 SHA-256」闭环，对象位于 `games/` 前缀，桶保持 private + AES256，匿名访问 403；损坏与缺失对象两种失败路径均不产生可下载快照。验收设备已全部撤销。详细步骤、结果与遗留测试数据见 [DEPLOYMENT-HANDOFF.md](DEPLOYMENT-HANDOFF.md)。

仍待 Windows 实机双设备验收与安装包；本机非 Windows 环境无法替代。

该主机已有 Nginx 和 Certbot，占用 80/443。部署时为 game.penlibra.xin 添加独立 Nginx 站点并签发独立证书，转发到 127.0.0.1:18090；不要启用 Compose 的 Caddy profile，也不要覆盖已有站点文件。

配置从 heyuan.env.example 开始，填入本项目的独立数据库密码、token 密钥、OSS 凭据及 .env.smtp 的发件配置。按 README 的显式迁移流程执行；不挂载其他项目数据库卷，不复用其他项目 JWT 密钥，不运行其他项目发布脚本。

静态验证：

    docker compose --env-file "server/deploy/heyuan.env.example" -f "server/deploy/compose.yaml" config --quiet

真实 .env 的 config 输出包含凭据，只使用 --quiet。容器启动和 HTTPS 发布仍需按实际部署操作完成；此文档不表示已经执行。

## 本站 HTTPS

独立配置为 `/etc/nginx/conf.d/libra-diary.conf`，模板见 `nginx-heyuan.conf`。HTTP-01 目录为 `/opt/apps/libra-diary/acme`；项目父目录须可被 Nginx 遍历，秘密 `.env` 单独保持 0600。证书位于 `/etc/letsencrypt/live/game.penlibra.xin/`，续期沿用已有 `certbot-renew.timer`，本站 deploy-hook 为 `nginx -t && systemctl reload nginx`。不得启用 Caddy。

仅验证本站续期：

    certbot renew --cert-name game.penlibra.xin --dry-run --no-random-sleep-on-renew

部署前旧容器已停止，包括 libra-stock-api；本次未启动或重建这些容器。原五个站点配置校验记录在服务器项目目录 `nginx-before.sha256`，均保持一致。
