//! 划词翻译 —— Tauri 界面层。
//!
//! 迁移中的第二套界面，跟 GTK 版并存到功能追平为止（迁移方案的 P5）。
//! 业务逻辑全在 `seltrans-core`，这里只有窗口、命令层和平台胶水。

// Windows 下 release 构建不要弹控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cmds;
mod state;

use tauri::{WebviewUrl, WebviewWindowBuilder};

use seltrans_core::config::Config;
use seltrans_core::logging;

/// 命令行带进来的启动参数，前端起来后用 `launch_args` 命令取。
///
/// 之所以不走 URL 参数：待翻译的文本可能很长、可能带任意字符，塞进 URL 要转义两次，
/// 出了问题极难排查。放在 Rust 侧让前端来取，文本原样不动。
#[derive(Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchArgs {
    /// `--text` 指定的内容；为空表示要现场取词
    pub text: Option<String>,
    /// `--input`：不取词，直接聚焦输入框等用户敲
    pub input_mode: bool,
}

#[tauri::command]
fn launch_args(state: tauri::State<'_, LaunchArgs>) -> LaunchArgs {
    state.inner().clone()
}

/// 从参数里取 `--text` 的值
fn arg_text(args: &[String]) -> Option<String> {
    let i = args.iter().position(|a| a == "--text" || a == "-t")?;
    args.get(i + 1).cloned()
}

fn help() {
    println!(
        "\
seltrans {ver} —— 划词翻译（Tauri 界面）

用法：
  seltrans-tauri popup [--text <文本>]   取当前选中的文本并弹窗翻译
  seltrans-tauri popup --input           打开弹窗并聚焦输入框
  seltrans-tauri --version               显示版本

配置文件：{cfg}
日志文件：{log}

注：设置页、托盘、开机自启还在 GTK 版里，用 `seltrans` 那个二进制。",
        ver = seltrans_core::VERSION,
        cfg = seltrans_core::config::config_path().display(),
        log = logging::log_path().display(),
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
    let cmd = args.get(1).map(String::as_str).unwrap_or("popup");

    // 不需要窗口的子命令在这里就地了结。**必须赶在 Tauri 初始化之前** ——
    // 将来接上 single-instance 插件后，进了 Tauri 的 argv 会被转交给常驻实例，
    // 那时候 `--version` 这类一次性命令的行为就全错了。
    match cmd {
        "-h" | "--help" | "help" => return help(),
        "-V" | "--version" => return println!("seltrans {}", seltrans_core::VERSION),
        _ => {}
    }

    logging::startup(&format!("tauri:{cmd}"));

    // 配置在编译期就烘进来了，这里先拿出来设 app-id，最后再交给 run()
    let ctx = tauri::generate_context!();
    set_wayland_app_id(&ctx.config().identifier);

    let launch = LaunchArgs {
        text: arg_text(&args),
        input_mode: args.iter().any(|a| a == "--input"),
    };
    let cfg = Config::load();
    let (w, h) = (cfg.popup_width as f64, cfg.popup_height as f64);

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .manage(launch)
        .invoke_handler(tauri::generate_handler![
            launch_args,
            cmds::load_state,
            cmds::grab_selection,
            cmds::translate,
            cmds::set_active_prompt,
            cmds::set_active_model,
        ])
        .setup(move |app| {
            // 无边框 + 透明：顶栏是自己画的，圆角要靠透明背景才不会露出黑角。
            // 窗口位置不在这里设 —— Wayland 下客户端根本没权限摆自己，
            // 那是合成器的事（niri 的 window-rule 按 app-id 匹配）。
            WebviewWindowBuilder::new(app, "popup", WebviewUrl::App("index.html".into()))
                .title("划词翻译")
                .inner_size(w, h)
                .decorations(false)
                .transparent(true)
                .build()?;
            Ok(())
        })
        .run(ctx)
        .expect("Tauri 启动失败");
}
