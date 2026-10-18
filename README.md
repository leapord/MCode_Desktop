<div align="center">

# MCode Desktop

**桌面端 AI 编程智能体 —— 全 Rust 实现，GPU 加速原生界面**

基于 GPUI 的 AI coding agent，能力对齐终端版 MCode，与 CLI 共享 `~/.mcode` 数据。

</div>

---

## 功能

### Agent 核心

- **compact-harness 主循环**：模型调用 → 工具执行 → 结果回喂，自动多轮推进直到任务完成（最多 25 轮工具循环）
- **流式输出**：回复与思考过程逐字实时渲染，思考块默认展开为滚动窗口，随时观察、随时打断（ESC）
- **思考深度调节**：`/effort low / medium / high` 控制模型推理深度
- **自动压缩**：上下文逼近阈值时自动压缩历史，长任务不爆窗口
- **目标循环**：`/goal <目标>` 持续自主推进，独立裁判判定达成，12 轮上限防失控
- **打断与排队**：ESC 立即打断运行中的回合；运行中输入自动排队，回合结束续跑

### 工具体系

内置 10 个工具，全部经工作区路径防逃逸校验：

| 工具 | 能力 |
|------|------|
| `read_file` / `write_file` / `edit_file` | 文件读写与精确替换（唯一匹配校验 + diff 展示） |
| `bash` | Shell 命令执行，真实 exit code，可取消、可超时 |
| `glob` / `grep` | 文件名模式匹配与正则内容搜索 |
| `webfetch` / `websearch` | 网页抓取与网络搜索 |
| `skill` | 按需加载技能全文（渐进式披露） |
| `task` | 派发 subagent（独立上下文） |

### 扩展生态

- **MCP 客户端**：stdio transport，JSON-RPC 握手 / 工具发现 / 动态调用，MCP 工具与内置工具同一权限门控
- **Hooks 引擎**：`PreToolUse` / `PostToolUse` / `Stop` 生命周期钩子，shell 命令 + JSON stdin，退出码 2 拒绝执行
- **Subagent**：内置 general-purpose / code-explorer / planner / code-reviewer，支持用户自定义 agent；独立上下文隔离，事件带 scope 冒泡
- **Skills**：323 个内置技能（ecc-core + pua-core）+ 用户/项目级技能目录，渐进式披露

### 权限与安全

- **四权限模式**：`passon`（全自动）/ `manual`（逐次确认）/ `auto`（白名单放行）/ `plan`（只读），⇧Tab 循环切换
- **权限弹窗**：允许 / 总是允许 / 拒绝 三路裁决，规则持久化
- **路径防逃逸**：所有文件操作收敛在工作区内

### 会话与界面

- **会话持久化**：JSONL 消息流 + 元数据，会话列表 / 恢复 / 快照导出
- **工作区管理**：启动选择器 + 最近打开列表，顶栏一键切换
- **文件浏览器**：VS Code 式树形浏览，文件级复制 / 剪切 / 粘贴 / 新建 / 删除，点文件即插入 `@path` 引用
- **@ 引用与 / 指令**：`@` 补全真实工作区文件树，38+ 条斜杠指令全量可执行
- **多模态**：图片附件按模型能力门控，非多模态模型自动降级
- **原生渲染**：GPUI GPU 加速，单二进制，无 Electron

## 快速开始

### 下载预编译版本

从 [Releases](https://github.com/materin-tech/MCode_Desktop/releases) 下载：

- **macOS**（Apple Silicon）：`MCode_Desktop-macos-arm64.dmg` 或 `.zip`
- **Windows** x64：`MCode_Desktop-win64.zip`

### 从源码构建

```bash
git clone https://github.com/materin-tech/MCode_Desktop.git
cd MCode_Desktop

# macOS
./scripts/build-macos.sh        # → target/release/bundle/MCode_Desktop.app

# Windows（PowerShell，需 Rust stable-msvc + VS Build Tools）
./scripts/build-windows.ps1     # → target/release/bundle/MCode_Desktop-win64.zip
```

依赖：Rust 1.85+（edition 2024）。macOS 构建自动编译 Metal 着色器；Windows 构建通过 build.rs 内嵌应用图标。

### 首次配置

启动后按引导完成两步：

1. **供应商配置**：填入名称、baseURL、API key（环境变量名或明文）、模型列表
2. **选择工作区**：从最近列表选择或输入目录

配置与 CLI 版 MCode 完全互通：`~/.mcode/config.json` 是唯一事实源，桌面扩展配置（权限规则 / MCP / hooks）在 `~/.mcode/desktop.json`。

## 与 MCode CLI 的关系

MCode Desktop 与 [MCode CLI](https://github.com/materin-tech/mcode) 共享同一套数据目录与配置格式：会话历史、技能、agent、`config.json` 双端互通，桌面版与终端版可以无缝接力工作。

## 许可证

[MIT](LICENSE) © 2026 Materin-tech
