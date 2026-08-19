//! selectionTranslation —— niri / Wayland 下的全局划词翻译。

mod config;
mod llm;
mod popup;
mod presets;
mod selection;
mod settings_ui;

use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;

pub const APP_ID_POPUP: &str = "xyz.brownie.SelectionTranslation.Popup";
pub const APP_ID_SETTINGS: &str = "xyz.brownie.SelectionTranslation.Settings";
pub const REPO_URL: &str = "https://github.com/haibara-brownie/selectionTranslation";

/// install.sh 会把快捷键和窗口规则写到这里
pub fn niri_snippet_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into())).join(".config")
        });
    base.join("niri/selectiontranslation.kdl")
}

fn help() {
    println!(
        "\
seltrans {ver} —— niri 下的划词翻译

用法：
  seltrans popup [--text <文本>]   取当前选中的文本并弹窗翻译（快捷键调用的就是这个）
  seltrans settings [页面]         打开图形配置界面
                                   页面可选 general / providers / prompts / about
  seltrans translate [--text <文本>]
                                   在终端里翻译并打印结果，不开窗口
  seltrans --version               显示版本
  seltrans --help                  显示本帮助

translate 的输入优先级：--text > 管道输入 > 当前选中的文本
配置文件：{cfg}
仓库：{repo}",
        ver = env!("CARGO_PKG_VERSION"),
        cfg = config::config_path().display(),
        repo = REPO_URL,
    );
}

/// 从参数里取 --text 的值
fn arg_text(args: &[String]) -> Option<String> {
    let i = args.iter().position(|a| a == "--text" || a == "-t")?;
    args.get(i + 1).cloned()
}

fn cli_translate(args: &[String]) -> i32 {
    let cfg = config::Config::load();

    let source = if let Some(t) = arg_text(args) {
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
        eprintln!("seltrans: 还没有配置任何模型供应商，先运行 `seltrans settings`");
        return 1;
    };
    let Some(prompt) = cfg.active_prompt().cloned() else {
        eprintln!("seltrans: 提示词列表是空的");
        return 1;
    };
    let system = cfg.render_system(&prompt);

    let code = llm::runtime().block_on(async move {
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
    });
    code
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("popup");

    let code = match cmd {
        "popup" => popup::run(arg_text(&args)),
        "settings" | "config" => {
            let page = args
                .get(2)
                .map(|s| s.trim_start_matches("--").to_string())
                .filter(|s| {
                    matches!(s.as_str(), "general" | "providers" | "prompts" | "about")
                });
            settings_ui::run(page)
        }
        "translate" => cli_translate(&args),
        "-h" | "--help" | "help" => {
            help();
            0
        }
        "-V" | "--version" => {
            println!("seltrans {}", env!("CARGO_PKG_VERSION"));
            0
        }
        other => {
            eprintln!("seltrans: 未知命令 `{other}`\n");
            help();
            2
        }
    };
    std::process::exit(code);
}
