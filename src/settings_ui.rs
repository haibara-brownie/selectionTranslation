//! 图形配置界面：通用 / 供应商 / 提示词 / 关于 四页。

use adw::prelude::*;
use gtk::glib;
use std::cell::RefCell;
use std::rc::Rc;

use crate::config::{Config, Prompt, Provider, new_id};
use crate::presets::{PROVIDER_PRESETS, preset_by_id};
use crate::{
    APP_ID_SETTINGS, REPO_URL, autostart, config, fonts, llm, logging, presets, selection, theme,
    tray,
};

const SEL_MODES: [(&str, &str); 3] = [
    ("auto", "自动（主选区优先，取不到再模拟 Ctrl+C）"),
    ("primary", "仅主选区（绝不碰剪贴板）"),
    ("clipboard", "仅模拟 Ctrl+C（兼容性最好）"),
];

struct St {
    cfg: RefCell<Config>,
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    providers_page: adw::PreferencesPage,
    prompts_page: adw::PreferencesPage,
    provider_groups: RefCell<Vec<adw::PreferencesGroup>>,
    prompt_groups: RefCell<Vec<adw::PreferencesGroup>>,
    prompt_combo: adw::ComboRow,
    quiet: std::cell::Cell<bool>,
}

impl St {
    fn save(&self) {
        if let Err(e) = self.cfg.borrow().save() {
            self.toast(&format!("保存失败：{e}"));
        }
    }

    fn toast(&self, msg: &str) {
        self.toasts.add_toast(adw::Toast::new(msg));
    }

    fn sync_prompt_combo(&self) {
        self.quiet.set(true);
        let cfg = self.cfg.borrow();
        let labels: Vec<String> = cfg.prompts.iter().map(|p| p.label()).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        self.prompt_combo
            .set_model(Some(&gtk::StringList::new(&refs)));
        if let Some(i) = cfg.prompts.iter().position(|p| p.id == cfg.active_prompt) {
            self.prompt_combo.set_selected(i as u32);
        }
        drop(cfg);
        self.quiet.set(false);
    }
}

// ---------------------------------------------------------------- 通用页

