//! 全局快捷键。
//!
//! 三个平台的路子不一样，而且**不可能统一**：
//!
//! | 平台 | 谁来注册 |
//! |---|---|
//! | Linux / Wayland | **合成器**。应用自己注册不了 |
//! | macOS | 应用自己（Carbon `RegisterEventHotKey`） |
//! | Windows | 应用自己（`RegisterHotKey`） |
//!
//! Wayland 的安全模型不允许应用抓全局按键，协议层根本没有这个接口。Tauri 的
//! global-shortcut 插件底层是 `global-hotkey`，Linux 路径纯 X11；tao 检测到 Wayland
//! 时**直接不启动**快捷键线程 —— 注册会「成功」，但回调永远不触发。这是上游长期
//! 未决的问题（tauri#3578），不是配置能绕过去的。
//!
//! 所以 Linux 上走的是另一条路：合成器按键时 spawn 一个新进程，新进程通过
//! single-instance 插件把 argv 递给常驻实例。快捷键定义在 niri 的配置里
//! （`data/niri-snippet.kdl`），改键要编辑那个文件，不在设置界面里。
//!
//! mac / Windows 上才由这个模块注册。

/// 默认快捷键。跟 Linux 上 niri 配置里那两条保持一致，用户换平台时肌肉记忆还在。
///
/// `CmdOrCtrl` 会在 mac 上映射成 ⌘、在 Windows 上映射成 Ctrl。
#[cfg(not(target_os = "linux"))]
const TRANSLATE: &str = "CmdOrCtrl+Shift+T";
#[cfg(not(target_os = "linux"))]
const SETTINGS: &str = "CmdOrCtrl+Alt+T";

/// Linux：什么都不做。快捷键归合成器管。
///
/// 刻意不去注册一个「注册得上但永远不触发」的快捷键 —— 那会让日志和设置界面
/// 显得一切正常，用户按了没反应却查不出原因。
#[cfg(target_os = "linux")]
pub fn register(_app: &tauri::AppHandle) -> Result<(), String> {
    seltrans_core::logging::info("Linux：全局快捷键由合成器提供，应用侧不注册");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn register(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    use crate::windows::{self, TranslateRequest};

    let handle = app.clone();
    app.global_shortcut()
        .on_shortcuts([TRANSLATE, SETTINGS], move |_app, shortcut, event| {
            // 按下和抬起各来一次，只认按下那次，否则会翻译两遍
            if event.state() != ShortcutState::Pressed {
                return;
            }
            let s = shortcut.to_string();
            let h = handle.clone();
            // 取词和开窗都得在主线程上做，而且取词要赶在窗口抢焦点之前
            let _ = h.clone().run_on_main_thread(move || {
                if s == SETTINGS {
                    windows::dispatch(&h, "settings", TranslateRequest::default(), None);
                } else {
                    windows::dispatch(&h, "popup", TranslateRequest::default(), None);
                }
            });
        })
        .map_err(|e| format!("注册全局快捷键失败：{e}"))?;

    seltrans_core::logging::info(&format!(
        "已注册全局快捷键：{TRANSLATE}（翻译）/ {SETTINGS}（设置）"
    ));
    Ok(())
}

/// 给「关于」页和帮助文本用：这个平台上快捷键是怎么来的。
pub fn describe() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "由合成器提供（Wayland 没有全局快捷键协议）。改键请编辑 \
         ~/.config/niri/selectiontranslation.kdl"
    }
    #[cfg(not(target_os = "linux"))]
    {
        "Ctrl/⌘+Shift+T 翻译选中文本，Ctrl/⌘+Alt+T 打开设置"
    }
}
