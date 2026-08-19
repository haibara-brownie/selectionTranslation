//! 按字符脚本分配字体。
//!
//! 三档：拉丁字体、中文字体、后备字体。
//!
//! 天真的做法是直接把三档串成 `font-family` 靠 CSS 逐字符回退，但这个做法有个硬伤：
//! **像 JetBrains Maple Mono 这类拉丁字体自带汉字字形**，排在第一档就会把「中文字体」
//! 那档整个架空 —— 用户明明选了思源宋，汉字却还是等宽的。
//!
//! 解法是给中文档加 `unicode-range` 并**排在最前**，让它只在汉字区生效，其余字符一律
//! 轮空落到拉丁档。反过来（给拉丁档列白名单）也能work，但白名单列不全西里尔、希腊、
//! 符号、emoji，漏一个就掉进后备档 —— 所以只限制中文档这一边。

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
    format!(
        "@font-face {{\n  \
           font-family: \"{family}\";\n  \
           src: local({});\n  \
           unicode-range: {};\n\
         }}\n\n",
        quote(local),
        range_list(ranges)
    )
}

/// 字体名是用户填的，直接插进 CSS 会被引号截断 —— 剔掉引号和反斜杠再包引号。
fn quote(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .filter(|c| !matches!(c, '"' | '\'' | '\\' | ';' | '{' | '}'))
        .collect();
    format!("\"{cleaned}\"")
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
        if cjk.is_empty() {
            // 用户只配了拉丁字体，那就让它管全部字符 —— 它自带汉字是好事，
            // 这时候去限制它反而会把汉字推给通用族，不是用户想要的。
            stack.push(quote(latin));
        } else {
            // 配了中文字体，拉丁档就要反过来**只在汉字区以外**参与。
            //
            // 光限制中文档是不够的 —— `unicode-range` 只决定"哪个字体有资格参与"，
            // 被选中的字体真缺字形时浏览器会继续往后回退，下一档正是拉丁字体，
            // 于是自带汉字的等宽字体又赢了，绕一圈回到原来的 bug。
            // 真实触发场景：用户把中文字体填成 "HarmonyOS Sans"（纯拉丁族，
            // 带汉字的是 "HarmonyOS Sans SC"）。
            //
            // 两档都挡住之后，汉字最终落到末尾的通用族，由系统字体配置兜底 ——
            // 不见得漂亮，但至少是真的汉字，不是豆腐块。
            let _ = write!(css, "{}", font_face(LATIN_FACE, latin, &non_cjk_ranges()));
            stack.push(format!("\"{LATIN_FACE}\""));
        }
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
