//! 系统字体查询。
//!
//! 这里查的是**真机上真装着的字体**，所以一条断言都不能写死某个字体名 ——
//! 换台机器、换个发行版，字体集就完全不一样了。能测的只有不随机器变的性质：
//! 列表的形状（非空、有序、无重复），以及 `covers_cjk` 的判别力
//! （不是恒真也不是恒假，且不会被不存在的名字骗到）。
//!
//! 唯一一条涉及具体字体的断言（HarmonyOS Sans 那对）先问 `families()` 有没有装，
//! 没装就跳过。它值得留着是因为那正是这个函数要防的真实坑。

use seltrans_core::system_fonts::{covers_cjk, families};

/// 跑测试的机器总归装了字体。这条挂了就说明后端根本没连上（fontconfig 没配、
/// 依赖没链上），后面几条测的东西也就没意义了。
#[test]
fn 能列出系统字体() {
    let f = families();
    assert!(!f.is_empty(), "一个字体家族都没查到，字体后端多半没工作");
    assert!(
        f.iter().all(|s| !s.trim().is_empty()),
        "列表里有空名字：{f:?}"
    );
}

/// 下拉框直接拿这个列表填，所以顺序和唯一性是接口的一部分，不是实现细节。
/// 排序按小写：用户找 "noto" 时不该被大小写把同名字体打散到两处。
#[test]
fn 列表按名字排序且无重复() {
    let f = families();
    let keys: Vec<String> = f.iter().map(|s| s.to_lowercase()).collect();

    assert!(
        keys.windows(2).all(|w| w[0] <= w[1]),
        "列表没有按名字排序：{:?}",
        keys.windows(2).find(|w| w[0] > w[1]).unwrap()
    );

    let mut uniq = f.clone();
    uniq.dedup();
    assert_eq!(uniq.len(), f.len(), "列表里有重复家族名");
}

/// 核心：这个判定必须有**判别力**。恒真恒假都能让下面所有断言以外的测试过，
/// 但在设置页上一个是永远不报警、一个是永远瞎报警，两种都等于没做。
///
/// 一台正常机器上两类字体都有（拉丁正文字体 + 至少一个中文字体），
/// 所以「至少一个 true、至少一个 false」是安全的断言。
#[test]
fn 对已装字体能分出有汉字和没汉字两类() {
    let f = families();
    let (mut 有, mut 没有) = (Vec::new(), Vec::new());
    for name in &f {
        if covers_cjk(name) {
            有.push(name.clone());
        } else {
            没有.push(name.clone());
        }
    }

    // 这条断言要求机器上**装了中日韩字体**。开发机上必然有，但 CI 的 runner 镜像
    // 不保证 —— ubuntu-24.04 默认就没有 CJK 字体。那种环境下判定恒假是**正确行为**
    // （确实一个含汉字的字体都没有），不该报测试失败。
    //
    // 这也是本文件开头立的规矩：断言不许依赖具体机器装了什么字体。
    if 有.is_empty() {
        eprintln!(
            "跳过「有汉字」那一半：本机 {} 个字体家族里一个中日韩字体都没有",
            f.len()
        );
    }
    assert!(
        !没有.is_empty(),
        "所有已装字体都被判定为含汉字，判定多半恒真（十有八九是被字体替换骗了）"
    );
}

/// 用户可以把字体框留空（表示不干预），空名字不该被当成一个"没有汉字的字体"去报警。
/// 全是空白的输入等同于留空。
#[test]
fn 空字体名不算有汉字() {
    assert!(!covers_cjk(""));
    assert!(!covers_cjk("   "));
    assert!(!covers_cjk("\t\n"));
}

/// 防字体替换。字体查询在有的平台上会替换 —— 问一个不存在的家族，系统乐意给你一个
/// 能用的顶上，于是任何乱码名字都被判成「有汉字」，判定就此恒真、警告永不出现。
///
/// 坦白：本机（fontconfig）对陌生名字直接返回「找不到」，所以这条在 Linux 上是抓不到
/// 实现里那道核对逻辑被删掉的 —— 它守的是 macOS/CoreText 那条路，而那条路在这儿验不了。
#[test]
fn 不存在的字体名不会被替换字体蒙混过关() {
    assert!(!covers_cjk("绝对不存在的字体名 xyzzy"));
    assert!(!covers_cjk("Xyzzy Plugh Nonexistent Family 12345"));
    // 中文名的假字体尤其危险：替换上来的十有八九真带汉字
    assert!(!covers_cjk("并不存在的中文字体"));
}

/// 真实踩过的坑：`HarmonyOS Sans` 是纯拉丁族，带汉字的是 `HarmonyOS Sans SC`。
/// 名字只差两个字母，用户在设置页选错了，汉字就全掉进兜底字体。
///
/// 这对字体是这个函数存在的理由，但不能假设每台机器都装了 —— 没装就跳过。
#[test]
fn 能分开_harmonyos_sans_和它的中文版() {
    let f = families();
    let 装了 = |name: &str| f.iter().any(|x| x == name);

    if !装了("HarmonyOS Sans") || !装了("HarmonyOS Sans SC") {
        eprintln!("跳过：本机没有同时装 HarmonyOS Sans 和 HarmonyOS Sans SC");
        return;
    }

    assert!(
        !covers_cjk("HarmonyOS Sans"),
        "HarmonyOS Sans 是纯拉丁族，不该判定为含汉字"
    );
    assert!(
        covers_cjk("HarmonyOS Sans SC"),
        "HarmonyOS Sans SC 带汉字，该判定为含汉字"
    );
}

/// 繁体、日文、韩文专用字体也归「中文字体」这一档（那一档覆盖的是整个 CJK，
/// 见 typography::CJK_RANGES），不能只认简体。
///
/// 实测 `HarmonyOS Sans TC` 只有 U+6F22「漢」没有 U+6C49「汉」—— 单探简体会把一个
/// 正经的繁体字体判成"没有汉字"。这个警告一旦误报，用户就学会无视它了，比不报还糟。
#[test]
fn 繁体专供字体也算含汉字() {
    let f = families();
    if !f.iter().any(|x| x == "HarmonyOS Sans TC") {
        eprintln!("跳过：本机没装 HarmonyOS Sans TC");
        return;
    }
    assert!(
        covers_cjk("HarmonyOS Sans TC"),
        "繁体专供字体该判定为含汉字（它有 U+6F22 漢，只是没有 U+6C49 汉）"
    );
}

/// 用户是手打字体名的，大小写不会跟系统报的一模一样。
/// 拿列表里真实存在的字体做，避免依赖具体字体。
#[test]
fn 字体名的大小写和首尾空白不影响判定() {
    let f = families();
    // 找一个 ASCII 名的字体来试大小写 —— 非 ASCII 名没有大小写之分，试不出东西
    let Some(name) = f.iter().find(|s| s.is_ascii() && covers_cjk(s)) else {
        eprintln!("跳过：本机没有 ASCII 名的含汉字字体");
        return;
    };

    assert!(
        covers_cjk(&name.to_uppercase()),
        "全大写应得到同样结果：{name}"
    );
    assert!(
        covers_cjk(&name.to_lowercase()),
        "全小写应得到同样结果：{name}"
    );
    assert!(
        covers_cjk(&format!("  {name}  ")),
        "带首尾空白应得到同样结果：{name}"
    );
}
