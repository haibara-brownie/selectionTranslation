//! 窗口的创建与复用。
//!
//! 两个窗口：`popup`（翻译弹窗，无边框透明）和 `settings`（设置页，正常窗口）。
//!
//! 关键行为是**复用**：常驻模式下按快捷键不该每次都新建 webview（那要几百毫秒，
//! 划词翻译最忌讳这个）。已经有窗口就把它显示出来、聚焦、把新的待翻译文本推给前端。

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use seltrans_core::config::Config;

pub const POPUP: &str = "popup";
pub const SETTINGS: &str = "settings";

/// 前端监听这个事件拿到新一轮的待翻译内容。
///
/// 只有**复用**已有窗口时才发 —— 首次创建时前端会主动调 `launch_args` 去取，
/// 那会儿它还没挂上监听器，发了也收不到。
pub const EVENT_TRANSLATE: &str = "seltrans://translate";

#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateRequest {
    /// 要翻译的文本。到了前端手上时这里已经是取好词的结果。
    pub text: Option<String>,
    /// 只打开输入框，不取词
    pub input_mode: bool,
    /// 取词失败的原因。前端把它显示出来，并让用户直接在输入框里敲。
    pub error: Option<String>,
}

/// 取词，**必须在开窗之前调用**。
///
/// 时序是硬约束：模拟复制键（Ctrl+C / ⌘C）发给的是**当前有焦点的窗口**。译文窗口
/// 一旦抢到焦点，复制的就是我们自己界面上的东西，用户选的那段再也拿不到了。
/// 所以取词这一步归 Rust，在窗口存在之前做完，前端拿到的是结果而不是任务。
///
/// Linux 上主选区那条路不受焦点影响，但兜底的 Ctrl+C 一样受，不能只照顾前者。
pub fn prepare(mut req: TranslateRequest) -> TranslateRequest {
    if req.input_mode || req.text.is_some() {
        return req;
    }
    let mode = Config::load().selection_mode;
    match seltrans_core::selection::grab(&mode) {
        Ok(text) => req.text = Some(text),
        // 取不到词不是死路：把原因带给前端，让用户手输
        Err(e) => req.error = Some(e),
    }
    req
}

/// 常驻模式标记。常驻时关窗口只是藏起来，不销毁 webview。
pub struct Resident(pub bool);

/// 打开（或复用）翻译弹窗
pub fn open_popup(app: &AppHandle, req: TranslateRequest) -> tauri::Result<WebviewWindow> {
    if let Some(win) = app.get_webview_window(POPUP) {
        win.show()?;
        win.set_focus()?;
        // 窗口是复用的，前端还停在上一轮的内容上，得告诉它换一批
        let _ = win.emit(EVENT_TRANSLATE, req);
        return Ok(win);
    }

    let cfg = Config::load();
    let win = WebviewWindowBuilder::new(app, POPUP, WebviewUrl::App("index.html".into()))
        .title("划词翻译")
        .inner_size(cfg.popup_width as f64, cfg.popup_height as f64)
        // 顶栏是自己画的，圆角要靠透明背景才不会露出黑角
        .decorations(false)
        .transparent(true)
        // 位置不在这里设 —— Wayland 下客户端没权限摆自己，那是合成器的事
        // （niri 的 window-rule 按 app-id 匹配，见 data/niri-snippet.kdl）
        .build()?;

    // 常驻模式下按 Esc / 点 ✕ 只藏窗口。重建一个 webview 要几百毫秒，
    // 而划词翻译的全部价值就在于按下快捷键就出来。
    //
    // 非常驻（命令行单次调用）则照常销毁，让进程正常退出 —— 否则用户跑一次
    // `seltrans-tauri popup` 就多一个看不见的常驻进程。
    if app.state::<Resident>().0 {
        let w = win.clone();
        win.on_window_event(move |ev| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = ev {
                api.prevent_close();
                let _ = w.hide();
            }
        });
    }
    Ok(win)
}

/// 打开（或复用）设置页。`page` 是要直接跳到的标签页。
pub fn open_settings(app: &AppHandle, page: Option<&str>) -> tauri::Result<WebviewWindow> {
    if let Some(win) = app.get_webview_window(SETTINGS) {
        win.show()?;
        win.set_focus()?;
        return Ok(win);
    }

    let mut url = "settings.html".to_string();
    if let Some(p) = page {
        url.push_str(&format!("?page={p}"));
    }
    WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App(url.into()))
        .title("划词翻译 · 设置")
        .inner_size(900.0, 700.0)
        .min_inner_size(640.0, 480.0)
        .decorations(false)
        .build()
}

/// 命令行 / 托盘都能调到的统一入口：把一次「用户想翻译点什么」变成窗口动作。
///
/// **取词在这里做完再开窗**，理由见 `prepare`。
pub fn dispatch(app: &AppHandle, cmd: &str, req: TranslateRequest, page: Option<&str>) {
    let r = match cmd {
        "settings" | "config" => open_settings(app, page).map(|_| ()),
        _ => open_popup(app, prepare(req)).map(|_| ()),
    };
    if let Err(e) = r {
        seltrans_core::logging::error(&format!("打开窗口失败：{e}"));
    }
}
