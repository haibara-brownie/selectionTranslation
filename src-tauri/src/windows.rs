//! 窗口的创建与复用。
//!
//! 两个窗口：`popup`（翻译弹窗，无边框透明）和 `settings`（设置页，正常窗口）。
//!
//! 关键行为是**复用**：常驻模式下按快捷键不该每次都新建 webview（那要几百毫秒，
//! 划词翻译最忌讳这个）。已经有窗口就把它显示出来、聚焦、把新的待翻译文本推给前端。

use tauri::{
    AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

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

/// 弹窗离屏幕边缘的留白，逻辑像素。
#[cfg(not(target_os = "linux"))]
const MARGIN: f64 = 24.0;

/// 把弹窗摆到「鼠标所在那块屏」的右上角。
///
/// # 为什么需要它
///
/// Linux 上摆窗口不归客户端管 —— Wayland 下应用没权限摆自己，niri 按 app-id 匹配的
/// window-rule 会把它钉在右上角（见 `data/niri-snippet.kdl`）。**mac 和 Windows 没有
/// 这个机制，于是谁都不管**，窗口落在系统默认的层叠位置上，README 承诺的「右上角浮出
/// 译文」在这两家上不成立。实测 mac 上落在 (620, 188)，屏幕逻辑宽 1512，偏中间。
///
/// # 为什么按鼠标所在的屏，而不是主屏
///
/// 多显示器时，用户正在看的是鼠标那块屏。译文弹到另一块屏上等于没弹。
/// 拿不到鼠标位置就退回窗口当前所在的屏，再拿不到就放弃摆位（保持系统默认），
/// 摆不准也好过不显示。
///
/// # 为什么只在创建时摆一次
///
/// 常驻模式下窗口是复用的。用户要是把它拖到别处，说明他想让它待在那儿；每次都强行拽
/// 回右上角是跟用户较劲。Linux 那边由合成器每次强制，是合成器的策略，不必强求一致。
#[cfg(not(target_os = "linux"))]
fn place_top_right<R: Runtime>(app: &AppHandle<R>, win: &WebviewWindow<R>) {
    use tauri::PhysicalPosition;

    // 查显示器一律走**窗口**上的那套接口，不用 AppHandle 上的同名方法：两者行为一致，
    // 但 AppHandle 版在 Tauri 的 MockRuntime 里是 `unimplemented!()`，一调就 panic，
    // 于是任何走到开窗这一步的单测都做不了。窗口版返回 `Ok(None)`，正好落进下面
    // 「拿不到显示器就放弃摆位」那条既有分支。
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| win.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| win.current_monitor().ok().flatten());

    let Some(monitor) = monitor else {
        seltrans_core::logging::warn("拿不到显示器信息，弹窗位置交给系统默认");
        return;
    };

    let Ok(size) = win.outer_size() else {
        seltrans_core::logging::warn("拿不到窗口尺寸，弹窗位置交给系统默认");
        return;
    };

    // 用可用区域而不是整块屏：mac 顶上有菜单栏、Windows 可能有任务栏，
    // 按整块屏算会把窗口塞到它们底下去
    let area = monitor.work_area();
    let margin = (MARGIN * monitor.scale_factor()) as i32;

    let x = area.position.x + area.size.width as i32 - size.width as i32 - margin;
    let y = area.position.y + margin;

    if let Err(e) = win.set_position(PhysicalPosition::new(x, y)) {
        seltrans_core::logging::warn(&format!("摆放弹窗失败：{e}"));
    }
}

