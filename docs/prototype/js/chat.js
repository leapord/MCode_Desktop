/* ============================================================
   MCode Desktop · 主对话交互逻辑
   CompletionState 状态机对齐 MCode.md 2.2：
   { trigger: '/' | '@' | null, query, selectedIndex }
   ============================================================ */

const input = document.getElementById('input');
const panel = document.getElementById('completion-panel');
const modeBadgeEl = document.getElementById('mode-badge');
const sendBtn = document.getElementById('send-btn');
const runningMsg = document.getElementById('running-msg');
const queueBar = document.getElementById('queue-bar');

// ---------- 状态 ----------
let state = {
  trigger: null,       // '/' | '@' | null
  query: '',
  selectedIndex: 0,
  history: ['修复多模态图片识别异常', '/pua:on', '看下 @src/api/client.ts 排查识别失败'],
  historyIdx: -1,
  draft: '',
};
let modeIdx = 0;                       // passon → manual → auto → plan
let isRunning = false;                 // 模拟运行中
let queue = [];                        // 排队插话
let runTimer = null;
let runStage = 0;
const RUN_STAGES = ['Thinking...', '读取上下文与 skill 索引...', '调用工具 edit_file...', '生成回复...'];

// ---------- 模式切换（shift+tab / 点击徽章） ----------
function renderMode() {
  const m = MODE_ORDER[modeIdx];
  modeBadgeEl.className = 'badge ' + MODES[m].cls;
  modeBadgeEl.innerHTML = `<span class="dot" style="background:currentColor"></span>⇥ ${m}`;
  modeBadgeEl.title = MODES[m].hint + ' · 点击或 shift+tab 切换';
}
modeBadgeEl.addEventListener('click', () => { modeIdx = (modeIdx + 1) % MODE_ORDER.length; renderMode(); toast(`权限模式 → ${MODE_ORDER[modeIdx]}`); });
renderMode();

// manual 模式演示：切到 manual 后发送消息会先弹权限确认
modeBadgeEl.addEventListener('dblclick', () => {
  if (MODE_ORDER[modeIdx] === 'manual') {
    openPermissionModal({
      tool: 'bash',
      args: 'npm test -- --filter client\n\n（manual 模式：非只读工具执行前必须确认）',
    });
  }
});

// ---------- 补全面板 ----------
function candidates() {
  if (state.trigger === '/') {
    const q = state.query.toLowerCase();
    return COMMANDS.filter(c => c.name.toLowerCase().includes(q));
  }
  if (state.trigger === '@') {
    const q = state.query.toLowerCase();
    return FILE_TREE.filter(f => f.path.toLowerCase().includes(q)).slice(0, 12);
  }
  return [];
}

function renderPanel() {
  const list = candidates();
  if (!state.trigger) { panel.classList.remove('open'); return; }
  panel.classList.add('open');
  if (state.selectedIndex >= list.length) state.selectedIndex = Math.max(0, list.length - 1);

  if (state.trigger === '/') {
    // 分组渲染（内置 + plugin + skill 指令统一入口）
    const groups = {};
    list.forEach(c => (groups[c.group] ??= []).push(c));
    let idx = 0;
    panel.innerHTML = Object.entries(groups).map(([g, items]) =>
      `<div class="cp-group">${g}</div>` + items.map(c => {
        const sel = idx++ === state.selectedIndex ? 'sel' : '';
        return `<div class="cp-item ${sel}" data-name="${c.name}"><span class="name">${c.name}</span><span class="desc">${c.desc}</span></div>`;
      }).join('')
    ).join('') + (list.length === 0 ? '<div class="cp-empty">无匹配指令</div>' : '');
  } else {
    panel.innerHTML = list.map((f, i) => {
      const sel = i === state.selectedIndex ? 'sel' : '';
      return `<div class="cp-item ${sel}" data-name="${f.path}"><span class="file-ico">${f.dir ? '📁' : '📄'}</span><span class="name">${f.path}</span></div>`;
    }).join('') + (list.length === 0 ? '<div class="cp-empty">无匹配文件</div>' : '');
  }

  panel.querySelectorAll('.cp-item').forEach(el =>
    el.addEventListener('click', () => applyCompletion(el.dataset.name)));
  const sel = panel.querySelector('.cp-item.sel');
  sel?.scrollIntoView({ block: 'nearest' });
}

function updateTrigger() {
  const v = input.value;
  // 简化版触发检测：取光标前最后一个 token
  const m = v.match(/(^|\s)(\/|@)([^\s]*)$/);
  if (m) {
    state.trigger = m[2];
    state.query = m[3];
    state.selectedIndex = 0;
  } else {
    state.trigger = null;
    state.query = '';
  }
  renderPanel();
}

function applyCompletion(name) {
  if (!state.trigger) return;
  input.value = input.value.replace(/(^|\s)(\/|@)([^\s]*)$/, (s, p1) => `${p1}${name}${state.trigger === '@' ? ' ' : ''}`);
  state.trigger = null;
  renderPanel();
  input.focus();
}