fn build_general(st: &Rc<St>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("通用")
        .icon_name("preferences-system-symbolic")
        .build();

    let g1 = adw::PreferencesGroup::builder().title("翻译").build();

    // 目标语言：下拉选常用语种，最后一项「自定义」才露出输入框
    let mut labels: Vec<&str> = presets::TARGET_LANGS.iter().map(|(_, l)| *l).collect();
    labels.push("自定义…");
    let custom_index = (labels.len() - 1) as u32;

    let lang_row = adw::ComboRow::builder()
        .title("目标语言")
        .subtitle("译文要输出成哪种语言")
        .model(&gtk::StringList::new(&labels))
        .build();

    let custom_row = adw::EntryRow::builder()
        .title("自定义语言（直接写模型看得懂的语言名）")
        .build();

    let current = st.cfg.borrow().target_lang.clone();
    match presets::TARGET_LANGS
        .iter()
        .position(|(v, _)| *v == current)
    {
        Some(i) => {
            lang_row.set_selected(i as u32);
            custom_row.set_visible(false);
        }
        None => {
            lang_row.set_selected(custom_index);
            custom_row.set_text(&current);
            custom_row.set_visible(true);
        }
    }

    lang_row.connect_selected_notify({
        let st = st.clone();
        let custom_row = custom_row.clone();
        move |c| {
            let i = c.selected();
            if i == custom_index {
                custom_row.set_visible(true);
                let v = custom_row.text().to_string();
                if !v.trim().is_empty() {
                    st.cfg.borrow_mut().target_lang = v;
                    st.save();
                }
            } else if let Some((value, _)) = presets::TARGET_LANGS.get(i as usize) {
                custom_row.set_visible(false);
                st.cfg.borrow_mut().target_lang = value.to_string();
                st.save();
            }
        }
    });

    custom_row.connect_changed({
        let st = st.clone();
        move |e| {
            let v = e.text().to_string();
            if !v.trim().is_empty() {
                st.cfg.borrow_mut().target_lang = v;
                st.save();
            }
        }
    });

    g1.add(&lang_row);
    g1.add(&custom_row);

    st.prompt_combo.set_title("默认提示词");
    st.prompt_combo.connect_selected_notify({
        let st = st.clone();
        move |c| {
            if st.quiet.get() {
                return;
            }
            let i = c.selected() as usize;
            let id = st.cfg.borrow().prompts.get(i).map(|p| p.id.clone());
            if let Some(id) = id {
                st.cfg.borrow_mut().active_prompt = id;
                st.save();
            }
        }
    });
    g1.add(&st.prompt_combo);
    page.add(&g1);

    let g2 = adw::PreferencesGroup::builder()
        .title("取词")
        .description(
            "Wayland 没有统一的划词接口。主选区（选中即生效）零侵入但少数应用不支持；\
             模拟 Ctrl+C 兼容性最好，读完会自动还原剪贴板原内容。",
        )
        .build();

    let modes: Vec<&str> = SEL_MODES.iter().map(|(_, l)| *l).collect();
    let mode_row = adw::ComboRow::builder()
        .title("取词方式")
        .model(&gtk::StringList::new(&modes))
        .build();
    if let Some(i) = SEL_MODES
        .iter()
        .position(|(k, _)| *k == st.cfg.borrow().selection_mode)
    {
        mode_row.set_selected(i as u32);
    }
    mode_row.connect_selected_notify({
        let st = st.clone();
        move |c| {
            if let Some((k, _)) = SEL_MODES.get(c.selected() as usize) {
                st.cfg.borrow_mut().selection_mode = k.to_string();
                st.save();
            }
        }
    });
    g2.add(&mode_row);
    page.add(&g2);

    // ---- 外观 ----
    let g_theme = adw::PreferencesGroup::builder()
        .title("外观")
        .description(
            "配色取自 Catppuccin 官方调色板。「跟随系统」会跟着桌面的深浅色设置\
             在 Latte 与 Mocha 之间自动切换。",
        )
        .build();

    let theme_labels: Vec<&str> = theme::CHOICES.iter().map(|(_, l)| *l).collect();
    let theme_row = adw::ComboRow::builder()
        .title("配色")
        .model(&gtk::StringList::new(&theme_labels))
        .build();
    if let Some(i) = theme::CHOICES
        .iter()
        .position(|(k, _)| *k == st.cfg.borrow().theme)
    {
        theme_row.set_selected(i as u32);
    }
    theme_row.connect_selected_notify({
        let st = st.clone();
        move |c| {
            if let Some((k, _)) = theme::CHOICES.get(c.selected() as usize) {
                st.cfg.borrow_mut().theme = k.to_string();
                st.save();
                // 立刻生效，不用重开窗口
                theme::apply(&st.cfg.borrow());
            }
        }
    });
    g_theme.add(&theme_row);
    page.add(&g_theme);

    // ---- 字体 ----
    let g_font = adw::PreferencesGroup::builder()
        .title("字体")
        .description(
            "原文和译文区按字符脚本直接分配：汉字一定用中文字体，其余按 \
             拉丁 → 中文 → 后备 的顺序回退。所以拉丁字体自带汉字也不影响。\
             三档都选「系统默认」就完全不干预。",
        )
        .build();

    // 拉丁字体那档要提示"这个字体自带汉字"—— 界面文字仍按回退顺序走，
    // 只有原文 / 译文区是按脚本强制分配的
    fn latin_subtitle(family: &str) -> String {
        if fonts::covers_cjk(family) {
            format!("英文、数字、代码 · 注意：{family} 自带汉字，界面文字的汉字会用它")
        } else {
            "英文、数字、代码".to_string()
        }
    }

    let families = fonts::families();
    for (title, subtitle, is_latin, getter, setter) in [
        (
            "拉丁字体",
            latin_subtitle(&st.cfg.borrow().font_latin),
            true,
            Box::new(|c: &Config| c.font_latin.clone()) as Box<dyn Fn(&Config) -> String>,
            Box::new(|c: &mut Config, v: String| c.font_latin = v)
                as Box<dyn Fn(&mut Config, String)>,
        ),
        (
            "中文字体",
            "汉字、中文标点、假名、谚文".to_string(),
            false,
            Box::new(|c: &Config| c.font_cjk.clone()),
            Box::new(|c: &mut Config, v: String| c.font_cjk = v),
        ),
        (
            "后备字体",
            "前两档都没有的字形，比如 emoji、西里尔字母".to_string(),
            false,
            Box::new(|c: &Config| c.font_fallback.clone()),
            Box::new(|c: &mut Config, v: String| c.font_fallback = v),
        ),
    ] {
        let row = font_combo(&families, &getter(&st.cfg.borrow()), title, &subtitle);
        row.connect_selected_notify({
            let st = st.clone();
            let families = families.clone();
            move |c| {
                let i = c.selected() as usize;
                // 第 0 项是「系统默认」，对应空字符串
                let value = if i == 0 {
                    String::new()
                } else {
                    families.get(i - 1).cloned().unwrap_or_default()
                };
                if is_latin {
                    c.set_subtitle(&latin_subtitle(&value));
                }
                setter(&mut st.cfg.borrow_mut(), value);
                st.save();
                theme::apply(&st.cfg.borrow());
                // 已经建好的 Label 要重算一遍 Pango 属性
                crate::fonts::hook_ui_script_fonts(&st.window);
            }
        });
        g_font.add(&row);
    }
    page.add(&g_font);

    let g3 = adw::PreferencesGroup::builder().title("弹窗").build();
    let w = adw::SpinRow::with_range(320.0, 1600.0, 20.0);
    w.set_title("宽度（像素）");
    w.set_value(st.cfg.borrow().popup_width as f64);
    w.connect_value_notify({
        let st = st.clone();
        move |s| {
            st.cfg.borrow_mut().popup_width = s.value() as i32;
            st.save();
        }
    });
    let h = adw::SpinRow::with_range(240.0, 1600.0, 20.0);
    h.set_title("高度（像素）");
    h.set_value(st.cfg.borrow().popup_height as f64);
    h.connect_value_notify({
        let st = st.clone();
        move |s| {
            st.cfg.borrow_mut().popup_height = s.value() as i32;
            st.save();
        }
    });
    g3.add(&w);
    g3.add(&h);
    page.add(&g3);

    // ---- 后台常驻 / 托盘 ----
    let g_tray = adw::PreferencesGroup::builder()
        .title("后台常驻")
        .description(
            "常驻后会在系统托盘显示图标，一眼就能看出程序在不在跑；\
             左键点图标直接翻译选中的文本，右键有完整菜单。\
             另外快捷键触发时是复用这个进程，省掉冷启动，弹窗几乎瞬间出来。",
        )
        .build();

    let auto_row = adw::SwitchRow::builder()
        .title("开机自启动")
        .subtitle("登录后自动常驻托盘")
        .active(autostart::is_enabled())
        .build();
    auto_row.connect_active_notify({
        let st = st.clone();
        move |r| {
            let on = r.is_active();
            match autostart::set_enabled(on) {
                Ok(()) => st.toast(if on {
                    "已开启开机自启动"
                } else {
                    "已关闭开机自启动"
                }),
                Err(e) => st.toast(&format!("设置失败：{e}")),
            }
        }
    });
    g_tray.add(&auto_row);

    let status_row = adw::ActionRow::builder().title("托盘状态").build();
    let start_btn = gtk::Button::builder()
        .label("启动")
        .valign(gtk::Align::Center)
        .build();
    let refresh_btn = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("刷新状态")
        .valign(gtk::Align::Center)
        .build();
    refresh_btn.add_css_class("flat");
    status_row.add_suffix(&refresh_btn);
    status_row.add_suffix(&start_btn);

    let sync_status = {
        let status_row = status_row.clone();
        let start_btn = start_btn.clone();
        move || {
            if tray::is_running() {
                status_row.set_subtitle("运行中 —— 退出请点托盘图标右键菜单里的「退出」");
                start_btn.set_sensitive(false);
                start_btn.set_label("已在运行");
            } else {
                status_row.set_subtitle("未运行");
                start_btn.set_sensitive(true);
                start_btn.set_label("启动");
            }
        }
    };
    sync_status();

    start_btn.connect_clicked({
        let st = st.clone();
        let sync_status = sync_status.clone();
        move |_| {
            if let Ok(exe) = std::env::current_exe() {
                match std::process::Command::new(exe).arg("tray").spawn() {
                    Ok(_) => st.toast("托盘已启动"),
                    Err(e) => st.toast(&format!("启动失败：{e}")),
                }
            }
            // 给它一点时间注册到会话总线上再刷新状态
            let sync_status = sync_status.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(1200), move || {
                sync_status();
            });
        }
    });
    refresh_btn.connect_clicked({
        let sync_status = sync_status.clone();
        move |_| sync_status()
    });

    g_tray.add(&status_row);
    page.add(&g_tray);

    let g4 = adw::PreferencesGroup::builder()
        .title("快捷键")
        .description("由 niri 提供，改键请编辑 ~/.config/niri/selectiontranslation.kdl")
        .build();
    for (k, v) in [
        ("Mod+Shift+T", "划词翻译"),
        ("Mod+Alt+T", "打开本配置界面"),
        ("Esc", "关闭翻译弹窗"),
        ("F5", "在弹窗里重新翻译"),
        ("Ctrl+Shift+C", "在弹窗里复制译文"),
    ] {
        let r = adw::ActionRow::builder().title(v).subtitle(k).build();
        g4.add(&r);
    }
    page.add(&g4);

    page
}

