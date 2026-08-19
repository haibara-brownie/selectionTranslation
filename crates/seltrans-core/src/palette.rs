//! Catppuccin 配色，四个风味：Latte（浅）、Frappé、Macchiato、Mocha（深）。
//!
//! 色值取自官方调色板 <https://github.com/catppuccin/palette>。
//!
//! 放在 core 而不是界面层，是因为 GTK 版和 web 版都要用同一组色 —— 抄两份必然漂移。
//! 这里只负责「有哪些色、叫什么名字」，怎么套到控件上是各界面层自己的事。

use std::fmt::Write as _;

pub struct Flavor {
    pub id: &'static str,
    pub label: &'static str,
    pub dark: bool,
    pub base: &'static str,
    pub mantle: &'static str,
    pub crust: &'static str,
    pub surface0: &'static str,
    pub surface1: &'static str,
    pub surface2: &'static str,
    pub overlay0: &'static str,
    pub overlay1: &'static str,
    pub text: &'static str,
    pub subtext0: &'static str,
    pub subtext1: &'static str,
    pub blue: &'static str,
    pub mauve: &'static str,
    pub red: &'static str,
    pub green: &'static str,
    pub yellow: &'static str,
}

impl Flavor {
    /// 全部色位的 (名字, 色值)。
    ///
    /// 有了它，「生成 CSS 变量」和「检查四个风味结构一致」都不用逐字段手写 ——
    /// 加一个色位只要改这里和结构体两处，不会漏掉某个风味。
    pub fn entries(&self) -> [(&'static str, &'static str); 16] {
        [
            ("base", self.base),
            ("mantle", self.mantle),
            ("crust", self.crust),
            ("surface0", self.surface0),
            ("surface1", self.surface1),
            ("surface2", self.surface2),
            ("overlay0", self.overlay0),
            ("overlay1", self.overlay1),
            ("text", self.text),
            ("subtext0", self.subtext0),
            ("subtext1", self.subtext1),
            ("blue", self.blue),
            ("mauve", self.mauve),
            ("red", self.red),
            ("green", self.green),
            ("yellow", self.yellow),
        ]
    }
}

pub const LATTE: Flavor = Flavor {
    id: "latte",
    label: "Latte（浅色）",
    dark: false,
    base: "#eff1f5",
    mantle: "#e6e9ef",
    crust: "#dce0e8",
    surface0: "#ccd0da",
    surface1: "#bcc0cc",
    surface2: "#acb0be",
    overlay0: "#9ca0b0",
    overlay1: "#8c8fa1",
    text: "#4c4f69",
    subtext0: "#6c6f85",
    subtext1: "#5c5f77",
    blue: "#1e66f5",
    mauve: "#8839ef",
    red: "#d20f39",
    green: "#40a02b",
    yellow: "#df8e1d",
};

pub const FRAPPE: Flavor = Flavor {
    id: "frappe",
    label: "Frappé（深色）",
    dark: true,
    base: "#303446",
    mantle: "#292c3c",
    crust: "#232634",
    surface0: "#414559",
    surface1: "#51576d",
    surface2: "#626880",
    overlay0: "#737994",
    overlay1: "#838ba7",
    text: "#c6d0f5",
    subtext0: "#a5adce",
    subtext1: "#b5bfe2",
    blue: "#8caaee",
    mauve: "#ca9ee6",
    red: "#e78284",
    green: "#a6d189",
    yellow: "#e5c890",
};

pub const MACCHIATO: Flavor = Flavor {
    id: "macchiato",
    label: "Macchiato（深色）",
    dark: true,
    base: "#24273a",
    mantle: "#1e2030",
    crust: "#181926",
    surface0: "#363a4f",
    surface1: "#494d64",
    surface2: "#5b6078",
    overlay0: "#6e738d",
    overlay1: "#8087a2",
    text: "#cad3f5",
    subtext0: "#a5adcb",
    subtext1: "#b8c0e0",
    blue: "#8aadf4",
    mauve: "#c6a0f6",
    red: "#ed8796",
    green: "#a6da95",
    yellow: "#eed49f",
};

pub const MOCHA: Flavor = Flavor {
    id: "mocha",
    label: "Mocha（深色）",
    dark: true,
    base: "#1e1e2e",
    mantle: "#181825",
    crust: "#11111b",
    surface0: "#313244",
    surface1: "#45475a",
    surface2: "#585b70",
    overlay0: "#6c7086",
    overlay1: "#7f849c",
    text: "#cdd6f4",
    subtext0: "#a6adc8",
    subtext1: "#bac2de",
    blue: "#89b4fa",
    mauve: "#cba6f7",
    red: "#f38ba8",
    green: "#a6e3a1",
    yellow: "#f9e2af",
};

pub const FLAVORS: [&Flavor; 4] = [&LATTE, &FRAPPE, &MACCHIATO, &MOCHA];

/// 设置里可选的项：跟随系统 + 四个风味
pub const CHOICES: [(&str, &str); 5] = [
    ("system", "跟随系统（浅色 Latte / 深色 Mocha）"),
    ("latte", "Latte（浅色）"),
    ("frappe", "Frappé（深色）"),
    ("macchiato", "Macchiato（深色）"),
    ("mocha", "Mocha（深色）"),
];

pub fn flavor_by_id(id: &str) -> Option<&'static Flavor> {
    FLAVORS.iter().copied().find(|f| f.id == id)
}

/// 跟随系统时用哪两个风味
pub fn for_system(dark: bool) -> &'static Flavor {
    if dark { &MOCHA } else { &LATTE }
}

/// 把一个风味铺成 CSS 自定义属性，供前端直接 `var(--ctp-base)` 取用。
///
/// 同时声明 `color-scheme`，让滚动条、表单控件这些原生部件跟着走明暗，
/// 否则深色主题下会冒出一条白色滚动条。
pub fn css_variables(f: &Flavor) -> String {
    let mut css = String::from(":root {\n");
    let _ = writeln!(
        css,
        "  color-scheme: {};",
        if f.dark { "dark" } else { "light" }
    );
    for (name, value) in f.entries() {
        let _ = writeln!(css, "  --ctp-{name}: {value};");
    }
    css.push_str("}\n");
    css
}
