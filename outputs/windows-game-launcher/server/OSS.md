# OSS 存档存储配置与权限

本文对应当前 `storage.rs` 和 `storage/` 实现。仅说明配置与运行边界，不执行创建桶、授权或部署。

## 接口与端点

使用 OSS 原生 V4 签名（`OSS4-HMAC-SHA256`），不是 S3 兼容接口。实际 URL 使用 virtual-hosted 形式，例如 `https://examplebucket.oss-cn-hangzhou.aliyuncs.com/objects/...`。签名固定主机、请求方法、对象键；上传同时固定长度、Content-Type、对象私有 ACL 和加密头。客户端必须原样发送上传接口返回的全部 headers。[官方 V4 规范](https://www.alibabacloud.com/help/en/oss/developer-reference/add-signatures-to-urls)

| 环境变量 | 配置 |
|---|---|
| `OSS_ENDPOINT` | 公网地域根域，例如 `https://oss-cn-hangzhou.aliyuncs.com`；允许省略 HTTPS 前缀，但不能包含桶名、路径、查询、端口或自定义域名 |
| `OSS_INTERNAL_ENDPOINT` | 可选，同地域内网根域，例如 `https://oss-cn-hangzhou-internal.aliyuncs.com` |
| `OSS_PREFIX` | 必须为非空相对目录，默认 `games`；对象写入该目录下，部署后不得随意更改 |
| `OSS_BUCKET` | 应用专用桶名 |
| `OSS_REGION` | 例如 `cn-hangzhou`，不带 `oss-` 前缀，必须与端点一致 |
| `OSS_ACCESS_KEY_ID`、`OSS_ACCESS_KEY_SECRET` | RAM 凭据或 STS 临时凭据的 AccessKey 对 |
| `OSS_SECURITY_TOKEN` | 可选，与临时 AccessKey 对配套的 STS token |
| `USER_QUOTA_BYTES` | 每用户对象及待清理容量预留上限，默认 10 GiB；统计正式对象 + GC 预留，不使用 `OSS_USER_QUOTA_BYTES` |

返回给桌面端的上传、下载 URL 始终使用公网端点。服务端安全查询、下载校验、发布和删除优先使用内网端点；未配置时使用公网端点。配置内网后失败不会自动改走公网，只有服务端确实能够访问同地域 OSS 内网时才应填写。

完全未配置 OSS 时存储功能禁用；只提供部分 OSS 配置时启动报错。应用配额本身不启用 OSS。

配额包含暂存对象、每次校验尝试的发布预留、失败/过期对象及延后删除的历史对象。正式发布将 GC 预留转为正式对象，不重复计费；实际删除成功并清除 GC 记录后才释放容量。相同内容查重命中且未执行发布时仅释放该次空发布预留；暂存对象继续占用容量。新内容通常需要暂存和发布两份容量，详见 [后台队列容量规则](SAVE-JOBS.md#oss-容量预留与清理)。

## 桶必须满足的条件

管理员事先设置以下内容。API 和 worker 只读取并验证，不自动修改桶配置：

| 检查接口 | 必须取得的结果 |
|---|---|
| `GetBucketAcl` | `AccessControlPolicy/AccessControlList/Grant = private` |
| `GetBucketPublicAccessBlock` | `PublicAccessBlockConfiguration/BlockPublicAccess = true` |
| `GetBucketPolicyStatus` | `PolicyStatus/IsPublic = false` |
| `GetBucketEncryption` | `ServerSideEncryptionRule/ApplyServerSideEncryptionByDefault/SSEAlgorithm` 为 `AES256` 或 `KMS` |

参考：[桶 ACL](https://www.alibabacloud.com/help/en/oss/developer-reference/getbucketacl)、[阻止公开访问](https://www.alibabacloud.com/help/en/oss/developer-reference/getbucketpublicaccessblock)、[策略公开状态](https://www.alibabacloud.com/help/en/oss/developer-reference/getbucketpolicystatus)、[默认加密](https://www.alibabacloud.com/help/en/oss/developer-reference/getbucketencryption)。

仅桶 ACL 私有还不够：阻止公开访问必须开启，策略检查必须明确返回非公开。403、超时、接口不可用、配置缺失、重复或不符合格式的关键字段均视为检查失败，不签发上传／下载 URL，也不降级为公开或未加密存储。

上传和发布对象显式带 `x-oss-object-acl: private` 以及检查得到的 SSE 算法；桶使用 KMS 且指定 `KMSMasterKeyID` 时，保留对应 `x-oss-server-side-encryption-key-id`，不改用 AES256。此加密由 OSS 解密后提供下载，不是端到端加密。

## 最小运行时 RAM 权限

以下是同一服务凭据供 API、worker 和回收逻辑使用时所需的最小 action 集合。授权范围只覆盖应用专用桶及 `staging/*`、`objects/*` 两个对象前缀：

| 范围 | Actions |
|---|---|
| 桶 `acs:oss:*:*:YOUR_BUCKET` | `oss:GetBucketAcl`、`oss:GetBucketPublicAccessBlock`、`oss:GetBucketPolicyStatus`、`oss:GetBucketEncryption` |
| 对象 `acs:oss:*:*:YOUR_BUCKET/games/staging/*` 与 `acs:oss:*:*:YOUR_BUCKET/games/objects/*` | `oss:PutObject`、`oss:GetObject`、`oss:DeleteObject` |
| 使用 SSE-KMS 时，对实际使用的 KMS 密钥另行授权 | `kms:GenerateDataKey`、`kms:Decrypt` |

对象 action 与 KMS 要求见官方 [PutObject](https://www.alibabacloud.com/help/en/oss/developer-reference/putobject)、[GetObject](https://www.alibabacloud.com/help/en/oss/developer-reference/getobject)、[DeleteObject](https://www.alibabacloud.com/help/en/oss/developer-reference/deleteobject)。KMS 密钥策略也必须允许当前主体使用密钥；不要把 KMS actions 放在 OSS 资源 ARN 上。

OSS 运行时策略模板如下，需替换 `YOUR_BUCKET`：

```json
{
  "Version": "1",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "oss:GetBucketAcl",
        "oss:GetBucketPublicAccessBlock",
        "oss:GetBucketPolicyStatus",
        "oss:GetBucketEncryption"
      ],
      "Resource": ["acs:oss:*:*:YOUR_BUCKET"]
    },
    {
      "Effect": "Allow",
      "Action": [
        "oss:PutObject",
        "oss:GetObject",
        "oss:DeleteObject"
      ],
      "Resource": [
        "acs:oss:*:*:YOUR_BUCKET/games/staging/*",
        "acs:oss:*:*:YOUR_BUCKET/games/objects/*"
      ]
    }
  ]
}
```

当前实现不调用列桶、列对象、修改桶权限、修改桶加密或独立的 PutObjectACL 接口；对象 ACL 随 PutObject 的头一起设置，不为未调用的 API 增加权限。初始化桶的管理权限应与上述运行时凭据分离。代码不处理 OSS 历史 versionId：若桶启用版本控制，普通删除可能只产生删除标记，应用配额不等于含历史版本的 OSS 实际账单空间。建议使用专用、从未启用版本控制的桶；暂停版本控制并不会自动删除已有历史版本，本轮不扩展 versionId 管理。

## STS、超时与签名期限

`OSS_SECURITY_TOKEN` 在进程启动时读取，属于静态 STS 配置。实现不调用 AssumeRole，不访问实例角色元数据，也不会自动刷新 token。临时凭据到期后请求会失败；外部凭据管理流程须更新配套的 AccessKey ID、Secret 和 token，并重启使用它们的 API／worker。预签名 URL 的声明期限不能延长底层 STS 凭据的有效期。

| 参数 | 当前值及含义 |
|---|---|
| HTTP 建连超时 | 10 秒 |
| 单次 OSS 控制查询 | 15 秒，包含响应体读取；四项检查依次执行，15 秒不是整个 API 的总时限 |
| 服务端对象请求 | 900 秒，包含对象传输；不是后台任务的总时限，也不改变桌面客户端的 HTTP 超时 |
| 对外上传／下载签名 | 600 秒 |
| 服务端对象 GET／PUT 签名 | 600 秒 |
| 控制查询／DELETE 签名 | 60 秒 |

所有客户端禁用自动重定向，避免已签名请求跟随到其他地址。控制查询响应最多读取 64 KiB。对象下载按预留大小流式限制；后台校验通过后才发布快照，上传完成通知不代表验证成功。

## 已验证范围

本地测试覆盖配置拒绝边界、公网与内网分离、公开配置与权限失败拒绝访问、KMS 头保留、对象传输与大小限制，并逐项匹配[阿里云官方 SDK 的三组 V4 签名向量](https://github.com/aliyun/alibabacloud-oss-python-sdk-v2/blob/master/tests/unit/signer/test_v4.py)。这些测试使用本机 HTTP mock 与隔离数据库，不代表已完成真实 OSS、RAM 或网络环境验收。

对象账本保留相对键，存储层统一添加 OSS_PREFIX，上传、下载、校验、发布与回收均经过相同前缀。空前缀、绝对路径与目录穿越会被拒绝。更改已部署的前缀会使旧对象不可达，必须先制定迁移方案。桶级安全查询不添加对象前缀。
