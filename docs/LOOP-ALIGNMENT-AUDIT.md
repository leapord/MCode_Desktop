# loop.ts ↔ runner.rs 对齐审计（2026-10-18）

逐机制核对 mcode/src/agent/loop.ts（CLI）与 MCode_Desktop/crates/mcode-core/src/agent/runner.rs。

## 结论：核心机制全部对齐（审计后修复 2 处缺口）

| # | loop.ts 机制 | runner.rs | 状态 |
|---|---|---|---|
| 1 | 压缩门控：估算 ≥100k（chars/4）触发，keepTurns=max(2,⌊msgs/4⌋)，摘要 "Earlier conversation had N messages, starting with: '前200字…'"，emit compacted | 同 | ✅ |
| 2 | `while (turnCount < MAX_TOOL_TURNS)`，常量 25 | 同（opts.max_turns，TaskTool 缺省对齐 25） | ✅（修复后） |
| 3 | 流式重试：TRANSIENT 正则族、10 次上限、退避 min(1000·2^n, 10s)、emit retrying、重试前查 abort | 同（含 reqwest 错误表述补充） | ✅ |
| 4 | usage → tracker.add + emit usage（每次流调用） | 同（ledger.record） | ✅ |
| 5 | 无工具调用 → pushTurn(**原始** userInput, reply)，工具中间轮不落 history | 同 | ✅ |
| 6 | abort 检查：每次流调用前 + 每个工具执行前 | 同 | ✅ |
| 7 | unknown tool → `[name] error: unknown tool` | 同 | ✅ |
| 8 | 顺序：permission → validateInput → emit tool_start → execute → emit tool_result | 同（前置 hooks PreToolUse，桌面扩展） | ✅ |
| 9 | 回喂格式 `Tool call results:\n[name] output\n\nContinue based on these results. When the task is complete...` | 逐字符一致 | ✅ |
| 10 | max turns → "(stopped: max tool turns reached)" push + done | 同 | ✅ |
| 11 | aborted → done 空回复返回 ''；其余错误 → error 事件返回 '' | 同 | ✅ |
| 12 | attachments 仅 turnCount===0 传入 | 同 | ✅ |
| 13 | /goal 外层 12 轮 + buildGoalPrompt + checkGoalComplete 独立裁判（fail-open） | core 已补 check_goal_complete + MAX_GOAL_ROUNDS；UI 接线由 M2 agent 完成 | ✅（修复后） |

## 有意差异（非缺口）

- hooks PreToolUse 在 permission 之前执行：CLI 无 hooks 引擎，桌面版按 Claude Code 惯例插入，不影响原有链路。
- Plan 模式拒绝理由为中文明确文案（CLI 为 "denied by permission mode"）。
- runner 的 while 条件使用 opts.max_turns 参数（CLI 硬编码常量，其 AgentOptions.maxTurns 实际未生效——Rust 版修正为参数化，TaskTool 缺省值取 25 以对齐 CLI 实际行为）。
