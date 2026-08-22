//! 按字符脚本分配字体。
//!
//! 三档：拉丁字体、中文字体、后备字体。
//!
//! 天真的做法是直接把三档串成 `font-family` 靠 CSS 逐字符回退，但这个做法有个硬伤：
//! **像 JetBrains Maple Mono 这类拉丁字体自带汉字字形**，排在第一档就会把「中文字体」
//! 那档整个架空 —— 用户明明选了思源宋，汉字却还是等宽的。
//!
//! 解法是**两档各自钉死在自己的区间**：中文档的 `unicode-range` 取 [`CJK_RANGES`]
//! 并排最前；拉丁档取同一张表的**补集**（从表派生，不必手列"非汉字"白名单——那种
//! 白名单列不全西里尔、希腊、符号、emoji，漏一个就掉进后备档）。汉字从此不归拉丁档
//! 管：中文档填了就归它，留空则落到后备档或通用族，即真正的系统默认。

use std::fmt::Write as _;

/// 该用「中文字体」那一档的 Unicode 区间。
///
/// 这是**单一事实源**：`is_cjk` 和生成的 `unicode-range` 都从这张表派生，
/// 改了一边另一边自动跟上（`tests/typography.rs` 会盯着两者不许漂移）。
pub const CJK_RANGES: &[(u32, u32)] = &[
    (0x1100, 0x11FF),   // 谚文字母
    (0x2E80, 0x303F),   // 部首扩展、康熙部首、中日韩符号和标点
    (0x3040, 0x30FF),   // 平假名、片假名
    (0x3100, 0x312F),   // 注音
    (0x3130, 0x318F),   // 谚文兼容字母
    (0x31C0, 0x31EF),   // 笔画
    (0x3200, 0x9FFF),   // 带圈字符、中日韩统一表意文字
    (0xA960, 0xA97F),   // 谚文字母扩展 A
    (0xAC00, 0xD7FF),   // 谚文音节
    (0xF900, 0xFAFF),   // 兼容表意文字
    (0xFE30, 0xFE4F),   // 中日韩兼容形式
    (0xFF00, 0xFFEF),   // 全角字符
    (0x20000, 0x3FFFF), // 扩展 B 及以后
];

/// 这个字符归不归「中文字体」那一档
pub fn is_cjk(c: char) -> bool {
    let v = c as u32;
    CJK_RANGES.iter().any(|&(a, b)| v >= a && v <= b)
}

/// Unicode 的最大码位
const MAX_CODEPOINT: u32 = 0x10FFFF;

/// `CJK_RANGES` 在整个 Unicode 上的补集 —— 拉丁档只在这些区间里参与。
///
/// 由那张表算出来而不是另抄一份，两边才不会漂移。
fn non_cjk_ranges() -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let mut cursor = 0u32;
    // CJK_RANGES 按码位升序排列且互不重叠，直接顺着扫
    for &(a, b) in CJK_RANGES {
        if cursor < a {
            out.push((cursor, a - 1));
        }
        cursor = b + 1;
    }
    if cursor <= MAX_CODEPOINT {
        out.push((cursor, MAX_CODEPOINT));
    }
    out
}

/// CSS 里 `@font-face` 用的内部字体名。用户看不到，只是给各档一个能加区间限制的壳。
const CJK_FACE: &str = "st-cjk";
const LATIN_FACE: &str = "st-latin";

