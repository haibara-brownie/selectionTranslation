//! 前端能调的命令。
//!
//! 一条原则：**业务逻辑一律在 `seltrans-core` 里**，这一层只做搬运和错误转字符串。
//! 界面层换过一次了（GTK → web），下次再换时这个文件应该是可以整个丢掉的。

use serde::Serialize;
use tauri::ipc::Channel;

use seltrans_core::config::Config;
use seltrans_core::{llm, logging, selection};

use crate::state::UiState;

/// 流式翻译推给前端的事件。
///
/// core 里的 `llm::Event` 是给 Rust 用的枚举，这里转成带 tag 的 JSON，
/// 前端拿到的是 `{ kind: "delta", text: "…" }` 这种好判别的形状。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum UiEvent {
    Delta { text: String },
    Done,
    Error { message: String },
}

impl From<llm::Event> for UiEvent {
    fn from(e: llm::Event) -> Self {
        match e {
            llm::Event::Delta(text) => UiEvent::Delta { text },
            llm::Event::Done => UiEvent::Done,
            llm::Event::Error(message) => UiEvent::Error { message },
        }
    }
}

#[tauri::command]
pub fn load_state(system_dark: bool) -> UiState {
    UiState::build(&Config::load(), system_dark)
}

/// 抓当前选中的文本。取不到时把 core 给的排查建议原样带上去。
#[tauri::command]
pub fn grab_selection() -> Result<String, String> {
    let cfg = Config::load();
    selection::grab(&cfg.selection_mode)
}

/// 流式翻译。前端传一个 Channel 进来，每来一段就推一个事件回去。
///
/// 用 Channel 而不是全局事件：全局事件会广播到所有窗口，将来同时开着弹窗和设置页时
/// 会互相串台。
#[tauri::command]
pub async fn translate(
    text: String,
    prompt_id: Option<String>,
    on_event: Channel<UiEvent>,
) -> Result<(), String> {
    let cfg = Config::load();

    let Some(provider) = cfg.active_provider().cloned() else {
        return Err("还没有配置任何模型供应商，先去设置里加一个".into());
    };
    let prompt = match prompt_id.as_deref() {
        Some(id) => cfg.prompt_by_id(id).or_else(|| cfg.active_prompt()),
        None => cfg.active_prompt(),
    };
    let Some(prompt) = prompt.cloned() else {
        return Err("提示词列表是空的".into());
    };
    let system = cfg.render_system(&prompt);

    let (tx, rx) = async_channel::unbounded();
    tauri::async_runtime::spawn(llm::stream_translate(provider, system, text, tx));

    while let Ok(ev) = rx.recv().await {
        let finished = matches!(ev, llm::Event::Done | llm::Event::Error(_));
        if on_event.send(ev.into()).is_err() {
            // 前端把窗口关了，没必要再收下去
            logging::info("前端通道已关闭，中止流式接收");
            break;
        }
        if finished {
            break;
        }
    }
    Ok(())
}

/// 切换当前提示词并落盘
#[tauri::command]
pub fn set_active_prompt(id: String) -> Result<(), String> {
    let mut cfg = Config::load();
    if cfg.prompt_by_id(&id).is_none() {
        return Err(format!("没有这个提示词：{id}"));
    }
    cfg.active_prompt = id;
    cfg.save().map_err(|e| format!("配置写入失败：{e}"))
}

/// 切换某个供应商正在用的模型并落盘。
///
/// **只动模型**。之前这里顺手把 `active_provider` 也改了，名字上完全看不出来 ——
/// 换供应商是另一件事，该有自己的命令（等设置页做出来再说）。
#[tauri::command]
pub fn set_active_model(provider_id: String, model: String) -> Result<(), String> {
    let mut cfg = Config::load();
    let Some(p) = cfg.providers.iter_mut().find(|p| p.id == provider_id) else {
        return Err(format!("没有这个供应商：{provider_id}"));
    };
    p.model = model;
    cfg.save().map_err(|e| format!("配置写入失败：{e}"))
}

/// 关掉首次使用提示（用户点了「我会用」）。
///
/// 单独一个命令而不是塞进 `save_config`：那条路要前端把整份配置传上来，
/// 而弹窗手里根本没有完整配置（它只拿了首屏要的那点东西）。
#[tauri::command]
pub fn dismiss_onboarding() -> Result<(), String> {
    let mut cfg = Config::load();
    cfg.onboarded = true;
    cfg.save().map_err(|e| format!("配置写入失败：{e}"))
}

/// 新手引导当前走到第几步。
///
/// 两个窗口都要读它 —— 引导跨窗口，而弹窗和设置页是各自独立的 webview，
/// 只能靠后端这一份状态对齐。
#[tauri::command]
pub fn tour_step() -> u32 {
    let cfg = Config::load();
    // 已经走完或跳过的，不该再被任何窗口捡起来
    if cfg.onboarded {
        u32::MAX
    } else {
        cfg.tour_step
    }
}

/// 记下引导走到哪一步。前进、后退、跨窗口交接都走它。
#[tauri::command]
pub fn set_tour_step(step: u32) -> Result<(), String> {
    let mut cfg = Config::load();
    cfg.tour_step = step;
    cfg.save().map_err(|e| format!("配置写入失败：{e}"))
}
