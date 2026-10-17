/* ============================================================
   MCode Desktop 原型 · 共享脚本
   指令全集（对齐 MCode.md 2.4）+ 页面导航 + 通用交互
   ============================================================ */

// ---------- 指令全集（/ 补全面板数据源，与 CLI /command 全量对齐） ----------
const COMMANDS = [
  { group: '会话与上下文', name: '/clear',     desc: '清空上下文，重置对话' },
  { group: '会话与上下文', name: '/compact',   desc: '手动压缩历史（支持附加聚焦指令）' },
  { group: '会话与上下文', name: '/context',   desc: '查看上下文占用分布' },
  { group: '会话与上下文', name: '/export',    desc: '导出当前会话为文件或剪贴板' },
  { group: '会话与上下文', name: '/resume',    desc: '恢复历史会话' },
  { group: '会话与上下文', name: '/rewind',    desc: '回退到检查点（代码与对话）' },
  { group: '会话与上下文', name: '/todos',     desc: '查看/管理当前任务列表' },
  { group: '会话与上下文', name: '/copy',      desc: '复制最后一条回复' },
  { group: '会话与上下文', name: '/save',      desc: '导出 ECC save-session 格式快照' },
  { group: '配置与状态',   name: '/config',    desc: '打开配置面板' },
  { group: '配置与状态',   name: '/model',     desc: '交互式切换模型' },
  { group: '配置与状态',   name: '/status',    desc: '展示版本、模型、cwd、权限模式' },
  { group: '配置与状态',   name: '/cost',      desc: 'token 与费用统计汇总' },
  { group: '配置与状态',   name: '/token',     desc: '按模型打印 token 明细（MCode 特有）' },
  { group: '配置与状态',   name: '/language',  desc: '切换语言环境（MCode 特有）' },
  { group: '配置与状态',   name: '/doctor',    desc: '诊断安装与配置健康度' },
  { group: '配置与状态',   name: '/output-style', desc: '切换输出风格' },
  { group: '配置与状态',   name: '/statusline',   desc: '配置状态栏' },
  { group: '配置与状态',   name: '/terminal-setup', desc: '终端按键适配' },
  { group: '配置与状态',   name: '/vim',       desc: 'vim 编辑模式开关' },
  { group: '配置与状态',   name: '/memory',    desc: '编辑记忆文件（MCODE.md / 规则文件）' },
  { group: '权限与安全',   name: '/permissions', desc: '权限规则管理（allow/deny 列表）' },
  { group: '权限与安全',   name: '/add-dir',   desc: '追加工作目录' },
  { group: '扩展生态',     name: '/agents',    desc: 'subagent 管理与查看' },
  { group: '扩展生态',     name: '/hooks',     desc: 'hook 管理与查看' },
  { group: '扩展生态',     name: '/mcp',       desc: 'MCP server 管理与连接状态' },
  { group: '扩展生态',     name: '/init',      desc: '扫描仓库生成 MCODE.md' },
  { group: '扩展生态',     name: '/review',    desc: '代码评审' },
  { group: '扩展生态',     name: '/pr-comments', desc: '获取 PR 评论' },
  { group: '扩展生态',     name: '/pua',       desc: 'PUA 效率教练模式开关' },
  { group: '账号与帮助',   name: '/help',      desc: '帮助' },
  { group: '账号与帮助',   name: '/login',     desc: '模型供应商配置' },
  { group: '账号与帮助',   name: '/logout',    desc: '清除当前供应商' },
  { group: '账号与帮助',   name: '/bug',       desc: '问题反馈' },
  { group: '账号与帮助',   name: '/release-notes', desc: '版本说明' },
  { group: '账号与帮助',   name: '/upgrade',   desc: '检查更新' },
  { group: '账号与帮助',   name: '/ide',       desc: 'CC 专属（暂未支持）' },
  { group: '账号与帮助',   name: '/install-github-app', desc: 'CC 专属（暂未支持）' },
];

