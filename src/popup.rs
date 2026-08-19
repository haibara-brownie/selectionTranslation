//! 划词翻译弹窗。
//!
//! 单实例：app-id 唯一，再按一次快捷键时第二个进程会把 activate 转交给已经在跑的实例，
//! 于是复用同一个窗口、省掉冷启动。

use adw::prelude::*;
use gtk::glib;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::config::{Config, Provider};
use crate::{APP_ID_POPUP, llm, selection};

struct Ui {
    window: adw::ApplicationWindow,
    prompt_dd: gtk::DropDown,
    provider_dd: gtk::DropDown,
    model_dd: gtk::DropDown,
    src_expander: gtk::Expander,
    src_label: gtk::Label,
    out_view: gtk::TextView,
    status: gtk::Label,
    spinner: gtk::Spinner,
    cfg: RefCell<Config>,
    source: RefCell<String>,
    /// 每次重新翻译自增，用来丢弃上一轮还在路上的流
    generation: Cell<u64>,
    /// 正在以代码方式改动下拉框时，不要触发重新翻译
    quiet: Cell<bool>,
}

impl Ui {
    fn out_buffer(&self) -> gtk::TextBuffer {
        self.out_view.buffer()
    }

    fn set_output(&self, text: &str) {
        self.out_buffer().set_text(text);
    }

    fn append_output(&self, text: &str) {
        let buf = self.out_buffer();
        let mut end = buf.end_iter();
        buf.insert(&mut end, text);
        let mut end = buf.end_iter();
        self.out_view.scroll_to_iter(&mut end, 0.0, false, 0.0, 0.0);
    }

    fn output_text(&self) -> String {
        let buf = self.out_buffer();
        buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string()
    }

    fn busy(&self, on: bool) {
        self.spinner.set_visible(on);
        if on {
            self.spinner.start();
        } else {
            self.spinner.stop();
        }
    }

    /// 按当前配置刷新三个下拉框的内容与选中项
    fn sync_controls(&self) {
        self.quiet.set(true);
        let cfg = self.cfg.borrow();

        let labels: Vec<String> = cfg.prompts.iter().map(|p| p.label()).collect();
        set_dropdown(&self.prompt_dd, &labels);
        if let Some(i) = cfg.prompts.iter().position(|p| p.id == cfg.active_prompt) {
            self.prompt_dd.set_selected(i as u32);
        }

        let pnames: Vec<String> = cfg.providers.iter().map(|p| p.name.clone()).collect();
        set_dropdown(&self.provider_dd, &pnames);
        let cur = cfg.providers.iter().position(|p| p.id == cfg.active_provider);
        if let Some(i) = cur {
            self.provider_dd.set_selected(i as u32);
        }
        self.provider_dd.set_visible(cfg.providers.len() > 1);

        let models: Vec<String> = match cfg.active_provider() {
            Some(p) if !p.models.is_empty() => p.models.clone(),
            Some(p) if !p.model.is_empty() => vec![p.model.clone()],
            _ => vec!["（未设置模型）".to_string()],
        };
        set_dropdown(&self.model_dd, &models);
        if let Some(p) = cfg.active_provider() {
            if let Some(i) = models.iter().position(|m| *m == p.model) {
                self.model_dd.set_selected(i as u32);
            }
        }

        drop(cfg);
        self.quiet.set(false);
    }

    fn save_cfg(&self) {
        if let Err(e) = self.cfg.borrow().save() {
            eprintln!("seltrans: 配置保存失败：{e}");
        }
    }

    fn start_translate(self: &Rc<Self>) {
        let source = self.source.borrow().clone();
        if source.trim().is_empty() {
            return;
        }

        let (provider, system) = {
            let cfg = self.cfg.borrow();
            let Some(provider) = cfg.active_provider().cloned() else {
                self.busy(false);
                self.status.set_text("尚未配置模型");
                self.set_output(
                    "还没有配置任何模型供应商。\n\n点右上角的齿轮打开配置界面，\
                     在「供应商」页里选一个预设、填上 API key，再点「拉取模型」挑一个模型。",
                );
                return;
            };
            let Some(prompt) = cfg.active_prompt().cloned() else {
                return;
            };
            (provider, cfg.render_system(&prompt))
        };

        let round = self.generation.get() + 1;
        self.generation.set(round);
        self.set_output("");
        self.busy(true);
        self.status.set_text(&format!("{} · {}", provider.name, provider.model));

        let (tx, rx) = async_channel::unbounded::<llm::Event>();
        llm::runtime().spawn(llm::stream_translate(provider, system, source, tx));

        let ui = self.clone();
        glib::spawn_future_local(async move {
            while let Ok(ev) = rx.recv().await {
                if ui.generation.get() != round {
                    return; // 已经开始新一轮翻译了，这一轮的结果丢掉
                }
                match ev {
                    llm::Event::Delta(d) => ui.append_output(&d),
                    llm::Event::Done => {
                        ui.busy(false);
                        break;
                    }
                    llm::Event::Error(e) => {
                        ui.busy(false);
                        ui.status.set_text("出错了");
                        let cur = ui.output_text();
                        if cur.is_empty() {
                            ui.set_output(&format!("翻译失败：\n\n{e}"));
                        } else {
                            ui.append_output(&format!("\n\n[中断] {e}"));
                        }
                        break;
                    }
                }
            }
            ui.busy(false);
        });
    }