/// 打开（或复用）翻译弹窗
pub fn open_popup<R: Runtime>(
    app: &AppHandle<R>,
    req: TranslateRequest,
) -> tauri::Result<WebviewWindow<R>> {
    // 当轮内容先写进 `Launch` 槽位 —— **两条路都要**。
    //
    // 新建那条路上前端收不到 `EVENT_TRANSLATE`（它还没挂上监听器），只能靠 `launch_args`
    // 来读；而槽位里原本躺着的是**进程启动时**那份，托盘常驻模式下是空的。不写就会开出
    // 一个空弹窗：取词明明成功了，文本却丢在这一步。实测过，第二次起才正常。
    //
    // 复用那条路上前端走事件，看似不必写。但 webview 被系统回收后重载时它还是会去读
    // `launch_args` —— 槽位停在上一轮，就会把旧文本再翻一遍。这个槽位的语义是「当前
    // 这一轮」，让它和事件指向同一个事实，比省一次写要紧。
    if let Some(launch) = app.try_state::<crate::Launch>()
        && let Ok(mut slot) = launch.0.lock()
    {
        *slot = req.clone();
    }

    if let Some(win) = app.get_webview_window(POPUP) {
        win.show()?;
        win.set_focus()?;
        // 窗口是复用的，前端还停在上一轮的内容上，得告诉它换一批
        let _ = win.emit(EVENT_TRANSLATE, req);
        return Ok(win);
    }

    let cfg = Config::load();
    let builder = WebviewWindowBuilder::new(app, POPUP, WebviewUrl::App("index.html".into()))
        .title("划词翻译")
        .inner_size(cfg.popup_width as f64, cfg.popup_height as f64)
        // 顶栏是自己画的
        .decorations(false);
    // 位置不在这里设 —— Wayland 下客户端没权限摆自己，那是合成器的事
    // （niri 的 window-rule 按 app-id 匹配，见 data/niri-snippet.kdl）

    // 透明窗口是为了让 CSS 的圆角不露出方角。
    //
    // **macOS 上做不到**：Tauri 把 `transparent()` 挡在 `macos-private-api` 特性后面，
    // 那是苹果的私有 API —— 会被 App Store 拒，也可能随系统更新失效。为一个圆角
    // 冒这个险不值得，所以 mac 上用不透明的方角窗口，并告诉前端别画圆角
    // （否则四角会露出白方块）。
    #[cfg(not(target_os = "macos"))]
    let builder = builder.transparent(true);
    #[cfg(target_os = "macos")]
    let builder = builder.initialization_script("document.documentElement.classList.add('opaque')");

    let win = builder.build()?;

    // 摆到鼠标那块屏的右上角。Linux 上这事归合成器，不在这里做。
    #[cfg(not(target_os = "linux"))]
    place_top_right(app, &win);

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
pub fn open_settings<R: Runtime>(
    app: &AppHandle<R>,
    page: Option<&str>,
) -> tauri::Result<WebviewWindow<R>> {
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
pub fn dispatch<R: Runtime>(
    app: &AppHandle<R>,
    cmd: &str,
    req: TranslateRequest,
    page: Option<&str>,
) {
    let r = match cmd {
        "settings" | "config" => open_settings(app, page).map(|_| ()),
        _ => open_popup(app, prepare(req)).map(|_| ()),
    };
    if let Err(e) = r {
        seltrans_core::logging::error(&format!("打开窗口失败：{e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Launch;

    /// 建一个跑在 MockRuntime 上的 App：不起真窗口系统，但 `.manage()` 的状态、
    /// 窗口的创建与查找都照常走真实代码路径。
    fn app() -> tauri::App<tauri::test::MockRuntime> {
        tauri::test::mock_builder()
            .manage(Launch(std::sync::Mutex::new(TranslateRequest::default())))
            // open_popup 里用的是 state::<Resident>()，没 manage 会 panic
            .manage(Resident(true))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("MockRuntime 建 App 失败")
    }

    fn with_text(s: &str) -> TranslateRequest {
        TranslateRequest {
            text: Some(s.to_string()),
            input_mode: false,
            error: None,
        }
    }

    /// 回归测试：**新建**弹窗时，取到的文本必须写进 `Launch` 槽位。
    ///
    /// 出过的事故：`open_popup` 在新建那条路上根本没用 `req`，文本掉在地上。前端新建时
    /// 收不到 `EVENT_TRANSLATE`（还没挂监听器），只能靠 `launch_args` 去读 `Launch`，
    /// 而那里存的是**进程启动时**那份 —— 托盘常驻模式下是空的。表现为常驻起来后
    /// **第一次**按快捷键开出一个空弹窗，第二次起才正常（那时走的是复用分支）。
    #[test]
    fn 新建弹窗时把待翻译文本写进_launch_槽位() {
        let app = app();
        let handle = app.handle();

        open_popup(handle, with_text("hello world")).expect("开弹窗失败");

        let stored = handle.state::<Launch>().0.lock().unwrap().clone();
        assert_eq!(
            stored.text.as_deref(),
            Some("hello world"),
            "新建弹窗没把文本写进 Launch，前端 launch_args 会读到空的"
        );
    }

    /// 复用已有窗口时也不能把槽位留在上一轮的内容上 —— 前端万一在这一轮重新读
    /// `launch_args`（比如 webview 被系统回收后重载），读到的必须是当前这次的文本。
    #[test]
    fn 复用弹窗时_launch_槽位跟着更新() {
        let app = app();
        let handle = app.handle();

        open_popup(handle, with_text("第一轮")).expect("开弹窗失败");
        open_popup(handle, with_text("第二轮")).expect("复用弹窗失败");

        let stored = handle.state::<Launch>().0.lock().unwrap().clone();
        assert_eq!(stored.text.as_deref(), Some("第二轮"));
    }
}
