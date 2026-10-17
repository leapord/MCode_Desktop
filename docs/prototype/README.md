# MCode Desktop · GUI 桌面版原型

> CLI → Desktop 全能力对齐的静态交互原型。纯 HTML/CSS/JS，零构建、零依赖，
> 浏览器直接打开 `index.html` 即可体验全部交互逻辑。

## 快速开始

```bash
open desktop-prototype/index.html        # macOS
# 或直接双击 index.html
```

右上角悬浮导航可在 6 个页面间切换。带「可试」标注的交互都是真实可操作的。

## 页面清单

| # | 文件 | 内容 | 可操作交互 |
|---|------|------|-----------|
| — | `index.html` | 总览 + CLI→Desktop 能力映射表 + 架构图 | 页面导航 |
| 01 | `screens/onboarding.html` | 首次启动引导（语言→供应商→权限→完成） | 四步向导、passon 风险警示、config.json 预览 |
| 02 | `screens/chat.html` | **主对话界面（核心）** | `/` 指令面板（38 条全量）、`@` 文件补全（目录下探）、shift+tab 四模式循环、manual 权限确认弹窗、发送/排队插话/打断、↑↓ 历史召回、thinking 折叠、工具卡片展开、附件多模态 |
| 03 | `screens/settings.html` | 设置中心 | 供应商切换/重置、默认模式选择、语言切换、权限规则、/token 明细 |
| 04 | `screens/sessions.html` | 会话管理 | resume/continue、rewind 检查点时间线、/compact、/context 占用、/save、/export |
| 05 | `screens/extensions.html` | 扩展生态 | skill 渐进式披露展开预览、agent 白名单、MCP 连接状态、hooks 表、插件加载顺序 |

## 已验证的交互链路（浏览器实测）

- `/` → 38 条指令分组面板 → 输入过滤 → ↑↓ 循环 → Esc 关闭 ✓
- `@src` → 文件树过滤（目录+文件）→ Esc 关闭 ✓
- shift+tab → passon → manual → auto → plan 循环 + 颜色语义 ✓
- manual 模式发送 → 运行至工具调用阶段自动弹权限确认（允许 / 总是允许 / 拒绝）✓
- 运行中发送 → 入队「排队插话」徽标；打断后队列自动开跑新一轮 ✓
- 停止按钮 / ESC → AbortController 打断语义 ✓
- ↑ 召回历史输入、↓ 恢复草稿 ✓

## 架构决策（对齐 MCode.md）

1. **内核零改动平移**：CLI 的 Agent Core（harness / tools / permission / sessions / plugins）
   整体作为桌面版 Main 进程库复用；Ink TUI 与 GUI 双前端共享同一内核，能力 100% 对齐。
2. **Renderer 是纯视图**：流式输出、工具事件、权限请求、补全索引全部走 IPC 事件流。
3. **数据互通**：`~/.mcode` 目录结构不变，CLI 与 Desktop 读写同一份 config.json / sessions / skills。
4. **多模态对齐**：模型带 `multimodal` 标记才允许图片附件输入，否则拒绝并降级为文本提示（对齐 CLI 近期修复）。
5. **PUA / pua:on**、异步扫描等特性由同一 registry 提供，双端同步生效。

## 下一步（静态原型 → 可运行桌面版）

1. Electron/Tauri shell + IPC 协议定型（invoke + event stream 两个通道）；
2. Renderer 迁移到 React + TS（本原型补全状态机直接翻成 `useCompletion` hook，逻辑 1:1）；
3. Main 进程加载现有 `src/` 内核，权限确认从 stdout 提示改为 IPC 请求-响应；
4. 按能力映射表逐项打勾验收。
