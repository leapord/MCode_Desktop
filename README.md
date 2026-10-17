# MCode_Desktop

mcode CLI 的全能力 Rust 重写版：**GPUI（Zed 同款 GPU 渲染框架，crates.io Apache-2.0 版）前端 + 纯 Rust 内核**，对标 desktop-prototype 的交互规范。

## 工程结构

```
crates/
├── mcode-core/          # 纯 Rust 内核（无 UI 依赖，可单测）
│   ├── config/          #   config.json 唯一事实源（camelCase 与 CLI 逐字段互通）
│   ├── permission/      #   四权限模式 + 决策记忆 + 规则表（shift+tab 循环顺序有单测）
│   ├── commands/        #   38 条 slash 指令全集 + 补全过滤
│   ├── agent/           #   compact-harness：消息模型/系统提示词/压缩阈值/补全状态机
│   ├── api/             #   OpenAI Responses 协议客户端（SSE 流式 + 取消）
│   ├── tools/           #   6 内置工具 + 路径收敛防逃逸 + unified diff
│   ├── sessions/        #   JSONL + meta + rewind 检查点
│   ├── skills/          #   SKILL.md frontmatter 解析 + 渐进式披露 + 三级加载
│   ├── plugins/         #   plugin.json 清单 + 覆盖冲突解析
│   └── util/            #   ~/.mcode 目录约定 + token 台账（按模型分组）
├── mcode-ui/            # GPUI 前端（对标 docs/prototype/screens/chat.html）
│   ├── text_input.rs    #   输入组件（移植 gpui input 示例，含 IME 路径 + Submit 事件）
│   ├── app.rs           #   主界面：侧栏/顶栏/消息流/补全面板/权限弹窗/排队插话
│   └── theme.rs         #   设计令牌（与原型 CSS 变量 1:1）
docs/prototype/         # UI 规范 = desktop-prototype 全量拷贝（交互验收口径）
scripts/                # macOS .app 打包 / Windows x64 构建
```

## 运行

```bash
cargo run -p mcode-ui          # 开发运行
cargo test -p mcode-core      # 内核单测（补全状态机/权限语义/路径逃逸/diff）
./scripts/build-macos.sh      # 产出 target/release/bundle/MCode_Desktop.app
```

Windows：`pwsh scripts/build-windows.ps1`（需 MSVC 工具链 + VS Build Tools）。

## 里程碑

| 里程碑 | 内容 | 状态 |
|--------|------|------|
| M0（本次） | workspace 骨架 + 内核全模块 + GPUI 主界面（原型 chat 页布局/交互对齐）+ 双平台打包脚本 | ✅ 可编译 |
| M1 | UI ↔ 内核接线：ResponsesBackend 真实流式回合、SessionStore 持久化、权限弹窗走 evaluate() 裁决 | 待做 |
| M2 | settings / sessions / extensions / onboarding 四个视图（对标 docs/prototype/screens/） | 待做 |
| M3 | task 工具派发 subagent、MCP 桥接、hooks 生命周期 | 待做 |

## 能力对齐验收表

以 `docs/prototype/index.html` 的 CLI→Desktop 能力映射表（22 项）为验收口径，逐项打勾。

## 已知简化（如实记录）

- bash 工具暂未回传真实 exit code（占位 0），M1 接线时补；
- 补全面板键盘导航绑定全局上下文，M1 收敛到 Composer 焦点上下文；
- UI 消息流当前为静态种子数据，M1 起由内核事件驱动。
