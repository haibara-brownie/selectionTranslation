//! Catppuccin 配色 + 明暗模式。
//!
//! 色值取自官方调色板 <https://github.com/catppuccin/palette>，四个风味：
//! Latte（浅）、Frappé、Macchiato、Mocha（深）。
//!
//! 同时输出 libadwaita 1.6+ 的 CSS 变量和旧版 `@define-color`：前者是现在推荐的写法，
//! 后者留着兼容，两边都发不会互相干扰。

use adw::prelude::*;
use std::cell::{Cell, RefCell};

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

fn css(f: &Flavor) -> String {
    // 输入 / 译文两个框：surface0 底 + surface1 描边，圆角卡片
    format!(
        r#"
:root {{
  --window-bg-color: {base};
  --window-fg-color: {text};
  --view-bg-color: {mantle};
  --view-fg-color: {text};
  --headerbar-bg-color: {mantle};
  --headerbar-fg-color: {text};
  --headerbar-border-color: {surface0};
  --headerbar-backdrop-color: {crust};
  --card-bg-color: {surface0};
  --card-fg-color: {text};
  --dialog-bg-color: {base};
  --dialog-fg-color: {text};
  --popover-bg-color: {mantle};
  --popover-fg-color: {text};
  --sidebar-bg-color: {mantle};
  --sidebar-fg-color: {text};
  --accent-bg-color: {blue};
  --accent-fg-color: {base};
  --accent-color: {blue};
  --destructive-bg-color: {red};
  --destructive-fg-color: {base};
  --success-color: {green};
  --warning-color: {yellow};
  --error-color: {red};
}}

@define-color window_bg_color {base};
@define-color window_fg_color {text};
@define-color view_bg_color {mantle};
@define-color view_fg_color {text};
@define-color headerbar_bg_color {mantle};
@define-color headerbar_fg_color {text};
@define-color card_bg_color {surface0};
@define-color card_fg_color {text};
@define-color popover_bg_color {mantle};
@define-color popover_fg_color {text};
@define-color dialog_bg_color {base};
@define-color dialog_fg_color {text};
@define-color accent_bg_color {blue};
@define-color accent_fg_color {base};
@define-color accent_color {blue};

window, .background {{
  background-color: {base};
  color: {text};
}}

headerbar {{
  background-color: {mantle};
  color: {text};
  box-shadow: inset 0 -1px {surface0};
}}

/* 原文输入框 / 译文框：明确画出边界 */
.st-card {{
  background-color: {surface0};
  border: 1px solid {surface1};
  border-radius: 12px;
}}
.st-card:focus-within {{
  border-color: {blue};
  box-shadow: 0 0 0 1px alpha({blue}, 0.35);
}}
.st-card textview,
.st-card textview text {{
  background-color: transparent;
  color: {text};
}}
.st-card textview text selection {{
  background-color: alpha({blue}, 0.35);
  color: {text};
}}
.st-card undershoot.top,
.st-card undershoot.bottom {{
  background: none;
}}

/* 小节标题：原文 / 译文 */
.st-section {{
  color: {subtext0};
  font-size: 0.85em;
  font-weight: 700;
  letter-spacing: 0.03em;
}}
.st-count {{
  color: {overlay1};
  font-size: 0.82em;
}}
.st-status {{
  color: {overlay1};
  font-size: 0.82em;
}}
.st-bottom {{
  background-color: {mantle};
  box-shadow: inset 0 1px {surface0};
}}
"#,
        base = f.base,
        mantle = f.mantle,
        crust = f.crust,
        surface0 = f.surface0,
        surface1 = f.surface1,
        text = f.text,
        subtext0 = f.subtext0,
        overlay1 = f.overlay1,
        blue = f.blue,
        red = f.red,
        green = f.green,
        yellow = f.yellow,
    )
}

thread_local! {
    static PROVIDER: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
    /// 「跟随系统」的监听只连一次，否则每改一次设置就多挂一个回调
    static WATCHING_SYSTEM: Cell<bool> = const { Cell::new(false) };
}

fn provider() -> gtk::CssProvider {
    PROVIDER.with(|p| {
        let mut slot = p.borrow_mut();
        if let Some(existing) = slot.as_ref() {
            return existing.clone();
        }
        let prov = gtk::CssProvider::new();
        if let Some(display) = gtk::gdk::Display::default() {
            // 用 USER 优先级，确保盖得住 libadwaita 自带的配色
            gtk::style_context_add_provider_for_display(
                &display,
                &prov,
                gtk::STYLE_PROVIDER_PRIORITY_USER,
            );
        }
        *slot = Some(prov.clone());
        prov
    })
}

fn load(f: &Flavor) {
    provider().load_from_string(&css(f));
}

/// 按配置里的选项套用主题。`setting` 是 CHOICES 里的 id。
///
/// 选「跟随系统」时会盯着 libadwaita 的深浅色状态，系统切换时自动跟着换。
pub fn apply(setting: &str) {
    let mgr = adw::StyleManager::default();

    match flavor_by_id(setting) {
        Some(f) => {
            mgr.set_color_scheme(if f.dark {
                adw::ColorScheme::ForceDark
            } else {
                adw::ColorScheme::ForceLight
            });
            load(f);
        }
        None => {
            mgr.set_color_scheme(adw::ColorScheme::Default);
            load(if mgr.is_dark() { &MOCHA } else { &LATTE });
            WATCHING_SYSTEM.with(|w| {
                if !w.get() {
                    w.set(true);
                    mgr.connect_dark_notify(|m| {
                        // 只有仍处于「跟随系统」时才跟着换
                        if crate::config::Config::load().theme == "system" {
                            load(if m.is_dark() { &MOCHA } else { &LATTE });
                        }
                    });
                }
            });
        }
    }
}