/// 字体下拉。系统上装几百个字体是常事，所以开搜索 —— AdwComboRow 要能搜，
/// 必须给它一个 expression 告诉它拿哪个属性去匹配。
fn font_combo(families: &[String], current: &str, title: &str, subtitle: &str) -> adw::ComboRow {
    let mut items: Vec<&str> = vec!["系统默认"];
    items.extend(families.iter().map(String::as_str));

    let expr = gtk::PropertyExpression::new(
        gtk::StringObject::static_type(),
        None::<gtk::Expression>,
        "string",
    );
    let row = adw::ComboRow::builder()
        .title(title)
        .subtitle(subtitle)
        .model(&gtk::StringList::new(&items))
        .expression(&expr)
        .enable_search(true)
        // 默认是前缀匹配，搜 "maple" 就找不到 "JetBrains Maple"。改成子串匹配。
        .search_match_mode(gtk::StringFilterMatchMode::Substring)
        .build();

    let idx = if current.trim().is_empty() {
        0
    } else {
        families
            .iter()
            .position(|f| f == current)
            .map(|i| i + 1)
            .unwrap_or(0)
    };
    row.set_selected(idx as u32);
    row
}

// ---------------------------------------------------------------- 供应商页

fn rebuild_providers(st: &Rc<St>) {
    for g in st.provider_groups.borrow_mut().drain(..) {
        st.providers_page.remove(&g);
    }

    let add_btn = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("添加供应商")
        .valign(gtk::Align::Center)
        .build();
    add_btn.add_css_class("flat");
    add_btn.connect_clicked({
        let st = st.clone();
        move |_| edit_provider(&st, None)
    });

    let group = adw::PreferencesGroup::builder()
        .title("模型供应商")
        .description(
            "选中左侧圆点即为当前使用的供应商。模型列表不写死在程序里，\
             点「编辑 → 拉取模型列表」实时从服务端获取。",
        )
        .header_suffix(&add_btn)
        .build();

    let providers = st.cfg.borrow().providers.clone();
    let active = st.cfg.borrow().active_provider.clone();

    if providers.is_empty() {
        let empty = adw::ActionRow::builder()
            .title("还没有配置供应商")
            .subtitle("点右上角的 + 添加一个，预设会自动填好 base_url，你只需要填 API key")
            .build();
        group.add(&empty);
    }

    let mut first_radio: Option<gtk::CheckButton> = None;
    for (idx, p) in providers.iter().enumerate() {
        let radio = gtk::CheckButton::builder()
            .valign(gtk::Align::Center)
            .tooltip_text("设为当前使用")
            .build();
        match &first_radio {
            None => first_radio = Some(radio.clone()),
            Some(f) => radio.set_group(Some(f)),
        }
        radio.set_active(p.id == active || (active.is_empty() && idx == 0));
        radio.connect_toggled({
            let st = st.clone();
            let id = p.id.clone();
            move |r| {
                if r.is_active() && st.cfg.borrow().active_provider != id {
                    st.cfg.borrow_mut().active_provider = id.clone();
                    st.save();
                }
            }
        });

        let model = if p.model.is_empty() {
            "未选模型".to_string()
        } else {
            p.model.clone()
        };
        let key_state = if p.api_key.trim().is_empty() {
            "未填 key"
        } else {
            "key 已填"
        };

        let row = adw::ActionRow::builder()
            .title(&p.name)
            .subtitle(format!("{model} · {key_state} · {}", p.base_url))
            .activatable(true)
            .build();
        row.add_prefix(&radio);

        let edit = gtk::Button::builder()
            .icon_name("document-edit-symbolic")
            .tooltip_text("编辑")
            .valign(gtk::Align::Center)
            .build();
        edit.add_css_class("flat");
        edit.connect_clicked({
            let st = st.clone();
            let id = p.id.clone();
            move |_| edit_provider(&st, Some(id.clone()))
        });

        let del = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text("删除")
            .valign(gtk::Align::Center)
            .build();
        del.add_css_class("flat");
        del.connect_clicked({
            let st = st.clone();
            let id = p.id.clone();
            let name = p.name.clone();
            move |_| {
                let dlg = adw::AlertDialog::new(
                    Some("删除供应商？"),
                    Some(&format!("「{name}」的配置和 API key 会一并删除。")),
                );
                dlg.add_response("cancel", "取消");
                dlg.add_response("delete", "删除");
                dlg.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
                dlg.set_default_response(Some("cancel"));
                dlg.connect_response(None, {
                    let st = st.clone();
                    let id = id.clone();
                    move |_, resp| {
                        if resp == "delete" {
                            {
                                let mut cfg = st.cfg.borrow_mut();
                                cfg.providers.retain(|x| x.id != id);
                                if cfg.active_provider == id {
                                    cfg.active_provider = cfg
                                        .providers
                                        .first()
                                        .map(|x| x.id.clone())
                                        .unwrap_or_default();
                                }
                            }
                            st.save();
                            rebuild_providers(&st);
                        }
                    }
                });
                dlg.present(Some(&st.window));
            }
        });

        row.add_suffix(&edit);
        row.add_suffix(&del);
        row.connect_activated({
            let st = st.clone();
            let id = p.id.clone();
            move |_| edit_provider(&st, Some(id.clone()))
        });
        group.add(&row);
    }

    st.providers_page.add(&group);
    st.provider_groups.borrow_mut().push(group);
    crate::theme::hook_widgets(&st.providers_page);
}

