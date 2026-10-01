# 源码工作约定

这是源码目录，完整工作目录位于上两级。

新会话开始工作前，读取以下文件：

- `../../AGENTS.md`：统一工程约定和目录说明。
- `../../PROJECT-HANDOFF.md`：最近完成的功能、掌机验证结果和待验证事项。
- `../../HANDHELD-OPERATIONS.md`：与 Windows 掌机连接、操作、构建和部署的具体方法。
- `README.md`、`XBOX-ACHIEVEMENTS.md`：功能说明和成就来源边界。

业务代码在此目录修改；不要把 `../../work/windows-build/project/` 当作主源码。
