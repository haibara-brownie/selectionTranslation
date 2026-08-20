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

/// 默认快捷键。
///
/// # 为什么不跟 Linux 那两条对齐
///
/// 早先这里是 `CmdOrCtrl+Shift+T` / `CmdOrCtrl+Alt+T`，为的是「用户换平台时肌肉记忆
/// 还在」。这个理由撑不住，因为**全局快捷键在 mac 和 Windows 上是系统级独占的**：
/// 注册上之后，所有应用里的这个键都归我们，别人再也收不到。
///
/// 而 `⌘⇧T`（mac）和 `Ctrl+Shift+T`（Windows）**恰好是所有浏览器的「重新打开关闭的
/// 标签页」**，还有编辑器、文件管理器在用。装上这个划词翻译，代价是浏览器少一个人人
/// 都在用的功能——为了肌肉记忆去换掉用户已有的肌肉记忆，不划算。
/// `⌘⌥T` 同理：Safari / 访达 / 邮件里是「显示或隐藏工具栏」。
///
/// Linux 不受影响：那边快捷键归合成器管（见模块头），niri 配置里 `Mod+Shift+T` 是
/// 合成器自己拦下来的，不存在独占别人按键的问题，所以 `data/niri-snippet.kdl` 不用改。
///
/// # 为什么是 `Alt+Shift`
///
/// mac 上映射成 `⌥⇧`、Windows 上是 `Alt+Shift`，两边这一组都少有人占。同时它跟 Linux
/// 那条 `Mod+Shift+T` 只差一个修饰键，肌肉记忆基本还在——**在不抢别人按键的前提下**
/// 尽量对齐，而不是为了对齐去抢。
///
/// 设置页沿用同一组修饰键、换成逗号，跟「偏好设置 = ⌘,」的习惯对齐；这样两个键属于
/// 同一个心理分组，好记。
///
/// **这只是默认值，可能撞上别人**（装了 Bob、Raycast 并自定义过之类）。撞上时
/// `register` 返回 Err，程序照常跑（托盘和命令行不受影响），用户可以在设置页里改成
/// 别的组合 —— 见 `configured` 和 `reload`。
#[cfg(not(target_os = "linux"))]
const DEFAULT_TRANSLATE: &str = "Alt+Shift+T";
#[cfg(not(target_os = "linux"))]
const DEFAULT_SETTINGS: &str = "Alt+Shift+Comma";

/// 当前该注册哪两组键：配置里非空就用配置的，否则用上面的默认值。
///
/// 每次都现读配置而不是缓存：改键之后要立刻重新注册，缓存只会多一个失效点。
/// 读一次配置是一次小文件 IO，发生在改键和启动时，不在取词热路径上。
#[cfg(not(target_os = "linux"))]
fn configured() -> (String, String) {
    let cfg = seltrans_core::config::Config::load();
    let pick = |v: String, d: &str| {
        if v.trim().is_empty() {
            d.to_string()
        } else {
            v
        }
    };
    (
        pick(cfg.hotkey_translate, DEFAULT_TRANSLATE),
        pick(cfg.hotkey_settings, DEFAULT_SETTINGS),
    )
}

/// 把全局快捷键插件挂到 builder 上，Linux 上是恒等变换。
///
/// 单独开一个函数而不是在 `main.rs` 里写 cfg：这个模块已经是「快捷键归谁管」的唯一
/// 去处，插件注册也该留在这儿，别让 `main.rs` 再长出一段平台分叉。
///
/// **漏掉这一步的后果不是「快捷键不好使」，是开机就崩。** `register` 里的
/// `app.global_shortcut()` 底下是 `state::<GlobalShortcut>()`，插件没 manage 过就 panic；
/// 而它跑在 `did_finish_launching` 这个 `extern "C"` 回调里，属于 non-unwinding panic，
/// 进程当场 abort —— `register` 返回 `Result` 让调用方「失败也继续跑」的那层兜底
/// 根本够不着。mac 上实测过一次，应用连窗口都没画出来。
///
/// Linux 分支连插件都不引入：Wayland 没有全局快捷键协议，注册会「成功」但回调永不
/// 触发（见模块头），挂上去只会让日志显得一切正常。
#[cfg(target_os = "linux")]
pub fn plugin<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
}

#[cfg(not(target_os = "linux"))]
pub fn plugin<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.plugin(tauri_plugin_global_shortcut::Builder::new().build())
}

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
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

    use crate::windows::{self, TranslateRequest};

    let (t, s) = configured();
    let translate: Shortcut = t
        .parse()
        .map_err(|e| format!("翻译快捷键「{t}」不是合法的组合：{e}"))?;
    let settings: Shortcut = s
        .parse()
        .map_err(|e| format!("设置快捷键「{s}」不是合法的组合：{e}"))?;

    let handle = app.clone();
    app.global_shortcut()
        .on_shortcuts([translate, settings], move |_app, shortcut, event| {
            // 按下和抬起各来一次，只认按下那次，否则会翻译两遍
            if event.state() != ShortcutState::Pressed {
                return;
            }
            // 比对解析后的 Shortcut 而不是字符串：用户配出来的写法和 `to_string()`
            // 归一化之后未必一致（大小写、别名如 Ctrl/Control），比字符串会漏判
            let is_settings = *shortcut == settings;
            let h = handle.clone();
            // 取词和开窗都得在主线程上做，而且取词要赶在窗口抢焦点之前
            let _ = h.clone().run_on_main_thread(move || {
                let cmd = if is_settings { "settings" } else { "popup" };
                windows::dispatch(&h, cmd, TranslateRequest::default(), None);
            });
        })
        .map_err(|e| format!("注册全局快捷键失败（多半是被别的程序占用了）：{e}"))?;

    seltrans_core::logging::info(&format!("已注册全局快捷键：{t}（翻译）/ {s}（设置）"));
    Ok(())
}

/// 改键之后重新注册。
#[cfg(target_os = "linux")]
pub fn reload(_app: &tauri::AppHandle) -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn reload(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    // 先把旧的全撤掉。不撤的话旧组合还占着系统，用户改完键会发现新旧两套都能触发，
    // 而且旧的那套还挡着别的程序
    if let Err(e) = app.global_shortcut().unregister_all() {
        seltrans_core::logging::warn(&format!("撤销旧快捷键失败：{e}"));
    }
    register(app)
}

/// 当前生效的两组键：(翻译, 设置)。Linux 上返回 niri 配置里那两条，只作展示。
pub fn current() -> (String, String) {
    #[cfg(target_os = "linux")]
    {
        ("Mod+Shift+T".to_string(), "Mod+Alt+T".to_string())
    }
    #[cfg(not(target_os = "linux"))]
    {
        configured()
    }
}

/// 给「关于」页和帮助文本用：这个平台上快捷键是怎么来的。
pub fn describe() -> String {
    #[cfg(target_os = "linux")]
    {
        "由合成器提供（Wayland 没有全局快捷键协议）。改键请编辑 \
         ~/.config/niri/selectiontranslation.kdl"
            .to_string()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let (t, s) = configured();
        format!("{t} 翻译选中文本，{s} 打开设置。可在设置页改键")
    }
}
