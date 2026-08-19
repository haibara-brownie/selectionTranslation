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

/// 这个字体家族自己有没有汉字字形。
///
/// 办法是让 Pango 用指定字体排一个「汉」字，再看它实际挑中的家族是不是同一个 ——
/// 如果字体没有该字形，Pango 会回退到别的字体，家族名就对不上了。
pub fn covers_cjk(family: &str) -> bool {
    if family.trim().is_empty() {
        return false;
    }
    let label = gtk::Label::new(None);
    let ctx = label.pango_context();
    let layout = gtk::pango::Layout::new(&ctx);
    layout.set_font_description(Some(&gtk::pango::FontDescription::from_string(family)));
    layout.set_text("汉");

    let mut iter = layout.iter();
    let Some(run) = iter.run_readonly() else {
        return false;
    };
    let actual = run.item().analysis().font().describe().family();
    actual
        .map(|a| a.eq_ignore_ascii_case(family))
        .unwrap_or(false)
}

/// 汉字、假名、谚文、全角标点这些该用「中文字体」那一档的字符
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF     // 谚文字母
        | 0x2E80..=0x303F   // 部首扩展、康熙部首、中日韩符号和标点
        | 0x3040..=0x30FF   // 平假名、片假名
        | 0x3100..=0x312F   // 注音
        | 0x3130..=0x318F   // 谚文兼容字母
        | 0x31C0..=0x31EF   // 笔画
        | 0x3200..=0x9FFF   // 带圈字符、中日韩统一表意文字
        | 0xA960..=0xA97F
        | 0xAC00..=0xD7FF   // 谚文音节
        | 0xF900..=0xFAFF   // 兼容表意文字
        | 0xFE30..=0xFE4F   // 中日韩兼容形式
        | 0xFF00..=0xFFEF   // 全角字符
        | 0x20000..=0x3FFFF // 扩展 B 及以后
    )
}

/// 给缓冲区里的汉字范围打上「用中文字体」的标记。
///
/// 为什么需要这一步：CSS 的 font-family 是**逐字符**回退的，第一个有该字形的字体就赢了。
/// 而像 JetBrains Maple Mono 这种字体自带汉字，选它当拉丁字体就会把「中文字体」那档
/// 彻底架空。所以译文和原文区直接按字符脚本指定，不靠回退顺序猜。
///
/// `from` 是起始字符偏移，流式追加时只处理新插入的部分。
pub fn tag_cjk(buffer: &gtk::TextBuffer, cjk_family: &str, from: i32) {
    let table = buffer.tag_table();
    let tag = match table.lookup("st-cjk") {
        Some(t) => t,
        None => {
            let t = gtk::TextTag::new(Some("st-cjk"));
            table.add(&t);
            t
        }
    };

    let family = cjk_family.trim();
    if family.is_empty() {
        // 没设中文字体就把标记清掉，交回给 CSS 的回退链
        tag.set_family(None);
        buffer.remove_tag(&tag, &buffer.start_iter(), &buffer.end_iter());
        return;
    }
    tag.set_family(Some(family));

    let start = buffer.iter_at_offset(from);
    let text = buffer.text(&start, &buffer.end_iter(), false);

    let mut run_start: Option<i32> = None;
    let mut off = from;
    for ch in text.chars() {
        if is_cjk(ch) {
            if run_start.is_none() {
                run_start = Some(off);
            }
        } else if let Some(s) = run_start.take() {
            buffer.apply_tag(&tag, &buffer.iter_at_offset(s), &buffer.iter_at_offset(off));
        }
        off += 1;
    }
    if let Some(s) = run_start {
        buffer.apply_tag(&tag, &buffer.iter_at_offset(s), &buffer.iter_at_offset(off));
    }
}

thread_local! {
    /// 当前的中文字体。放这里是为了让控件的回调不用每次去读配置文件。
    static CURRENT_CJK: std::cell::RefCell<String> =
        const { std::cell::RefCell::new(String::new()) };
}

pub fn set_current_cjk(family: &str) {
    CURRENT_CJK.with(|c| *c.borrow_mut() = family.to_string());
}

pub fn current_cjk() -> String {
    CURRENT_CJK.with(|c| c.borrow().clone())
}

/// 给一段文本按脚本生成 Pango 属性：只把汉字区段指定成中文字体，其余不动。
fn cjk_attrs(text: &str, family: &str) -> Option<gtk::pango::AttrList> {
    if family.trim().is_empty() || text.is_empty() {
        return None;
    }
    let list = gtk::pango::AttrList::new();
    let mut any = false;
    let push = |start: usize, end: usize| {
        let mut a = gtk::pango::AttrString::new_family(family);
        a.set_start_index(start as u32);
        a.set_end_index(end as u32);
        list.insert(a);
    };

    // Pango 属性的索引是**字节**偏移，不是字符
    let mut run: Option<usize> = None;
    for (idx, ch) in text.char_indices() {
        if is_cjk(ch) {
            if run.is_none() {
                run = Some(idx);
            }
        } else if let Some(s) = run.take() {
            push(s, idx);
            any = true;
        }
    }
    if let Some(s) = run {
        push(s, text.len());
        any = true;
    }

    if any { Some(list) } else { None }
}

fn apply_to_label(label: &gtk::Label) {
    let family = current_cjk();
    let text = label.text().to_string();
    label.set_attributes(cjk_attrs(&text, &family).as_ref());
}

/// 让界面文字也按字符脚本走字体，而不是只靠 CSS 的回退链。
///
/// CSS 没法按脚本拆，所以只能逐个 Label 挂 Pango 属性。文字会变的 Label
/// （字数、状态栏之类）还要跟着 notify::label 重新算。
///
/// 和 popover 动画钩子一样：建窗口时调一次，窗口 map 后补一次，
/// 列表重建和对话框弹出时再调。用 CSS 类当"已挂钩"标记，避免重复连接。
pub fn hook_ui_script_fonts(root: &impl IsA<gtk::Widget>) {
    fn walk(w: &gtk::Widget) {
        if let Some(l) = w.downcast_ref::<gtk::Label>() {
            apply_to_label(l);
            if !l.has_css_class("st-fonted") {
                l.add_css_class("st-fonted");
                l.connect_label_notify(apply_to_label);
            }
        }
        let mut c = w.first_child();
        while let Some(child) = c {
            walk(&child);
            c = child.next_sibling();
        }
    }
    walk(root.as_ref());
}
