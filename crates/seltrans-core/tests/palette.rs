//! Catppuccin 配色。
//!
//! 界面层（GTK 也好、web 也好）都从这里取色，不各自抄一份 —— 抄两份必然漂移。
//! 这里测的是「四个风味结构一致」这类不变量，具体色值对不对是眼睛的事。

use seltrans_core::palette::{FLAVORS, Flavor, css_variables, flavor_by_id};

fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

#[test]
fn 四个风味齐全且_id_唯一() {
    let ids: Vec<&str> = FLAVORS.iter().map(|f| f.id).collect();
    assert_eq!(ids, ["latte", "frappe", "macchiato", "mocha"]);

    // Latte 是唯一的浅色
    let dark: Vec<bool> = FLAVORS.iter().map(|f| f.dark).collect();
    assert_eq!(dark, [false, true, true, true]);
}

#[test]
fn 按_id_能取回同一个风味() {
    for f in FLAVORS {
        assert_eq!(flavor_by_id(f.id).map(|x| x.id), Some(f.id));
    }
    assert!(flavor_by_id("不存在的风味").is_none());
}

/// 每个色位都得是合法的 #rrggbb —— 少一位就会让整条 CSS 规则被浏览器丢掉
#[test]
fn 所有色值都是合法十六进制() {
    for f in FLAVORS {
        for (name, value) in f.entries() {
            assert!(
                is_hex_color(value),
                "{}.{name} = {value} 不是合法色值",
                f.id
            );
        }
    }
}

/// 四个风味必须暴露**同一组**变量名。少一个，切到那个风味时界面就会缺色。
#[test]
fn 四个风味的变量集合完全一致() {
    let names_of = |f: &Flavor| {
        let mut v: Vec<&str> = f.entries().iter().map(|(n, _)| *n).collect();
        v.sort_unstable();
        v
    };
    let first = names_of(FLAVORS[0]);
    for f in &FLAVORS[1..] {
        assert_eq!(names_of(f), first, "{} 的色位与 latte 对不上", f.id);
    }
}

/// 生成的 CSS 要把每个色位都变成一个自定义属性，供前端直接引用
#[test]
fn css_变量覆盖全部色位() {
    for f in FLAVORS {
        let css = css_variables(f);
        for (name, value) in f.entries() {
            let decl = format!("--ctp-{name}: {value};");
            assert!(css.contains(&decl), "{} 的 CSS 里缺 {decl}", f.id);
        }
        // 明暗要能被 CSS 直接用上（决定滚动条、表单控件的原生配色）
        let scheme = if f.dark { "dark" } else { "light" };
        assert!(
            css.contains(&format!("color-scheme: {scheme}")),
            "{} 应声明 color-scheme: {scheme}",
            f.id
        );
    }
}
