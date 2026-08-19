//! 取词。
//!
//! Wayland 下没有 X11 那样的全局取词 API，这里用两条路：
//! 1. **主选区**（`wl-paste --primary`）—— 划完词就能读到，零侵入，绝大多数 GTK/Qt/终端
//!    应用都支持；
//! 2. **模拟 Ctrl+C**（ydotool）—— 主选区拿不到时的兜底，读完之后会**还原原来的剪贴板
//!    内容**。
//!
//! 第 2 条路有个必须处理的坑：快捷键是 `Mod+Shift+T`，程序跑起来的那一刻用户**手还按着
//! Super+Shift**，这时候直接发 Ctrl+C，应用收到的是 `Super+Shift+Ctrl+C` —— 复制不到东西，
//! 还可能把合成器的修饰键状态搞乱（表现为键盘"卡住"）。所以发 Ctrl+C 之前必须先把所有
//! 修饰键显式抬起，并且无论中途出什么岔子（提前 return、panic、进程被杀）都要再抬一次。

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::logging;

const KEY_LEFTCTRL: &str = "29";
const KEY_C: &str = "46";

/// 左右 Ctrl / Shift / Alt / Super 的 evdev 键码
const MODIFIER_KEYS: [&str; 8] = ["29", "97", "42", "54", "56", "100", "125", "126"];

fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("无法执行 {cmd}：{e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// 把所有修饰键强制抬起。宁可多发一次，也不能让键卡住。
fn release_all_modifiers() {
    let args: Vec<String> = std::iter::once("key".to_string())
        .chain(MODIFIER_KEYS.iter().map(|k| format!("{k}:0")))
        .collect();
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    if let Err(e) = run("ydotool", &refs) {
        if !e.is_empty() {
            logging::warn(&format!("抬起修饰键失败：{e}"));
        }
    }
}

/// 只要这个守卫还活着，析构时一定会再抬一次修饰键 —— 提前 return / panic 都覆盖得到。
struct ModifierGuard;

impl ModifierGuard {
    fn engage() -> Self {
        release_all_modifiers();
        // 给合成器一点时间处理抬起事件；顺便也给用户松开快捷键的时间
        std::thread::sleep(Duration::from_millis(120));
        ModifierGuard
    }
}

impl Drop for ModifierGuard {
    fn drop(&mut self) {
        release_all_modifiers();
    }
}

fn read_primary() -> Option<String> {
    for args in [
        &["--primary", "--no-newline", "-t", "text/plain"][..],
        &["--primary", "--no-newline"][..],
    ] {
        match run("wl-paste", args) {
            Ok(s) if !logging::is_blank(&s) => return Some(s),
            Ok(s) => {
                if !s.is_empty() {
                    logging::warn(&format!(
                        "主选区读到 {} 字符但全是空白/零宽字符：{}",
                        s.chars().count(),
                        logging::preview(&s)
                    ));
                }
            }
            Err(e) => {
                if !e.is_empty() {
                    logging::info(&format!("wl-paste {args:?} 失败：{e}"));
                }
            }
        }
    }
    None
}

fn read_clipboard() -> Option<String> {
    for args in [
        &["--no-newline", "-t", "text/plain"][..],
        &["--no-newline"][..],
    ] {
        if let Ok(s) = run("wl-paste", args) {
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

fn write_clipboard(text: Option<&str>) {
    match text {
        Some(t) => {
            // wl-copy 会 fork 出后台进程持有剪贴板，这里只管把内容喂进去
            if let Ok(mut child) = Command::new("wl-copy")
                .arg("--")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(t.as_bytes());
                }
                let _ = child.wait();
            }
        }
        None => {
            let _ = Command::new("wl-copy")
                .arg("--clear")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

/// 模拟一次 Ctrl+C，读走剪贴板内容，然后把剪贴板还原成原样。
fn copy_via_ydotool() -> Result<String, String> {
    let original = read_clipboard();
    logging::info(&format!(
        "走 Ctrl+C 兜底取词，先备份剪贴板（{} 字符）",
        original.as_ref().map(|s| s.chars().count()).unwrap_or(0)
    ));

    // 先抬修饰键再发 Ctrl+C；守卫析构时会再抬一次
    let _guard = ModifierGuard::engage();

    run(
        "ydotool",
        &[
            "key",
            &format!("{KEY_LEFTCTRL}:1"),
            &format!("{KEY_C}:1"),
            &format!("{KEY_C}:0"),
            &format!("{KEY_LEFTCTRL}:0"),
        ],
    )
    .map_err(|e| {
        let msg = if e.is_empty() {
            "ydotool 执行失败，请确认 ydotool 服务在跑：systemctl --user status ydotool"
                .to_string()
        } else {
            format!("ydotool 执行失败：{e}")
        };
        logging::error(&msg);
        msg
    })?;

    // 等剪贴板内容发生变化，最多 800ms
    let deadline = Instant::now() + Duration::from_millis(800);
    let mut captured = None;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(40));
        let now = read_clipboard();
        if now.is_some() && now != original {
            captured = now;
            break;
        }
    }

    let result = match captured {
        Some(t) if !logging::is_blank(&t) => {
            logging::info(&format!("Ctrl+C 取到 {} 字符", t.chars().count()));
            Ok(t)
        }
        Some(t) => {
            let msg = format!(
                "Ctrl+C 取到的内容全是空白/零宽字符（{} 字符）：{}",
                t.chars().count(),
                logging::preview(&t)
            );
            logging::warn(&msg);
            Err(msg)
        }
        None => {
            let msg = "模拟 Ctrl+C 后剪贴板没有变化，可能是当前应用不响应复制，或者并没有选中文本"
                .to_string();
            logging::warn(&msg);
            Err(msg)
        }
    };

    // 不管有没有拿到，都把剪贴板还原回去
    write_clipboard(original.as_deref());
    result
}

/// 按配置的取词方式抓取选中文本。
pub fn grab(mode: &str) -> Result<String, String> {
    logging::info(&format!("开始取词，方式={mode}"));

    let result = match mode {
        "primary" => read_primary().ok_or_else(|| {
            "主选区是空的。可能是没有选中文本，或者这个应用不支持主选区\
             （可在设置里把取词方式改成「自动」以启用 Ctrl+C 兜底）"
                .to_string()
        }),
        "clipboard" => copy_via_ydotool(),
        _ => match read_primary() {
            Some(s) => {
                logging::info("主选区命中，无需 Ctrl+C 兜底");
                Ok(s)
            }
            None => {
                logging::info("主选区为空，转 Ctrl+C 兜底");
                copy_via_ydotool()
            }
        },
    };

    match &result {
        Ok(s) => logging::info(&format!(
            "取词成功：{} 字符 | {}",
            s.chars().count(),
            logging::preview(s)
        )),
        Err(e) => logging::warn(&format!("取词失败：{e}")),
    }
    result
}

fn have(cmd: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {cmd}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 「关于」页的依赖自检：(名称, 是否就绪, 说明)
pub fn deps_report() -> Vec<(String, bool, String)> {
    let mut out = Vec::new();

    let wl = have("wl-paste") && have("wl-copy");
    out.push((
        "wl-clipboard".into(),
        wl,
        if wl {
            "已安装，主选区取词可用".into()
        } else {
            "缺失，请安装：pac wl-clipboard".into()
        },
    ));

    let yd = have("ydotool");
    let yd_running = Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", "ydotool"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    out.push((
        "ydotool".into(),
        yd && yd_running,
        match (yd, yd_running) {
            (false, _) => "缺失，Ctrl+C 兜底取词不可用：pac ydotool".into(),
            (true, false) => "已安装但服务未运行：systemctl --user enable --now ydotool".into(),
            (true, true) => "已安装且服务在运行，Ctrl+C 兜底取词可用".into(),
        },
    ));

    let kdl = crate::niri_snippet_path();
    let installed = kdl.exists();
    out.push((
        "niri 快捷键 / 窗口规则".into(),
        installed,
        if installed {
            format!("已安装：{}", kdl.display())
        } else {
            "未安装，运行仓库里的 install.sh 可自动写入".into()
        },
    ));

    let log = logging::log_path();
    let has_log = log.exists();
    out.push((
        "运行日志".into(),
        true,
        if has_log {
            format!(
                "{}（{} KB）",
                log.display(),
                std::fs::metadata(&log).map(|m| m.len() / 1024).unwrap_or(0)
            )
        } else {
            format!("{}（还没有内容）", log.display())
        },
    ));

    out
}