fn range_list(ranges: &[(u32, u32)]) -> String {
    ranges
        .iter()
        .map(|&(a, b)| format!("U+{a:04X}-{b:04X}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn font_face(family: &str, local: &str, ranges: &[(u32, u32)]) -> String {
    // `local()` 按规范匹配的是「字体全名」或 PostScript 名，**不保证认家族名**。
    // 实测分歧：WebKitGTK（Linux）认家族名；Chromium / WebView2（Windows）不认 ——
    // local("Maple Mono SC NF") 解析失败，local("Maple Mono SC NF Regular")（全名）和
    // local("MapleMonoSCNF-Regular")（PostScript 名）都能中。只写家族名的话，
    // Windows 上两档 @font-face 全部落空，用户选的字体一个都不生效。
    //
    // 所以三种写法全列上：家族名、家族名 + " Regular"（常规体的全名惯例）、
    // 去空格家族名 + "-Regular"（PostScript 名惯例）。src 列表按顺序试到能用为止，
    // 多余的候选没有副作用。代价是粗细变体由浏览器从常规体合成，可以接受。
    let name = clean(local);
    let ps = name.replace(' ', "");
    format!(
        "@font-face {{\n  \
           font-family: \"{family}\";\n  \
           src: local(\"{name}\"), local(\"{name} Regular\"), local(\"{ps}-Regular\");\n  \
           unicode-range: {};\n\
         }}\n\n",
        range_list(ranges)
    )
}

/// 字体名是用户填的，直接插进 CSS 会被引号截断 —— 剔掉引号、反斜杠等危险字符。
fn clean(name: &str) -> String {
    name.trim()
        .chars()
        .filter(|c| !matches!(c, '"' | '\'' | '\\' | ';' | '{' | '}'))
        .collect()
}

/// [`clean`] 之后再包引号，塞进 `font-family` 栈用。
fn quote(name: &str) -> String {
    format!("\"{}\"", clean(name))
}

/// 生成三档字体的 CSS：中文档的 `@font-face`（带区间限制）+ `--st-font` 字体栈。
///
/// 三档都可以留空。中文档留空时不发 `@font-face` —— 空的 `local()` 会让整条规则失效。
pub fn font_css(latin: &str, cjk: &str, fallback: &str) -> String {
    let mut css = String::new();
    let (latin, cjk, fallback) = (latin.trim(), cjk.trim(), fallback.trim());
    let mut stack: Vec<String> = Vec::new();

    // 中文档：只在汉字区参与，排最前
    if !cjk.is_empty() {
        let _ = write!(css, "{}", font_face(CJK_FACE, cjk, CJK_RANGES));
        stack.push(format!("\"{CJK_FACE}\""));
    }

    if !latin.is_empty() {
        // 拉丁档**一律**只在汉字区以外参与，中文档填没填都一样。
        //
        // 光限制中文档是不够的 —— `unicode-range` 只决定"哪个字体有资格参与"，
        // 被选中的字体真缺字形时浏览器会继续往后回退，下一档正是拉丁字体，
        // 于是自带汉字的等宽字体又赢了，绕一圈回到原来的 bug。
        // 真实触发场景：用户把中文字体填成 "HarmonyOS Sans"（纯拉丁族，
        // 带汉字的是 "HarmonyOS Sans SC"）。
        //
        // 中文档留空时**也要挡**。早先这里不挡、让拉丁字体管全部字符，理由是
        // 「用户没表达过汉字要用别的字体的意思」—— 真实用户推翻了这个预设：
        // 设置页里「中文字体：系统默认」是一个明确的选项，选它就是在说「汉字用
        // 系统默认」；拉丁档的副标题也承诺过「自带汉字会被挡在汉字之外」。
        // 用户选了 Maple 之后连汉字都变等宽，报的就是这个。见 ADR 0001 修订记录。
        //
        // 两处都挡住之后，汉字落到后备档或末尾的通用族，由系统字体配置兜底 ——
        // 不见得漂亮，但至少是「系统默认」说的那个意思。
        let _ = write!(css, "{}", font_face(LATIN_FACE, latin, &non_cjk_ranges()));
        stack.push(format!("\"{LATIN_FACE}\""));
    }

    if !fallback.is_empty() {
        stack.push(quote(fallback));
    }
    // 收尾的通用族：三档都空时字体栈不至于是空的，汉字也总有人管
    stack.push("system-ui".into());
    stack.push("sans-serif".into());

    let _ = write!(css, ":root {{\n  --st-font: {};\n}}\n", stack.join(", "));
    css
}