// ---------- @ 文件补全数据源 ----------
const FILE_TREE = [
  { path: 'src/', dir: true },
  { path: 'src/agent/', dir: true },
  { path: 'src/agent/loop.ts' },
  { path: 'src/agent/harness/', dir: true },
  { path: 'src/agent/harness/compaction.ts' },
  { path: 'src/agent/harness/context.ts' },
  { path: 'src/api/', dir: true },
  { path: 'src/api/client.ts' },
  { path: 'src/commands/', dir: true },
  { path: 'src/commands/builtin.ts' },
  { path: 'src/config/', dir: true },
  { path: 'src/config/config.ts' },
  { path: 'src/tools/', dir: true },
  { path: 'src/tools/bash.ts' },
  { path: 'src/tools/edit-file.ts' },
  { path: 'src/tools/registry.ts' },
  { path: 'MCode.md' },
  { path: 'package.json' },
];

// ---------- 四种权限模式 ----------
const MODES = {
  manual: { label: 'manual', cls: 'mode-manual', hint: '每个非只读工具执行前弹确认' },
  auto:   { label: 'auto',   cls: 'mode-auto',   hint: '白名单工具自动放行，其余确认' },
  plan:   { label: 'plan',   cls: 'mode-plan',   hint: '只读模式：仅只读工具，输出计划不落盘' },
  passon: { label: 'passon', cls: 'mode-passon', hint: '全自动放行（默认）' },
};
const MODE_ORDER = ['passon', 'manual', 'auto', 'plan'];

// ---------- 原型导航 ----------
const NAV_PAGES = [
  ['index.html',            '总览 · 能力映射'],
  ['screens/onboarding.html', '01 首次启动引导'],
  ['screens/chat.html',     '02 主对话界面 ★'],
  ['screens/settings.html', '03 设置中心'],
  ['screens/sessions.html', '04 会话管理'],
  ['screens/extensions.html', '05 扩展生态'],
];

function initProtoNav() {
  const nav = document.createElement('div');
  nav.className = 'proto-nav';
  const cur = location.pathname.split('/').pop() || 'index.html';
  const base = cur === 'index.html' ? '' : '..';
  nav.innerHTML = NAV_PAGES.map(([f, label]) =>
    `<a href="${base}/${f}" class="${f.endsWith(cur) ? 'current' : ''}">${label}</a>`
  ).join('');
  document.body.appendChild(nav);
}

// ---------- Toast ----------
let toastTimer;
function toast(msg) {
  let el = document.getElementById('toast');
  if (!el) { el = document.createElement('div'); el.id = 'toast'; document.body.appendChild(el); }
  el.textContent = msg;
  el.classList.add('show');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.remove('show'), 2200);
}

// ---------- 通用模态开关 ----------
function openModal(id) { document.getElementById(id)?.classList.add('open'); }
function closeModal(id) { document.getElementById(id)?.classList.remove('open'); }

// 权限确认弹窗（manual 模式演示，可复用）
function openPermissionModal({ tool, args, onAllow }) {
  const ov = document.getElementById('perm-overlay');
  if (!ov) return;
  ov.querySelector('.tool-name').textContent = tool;
  ov.querySelector('.tool-args').textContent = args;
  ov.classList.add('open');
  ov._onAllow = onAllow;
}

function initPermissionOverlay() {
  const ov = document.getElementById('perm-overlay');
  if (!ov) return;
  ov.addEventListener('click', (e) => {
    const act = e.target.closest('[data-act]')?.dataset.act;
    if (!act) return;
    ov.classList.remove('open');
    if (act === 'allow' || act === 'always') {
      toast(act === 'always' ? '✓ 已放行并记住：本会话内该工具不再询问' : '✓ 已放行，工具执行中…');
      ov._onAllow?.(act);
    } else if (act === 'deny') {
      toast('✕ 已拒绝，模型将收到拒绝原因');
    }
  });
}

// ---------- 模式徽章渲染 ----------
function modeBadge(mode) {
  const m = MODES[mode];
  return `<span class="badge ${m.cls}" title="${m.hint}"><span class="dot" style="background:currentColor"></span>${m.label}</span>`;
}

document.addEventListener('DOMContentLoaded', () => {
  initProtoNav();
  initPermissionOverlay();
  // 所有 [data-close] 关闭所在 overlay
  document.querySelectorAll('[data-close]').forEach(el =>
    el.addEventListener('click', () => el.closest('.overlay')?.classList.remove('open')));
  // 点 overlay 背景关闭
  document.querySelectorAll('.overlay').forEach(ov =>
    ov.addEventListener('click', e => { if (e.target === ov) ov.classList.remove('open'); }));
});