/// 供应商编辑对话框。`id` 为 None 表示新建。
fn edit_provider(st: &Rc<St>, id: Option<String>) {
    let existing = id.as_ref().and_then(|i| {
        st.cfg
            .borrow()
            .providers
            .iter()
            .find(|p| p.id == *i)
            .cloned()
    });

    let mut p = existing.clone().unwrap_or_else(|| Provider {
        id: new_id(),
        name: String::new(),
        preset: "custom".into(),
        kind: "openai".into(),
        base_url: String::new(),
        api_key: String::new(),
        model: String::new(),
        models: Vec::new(),
        extra_body: String::new(),
    });
    if p.name.is_empty() {
        // 新建时默认落在第一个预设上
        let d = &PROVIDER_PRESETS[0];
        p.preset = d.id.into();
        p.kind = d.kind.into();
        p.name = d.name.into();
        p.base_url = d.base_url.into();
    }
    let draft = Rc::new(RefCell::new(p));

    let dialog = adw::Dialog::builder()
        .title(if existing.is_some() {
            "编辑供应商"
        } else {
            "添加供应商"
        })
        .content_width(620)
        .content_height(640)
        .build();

    let page = adw::PreferencesPage::new();
    let g = adw::PreferencesGroup::new();

    // 预设
    let names: Vec<&str> = PROVIDER_PRESETS.iter().map(|x| x.name).collect();
    let preset_row = adw::ComboRow::builder()
        .title("预设")
        .model(&gtk::StringList::new(&names))
        .build();
    if let Some(i) = PROVIDER_PRESETS
        .iter()
        .position(|x| x.id == draft.borrow().preset)
    {
        preset_row.set_selected(i as u32);
    }

    let name_row = adw::EntryRow::builder().title("名称").build();
    name_row.set_text(&draft.borrow().name);

    let url_row = adw::EntryRow::builder().title("base_url").build();
    url_row.set_text(&draft.borrow().base_url);

    let key_row = adw::PasswordEntryRow::builder().title("API Key").build();
    key_row.set_text(&draft.borrow().api_key);

    let model_row = adw::EntryRow::builder().title("模型").build();
    model_row.set_text(&draft.borrow().model);
    let fetch_btn = gtk::Button::builder()
        .label("拉取列表")
        .valign(gtk::Align::Center)
        .build();
    fetch_btn.add_css_class("flat");
    model_row.add_suffix(&fetch_btn);

    let extra_row = adw::EntryRow::builder()
        .title("附加请求体（JSON，可留空）")
        .build();
    extra_row.set_text(&draft.borrow().extra_body);

    g.add(&preset_row);
    g.add(&name_row);
    g.add(&url_row);
    g.add(&key_row);
    g.add(&model_row);
    g.add(&extra_row);
    page.add(&g);

    // 预设说明 + 申请 key 链接
    let hint_group = adw::PreferencesGroup::new();
    let hint = gtk::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .margin_bottom(6)
        .build();
    hint.add_css_class("dim-label");
    hint.add_css_class("caption");
    let key_link = gtk::LinkButton::builder()
        .label("打开申请 API Key 的页面")
        .build();
    key_link.set_halign(gtk::Align::Start);
    let hint_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .build();
    hint_box.append(&hint);
    hint_box.append(&key_link);
    hint_group.add(&hint_box);
    page.add(&hint_group);

    // 测试连接
    let test_group = adw::PreferencesGroup::new();
    let test_btn = gtk::Button::builder().label("测试连接").build();
    let test_label = gtk::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .selectable(true)
        .build();
    test_label.add_css_class("caption");
    let test_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .build();
    test_box.append(&test_btn);
    test_box.append(&test_label);
    test_group.add(&test_box);
    page.add(&test_group);

    // 把界面上的输入同步回 draft
    let collect = {
        let draft = draft.clone();
        let name_row = name_row.clone();
        let url_row = url_row.clone();
        let key_row = key_row.clone();
        let model_row = model_row.clone();
        let extra_row = extra_row.clone();
        move || {
            let mut d = draft.borrow_mut();
            d.name = name_row.text().to_string();
            d.base_url = url_row.text().to_string();
            d.api_key = key_row.text().to_string();
            d.model = model_row.text().trim().to_string();
            d.extra_body = extra_row.text().to_string();
            d.clone()
        }
    };

    let apply_hint = {
        let hint = hint.clone();
        let key_link = key_link.clone();
        move |pre: &presets::ProviderPreset| {
            hint.set_text(pre.hint);
            if pre.keys_url.is_empty() {
                key_link.set_visible(false);
            } else {
                key_link.set_visible(true);
                key_link.set_uri(pre.keys_url);
            }
        }
    };
    if let Some(pre) = preset_by_id(&draft.borrow().preset) {
        apply_hint(pre);
    }

    preset_row.connect_selected_notify({
        let draft = draft.clone();
        let name_row = name_row.clone();
        let url_row = url_row.clone();
        let apply_hint = apply_hint.clone();
        move |c| {
            let Some(pre) = PROVIDER_PRESETS.get(c.selected() as usize) else {
                return;
            };
            {
                let mut d = draft.borrow_mut();
                d.preset = pre.id.into();
                d.kind = pre.kind.into();
                d.models.clear();
            }
            // 换预设时自动填好 base_url；名称若还是上一个预设的名字也一并换掉
            url_row.set_text(pre.base_url);
            let cur = name_row.text().to_string();
            if cur.trim().is_empty() || PROVIDER_PRESETS.iter().any(|x| x.name == cur) {
                name_row.set_text(pre.name);
            }
            apply_hint(pre);
        }
    });

    fetch_btn.connect_clicked({
        let st = st.clone();
        let draft = draft.clone();
        let collect = collect.clone();
        let model_row = model_row.clone();
        let fetch_btn = fetch_btn.clone();
        let dialog = dialog.clone();
        move |_| {
            let p = collect();
            fetch_btn.set_sensitive(false);
            fetch_btn.set_label("拉取中…");

            let (tx, rx) = async_channel::bounded(1);
            llm::runtime().spawn(async move {
                let _ = tx.send(llm::list_models(p).await).await;
            });

            let st = st.clone();
            let draft = draft.clone();
            let model_row = model_row.clone();
            let fetch_btn = fetch_btn.clone();
            let dialog = dialog.clone();
            glib::spawn_future_local(async move {
                let res = rx.recv().await;
                fetch_btn.set_sensitive(true);
                fetch_btn.set_label("拉取列表");
                match res {
                    Ok(Ok(models)) => {
                        draft.borrow_mut().models = models.clone();
                        pick_model(&st, &dialog, models, {
                            let model_row = model_row.clone();
                            move |m| model_row.set_text(&m)
                        });
                    }
                    Ok(Err(e)) => st.toast(&format!("拉取失败：{e}")),
                    Err(_) => {}
                }
            });
        }
    });

    test_btn.connect_clicked({
        let collect = collect.clone();
        let test_btn = test_btn.clone();
        let test_label = test_label.clone();
        move |_| {
            let p = collect();
            test_btn.set_sensitive(false);
            test_label.set_text("测试中…");
            let (tx, rx) = async_channel::bounded(1);
            llm::runtime().spawn(async move {
                let _ = tx.send(llm::test_connection(p).await).await;
            });
            let test_btn = test_btn.clone();
            let test_label = test_label.clone();
            glib::spawn_future_local(async move {
                if let Ok(res) = rx.recv().await {
                    test_btn.set_sensitive(true);
                    match res {
                        Ok(msg) => test_label.set_text(&format!("✅ {msg}")),
                        Err(e) => test_label.set_text(&format!("❌ {e}")),
                    }
                }
            });
        }
    });

    // 头部：取消 / 保存
    let header = adw::HeaderBar::new();
    let cancel = gtk::Button::with_label("取消");
    let save = gtk::Button::with_label("保存");
    save.add_css_class("suggested-action");
    header.pack_start(&cancel);
    header.pack_end(&save);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&page));
    dialog.set_child(Some(&toolbar));

    cancel.connect_clicked({
        let dialog = dialog.clone();
        move |_| {
            dialog.close();
        }
    });

    save.connect_clicked({
        let st = st.clone();
        let dialog = dialog.clone();
        let collect = collect.clone();
        move |_| {
            let mut p = collect();
            if p.name.trim().is_empty() {
                st.toast("名称不能为空");
                return;
            }
            if p.base_url.trim().is_empty() {
                st.toast("base_url 不能为空");
                return;
            }
            if !p.extra_body.trim().is_empty()
                && serde_json::from_str::<serde_json::Value>(&p.extra_body)
                    .map(|v| !v.is_object())
                    .unwrap_or(true)
            {
                st.toast("附加请求体必须是合法的 JSON 对象，例如 {\"reasoning_effort\":\"none\"}");
                return;
            }
            p.base_url = p.base_url.trim().to_string();

            {
                let mut cfg = st.cfg.borrow_mut();
                match cfg.providers.iter_mut().find(|x| x.id == p.id) {
                    Some(slot) => *slot = p.clone(),
                    None => cfg.providers.push(p.clone()),
                }
                if cfg.active_provider.is_empty() {
                    cfg.active_provider = p.id.clone();
                }
            }
            st.save();
            rebuild_providers(&st);
            dialog.close();
        }
    });

    crate::theme::hook_widgets(&dialog);
    dialog.present(Some(&st.window));
}

