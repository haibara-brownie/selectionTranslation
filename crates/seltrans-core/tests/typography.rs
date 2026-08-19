//! 按字符脚本分配字体。
//!
//! 背景：像 JetBrains Maple Mono 这类拉丁字体自带汉字字形，直接靠 CSS 的回退顺序
//! 会让它把「中文字体」那一档整个架空。解法是给中文档加 `unicode-range` 并排在最前，
//! 让它只在汉字区生效，其余字符一律轮空落到拉丁档。
//!
//! 浏览器里的实际渲染效果没法在这儿测（那是 .scratch 里的探针干的活）。
//! 这里测的是**两份表不许漂移**：`is_cjk` 的判定和 CSS 里声明的区间必须说同一件事。

use seltrans_core::typography::{font_css, is_cjk};

/// 从生成的 CSS 里把 `unicode-range` 的区间解析回来。
/// 刻意不复用实现里的常量 —— 走一遍文本才能发现「表改了但 CSS 没跟上」。
fn ranges_in(css: &str) -> Vec<(u32, u32)> {
    let start = css
        .find("unicode-range:")
        .expect("CSS 里没有 unicode-range");
    let body = &css[start + "unicode-range:".len()..];
    let body = &body[..body.find(';').expect("unicode-range 没有以分号结束")];

    body.split(',')
        .map(|item| {
            let item = item.trim().trim_start_matches("U+");
            match item.split_once('-') {
                Some((a, b)) => (
                    u32::from_str_radix(a, 16).unwrap(),
                    u32::from_str_radix(b, 16).unwrap(),
                ),
                None => {
                    let v = u32::from_str_radix(item, 16).unwrap();
                    (v, v)
                }
            }
        })
        .collect()
}

fn in_ranges(ranges: &[(u32, u32)], c: char) -> bool {
    ranges
        .iter()
        .any(|&(a, b)| (c as u32) >= a && (c as u32) <= b)
}

/// 具体字符该不该算「中文字体那一档」。判据是 Unicode 区块，不是实现。
#[test]
fn 判定哪些字符归中文档() {
    // 汉字、全角标点、全角空格、假名、谚文、全角拉丁、带圈中日韩字符
    for c in ['汉', '字', '，', '。', '　', 'あ', 'カ', '한', 'Ａ', '㈱'] {
        assert!(is_cjk(c), "{c} 应归中文档");
    }
    // 拉丁、西里尔、希腊、货币、箭头、带重音拉丁
    for c in ['A', 'z', '0', ' ', ',', '.', 'Я', 'Ω', '€', '→', 'é'] {
        assert!(!is_cjk(c), "{c} 不该归中文档");
    }
    // 边界：带圈数字 ①（U+2460）在 Enclosed Alphanumerics，不是中日韩专属区。
    // 中文排版里确实常见，但收进来会连带影响纯拉丁文本里的 ①，故沿用 GTK 版的判定不收。
    assert!(!is_cjk('①'));
}

/// 核心不变量：CSS 里声明的区间和 is_cjk 必须一致，任何一边单独改都要被抓住
#[test]
fn css_区间与_is_cjk_不许漂移() {
    let css = font_css("JetBrains Maple Mono", "Noto Serif CJK SC", "");
    let ranges = ranges_in(&css);
    assert!(!ranges.is_empty());

    // 每个区间的两端各取内外两个码位，逐一比对两种判定
    for &(a, b) in &ranges {
        for probe in [a.saturating_sub(1), a, b, b + 1] {
            let Some(c) = char::from_u32(probe) else {
                continue;
            };
            assert_eq!(
                is_cjk(c),
                in_ranges(&ranges, c),
                "U+{probe:04X}（{c:?}）两边判定不一致"
            );
        }
    }
}

/// 中文档排在字体栈最前，拉丁档跟在后面且不受区间限制
#[test]
fn 中文档排在拉丁档前面() {
    let css = font_css("JetBrains Maple Mono", "Noto Serif CJK SC", "Noto Sans");
    let stack = stack_of(&css);

    let cjk = stack.find("st-cjk").expect("字体栈里没有中文档");
    let latin = stack
        .find("JetBrains Maple Mono")
        .expect("字体栈里没有拉丁档");
    let fallback = stack.find("Noto Sans").expect("字体栈里没有后备档");
    assert!(
        cjk < latin,
        "中文档必须排在拉丁档之前，否则区间限制没有意义"
    );
    assert!(latin < fallback);
}

/// 取出 `--st-font:` 那一行的值
fn stack_of(css: &str) -> String {
    let start = css.find("--st-font:").expect("CSS 里没有 --st-font");
    let body = &css[start + "--st-font:".len()..];
    body[..body.find(';').expect("--st-font 没有以分号结束")].to_string()
}

/// 没设中文字体时不能发空的 `local()` —— 那会让整条 @font-face 失效
#[test]
fn 中文档留空则不发_font_face() {
    let css = font_css("JetBrains Maple Mono", "", "");
    assert!(!css.contains("@font-face"), "中文档为空时不该有 @font-face");
    assert!(stack_of(&css).contains("JetBrains Maple Mono"));
}

#[test]
fn 三档全空时给一个中性默认() {
    let css = font_css("", "", "");
    assert!(!css.contains("@font-face"));
    let stack = stack_of(&css);
    assert!(
        stack.contains("system-ui") || stack.contains("sans-serif"),
        "三档都空时应回落到系统默认，实际：{stack}"
    );
}

/// 字体名是用户从设置里填的，带引号或花括号就能把 CSS 提前截断、注入自己的规则
#[test]
fn 字体名不能突破_css_字符串() {
    let evil = "Ev\"il; } body { display:none } .x{";
    let css = font_css(evil, evil, "");

    // 关键不是「注入的字样有没有出现」，而是「有没有字符能跳出引号」——
    // 剩下的字面量待在引号里，只是个找不到的字体名，无害。
    // 按引号切开：下标为奇数的片段就是引号内部，那里面不许有结构字符。
    let parts: Vec<&str> = css.split('"').collect();
    assert!(
        parts.len() % 2 == 1,
        "引号没有配对，说明字体名里的引号漏出来了：\n{css}"
    );
    for inside in parts.iter().skip(1).step_by(2) {
        for bad in ['\'', '\\', ';', '{', '}'] {
            assert!(
                !inside.contains(bad),
                "引号内出现 {bad:?}，能注入 CSS。片段：{inside:?}"
            );
        }
    }
}