    /// 抓一次词并翻译；`over` 非空时直接用它（`--text` 调试用）
    fn refresh(self: &Rc<Self>, over: Option<String>) {
        *self.cfg.borrow_mut() = Config::load();
        self.sync_controls();

        let mode = self.cfg.borrow().selection_mode.clone();
        let text = match over {
            Some(t) => Ok(t),
            None => selection::grab(&mode),
        };

        match text {
            Ok(t) if !t.trim().is_empty() => {
                self.src_label.set_text(t.trim());
                self.src_expander.set_visible(true);
                *self.source.borrow_mut() = t;
                self.start_translate();
            }
            Ok(_) | Err(_) => {
                let msg = match text {
                    Err(e) => e,
                    _ => "选中的内容是空的".to_string(),
                };
                self.src_expander.set_visible(false);
                *self.source.borrow_mut() = String::new();
                self.busy(false);
                self.status.set_text("没取到文本");
                self.set_output(&format!(
                    "没有取到选中的文本。\n\n{msg}\n\n\
                     排查建议：\n\
                     · 确认在按快捷键之前文字确实处于选中状态\n\
                     · 部分应用（如某些 Electron / Java 程序）不提供主选区，\
                     可在设置里把取词方式改成「自动」或「仅模拟 Ctrl+C」\n\
                     · 模拟 Ctrl+C 需要 ydotool 服务：systemctl --user status ydotool"
                ));
            }
        }
    }
}

fn set_dropdown(dd: &gtk::DropDown, items: &[String]) {
    let refs: Vec<&str> = items.iter().map(String::as_str).collect();
    dd.set_model(Some(&gtk::StringList::new(&refs)));
}

fn open_settings(page: Option<&str>) {
    if let Ok(exe) = std::env::current_exe() {
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("settings");
        if let Some(p) = page {
            cmd.arg(p);
        }
        let _ = cmd.spawn();
    }
}