/// 模型太多（聚合平台动辄上百个），给个带搜索的挑选框
fn pick_model(
    st: &Rc<St>,
    parent: &adw::Dialog,
    models: Vec<String>,
    on_pick: impl Fn(String) + 'static,
) {
    let dialog = adw::Dialog::builder()
        .title("选择模型")
        .content_width(520)
        .content_height(560)
        .build();

    let search = gtk::SearchEntry::builder()
        .placeholder_text("搜索模型…")
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(12)
        .margin_end(12)
        .build();

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(12)
        .build();
    list.add_css_class("boxed-list");

    let on_pick = Rc::new(on_pick);
    let fill = {
        let list = list.clone();
        let models = models.clone();
        let dialog = dialog.clone();
        let on_pick = on_pick.clone();
        move |filter: &str| {
            while let Some(c) = list.first_child() {
                list.remove(&c);
            }
            let f = filter.to_lowercase();
            let mut shown = 0;
            for m in models.iter().filter(|m| m.to_lowercase().contains(&f)) {
                let row = adw::ActionRow::builder().title(m).activatable(true).build();
                row.connect_activated({
                    let dialog = dialog.clone();
                    let on_pick = on_pick.clone();
                    let m = m.clone();
                    move |_| {
                        on_pick(m.clone());
                        dialog.close();
                    }
                });
                list.append(&row);
                shown += 1;
                if shown >= 400 {
                    break;
                }
            }
            if shown == 0 {
                list.append(&adw::ActionRow::builder().title("没有匹配的模型").build());
            }
        }
    };
    fill("");
    search.connect_search_changed({
        let fill = fill.clone();
        move |e| fill(&e.text())
    });

    let scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .child(&list)
        .build();
    let vbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    vbox.append(&search);
    vbox.append(&scroller);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&vbox));
    dialog.set_child(Some(&toolbar));

    let _ = st;
    crate::theme::hook_widgets(&dialog);
    dialog.present(Some(parent));
}

