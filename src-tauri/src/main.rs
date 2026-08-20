//! 划词翻译 —— Tauri 界面层。
//!
//! 迁移中的第二套界面，跟 GTK 版并存到功能追平为止（迁移方案的 P5）。
//! 业务逻辑全在 `seltrans-core`，这里只有窗口、命令层和平台胶水。

// Windows 下 release 构建不要弹控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cmds;
mod headless;
mod hotkey;
mod settings_cmds;
mod state;
mod tray;
mod windows;

use seltrans_core::logging;
use windows::TranslateRequest;

/// 新窗口起来后要的东西，前端用 `launch_args` 命令取。
///
/// 之所以不走 URL 参数：待翻译的文本可能很长、可能带任意字符，塞进 URL 要转义两次，
/// 出了问题极难排查。放在 Rust 侧让前端来取，文本原样不动。
///
/// 里面的 `text` **已经是取好词的结果**（取词必须赶在窗口拿到焦点之前，见
/// `windows::prepare`）。
///
/// **这是一个会被改写的槽位，不是「进程启动参数」。** 每次新建弹窗前都由
/// `windows::open_popup` 写入当轮的内容 —— 托盘常驻模式下进程启动时这里是空的，
/// 只用启动值的话，常驻后的第一次触发会开出一个空弹窗（取词成功了，文本却丢在
/// 新建窗口这条路上）。
pub struct Launch(pub std::sync::Mutex<TranslateRequest>);

#[tauri::command]
fn launch_args(state: tauri::State<'_, Launch>) -> TranslateRequest {
    state.0.lock().map(|g| g.clone()).unwrap_or_default()
}

/// 打开设置窗口。托盘菜单和弹窗里的齿轮按钮都走这个。
#[tauri::command]
fn open_settings(app: tauri::AppHandle, page: Option<String>) -> Result<(), String> {
    windows::open_settings(&app, page.as_deref())
        .map(|_| ())
        .map_err(|e| format!("打不开设置窗口：{e}"))
}

/// 从参数里取 `--text` 的值
fn arg_text(args: &[String]) -> Option<String> {
    let i = args.iter().position(|a| a == "--text" || a == "-t")?;
    args.get(i + 1).cloned()
}

/// 从一批命令行参数里解析出「用户想干什么」
fn parse(args: &[String]) -> (String, TranslateRequest, Option<String>) {
    let cmd = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("popup")
        .to_string();
    let page = args
        .get(2)
        .map(|s| s.trim_start_matches("--").to_string())
        .filter(|s| matches!(s.as_str(), "general" | "providers" | "prompts" | "about"));
    let req = TranslateRequest {
        text: arg_text(args),
        input_mode: args.iter().any(|a| a == "--input"),
        // 取词还没做，在 windows::prepare 里填
        error: None,
    };
    (cmd, req, page)
}

fn help() {
    println!(
        "\
seltrans {ver} —— 划词翻译（Tauri 界面）

用法：
  seltrans-tauri popup [--text <文本>]   取当前选中的文本并弹窗翻译
  seltrans-tauri popup --input           打开弹窗并聚焦输入框
  seltrans-tauri settings [页面]         打开设置，页面可选 general/providers/prompts/about
  seltrans-tauri tray                    常驻后台并在托盘显示图标
  seltrans-tauri translate [--text <文本>]
                                         在终端里翻译并打印结果，不开窗口
  seltrans-tauri log [-f]                查看运行日志
  seltrans-tauri --version               显示版本

translate 的输入优先级：--text > 管道输入 > 当前选中的文本

配置文件：{cfg}
日志文件：{log}

全局快捷键：{hk}",
        ver = seltrans_core::VERSION,
        cfg = seltrans_core::config::config_path().display(),
        log = logging::log_path().display(),
        hk = hotkey::describe(),
    );
}

