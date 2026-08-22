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
    parse_ranges(css)
}

/// 从一段含 `unicode-range:` 的 CSS 里解析出区间列表
fn parse_ranges(css: &str) -> Vec<(u32, u32)> {
    let start = css
        .find("unicode-range:")
        .expect("这段 CSS 里没有 unicode-range");
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

/// 字体栈的次序：中文档 → 拉丁档 → 后备档 → 通用族。
/// 中文档排不到最前，它的区间限制就没有意义了。
#[test]
fn 字体栈按中文档拉丁档后备档排序() {
    let css = font_css("JetBrains Maple Mono", "Noto Serif CJK SC", "Noto Sans");
    let stack = stack_of(&css);

    let cjk = stack.find("st-cjk").expect("字体栈里没有中文档");
    let latin = stack.find("st-latin").expect("字体栈里没有拉丁档");
    let fallback = stack.find("Noto Sans").expect("字体栈里没有后备档");
    assert!(cjk < latin, "中文档必须排在拉丁档之前");
    assert!(latin < fallback);

    // 两个内部壳各自指向用户填的真字体
    assert!(css.contains("local(\"Noto Serif CJK SC\")"));
    assert!(css.contains("local(\"JetBrains Maple Mono\")"));
}

/// 拉丁档也必须被挡在汉字区之外。
///
/// 只限制中文档是不够的：`unicode-range` 只决定「哪个字体有资格参与」，
/// 被选中的字体真缺字形时，浏览器会**继续往后回退** —— 下一档正是拉丁字体，
/// 于是自带汉字的 Maple 又赢了，绕一圈回到原来的 bug。
///
/// 真实触发场景：用户把中文字体填成 "HarmonyOS Sans"（纯拉丁族，没有汉字，
/// 带汉字的是 "HarmonyOS Sans SC"）。
#[test]
fn 拉丁档不许参与汉字区() {
    let css = font_css("JetBrains Maple Mono", "看不见的中文字体", "");
    let latin = face_ranges(&css, "st-latin").expect("没有生成拉丁档的 @font-face");

    for c in ['汉', '字', '，', 'あ', '한', 'Ａ'] {
        assert!(
            !in_ranges(&latin, c),
            "{c} 落在拉丁档的区间里，中文字体缺字形时会被它接管"
        );
    }
    for c in ['A', 'z', '0', ',', 'Я', 'Ω', '€'] {
        assert!(!is_cjk(c) && in_ranges(&latin, c), "{c} 应由拉丁档负责");
    }
}

/// 汉字最终得有个真能渲染的兜底：两档都缺字形时要落到通用族，
/// 由系统的字体配置给一个真有汉字的字体，而不是显示成豆腐块。
#[test]
fn 字体栈以通用族收尾() {
    let stack = stack_of(&font_css("Maple", "思源宋", "某个后备"));
    let last = stack.rsplit(',').next().unwrap().trim();
    assert!(
        last == "sans-serif" || last == "serif" || last == "system-ui",
        "字体栈应以通用族收尾，实际结尾是 {last}"
    );
}

/// 取出指定 `@font-face` 的 unicode-range。没有该 face 时返回 None。
fn face_ranges(css: &str, family: &str) -> Option<Vec<(u32, u32)>> {
    let needle = format!("font-family: \"{family}\"");
    let at = css.find(&needle)?;
    let rest = &css[at..];
    let end = rest.find('}')?;
    Some(parse_ranges(&rest[..end]))
}

/// 取出 `--st-font:` 那一行的值
fn stack_of(css: &str) -> String {
    let start = css.find("--st-font:").expect("CSS 里没有 --st-font");
    let body = &css[start + "--st-font:".len()..];
    body[..body.find(';').expect("--st-font 没有以分号结束")].to_string()
}

/// 中文字体留空（＝系统默认）时，拉丁档**照样**只管非汉字区。
///
/// 回归测试：早先这条路不加限制、让拉丁字体管全部字符，理由是「用户没表达过
/// 汉字要用别的字体的意思」。真实用户推翻了它 —— 设置页里「中文字体：系统默认」
/// 是一个明确的选项，选它就是在说「汉字用系统默认」；用户把拉丁字体设成
/// Maple（自带汉字）之后，整个界面连汉字都变成了等宽。拉丁档的副标题承诺的是
/// 「自带汉字也会被挡在汉字之外」，实现必须无条件兑现。见 ADR 0001 修订记录。
#[test]
fn 没设中文字体时拉丁档也只管非汉字区() {
    let css = font_css("JetBrains Maple Mono", "", "");

    let latin = face_ranges(&css, "st-latin").expect("没有生成拉丁档的 @font-face");
    for c in ['汉', '字', '，', 'あ', '한', 'Ａ'] {
        assert!(
            !in_ranges(&latin, c),
            "{c} 落在拉丁档区间里，自带汉字的拉丁字体会把「系统默认」架空"
        );
    }
    for c in ['A', 'z', '0', ',', 'Я', 'Ω', '€'] {
        assert!(!is_cjk(c) && in_ranges(&latin, c), "{c} 应由拉丁档负责");
    }

    // 汉字这时不该有任何具名字体认领，落到通用族由系统兜底
    let stack = stack_of(&css);
    assert!(stack.contains("\"st-latin\""));
    assert!(
        !stack.contains("\"JetBrains Maple Mono\""),
        "真字体名只该藏在 local() 里"
    );
    assert!(stack.contains("system-ui"));

    // local() 必须给足三种写法：家族名（WebKitGTK 认）、全名和 PostScript 名
    // （Chromium / WebView2 只认后两种）。少列一种，某个平台上用户选的字体就
    // 整个不生效 —— Windows 真机踩过（家族名写法在 WebView2 里解析失败）。
    assert!(css.contains("local(\"JetBrains Maple Mono\")"));
    assert!(css.contains("local(\"JetBrains Maple Mono Regular\")"));
    assert!(css.contains("local(\"JetBrainsMapleMono-Regular\")"));
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
