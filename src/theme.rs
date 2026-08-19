//! GTK 界面的配色落地。
//!
//! 色值本身住在 `seltrans_core::palette`（GTK 版和将来的 web 版共用一套，抄两份必然漂移），
//! 这里只负责把它翻译成 GTK / libadwaita 认识的 CSS。
//!
//! 同时输出 libadwaita 1.6+ 的 CSS 变量和旧版 `@define-color`：前者是现在推荐的写法，
//! 后者留着兼容，两边都发不会互相干扰。

use adw::prelude::*;
use std::cell::{Cell, RefCell};

pub use seltrans_core::palette::{CHOICES, Flavor, LATTE, MOCHA, flavor_by_id};

fn css(f: &Flavor, font_family: Option<&str>) -> String {
    // 输入 / 译文两个框：surface0 底 + surface1 描边，圆角卡片
    let font_rule = match font_family {
        Some(list) => {
            format!("\nwindow, popover, tooltip, dialog {{\n  font-family: {list};\n}}\n")
        }
        None => String::new(),
    };

    format!(
        r#"{font_rule}
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

/* 注意别写成 `.background` —— GtkPopover 自己也带这个样式类，
   一刷就会在弹层外面露出一圈窗口底色的方块 */
window {{
  background-color: {base};
  color: {text};
}}
popover.background {{
  background-color: transparent;
  background-image: none;
  box-shadow: none;
  border: none;
  padding: 0;
}}

headerbar {{
  background-color: {mantle};
  color: {text};
  box-shadow: none;
  border: none;
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
  font-size: 0.88em;
}}

/* 底部的供应商 / 模型选择器：常用控件，得看得清也点得着，
   所以用跟卡片一样的 surface0 底 + surface1 描边，但比顶栏那个主控件收敛一点。
   注意 GtkDropDown 内部还套着一个 button，flat 类穿不透，CSS 得直接选中它。 */
.st-chip,
.st-chip > button {{
  min-height: 0;
}}
.st-chip > button {{
  background-color: {surface0};
  background-image: none;
  box-shadow: none;
  border: 1px solid {surface1};
  border-radius: 9px;
  color: {text};
  padding: 5px 12px;
}}
.st-chip > button:hover {{
  background-color: {surface1};
}}
.st-chip > button > * {{
  color: {text};
}}

/* ---- 下拉弹层 ----
   GtkDropDown 的弹层里除了 popover 本体，还套着 listview / scrolledwindow，
   它们各自带默认底色，不一起改就会看到一块跟主题对不上的方块。 */
/* 弹层跟卡片保持同一套视觉语言：一样的 surface0 底、surface1 描边、一样的圆角。
   之前用 mantle 是往暗走，而卡片是往亮走，两者方向相反，看着就不像一个软件。 */
popover > contents {{
  background-color: {surface0};
  color: {text};
  border: 1px solid {surface1};
  border-radius: 12px;
  padding: 5px;
  box-shadow: 0 8px 24px alpha(#000000, 0.22);
}}
popover > arrow {{
  background-color: {surface0};
  border: 1px solid {surface1};
}}
popover listview,
popover list,
popover scrolledwindow,
popover .view,
popover viewport,
dropdown > popover > contents > * {{
  background-color: transparent;
  background-image: none;
  color: {text};
}}
popover listview > row,
popover list > row {{
  border-radius: 8px;
  padding: 5px 9px;
  min-height: 0;
  color: {text};
}}
popover listview > row:hover,
popover list > row:hover {{
  background-color: {surface1};
}}
popover listview > row:selected,
popover list > row:selected {{
  background-color: alpha({blue}, 0.16);
  color: {blue};
  font-weight: 600;
}}
popover listview > row:selected:hover,
popover list > row:selected:hover {{
  background-color: alpha({blue}, 0.24);
}}
/* 下拉里的搜索框 */
popover entry,
popover .search {{
  background-color: {mantle};
  color: {text};
  border: 1px solid {surface1};
  border-radius: 8px;
}}

/* 弹层出现动画。GTK4 默认不给 popover 任何动画（GTK3 有，GTK4 去掉了），
   所以这里自己写一段：轻微下移 + 缩放 + 淡入。 */
@keyframes st-popover-in-a {{
  from {{ opacity: 0; transform: translateY(-8px) scale(0.96); }}
  to {{ opacity: 1; transform: translateY(0) scale(1); }}
}}
/* 和上面完全一样，只是名字不同。GTK 的 CSS 动画只在样式变化时才重新播放，
   而 popover 的节点是复用的 —— 第二次打开样式没变，动画就不播了。
   所以每次 map 时在 a / b 两个类之间来回切，让 animation-name 真的变化。 */
@keyframes st-popover-in-b {{
  from {{ opacity: 0; transform: translateY(-8px) scale(0.96); }}
  to {{ opacity: 1; transform: translateY(0) scale(1); }}
}}
popover.background > contents,
popover.st-pop-a > contents {{
  animation: st-popover-in-a 200ms cubic-bezier(0.2, 0.9, 0.3, 1);
}}
popover.st-pop-b > contents {{
  animation: st-popover-in-b 200ms cubic-bezier(0.2, 0.9, 0.3, 1);
}}
/* 弹层里的条目依次淡入，鼠标扫过时更有层次 */
popover listview > row {{
  animation: st-row-in 220ms cubic-bezier(0.16, 1, 0.3, 1);
}}
@keyframes st-row-in {{
  from {{ opacity: 0; }}
  to {{ opacity: 1; }}
}}

/* 顶部那个提示词下拉：主控件，给它实体感 */
dropdown:not(.st-chip) > button {{
  background-color: {surface0};
  color: {text};
  border: 1px solid {surface1};
  border-radius: 10px;
}}
dropdown:not(.st-chip) > button:hover {{
  background-color: {surface1};
}}

/* ---- 动画 ----
   统一给会变色 / 变边框的控件加过渡，鼠标划过和聚焦都是渐变而不是硬切。 */
button,
dropdown > button,
row,
listview > row,
list > row,
entry,
.st-card {{
  transition:
    background-color 160ms cubic-bezier(0.4, 0, 0.2, 1),
    border-color 160ms cubic-bezier(0.4, 0, 0.2, 1),
    box-shadow 200ms cubic-bezier(0.4, 0, 0.2, 1),
    color 160ms cubic-bezier(0.4, 0, 0.2, 1),
    opacity 160ms ease;
}}
button:active,
dropdown > button:active {{
  transition-duration: 60ms;
}}
/* 卡片聚焦时描边渐变到强调色 */
.st-card:focus-within {{
  transition:
    border-color 180ms cubic-bezier(0.4, 0, 0.2, 1),
    box-shadow 220ms cubic-bezier(0.4, 0, 0.2, 1);
}}
"#,
        font_rule = font_rule,
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

/// 给控件树里所有 popover 装上"每次打开都重播动画"的钩子。
///
/// 窗口建好后调一次即可；popover 是懒创建的，所以窗口 map 之后再补一次。
/// 窗口 / 对话框建好后统一挂钩：弹层动画 + 界面文字按脚本分配字体。
pub fn hook_widgets(root: &impl IsA<gtk::Widget>) {
    hook_popover_animations(root);
    crate::fonts::hook_ui_script_fonts(root);
}

pub fn hook_popover_animations(root: &impl IsA<gtk::Widget>) {
    fn walk(w: &gtk::Widget) {
        if let Some(p) = w.downcast_ref::<gtk::Popover>()
            && !p.has_css_class("st-pop-a")
            && !p.has_css_class("st-pop-b")
        {
            p.add_css_class("st-pop-a");
            p.connect_map(|p| {
                if p.has_css_class("st-pop-a") {
                    p.remove_css_class("st-pop-a");
                    p.add_css_class("st-pop-b");
                } else {
                    p.remove_css_class("st-pop-b");
                    p.add_css_class("st-pop-a");
                }
            });
        }
        let mut c = w.first_child();
        while let Some(child) = c {
            walk(&child);
            c = child.next_sibling();
        }
    }
    walk(root.as_ref());
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
        // CSS 写错了默认是静默忽略的，接上来免得改坏了自己不知道
        prov.connect_parsing_error(|_, section, error| {
            crate::logging::warn(&format!("主题 CSS 解析出错 @ {section}: {error}"));
        });
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

fn load(f: &Flavor, font_family: Option<&str>) {
    provider().load_from_string(&css(f, font_family));
}

/// 按配置套用主题与字体。
///
/// 选「跟随系统」时会盯着 libadwaita 的深浅色状态，系统切换时自动跟着换。
pub fn apply(cfg: &crate::config::Config) {
    let mgr = adw::StyleManager::default();
    crate::fonts::set_current_cjk(&cfg.font_cjk);
    let font = crate::fonts::css_family(cfg);
    let font = font.as_deref();

    match flavor_by_id(&cfg.theme) {
        Some(f) => {
            mgr.set_color_scheme(if f.dark {
                adw::ColorScheme::ForceDark
            } else {
                adw::ColorScheme::ForceLight
            });
            load(f, font);
        }
        None => {
            mgr.set_color_scheme(adw::ColorScheme::Default);
            load(if mgr.is_dark() { &MOCHA } else { &LATTE }, font);
            WATCHING_SYSTEM.with(|w| {
                if !w.get() {
                    w.set(true);
                    mgr.connect_dark_notify(|m| {
                        // 只有仍处于「跟随系统」时才跟着换
                        let cfg = crate::config::Config::load();
                        if cfg.theme == "system" {
                            let f = crate::fonts::css_family(&cfg);
                            load(if m.is_dark() { &MOCHA } else { &LATTE }, f.as_deref());
                        }
                    });
                }
            });
        }
    }
}
