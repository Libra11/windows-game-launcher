# 存档路径识别规则

`save-rules.json` 是 Ludusavi Manifest 的保守 Windows 子集，来源：
https://github.com/mtkennerly/ludusavi-manifest

原始数据下载于 2026-10-21，SHA-256 记录于 JSON 的 `sha256` 字段。
上游 MIT 许可证完整保留在 `LUDUSAVI-LICENSE`。本快照含 12,194 款游戏、14,349 条路径模式，规则数量不代表每款游戏都能在所有版本上定位成功。

仅保留标记为 save、适用于 Windows、可由已知系统目录或安装目录解析的模式。
不支持注册表存档、递归通配符、父目录跳转、未知占位符，也不实现完整 Ludusavi 备份格式。
Steam 与已填写 Steam AppID 的本地游戏按 ID 匹配；其他条目按准确名称匹配（忽略英文大小写），不做模糊猜测。
`{account}` 只匹配数字账号子目录，Steam userdata 的多个账号分别作为候选。

识别在后台仅读取文件元信息，最多 64 个候选，共享遍历预算 20,000 项和 3 秒的协作式截止时间；单次系统文件操作仍可能受磁盘延迟影响。
不会创建存档目录、确认绑定、上传或恢复文件。候选可能包含配置文件，用户必须确认它是该游戏的专用目录，因为现有恢复会替换整个目录。

更新规则：下载上游 manifest.yaml，在开发用 Python 环境安装 PyYAML，运行：

```
python scripts/import-save-rules.py <manifest.yaml>
```

应用运行不依赖 Python、不访问上游网站、不上传本机路径。不自动热更新规则；随客户端源码版本更新。
