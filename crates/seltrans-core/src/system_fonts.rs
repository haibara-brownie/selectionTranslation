//! 查系统装了哪些字体，以及某个字体家族到底有没有汉字。
//!
//! GTK 版是拿 Pango 干这件事的（`src/fonts.rs`），但 Pango 是 GUI 工具包的一部分，
//! mac 和 Windows 上的 Tauri 版根本没有它。这里换成 font-kit：它在三个平台上分别走
//! fontconfig / CoreText / DirectWrite，接口是统一的，也不需要先建出一个窗口。
//!
//! `covers_cjk` 是这个模块存在的真正理由。用户在设置里填「中文字体」时踩过的坑是
//! `HarmonyOS Sans` 是**纯拉丁族**，带汉字的那个叫 `HarmonyOS Sans SC` —— 名字只差
//! 两个字母，选错了汉字就全掉进兜底字体。设置界面要能当场警告，就得有个办法问
//! 「这个家族自己有没有汉字字形」。

use font_kit::source::SystemSource;

/// 判定用的探针字：命中任意一个就算这个家族能担「中文字体」这一档。
///
/// 四个字分别代表这一档实际覆盖的四种文字（见 `typography::CJK_RANGES`）——
/// 简体汉字、繁体/日文汉字、平假名、谚文音节。
///
/// **只用表意文字和音节文字，绝不用标点。** 这是关键：本函数要防的假阳性是
/// 「某些拉丁字体顺手带了几个全角标点就被当成中文字体」，而拉丁字体不可能带上
/// 「漢」或「한」这样的字。所以放宽到四个探针不会把那类字体放进来。
///
/// 只探一个「汉」是不够的：实测 `HarmonyOS Sans TC` 只有 U+6F22「漢」没有 U+6C49，
/// 单探简体会把一个正经的繁体字体判成"没有汉字"，而这个警告一旦误报，用户就会
/// 学会无视它 —— 那比不报还糟。日文、韩文专用字体同理。
const PROBES: [char; 4] = ['汉', '漢', 'あ', '한'];

/// 系统已装的字体家族名，按名字排序去重。
///
/// 不缓存。实测（本机 295 个家族，debug 构建）一次约 5-6 ms —— 设置页打开时调一次，
/// 这个量级用户感知不到，不值得为它引入「刚装的字体不在下拉框里」这种陈旧状态。
pub fn families() -> Vec<String> {
    // 系统字体配置坏掉时（fontconfig 读不到配置、DirectWrite 初始化失败）就当一个都没有，
    // 设置页会退化成一个空下拉框，总好过整个程序起不来
    let Ok(mut v) = SystemSource::new().all_families() else {
        return Vec::new();
    };
    // 按小写排序是给人看的：用户在下拉框里找 "noto" 时不该被大小写打散。
    // 排完再去重 —— fontconfig 同一家族装在多个目录下会报多次。
    v.sort_by_key(|s| s.to_lowercase());
    v.dedup();
    v
}

/// 这个字体家族自己有没有汉字字形。
///
/// 同样不缓存：用户可能刚装完字体就回来改设置，这时候给他一个陈旧的答案，
/// 警告条就会指着一个其实已经没问题的字体不放。单次查询只加载这一个家族，实测约 6 ms。
///
/// 注意「自己有没有」这个措辞。字体查询在有的平台上是**会替换**的：问一个不存在的家族，
/// 系统热心地给你一个能用的顶上，于是什么乱码名字都变成「有汉字」，判定恒真。所以先拿
/// `families()` 核一遍名字，确认这个家族真的装了，再去问它有没有字形。
///
/// （实测 Linux 的 fontconfig 和 Windows 的 DirectWrite 对陌生名字都直接返回「找不到」，
/// 这道核对在那两边是白做的。留着是为 macOS：CoreText 那边 font-kit 是拿你给的名字
/// 现搭一个 descriptor 再建集合，替换风险实打实存在，而这台机器上验不了。）
pub fn covers_cjk(family: &str) -> bool {
    let family = family.trim();
    if family.is_empty() {
        return false;
    }

    // 用系统自己报的写法去查，而不是用户手打的那个 —— 各平台对家族名大小写的宽容度不一样，
    // 统一成列表里的规范写法，三边行为才一致
    let Some(canonical) = families()
        .into_iter()
        .find(|f| f.eq_ignore_ascii_case(family))
    else {
        return false;
    };

    let Ok(handle) = SystemSource::new().select_family_by_name(&canonical) else {
        return false;
    };

    // 一个家族下有多个 face（常规、粗体、斜体……）。只要有一个带汉字就算这个家族能打 ——
    // 缺字形的往往是某个单独的字重，不该因此判定整族没有中文。
    //
    // 坏字体文件、没有读权限、格式不认识：`load` 会给 Err，跳过这个 face 就是了，
    // 别让一个装坏的字体把整次查询搞崩。
    handle.fonts().iter().any(|h| {
        h.load().is_ok_and(|font| {
            PROBES.iter().any(|&c| {
                // 字形号 0 是 .notdef（那个豆腐块），有它不等于有字
                matches!(font.glyph_for_char(c), Some(g) if g != 0)
            })
        })
    })
}
