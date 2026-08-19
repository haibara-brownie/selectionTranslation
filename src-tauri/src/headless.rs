//! 不需要窗口的子命令：`translate` / `log` / `autostart`。
//!
//! **这些必须在 Tauri 初始化之前跑完并退出。** 进了 Tauri 的 argv 会被
//! single-instance 插件转交给常驻实例，然后本进程直接退出 —— 那时候
//! `seltrans-tauri translate --text hi` 的行为就变成「让常驻实例弹个窗口」，
//! 而不是「在终端里打印译文」，完全不是用户要的。
//!
//! 逻辑和 GTK 版的 `src/main.rs` 一致，共用 `seltrans-core`。

use std::io::{IsTerminal, Read, Write};

use seltrans_core::config::Config;
use seltrans_core::{llm, logging, selection};

/// 命中就返回 `Some(退出码)`，调用方直接 exit；不命中返回 `None`，继续走 GUI 那条路。
pub fn run(cmd: &str, args: &[String]) -> Option<i32> {
    match cmd {
        "translate" => Some(translate(args)),
        "log" | "logs" => Some(show_log(args)),
        "autostart" => Some(autostart(args)),
        _ => None,
    }
}

/// 输入优先级：`--text` > 管道 > 当前选中的文本
fn translate(args: &[String]) -> i32 {
    let cfg = Config::load();

    let source = if let Some(t) = crate::arg_text(args) {
        t
    } else if !std::io::stdin().is_terminal() {
        let mut s = String::new();
        if std::io::stdin().read_to_string(&mut s).is_err() {
            eprintln!("seltrans: 读取标准输入失败");
            return 1;
        }
        s
    } else {
        match selection::grab(&cfg.selection_mode) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("seltrans: 取词失败：{e}");
                return 1;
            }
        }
    };

    if source.trim().is_empty() {
        eprintln!("seltrans: 没有可翻译的文本");
        return 1;
    }

    let Some(provider) = cfg.active_provider().cloned() else {
        eprintln!("seltrans: 还没有配置任何模型供应商，先运行 `seltrans-tauri settings`");
        return 1;
    };
    let Some(prompt) = cfg.active_prompt().cloned() else {
        eprintln!("seltrans: 提示词列表是空的");
        return 1;
    };
    let system = cfg.render_system(&prompt);

    llm::runtime().block_on(async move {
        let (tx, rx) = async_channel::unbounded();
        tokio::spawn(llm::stream_translate(provider, system, source, tx));
        let mut out = std::io::stdout();
        let mut code = 0;
        let mut wrote = false;
        while let Ok(ev) = rx.recv().await {
            match ev {
                llm::Event::Delta(d) => {
                    let _ = out.write_all(d.as_bytes());
                    let _ = out.flush();
                    wrote = true;
                }
                llm::Event::Done => break,
                llm::Event::Error(e) => {
                    let _ = out.flush();
                    // 已经吐了一半才出错的话，先断行再报错
                    eprintln!("{}seltrans: {e}", if wrote { "\n" } else { "" });
                    code = 1;
                    break;
                }
            }
        }
        if wrote {
            let _ = out.write_all(b"\n");
        }
        code
    })
}

fn show_log(args: &[String]) -> i32 {
    let path = logging::log_path();
    if !path.exists() {
        println!("还没有日志：{}", path.display());
        return 0;
    }
    let follow = args.iter().any(|a| a == "-f" || a == "--follow");

    // Windows 上没有 tail，直接自己读末尾几行
    #[cfg(target_os = "windows")]
    {
        if follow {
            eprintln!(
                "seltrans: Windows 上不支持 -f，请用编辑器打开：{}",
                path.display()
            );
        }
        match std::fs::read_to_string(&path) {
            Ok(s) => {
                let lines: Vec<&str> = s.lines().collect();
                for l in lines.iter().skip(lines.len().saturating_sub(200)) {
                    println!("{l}");
                }
                0
            }
            Err(e) => {
                eprintln!("seltrans: 读不了日志：{e}");
                1
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut cmd = std::process::Command::new("tail");
        if follow {
            cmd.arg("-f");
        }
        cmd.arg("-n").arg("200").arg(&path);
        match cmd.status() {
            Ok(s) => s.code().unwrap_or(0),
            Err(e) => {
                eprintln!("seltrans: 无法执行 tail：{e}\n日志路径：{}", path.display());
                1
            }
        }
    }
}

/// 开机自启的读写在 Tauri 插件里，而插件要有 `AppHandle` 才能用 —— 这里拿不到。
/// 所以只做说明，把人引到设置界面去。
fn autostart(_args: &[String]) -> i32 {
    println!(
        "开机自启请在设置界面里开关：seltrans-tauri settings general\n\
         （Tauri 版的自启由插件按平台实现：Linux 写 XDG autostart、\
         macOS 写 LaunchAgent、Windows 写注册表，命令行拿不到它的句柄）"
    );
    0
}
