//! 运行时桥接 — tokio 后台线程跑 harness，GPUI 主线程经 channel 收事件。
//! UI 事件流统一为 UiEvent（Agent 事件 + 权限确认请求），单泵转发进 GPUI 实体。

use anyhow::Result;
use mcode_core::agent::context::load_project_doc;
use mcode_core::agent::runner::{AgentDeps, ConfirmRequest, ConfirmSender};
use mcode_core::agent::subagent::{all_agents, TaskTool};
use mcode_core::agent::ModelBackend;
use mcode_core::api::ResponsesBackend;
use mcode_core::hooks::HookEngine;
use mcode_core::permission::PermissionManager;
use mcode_core::tools::{EventSink, ToolRegistry};
use mcode_core::util::TokenLedger;
use mcode_core::{agent::AgentEvent, config::Config};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// tokio 运行时（独立线程，GPUI 主线程不阻塞）
pub struct UiRuntime {
    rt: tokio::runtime::Runtime,
}

impl UiRuntime {
    pub fn new() -> Result<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        Ok(Self { rt })
    }

    pub fn spawn<F>(&self, fut: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.rt.spawn(fut);
    }

    pub fn block_on<F: std::future::Future>(&self, fut: F) -> F::Output {
        self.rt.block_on(fut)
    }
}

/// UI 统一事件流
pub enum UiEvent {
    Agent(AgentEvent),
    Confirm(ConfirmRequest),
}

/// harness 组装结果：deps 交给回合任务，事件流交给 GPUI 泵
pub struct Harness {
    pub deps: Arc<AgentDeps>,
    pub ui_tx: tokio::sync::mpsc::UnboundedSender<UiEvent>,
    pub ui_rx: tokio::sync::mpsc::UnboundedReceiver<UiEvent>,
    pub mcp_report: Vec<(String, Result<usize>)>,
}

/// 构建 harness：后端 + 工具注册表（内置/web/skill/task/MCP）+ 权限 + hooks + ledger。
/// MCP 连接为真实进程握手（initialize → tools/list）。
pub fn build_harness(cfg: &Config, cwd: &PathBuf, rt: &UiRuntime) -> Result<Harness> {
    rt.block_on(build_harness_async(cfg, cwd))
}

async fn build_harness_async(cfg: &Config, cwd: &PathBuf) -> Result<Harness> {
    cfg.provider_ready()?;
    let provider = cfg
        .active_provider()
        .ok_or_else(|| anyhow::anyhow!("未配置供应商"))?
        .clone();
    let model = cfg.active_model().unwrap_or_default();
    let backend: Arc<dyn ModelBackend> = Arc::new(ResponsesBackend::new(provider, model)?);

    let (confirm_tx, mut confirm_rx): (ConfirmSender, _) = tokio::sync::mpsc::channel(16);
    let (ui_tx, ui_rx) = tokio::sync::mpsc::unbounded_channel::<UiEvent>();

    let desktop = mcode_core::config::DesktopConfig::load();
    let perms = PermissionManager::new(cfg.default_mode, desktop.permission_rules.clone());

    let registry = Arc::new(ToolRegistry::with_builtins());
    registry.register(Arc::new(mcode_core::tools::web::WebFetch));
    registry.register(Arc::new(mcode_core::tools::web::WebSearch));
    registry.register(Arc::new(mcode_core::tools::skill_tool::SkillTool));

    let (cancel_tx, _) = tokio::sync::watch::channel(false);
    let skills = mcode_core::skills::load_all(cwd);
    let deps = Arc::new(AgentDeps {
        cfg: cfg.clone(),
        backend,
        registry: registry.clone(),
        perms: Arc::new(tokio::sync::Mutex::new(perms)),
        ledger: Arc::new(Mutex::new(TokenLedger::default())),
        hooks: Arc::new(HookEngine::load(cwd)),
        confirm: confirm_tx,
        cancel_tx,
        cwd: cwd.clone(),
        skills,
        agents: Arc::new(all_agents()),
        project_doc: load_project_doc(cwd),
    });

    // task 工具（需要 deps 自引用，deps 构建后注册）
    let agents = deps.agents.clone();
    deps.registry.register(Arc::new(TaskTool { deps: deps.clone(), agents }));

    // MCP：真实连接 + 工具注册（失败的 server 记入报告，不阻断）
    let mcp_report = mcode_core::mcp::attach_all(&desktop.mcp_servers, &deps.registry).await;

    // 确认泵：ConfirmRequest → UI 事件流
    let pump_tx = ui_tx.clone();
    tokio::spawn(async move {
        while let Some(req) = confirm_rx.recv().await {
            if pump_tx.send(UiEvent::Confirm(req)).is_err() {
                break;
            }
        }
    });

    Ok(Harness { deps, ui_tx, ui_rx, mcp_report })
}

/// 事件汇：AgentEvent → UI 事件流
pub fn event_sink(tx: tokio::sync::mpsc::UnboundedSender<UiEvent>) -> EventSink {
    Arc::new(move |ev| {
        let _ = tx.send(UiEvent::Agent(ev));
    })
}
