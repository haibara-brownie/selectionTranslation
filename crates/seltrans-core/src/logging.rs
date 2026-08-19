//! 日志。写到 `${XDG_STATE_HOME:-~/.local/state}/seltrans/seltrans.log`。
//!
//! niri 用 spawn 启动程序时 stderr 会进 niri 自己的日志，用户根本看不到，
//! 所以取词、请求、响应这些关键节点必须落到自己的文件里才能排查。
//!
//! 日志里会记录**待翻译文本的前若干字符**（排查"发出去的内容是空的"这类问题必须要），
//! 但绝不记录 API key。

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;

const MAX_BYTES: u64 = 1024 * 1024; // 超过 1MB 就轮转一次
const PREVIEW_CHARS: usize = 120;

pub fn log_dir() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into()))
                .join(".local/state")
        });
    base.join("seltrans")
}

pub fn log_path() -> PathBuf {
    log_dir().join("seltrans.log")
}

fn timestamp() -> String {
    // 本地时区。不用 time crate 的 local offset —— 它在多线程程序里会静默退回 UTC。
    jiff::Zoned::now().strftime("%Y-%m-%d %H:%M:%S").to_string()
}

fn rotate_if_needed(path: &PathBuf) {
    if let Ok(meta) = std::fs::metadata(path)
        && meta.len() > MAX_BYTES
    {
        let _ = std::fs::rename(path, path.with_extension("log.1"));
    }
}

/// 写一行日志。`level` 用 INFO / WARN / ERROR。
pub fn write(level: &str, msg: &str) {
    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    rotate_if_needed(&path);

    let line = format!("{} [{}] {}\n", timestamp(), level, msg);

    if std::env::var_os("SELTRANS_DEBUG").is_some() {
        eprint!("{line}");
    }

    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = f.write_all(line.as_bytes());
    }
}

pub fn info(msg: &str) {
    write("INFO", msg);
}
pub fn warn(msg: &str) {
    write("WARN", msg);
}
pub fn error(msg: &str) {
    write("ERROR", msg);
}

/// 把一段文本压成适合写进日志的一行预览：截断 + 转义换行 + 标出不可见字符
pub fn preview(text: &str) -> String {
    let mut out = String::new();
    for (n, c) in text.chars().enumerate() {
        if n >= PREVIEW_CHARS {
            out.push('…');
            break;
        }
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // 零宽字符肉眼看不见，但会让"看起来非空"的文本实际上什么也没有
            '\u{200b}' => out.push_str("<ZWSP>"),
            '\u{200c}' => out.push_str("<ZWNJ>"),
            '\u{200d}' => out.push_str("<ZWJ>"),
            '\u{feff}' => out.push_str("<BOM>"),
            '\u{00a0}' => out.push_str("<NBSP>"),
            c => out.push(c),
        }
    }
    out
}

/// 除了空白，还把零宽字符也算作"什么都没有"。
/// 网页上选到空行、图标字体时很容易拿到一串肉眼看不见的字符，
/// 直接发给模型就会得到"你没有提供要翻译的内容"这种回复。
pub fn is_blank(text: &str) -> bool {
    !text.chars().any(|c| {
        !c.is_whitespace()
            && !matches!(
                c,
                '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{feff}' | '\u{00a0}'
            )
    })
}

/// 程序启动时记一条，方便对齐时间线
pub fn startup(cmd: &str) {
    let mut s = String::new();
    let _ = write!(
        s,
        "===== seltrans {} 启动，子命令={} pid={} =====",
        crate::VERSION,
        cmd,
        std::process::id()
    );
    info(&s);
}