// ---------------------------------------------------------------- 提示词页

fn rebuild_prompts(st: &Rc<St>) {
    for g in st.prompt_groups.borrow_mut().drain(..) {
        st.prompts_page.remove(&g);
    }

    let add_btn = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("新增提示词")
        .valign(gtk::Align::Center)
        .build();
    add_btn.add_css_class("flat");
    add_btn.connect_clicked({
        let st = st.clone();
        move |_| edit_prompt(&st, None)
    });

    let group = adw::PreferencesGroup::builder()
        .title("提示词")
        .description(
            "决定翻译的风格。弹窗顶部可以随时切换，切换后会立刻用新风格重译。\
             提示词里写 {target_lang} 会被替换成上面设置的目标语言。",
        )
        .header_suffix(&add_btn)
        .build();

    let prompts = st.cfg.borrow().prompts.clone();
    for (i, p) in prompts.iter().enumerate() {
        let first_line = p
            .system
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .chars()
            .take(60)
            .collect::<String>();

        let row = adw::ActionRow::builder()
            .title(p.label())
            .subtitle(first_line)
            .activatable(true)
            .build();

        let up = gtk::Button::builder()
            .icon_name("go-up-symbolic")
            .valign(gtk::Align::Center)
            .tooltip_text("上移")
            .sensitive(i > 0)
            .build();
        up.add_css_class("flat");
        up.connect_clicked({
            let st = st.clone();
            move |_| {
                st.cfg.borrow_mut().prompts.swap(i, i - 1);
                st.save();
                rebuild_prompts(&st);
                st.sync_prompt_combo();
            }
        });

        let down = gtk::Button::builder()
            .icon_name("go-down-symbolic")
            .valign(gtk::Align::Center)
            .tooltip_text("下移")
            .sensitive(i + 1 < prompts.len())
            .build();
        down.add_css_class("flat");
        down.connect_clicked({
            let st = st.clone();
            move |_| {
                st.cfg.borrow_mut().prompts.swap(i, i + 1);
                st.save();
                rebuild_prompts(&st);
                st.sync_prompt_combo();
            }
        });

        let edit = gtk::Button::builder()
            .icon_name("document-edit-symbolic")
            .valign(gtk::Align::Center)
            .tooltip_text("编辑")
            .build();
        edit.add_css_class("flat");
        edit.connect_clicked({
            let st = st.clone();
            let id = p.id.clone();
            move |_| edit_prompt(&st, Some(id.clone()))
        });

        let del = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .valign(gtk::Align::Center)
            .tooltip_text("删除")
            .sensitive(prompts.len() > 1)
            .build();
        del.add_css_class("flat");
        del.connect_clicked({
            let st = st.clone();
            let id = p.id.clone();
            move |_| {
                {
                    let mut cfg = st.cfg.borrow_mut();
                    cfg.prompts.retain(|x| x.id != id);
                    if cfg.active_prompt == id {
                        cfg.active_prompt = cfg
                            .prompts
                            .first()
                            .map(|x| x.id.clone())
                            .unwrap_or_default();
                    }
                }
                st.save();
                rebuild_prompts(&st);
                st.sync_prompt_combo();
            }
        });

        row.add_suffix(&up);
        row.add_suffix(&down);
        row.add_suffix(&edit);
        row.add_suffix(&del);
        row.connect_activated({
            let st = st.clone();
            let id = p.id.clone();
            move |_| edit_prompt(&st, Some(id.clone()))
        });
        group.add(&row);
    }

    st.prompts_page.add(&group);
    st.prompt_groups.borrow_mut().push(group);

    // 恢复内置
    let restore_group = adw::PreferencesGroup::new();
    let restore = gtk::Button::builder()
        .label("恢复内置提示词")
        .tooltip_text("把内置的 7 条提示词恢复成出厂内容；你自己新增的不受影响")
        .halign(gtk::Align::Start)
        .build();
    restore.connect_clicked({
        let st = st.clone();
        move |_| {
            {
                let mut cfg = st.cfg.borrow_mut();
                for b in config::builtin_prompts() {
                    match cfg.prompts.iter_mut().find(|x| x.id == b.id) {
                        Some(slot) => *slot = b,
                        None => cfg.prompts.push(b),
                    }
                }
            }
            st.save();
            rebuild_prompts(&st);
            st.sync_prompt_combo();
            st.toast("内置提示词已恢复");
        }
    });
    restore_group.add(&restore);
    st.prompts_page.add(&restore_group);
    st.prompt_groups.borrow_mut().push(restore_group);
    crate::theme::hook_widgets(&st.prompts_page);
}