// ---------- 键盘（对齐 CLI 按键优先级：补全打开时 ↑↓ 归补全，否则召回历史） ----------
input.addEventListener('keydown', (e) => {
  const list = candidates();

  // shift+tab：模式循环
  if (e.shiftKey && e.key === 'Tab') {
    e.preventDefault();
    modeIdx = (modeIdx + 1) % MODE_ORDER.length;
    renderMode();
    toast(`权限模式 → ${MODE_ORDER[modeIdx]}`);
    return;
  }

  // 补全面板打开时接管 ↑↓ / tab / enter
  if (state.trigger) {
    if (e.key === 'ArrowDown') { e.preventDefault(); state.selectedIndex = (state.selectedIndex + 1) % Math.max(1, list.length); renderPanel(); return; }
    if (e.key === 'ArrowUp')   { e.preventDefault(); state.selectedIndex = (state.selectedIndex - 1 + list.length) % Math.max(1, list.length); renderPanel(); return; }
    if (e.key === 'Tab' && list.length) { e.preventDefault(); applyCompletion(list[state.selectedIndex].name ?? list[state.selectedIndex].path); return; }
    if (e.key === 'Enter' && !e.shiftKey && list.length) {
      e.preventDefault();
      applyCompletion(list[state.selectedIndex].name ?? list[state.selectedIndex].path);
      return;
    }
    if (e.key === 'Escape') { state.trigger = null; renderPanel(); return; }
  } else {
    // 历史召回
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (state.historyIdx === -1) state.draft = input.value;
      if (state.historyIdx < state.history.length - 1) {
        state.historyIdx++;
        input.value = state.history[state.history.length - 1 - state.historyIdx];
      }
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (state.historyIdx > 0) {
        state.historyIdx--;
        input.value = state.history[state.history.length - 1 - state.historyIdx];
      } else if (state.historyIdx === 0) {
        state.historyIdx = -1;
        input.value = state.draft;
      }
      return;
    }
  }

  if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); onSend(); }
  if (e.key === 'Escape' && isRunning) { e.preventDefault(); interrupt(); }
});

input.addEventListener('input', updateTrigger);
input.addEventListener('click', updateTrigger);

// ---------- 发送 / 排队插话 / 打断 ----------
function onSend() {
  const text = input.value.trim();
  if (!text) return;
  input.value = '';
  state.trigger = null;
  renderPanel();
  state.history = [...state.history.filter(h => h !== text), text];
  state.historyIdx = -1;

  if (isRunning) {
    // 运行中 → 排队插话（对齐 CLI 排队特性）
    queue.push(text);
    renderQueue();
    return;
  }
  appendUserMsg(text);
  startRun();
}

function renderQueue() {
  queueBar.innerHTML = queue.length
    ? `<span class="queue-pill">📥 ${queue.length} 条排队插话：${escapeHtml(queue[0]).slice(0, 24)}…
         <span class="cancel" onclick="cancelQueue(0)">✕</span></span>`
    : '';
}
function cancelQueue(i) { queue.splice(i, 1); renderQueue(); }
window.cancelQueue = cancelQueue;

function appendUserMsg(text) {
  const messages = document.getElementById('messages');
  const div = document.createElement('div');
  div.className = 'msg user';
  div.innerHTML = `<div class="role"><span class="avatar">你</span> 用户 <span class="time">刚刚</span></div>
    <div class="bubble">${escapeHtml(text)}</div>`;
  messages.insertBefore(div, runningMsg);
  messages.scrollTop = messages.scrollHeight;
}

function startRun() {
  isRunning = true;
  runStage = 0;
  runningMsg.style.display = 'block';
  sendBtn.classList.add('stop');
  sendBtn.innerHTML = '⏹ 停止';
  document.getElementById('idle-state').style.display = 'none';
  tickRun();
}
function tickRun() {
  document.getElementById('running-text').innerHTML = RUN_STAGES[runStage] + '<span class="dotpulse"><span>.</span><span>.</span><span>.</span></span>';
  if (runStage === 2 && MODE_ORDER[modeIdx] === 'manual' && !window._permShownThisRun) {
    window._permShownThisRun = true;
    openPermissionModal({ tool: 'edit_file', args: 'path: src/api/client.ts\nedit: 修复 multimodal 校验逻辑\n\n（manual 模式下自动弹出，演示权限门控）' });
  }
  runStage++;
  runTimer = setTimeout(() => {
    if (runStage >= RUN_STAGES.length + 1) finishRun();
    else tickRun();
  }, 900);
}
function finishRun() {
  isRunning = false;
  window._permShownThisRun = false;
  runningMsg.style.display = 'none';
  sendBtn.classList.remove('stop');
  sendBtn.innerHTML = '➤ 发送';
  document.getElementById('idle-state').style.display = 'flex';
  if (queue.length) {
    const next = queue.shift();
    renderQueue();
    appendUserMsg(next);
    startRun();
  }
}
function interrupt() {
  clearTimeout(runTimer);
  finishRun();
  toast('⏹ 已打断（AbortController 终止当前请求与工具执行）');
}
sendBtn.addEventListener('click', () => { if (isRunning) interrupt(); });

// ---------- 附件（多模态） ----------
function addAttach() {
  document.getElementById('attach-row').style.display = 'flex';
  toast('已附加图片（生产版支持拖拽 / ⌘V 粘贴 / 文件选择）');
}
function removeAttach() {
  document.getElementById('attach-row').style.display = 'none';
}

// ---------- 侧栏 tabs ----------
document.querySelectorAll('.side-tabs .st').forEach(el =>
  el.addEventListener('click', () => {
    document.querySelectorAll('.side-tabs .st').forEach(x => x.classList.remove('active'));
    el.classList.add('active');
    toast(el.dataset.tab === 'files' ? '文件页签：工作区文件树（对齐 @ 补全同一索引）' : '会话页签');
  }));

document.querySelectorAll('#session-list .side-item').forEach(el =>
  el.addEventListener('click', () => {
    document.querySelectorAll('#session-list .side-item').forEach(x => x.classList.remove('active'));
    el.classList.add('active');
    toast('切换会话：恢复消息流与配置（/resume）');
  }));

// ---------- utils ----------
function escapeHtml(s) {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

// textarea 自动增高
input.addEventListener('input', () => {
  input.style.height = 'auto';
  input.style.height = Math.min(180, input.scrollHeight) + 'px';
});

input.focus();
