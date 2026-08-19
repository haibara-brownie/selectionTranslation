//! 字体设置。
//!
//! 分三档：拉丁字体、中文字体、后备字体。CSS 的 `font-family` 本来就是按顺序回退的 ——
//! 拉丁字体通常没有汉字字形，遇到汉字自然落到第二档，再不行落到第三档。三档都留空
//! 就完全不发 `font-family`，用系统默认。

use gtk::prelude::*;

use crate::config::Config;

/// 列出系统已装的字体家族，按名字排序
pub fn families() -> Vec<String> {
    // 随便建个控件拿 PangoContext，FontMap 是全局共享的
    let label = gtk::Label::new(None);
    let Some(map) = label.pango_context().font_map() else {
        return Vec::new();
    };
    let mut v: Vec<String> = map
        .list_families()
        .iter()
        .map(|f| f.name().to_string())
        .collect();
    v.sort_by_key(|s| s.to_lowercase());
    v.dedup();
    v
}

/// 生成 CSS 的 font-family 值。三档都空则返回 None，表示不干预系统默认。
pub fn css_family(cfg: &Config) -> Option<String> {
    let list: Vec<String> = [&cfg.font_latin, &cfg.font_cjk, &cfg.font_fallback]
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        // 字体名里有空格，必须加引号；名字里的引号先剔掉，免得把 CSS 打断
        .map(|s| format!("\"{}\"", s.replace('"', "")))
        .collect();

    if list.is_empty() {
        None
    } else {
        Some(list.join(", "))
    }
}