/// 把 Wayland 的 app-id 设成反向域名标识。
///
/// 为什么需要这一步：合成器靠 app-id 认窗口 —— niri 的窗口规则（浮动、尺寸、
/// 停在右上角）全靠它匹配，图标查找也靠它。而 GTK3 在 Wayland 下的 app-id
/// 取自 `g_get_prgname()`，默认就是**二进制文件名**，于是变成 `seltrans-tauri`，
/// 规则一条都匹配不上，弹窗会被当成普通窗口平铺进布局里。
///
/// `tauri.conf.json` 的 `enableGTKAppId` 只管 GtkApplication 的 id（D-Bus 那套），
/// 管不到 Wayland 的 app-id，两者是两回事。所以这里显式设 prgname。
/// 必须赶在 gtk 初始化之前。
///
/// `id` 直接取自编译进来的 tauri 配置，不另外写一份常量 —— 两份迟早对不上。
#[cfg(target_os = "linux")]
fn set_wayland_app_id(id: &str) {
    glib::set_prgname(Some(id));
    // X11 下走的是 WM_CLASS，顺手一起设，XWayland 回退时也能对上
    glib::set_application_name(id);
}

#[cfg(not(target_os = "linux"))]
fn set_wayland_app_id(_id: &str) {}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (cmd, req, page) = parse(&args);

    // 不需要窗口的子命令在这里就地了结。**必须赶在 Tauri 初始化之前** ——
    // 进了 Tauri 的 argv 会被 single-instance 插件转交给常驻实例、本进程立刻退出，
    // 那时候 `translate` 就变成「让常驻实例弹个窗口」而不是「在终端打印译文」。
    match cmd.as_str() {
        "-h" | "--help" | "help" => return help(),
        "-V" | "--version" => return println!("seltrans {}", seltrans_core::VERSION),
        _ => {}
    }
    if matches!(cmd.as_str(), "translate" | "log" | "logs" | "autostart") {
        logging::startup(&format!("tauri:{cmd}"));
        if let Some(code) = headless::run(&cmd, &args) {
            std::process::exit(code);
        }
    }

    logging::startup(&format!("tauri:{cmd}"));

    // 配置在编译期就烘进来了，这里先拿出来设 app-id，最后再交给 run()
    let ctx = tauri::generate_context!();
    set_wayland_app_id(&ctx.config().identifier);

    // 托盘模式只常驻、不开窗口，等用户点图标或按快捷键
    let tray_mode = matches!(cmd.as_str(), "tray" | "daemon");

    // 取词**不在这里做**，交给 `windows::dispatch`（它在开窗之前取，时序照样是对的）。
    //
    // 早先这里会先取一次词再进 Tauri，结果是**一次触发取两遍**：本进程取一次，
    // 而一旦发现已有常驻实例，single-instance 插件会把 argv 转交过去、本进程立刻退出，
    // 刚取的结果直接丢掉；常驻实例在回调里又取一遍。兜底路径上这意味着两轮
    // 「抬修饰键 → 模拟复制 → 等剪贴板 → 还原」，Electron 场景实测剪贴板 changeCount
    // 被推 79 → 81，用户的剪贴板在一次翻译里被改写还原两遍。Linux 上这还是日常主路径。
    //
    // 为什么挪进去就够了：**次要进程根本走不到我们的 `setup()`** —— single-instance 是
    // 插件，它的 setup 钩子先于应用的 setup 跑，发现已有实例就直接 `exit`。于是
    // 「会开窗的路」只剩两条，都归 dispatch：主实例的 setup、常驻实例的 single-instance
    // 回调。一次触发只可能命中其中一条。
    let boot = (cmd.clone(), req.clone(), page);

    let builder = tauri::Builder::default()
        // single-instance 必须第一个注册（官方要求）。第二个进程的 argv 会送到这里，
        // 我们照常解析一遍再派活 —— 这就是 Wayland 下快捷键的通路：
        // 合成器 spawn 一个新进程，它把意图递给常驻实例然后自己退出。
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let (cmd, req, page) = parse(&argv);
            logging::info(&format!("常驻实例收到第二次启动：{cmd}"));
            windows::dispatch(app, &cmd, req, page.as_deref());
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            // Linux 用 XDG autostart 的 .desktop；mac 用 LaunchAgent；Windows 写注册表
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            // 自启起来的是常驻模式，不是弹窗
            Some(vec!["tray"]),
        ));

    // 全局快捷键插件（Linux 上是空操作，理由见 hotkey.rs）。
    // 必须在 setup() 里调 hotkey::register 之前挂上，否则 mac / Windows 一启动就 abort。
    hotkey::plugin(builder)
        .manage(Launch(std::sync::Mutex::new(req)))
        // 托盘模式常驻：关窗口只藏不销毁，下次按快捷键立刻就出来
        .manage(windows::Resident(tray_mode))
        .invoke_handler(tauri::generate_handler![
            launch_args,
            open_settings,
            cmds::load_state,
            cmds::grab_selection,
            cmds::translate,
            cmds::set_active_prompt,
            cmds::set_active_model,
            cmds::dismiss_onboarding,
            cmds::tour_step,
            cmds::set_tour_step,
            settings_cmds::load_config,
            settings_cmds::save_config,
            settings_cmds::theme_css,
            settings_cmds::theme_choices,
            settings_cmds::list_fonts,
            settings_cmds::font_covers_cjk,
            settings_cmds::has_cjk,
            settings_cmds::platform,
            settings_cmds::provider_presets,
            settings_cmds::prompt_presets,
            settings_cmds::target_langs,
            settings_cmds::list_models,
            settings_cmds::test_connection,
            settings_cmds::about_info,
            settings_cmds::open_path,
            settings_cmds::autostart_enabled,
            settings_cmds::set_autostart,
            settings_cmds::hotkeys,
            settings_cmds::set_hotkeys,
        ])
        .setup(move |app| {
            // mac：常驻托盘的工具不该占 Dock 图标，也不该在启动时把前台应用挤下去。
            //
            // Accessory 等价于 Info.plist 里的 LSUIElement，但走代码设置能跟着 Tauri 的
            // 生命周期走，不必额外维护一份 plist 覆盖。窗口照样能开、能拿焦点 ——
            // Accessory 只是说"我不在 Dock 和 ⌘Tab 里露面"。
            //
            // 另两个平台没有对应概念：Linux 的托盘由 StatusNotifierItem 管，
            // Windows 的托盘程序本来就不占任务栏。
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();

            if let Err(e) = tray::spawn(&handle) {
                // 托盘起不来不该拖垮整个程序 —— 面板不支持 StatusNotifierItem 是常见情况
                logging::warn(&format!("托盘启动失败（不影响翻译功能）：{e}"));
            }
            if let Err(e) = hotkey::register(&handle) {
                // 快捷键被别的程序占了也不该拖垮程序，托盘和命令行还能用
                logging::warn(&format!("{e}（托盘和命令行不受影响）"));
            }

            if !tray_mode {
                let (cmd, req, page) = boot;
                // 走 dispatch 而不是自己分派：取词和开窗的时序归它管，和常驻实例那条路
                // 用同一套规矩。这也是「一次触发只取一次词」的落点（见上面 boot 处的注释）。
                //
                // 时序仍然成立：这会儿一个窗口都还没建出来，取词在 open_popup 之前完成。
                // mac 上更早一步已经把激活策略设成 Accessory，进程不会把前台应用挤下去。
                windows::dispatch(&handle, &cmd, req, page.as_deref());
            }
            Ok(())
        })
        .build(ctx)
        .expect("Tauri 启动失败")
        .run(move |_app, event| {
            // 常驻模式下，最后一个窗口关掉了也不能让程序退出。
            //
            // Tauri 的默认行为是「所有窗口都关了就退出」。弹窗那边靠 `prevent_close`
            // 只藏不销毁，绕开了这条；**设置窗口是真销毁的**，于是在托盘模式下按一次
            // Esc 关掉设置页，整个程序就跟着没了 —— 托盘图标消失、全局快捷键失效，
            // 用户完全不知道发生了什么。实测踩过。
            //
            // 设置窗口刻意不学弹窗去「只藏不销毁」：托盘菜单可以直接打开到某一个标签页
            // （`settings --providers` 之类），而复用已有窗口时是没法换页的，藏起来会让
            // 「打开到指定页」变成「打开上次那页」。宁可每次重建，也别让入口失真。
            //
            // 非常驻模式（命令行单次调用）保持默认：窗口关了就该退出，否则用户跑一次
            // `seltrans-tauri settings` 就多一个看不见的常驻进程。
            if tray_mode && let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}