fn build(app: &adw::Application) -> Rc<Ui> {
    let cfg = Config::load();

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("划词翻译")
        .default_width(cfg.popup_width)
        .default_height(cfg.popup_height)
        .build();

    let prompt_dd = gtk::DropDown::builder()
        .tooltip_text("翻译风格（提示词）")
        .build();

    let copy_btn = gtk::Button::builder()
        .icon_name("edit-copy-symbolic")
        .tooltip_text("复制译文（Ctrl+Shift+C）")
        .build();
    let redo_btn = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("重新翻译（F5）")
        .build();
    let settings_btn = gtk::Button::builder()
        .icon_name("emblem-system-symbolic")
        .tooltip_text("设置")
        .build();

    let header = adw::HeaderBar::builder()
        .title_widget(&prompt_dd)
        .build();
    header.pack_end(&settings_btn);
    header.pack_end(&copy_btn);
    header.pack_start(&redo_btn);

    // ---- 正文 ----
    let src_label = gtk::Label::builder()
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .selectable(true)
        .xalign(0.0)
        .margin_top(6)
        .build();
    src_label.add_css_class("dim-label");

    let src_expander = gtk::Expander::builder()
        .label("原文")
        .child(&src_label)
        .build();

    let out_view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(6)
        .bottom_margin(6)
        .vexpand(true)
        .build();

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(&src_expander);
    content.append(&out_view);

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&content)
        .build();

    // ---- 底栏 ----
    let provider_dd = gtk::DropDown::builder().tooltip_text("供应商").build();
    let model_dd = gtk::DropDown::builder().tooltip_text("模型").build();
    let spinner = gtk::Spinner::new();
    let status = gtk::Label::builder()
        .xalign(1.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .build();
    status.add_css_class("dim-label");
    status.add_css_class("caption");

    let bottom = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(12)
        .margin_end(12)
        .build();
    bottom.append(&provider_dd);
    bottom.append(&model_dd);
    bottom.append(&spinner);
    bottom.append(&status);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&scroller));
    toolbar.add_bottom_bar(&bottom);
    window.set_content(Some(&toolbar));

    let ui = Rc::new(Ui {
        window: window.clone(),
        prompt_dd: prompt_dd.clone(),
        provider_dd: provider_dd.clone(),
        model_dd: model_dd.clone(),
        src_expander,
        src_label,
        out_view,
        status,
        spinner,
        cfg: RefCell::new(cfg),
        source: RefCell::new(String::new()),
        generation: Cell::new(0),
        quiet: Cell::new(false),
    });
    ui.busy(false);
    ui.sync_controls();

    // ---- 交互 ----
    prompt_dd.connect_selected_notify({
        let ui = ui.clone();
        move |dd| {
            if ui.quiet.get() {
                return;
            }
            let i = dd.selected() as usize;
            let id = ui.cfg.borrow().prompts.get(i).map(|p| p.id.clone());
            if let Some(id) = id {
                ui.cfg.borrow_mut().active_prompt = id;
                ui.save_cfg();
                ui.start_translate();
            }
        }
    });

    provider_dd.connect_selected_notify({
        let ui = ui.clone();
        move |dd| {
            if ui.quiet.get() {
                return;
            }
            let i = dd.selected() as usize;
            let id = ui.cfg.borrow().providers.get(i).map(|p| p.id.clone());
            if let Some(id) = id {
                ui.cfg.borrow_mut().active_provider = id;
                ui.save_cfg();
                ui.sync_controls();
                ui.start_translate();
            }
        }
    });

    model_dd.connect_selected_notify({
        let ui = ui.clone();
        move |dd| {
            if ui.quiet.get() {
                return;
            }
            let Some(model) = dropdown_text(dd) else {
                return;
            };
            if model.starts_with('（') {
                return;
            }
            {
                let mut cfg = ui.cfg.borrow_mut();
                let active = cfg.active_provider.clone();
                if let Some(p) = provider_mut(&mut cfg, &active) {
                    p.model = model;
                }
            }
            ui.save_cfg();
            ui.start_translate();
        }
    });

    copy_btn.connect_clicked({
        let ui = ui.clone();
        move |_| {
            ui.window.clipboard().set_text(&ui.output_text());
            ui.status.set_text("已复制译文");
        }
    });

    redo_btn.connect_clicked({
        let ui = ui.clone();
        move |_| ui.start_translate()
    });

    settings_btn.connect_clicked({
        let ui = ui.clone();
        move |_| {
            // 一个供应商都没有时，直接把用户带到「供应商」页
            let page = if ui.cfg.borrow().providers.is_empty() {
                Some("providers")
            } else {
                None
            };
            open_settings(page);
        }
    });

    // 快捷键：Esc 关闭、Ctrl+Shift+C 复制、F5 重译
    let key = gtk::EventControllerKey::new();
    key.connect_key_pressed({
        let ui = ui.clone();
        move |_, keyval, _, state| {
            let ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            let shift = state.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            match keyval {
                gtk::gdk::Key::Escape => {
                    ui.window.close();
                    glib::Propagation::Stop
                }
                gtk::gdk::Key::F5 => {
                    ui.start_translate();
                    glib::Propagation::Stop
                }
                gtk::gdk::Key::C | gtk::gdk::Key::c if ctrl && shift => {
                    ui.window.clipboard().set_text(&ui.output_text());
                    ui.status.set_text("已复制译文");
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        }
    });
    window.add_controller(key);

    ui
}

fn dropdown_text(dd: &gtk::DropDown) -> Option<String> {
    dd.model()
        .and_then(|m| m.downcast::<gtk::StringList>().ok())
        .and_then(|l| l.string(dd.selected()))
        .map(|s| s.to_string())
}

fn provider_mut<'a>(cfg: &'a mut Config, id: &str) -> Option<&'a mut Provider> {
    if let Some(i) = cfg.providers.iter().position(|p| p.id == id) {
        cfg.providers.get_mut(i)
    } else {
        cfg.providers.first_mut()
    }
}

pub fn run(text_override: Option<String>) -> i32 {
    // 带 --text 时不复用已有实例，方便调试
    let flags = if text_override.is_some() {
        gtk::gio::ApplicationFlags::NON_UNIQUE
    } else {
        gtk::gio::ApplicationFlags::empty()
    };

    let app = adw::Application::builder()
        .application_id(APP_ID_POPUP)
        .flags(flags)
        .build();

    let ui_slot: Rc<RefCell<Option<Rc<Ui>>>> = Rc::new(RefCell::new(None));

    app.connect_activate(move |app| {
        let ui = {
            let mut slot = ui_slot.borrow_mut();
            match slot.as_ref() {
                Some(ui) => ui.clone(),
                None => {
                    let ui = build(app);
                    *slot = Some(ui.clone());
                    ui
                }
            }
        };
        ui.window.present();
        ui.refresh(text_override.clone());
    });

    glib::ExitCode::get(&app.run_with_args::<&str>(&[])) as i32
}