fn edit_prompt(st: &Rc<St>, id: Option<String>) {
    let existing = id
        .as_ref()
        .and_then(|i| st.cfg.borrow().prompts.iter().find(|p| p.id == *i).cloned());
    let p = existing.clone().unwrap_or_else(|| Prompt {
        id: new_id(),
        name: String::new(),
        icon: "📝".into(),
        system: "你是一个专业翻译引擎。把用户提供的文本翻译成{target_lang}。\n\n\
                 要求：\n- 只输出译文，不要任何解释或前言。"
            .into(),
    });

    let dialog = adw::Dialog::builder()
        .title(if existing.is_some() {
            "编辑提示词"
        } else {
            "新增提示词"
        })
        .content_width(680)
        .content_height(640)
        .build();

    let page = adw::PreferencesPage::new();
    let g = adw::PreferencesGroup::new();
    let name_row = adw::EntryRow::builder().title("名称").build();
    name_row.set_text(&p.name);
    let icon_row = adw::EntryRow::builder().title("图标（一个 emoji）").build();
    icon_row.set_text(&p.icon);
    g.add(&name_row);
    g.add(&icon_row);
    page.add(&g);

    let g2 = adw::PreferencesGroup::builder()
        .title("System Prompt")
        .description("{target_lang} 会被替换成设置里的目标语言。建议明确要求模型只输出译文。")
        .build();
    let text = gtk::TextView::builder()
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(8)
        .right_margin(8)
        .monospace(true)
        .build();
    text.buffer().set_text(&p.system);
    let scroller = gtk::ScrolledWindow::builder()
        .height_request(320)
        .child(&text)
        .build();
    scroller.add_css_class("card");
    g2.add(&scroller);
    page.add(&g2);

    let header = adw::HeaderBar::new();
    let cancel = gtk::Button::with_label("取消");
    let save = gtk::Button::with_label("保存");
    save.add_css_class("suggested-action");
    header.pack_start(&cancel);
    header.pack_end(&save);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&page));
    dialog.set_child(Some(&toolbar));

    cancel.connect_clicked({
        let dialog = dialog.clone();
        move |_| {
            dialog.close();
        }
    });

    save.connect_clicked({
        let st = st.clone();
        let dialog = dialog.clone();
        move |_| {
            let buf = text.buffer();
            let system = buf
                .text(&buf.start_iter(), &buf.end_iter(), false)
                .to_string();
            let name = name_row.text().to_string();
            if name.trim().is_empty() {
                st.toast("名称不能为空");
                return;
            }
            if system.trim().is_empty() {
                st.toast("System Prompt 不能为空");
                return;
            }
            let np = Prompt {
                id: p.id.clone(),
                name,
                icon: icon_row.text().to_string(),
                system,
            };
            {
                let mut cfg = st.cfg.borrow_mut();
                match cfg.prompts.iter_mut().find(|x| x.id == np.id) {
                    Some(slot) => *slot = np.clone(),
                    None => cfg.prompts.push(np.clone()),
                }
            }
            st.save();
            rebuild_prompts(&st);
            st.sync_prompt_combo();
            dialog.close();
        }
    });

    crate::theme::hook_widgets(&dialog);
    dialog.present(Some(&st.window));
}

// ---------------------------------------------------------------- 关于页

fn build_about() -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("关于")
        .icon_name("help-about-symbolic")
        .build();

    let g = adw::PreferencesGroup::builder()
        .title("selectionTranslation")
        .build();
    g.add(
        &adw::ActionRow::builder()
            .title("版本")
            .subtitle(env!("CARGO_PKG_VERSION"))
            .build(),
    );

    let repo_row = adw::ActionRow::builder()
        .title("仓库")
        .subtitle(REPO_URL)
        .build();
    let link = gtk::LinkButton::builder()
        .uri(REPO_URL)
        .label("打开")
        .valign(gtk::Align::Center)
        .build();
    repo_row.add_suffix(&link);
    g.add(&repo_row);

    g.add(
        &adw::ActionRow::builder()
            .title("配置文件")
            .subtitle(config::config_path().display().to_string())
            .build(),
    );
    page.add(&g);

    let g_log = adw::PreferencesGroup::builder()
        .title("运行日志")
        .description(
            "取词结果、实际发给模型的内容、服务端返回的状态都会记在这里。\
             翻译结果不对时先看这个文件。超过 1MB 自动轮转。",
        )
        .build();
    let log_row = adw::ActionRow::builder()
        .title("日志文件")
        .subtitle(logging::log_path().display().to_string())
        .build();
    let open_log = gtk::Button::builder()
        .label("打开")
        .valign(gtk::Align::Center)
        .build();
    open_log.connect_clicked(|_| {
        let p = logging::log_path();
        if !p.exists() {
            let _ = std::fs::write(&p, "");
        }
        let _ = std::process::Command::new("xdg-open").arg(&p).spawn();
    });
    let copy_path = gtk::Button::builder()
        .icon_name("edit-copy-symbolic")
        .tooltip_text("复制路径")
        .valign(gtk::Align::Center)
        .build();
    copy_path.connect_clicked(|b| {
        b.clipboard()
            .set_text(&logging::log_path().display().to_string());
    });
    log_row.add_suffix(&copy_path);
    log_row.add_suffix(&open_log);
    g_log.add(&log_row);

    let tip = adw::ActionRow::builder()
        .title("在终端里跟踪")
        .subtitle("seltrans log -f")
        .build();
    g_log.add(&tip);
    page.add(&g_log);

    let g2 = adw::PreferencesGroup::builder().title("依赖自检").build();
    for (name, ok, note) in selection::deps_report() {
        let row = adw::ActionRow::builder()
            .title(&name)
            .subtitle(&note)
            .build();
        let icon = gtk::Image::from_icon_name(if ok {
            "emblem-ok-symbolic"
        } else {
            "dialog-warning-symbolic"
        });
        icon.add_css_class(if ok { "success" } else { "warning" });
        row.add_prefix(&icon);
        g2.add(&row);
    }
    page.add(&g2);

    page
}

// ---------------------------------------------------------------- 入口

fn build(app: &adw::Application, page: Option<&str>) {
    crate::theme::apply(&Config::load());

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("划词翻译 · 设置")
        .default_width(900)
        .default_height(700)
        .build();

    let stack = adw::ViewStack::new();
    // 切页面时淡入淡出，不是硬切
    stack.set_enable_transitions(true);
    stack.set_transition_duration(200);
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&stack));

    let providers_page = adw::PreferencesPage::builder()
        .title("供应商")
        .icon_name("network-server-symbolic")
        .build();
    let prompts_page = adw::PreferencesPage::builder()
        .title("提示词")
        .icon_name("document-edit-symbolic")
        .build();

    let st = Rc::new(St {
        cfg: RefCell::new(Config::load()),
        window: window.clone(),
        toasts: toasts.clone(),
        providers_page: providers_page.clone(),
        prompts_page: prompts_page.clone(),
        provider_groups: RefCell::new(Vec::new()),
        prompt_groups: RefCell::new(Vec::new()),
        prompt_combo: adw::ComboRow::new(),
        quiet: std::cell::Cell::new(false),
    });

    let general = build_general(&st);
    st.sync_prompt_combo();
    rebuild_providers(&st);
    rebuild_prompts(&st);

    stack.add_titled_with_icon(
        &general,
        Some("general"),
        "通用",
        "preferences-system-symbolic",
    );
    stack.add_titled_with_icon(
        &providers_page,
        Some("providers"),
        "供应商",
        "network-server-symbolic",
    );
    stack.add_titled_with_icon(
        &prompts_page,
        Some("prompts"),
        "提示词",
        "document-edit-symbolic",
    );
    stack.add_titled_with_icon(&build_about(), Some("about"), "关于", "help-about-symbolic");

    let switcher = adw::ViewSwitcher::builder()
        .stack(&stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();
    let header = adw::HeaderBar::builder().title_widget(&switcher).build();

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&toasts));
    window.set_content(Some(&toolbar));
    crate::theme::hook_widgets(&window);
    window.connect_map(crate::theme::hook_widgets);

    if let Some(name) = page {
        stack.set_visible_child_name(name);
    }

    window.present();
}

/// `page` 可以是 general / providers / prompts / about，用来直接跳到某一页
pub fn run(page: Option<String>) -> i32 {
    let app = adw::Application::builder()
        .application_id(APP_ID_SETTINGS)
        .build();
    app.connect_activate(move |app| build(app, page.as_deref()));
    glib::ExitCode::get(&app.run_with_args::<&str>(&[])) as i32
}
