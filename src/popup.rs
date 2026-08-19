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
    /// 原文卡片右上角的字数
    src_count: gtk::Label,
    /// 原文输入框，可编辑
    src_view: gtk::TextView,
    out_view: gtk::TextView,
    status: gtk::Label,
    spinner: gtk::Spinner,
    spinner_rev: gtk::Revealer,
    cfg: RefCell<Config>,
    /// 每次重新翻译自增，用来丢弃上一轮还在路上的流
    generation: Cell<u64>,
    /// 正在以代码方式改动下拉框时，不要触发重新翻译
    quiet: Cell<bool>,
    /// 常驻（托盘）模式：关窗口只藏起来，不退出进程
    resident: Cell<bool>,
    /// 持有它，GTK 才不会在最后一个窗口关掉时退出。丢弃即等于放弃常驻。
    hold: RefCell<Option<gtk::gio::ApplicationHoldGuard>>,
}

impl Ui {
    fn out_buffer(&self) -> gtk::TextBuffer {
        self.out_view.buffer()
    }

    /// 当前配置里的中文字体
    fn cjk_font(&self) -> String {
        self.cfg.borrow().font_cjk.clone()
    }

    fn set_output(&self, text: &str) {
        let buf = self.out_buffer();
        buf.set_text(text);
        crate::fonts::tag_cjk(&buf, &self.cjk_font(), 0);
    }

    fn append_output(&self, text: &str) {
        let buf = self.out_buffer();
        // 流式追加，只给新插入的这一段打标记
        let from = buf.end_iter().offset();
        let mut end = buf.end_iter();
        buf.insert(&mut end, text);
        crate::fonts::tag_cjk(&buf, &self.cjk_font(), from);
        let mut end = buf.end_iter();
        self.out_view.scroll_to_iter(&mut end, 0.0, false, 0.0, 0.0);
    }

    /// 当前输入框里的内容 —— 翻译的唯一数据源
    fn input_text(&self) -> String {
        let buf = self.src_view.buffer();
        buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string()
    }

    fn set_input(&self, text: &str) {
        let buf = self.src_view.buffer();
        buf.set_text(text);
        crate::fonts::tag_cjk(&buf, &self.cjk_font(), 0);
    }

    /// 卡片右上角显示字数，取词取空时不用看日志就能发现
    fn sync_input_label(&self) {
        let n = self.input_text().trim().chars().count();
        self.src_count.set_text(&if n == 0 {
            "空 · 可直接在这里输入".to_string()
        } else {
            format!("{n} 字")
        });
    }

    fn output_text(&self) -> String {
        let buf = self.out_buffer();
        buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string()
    }

    fn busy(&self, on: bool) {
        self.spinner_rev.set_reveal_child(on);
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

    /// 收起弹窗：常驻模式下只是隐藏，否则真的关掉
    fn dismiss(&self) {
        // 让还在路上的流失效，免得藏起来之后还在往里写
        self.generation.set(self.generation.get() + 1);
        self.busy(false);
        if self.resident.get() {
            self.window.set_visible(false);
        } else {
            self.window.close();
        }
    }

    fn save_cfg(&self) {
        if let Err(e) = self.cfg.borrow().save() {
            eprintln!("seltrans: 配置保存失败：{e}");
        }
    }

    fn start_translate(self: &Rc<Self>) {
        let source = self.input_text();
        if crate::logging::is_blank(&source) {
            crate::logging::warn("待翻译文本为空，跳过本次翻译");
            self.busy(false);
            self.status.set_text("请先输入或选中要翻译的文本");
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

    /// 打开成"输入模式"：清空、聚焦输入框，等着用户打字或粘贴。
    /// 点托盘图标走的就是这条路 —— 那时候通常并没有选中任何文本。
    fn open_input(self: &Rc<Self>) {
        *self.cfg.borrow_mut() = Config::load();
        self.sync_controls();

        self.generation.set(self.generation.get() + 1);
        self.busy(false);
        self.set_input("");
        self.sync_input_label();
        self.set_output("");
        self.status.set_text("输入或粘贴文本，Ctrl+Enter 翻译");
        self.window.present();
        self.src_view.grab_focus();
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
            Ok(t) if !crate::logging::is_blank(&t) => {
                self.set_input(t.trim());
                self.sync_input_label();
                self.start_translate();
            }
            Ok(_) | Err(_) => {
                let msg = match text {
                    Err(e) => e,
                    _ => "选中的内容是空的".to_string(),
                };
                self.set_input("");
                self.sync_input_label();
                self.busy(false);
                self.status.set_text("没取到文本");
                self.set_output(&format!(
                    "没有取到选中的文本。\n\n{msg}\n\n\
                     排查建议：\n\
                     · 确认在按快捷键之前文字确实处于选中状态\n\
                     · 部分应用（如某些 Electron / Java 程序）不提供主选区，\
                     可在设置里把取词方式改成「自动」或「仅模拟 Ctrl+C」\n\
                     · 模拟 Ctrl+C 需要 ydotool 服务：systemctl --user status ydotool\n\n\
                     详细过程见日志：{}",
                    crate::logging::log_path().display()
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
    crate::theme::apply(&cfg);

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
        .tooltip_text("翻译（Ctrl+Enter / F5）")
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

    // ---- 正文：原文、译文各画成一张卡片 ----
    // 原文区是**可编辑**的：取词取歪了可以就地改，也可以什么都不选、
    // 直接点托盘图标打开这里手敲或粘贴要翻译的内容。
    let src_view = gtk::TextView::builder()
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(10)
        .right_margin(10)
        .build();

    let src_count = gtk::Label::builder()
        .xalign(1.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    src_count.add_css_class("st-count");

    let src_head = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .build();
    let src_title = gtk::Label::builder().label("原文").xalign(0.0).build();
    src_title.add_css_class("st-section");
    src_head.append(&src_title);
    src_head.append(&src_count);

    let src_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_height(76)
        .max_content_height(170)
        .propagate_natural_height(true)
        .child(&src_view)
        .build();
    src_scroller.add_css_class("st-card");

    let out_view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(10)
        .right_margin(10)
        .build();

    let spinner = gtk::Spinner::new();
    // 用 Revealer 包一层：忙碌指示淡入淡出，不硬闪
    let spinner_rev = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::Crossfade)
        .transition_duration(220)
        .reveal_child(false)
        .child(&spinner)
        .build();
    let out_head = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_top(10)
        .build();
    let out_title = gtk::Label::builder().label("译文").xalign(0.0).build();
    out_title.add_css_class("st-section");
    out_head.append(&out_title);
    out_head.append(&spinner_rev);

    let out_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&out_view)
        .build();
    out_scroller.add_css_class("st-card");

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(5)
        .margin_top(10)
        .margin_bottom(14)
        .margin_start(14)
        .margin_end(14)
        .build();
    content.append(&src_head);
    content.append(&src_scroller);
    content.append(&out_head);
    content.append(&out_scroller);

    // ---- 底栏 ----
    let provider_dd = gtk::DropDown::builder().tooltip_text("供应商").build();
    let model_dd = gtk::DropDown::builder().tooltip_text("模型").build();
    let status = gtk::Label::builder()
        .xalign(1.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .build();
    status.add_css_class("st-status");

    let bottom = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .margin_top(12)
        .build();
    // 底部这两个是"当前用什么"的次要信息，做成扁平的，别跟正文抢注意力
    provider_dd.add_css_class("flat");
    provider_dd.add_css_class("st-chip");
    model_dd.add_css_class("flat");
    model_dd.add_css_class("st-chip");
    bottom.append(&provider_dd);
    bottom.append(&model_dd);
    bottom.append(&status);
    content.append(&bottom);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&content));
    window.set_content(Some(&toolbar));
    crate::theme::hook_widgets(&window);
    window.connect_map(|w| crate::theme::hook_widgets(w));

    let ui = Rc::new(Ui {
        window: window.clone(),
        prompt_dd: prompt_dd.clone(),
        provider_dd: provider_dd.clone(),
        model_dd: model_dd.clone(),
        src_count,
        src_view: src_view.clone(),
        out_view,
        status,
        spinner,
        spinner_rev,
        cfg: RefCell::new(cfg),
        generation: Cell::new(0),
        quiet: Cell::new(false),
        resident: Cell::new(false),
        hold: RefCell::new(None),
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

    // 输入框内容变了就更新标题上的字数
    src_view.buffer().connect_changed({
        let ui = ui.clone();
        move |buf| {
            ui.sync_input_label();
            crate::fonts::tag_cjk(buf, &ui.cjk_font(), 0);
        }
    });

    // 快捷键：Esc 关闭、Ctrl+Enter / F5 翻译、Ctrl+Shift+C 复制
    let key = gtk::EventControllerKey::new();
    key.connect_key_pressed({
        let ui = ui.clone();
        move |_, keyval, _, state| {
            let ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            let shift = state.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            match keyval {
                gtk::gdk::Key::Escape => {
                    ui.dismiss();
                    glib::Propagation::Stop
                }
                // 输入框里回车是换行，Ctrl+Enter 才是"翻译"
                gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter if ctrl => {
                    ui.start_translate();
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

    // 常驻模式下点标题栏的关闭按钮只是把窗口藏起来，进程和托盘图标都留着
    window.connect_close_request({
        let ui = ui.clone();
        move |_| {
            if ui.resident.get() {
                ui.dismiss();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
    });

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

/// 前台模式：弹一次窗，窗口关掉进程就退出
pub fn run(argv: Vec<String>) -> i32 {
    run_inner(false, argv)
}

/// 常驻模式：注册托盘图标，不主动弹窗，进程一直在
///
/// 好处不止是"能看见它在跑" —— 快捷键再触发时是复用这个进程，省掉 GTK 冷启动，
/// 弹窗几乎是瞬间出来的。
pub fn run_tray() -> i32 {
    // 已经有常驻进程了就直接退出。否则第二个进程会把命令转给已在跑的那个，
    // 结果登录时凭空弹出一个翻译窗口。
    if crate::tray::is_running() {
        crate::logging::info("已有常驻进程在跑，本次 tray 启动直接退出");
        return 0;
    }
    run_inner(true, vec!["seltrans".to_string(), "tray".to_string()])
}

fn run_inner(tray_mode: bool, local_argv: Vec<String>) -> i32 {
    // 用 HANDLES_COMMAND_LINE 而不是 activate：这样 `seltrans popup --input`
    // 这类参数能原样送到已经常驻的那个进程，而不是被丢掉。
    let mut flags = gtk::gio::ApplicationFlags::HANDLES_COMMAND_LINE;
    // 带 --text 时不复用已有实例，方便调试
    if local_argv.iter().any(|a| a == "--text") {
        flags |= gtk::gio::ApplicationFlags::NON_UNIQUE;
    }

    let app = adw::Application::builder()
        .application_id(APP_ID_POPUP)
        .flags(flags)
        .build();

    let ui_slot: Rc<RefCell<Option<Rc<Ui>>>> = Rc::new(RefCell::new(None));
    let tray_ready = Rc::new(Cell::new(false));

    app.connect_command_line(move |app, cmdline| {
        let argv: Vec<String> = cmdline
            .arguments()
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();

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

        if tray_mode && !tray_ready.get() {
            // 这次是托盘进程自己启动，不是用户按快捷键，所以不弹窗
            tray_ready.set(true);
            ui.resident.set(true);
            *ui.hold.borrow_mut() = Some(app.hold());
            setup_tray(app, &ui);
            return glib::ExitCode::SUCCESS;
        }

        if argv.iter().any(|a| a == "--input") {
            ui.open_input();
            return glib::ExitCode::SUCCESS;
        }

        ui.window.present();
        ui.refresh(crate::arg_text(&argv));
        glib::ExitCode::SUCCESS
    });

    glib::ExitCode::get(&app.run_with_args(&local_argv)) as i32
}

/// 注册托盘图标，并把托盘菜单的点击派发回 GTK 主线程
fn setup_tray(app: &adw::Application, ui: &Rc<Ui>) {
    use crate::tray::{Cmd, SelTray, Snapshot};

    let (tx, rx) = async_channel::unbounded::<Cmd>();
    let snap = Snapshot::from_config(&ui.cfg.borrow());
    // 图标必须在 GTK 主线程上光栅化，之后 ksni 只是搬运像素
    let icons = crate::tray::render_icons();
    if icons.is_empty() {
        crate::logging::warn("托盘图标光栅化失败，将退回按名字查找主题图标");
    }
    let tray = SelTray::new(tx, snap, icons);

    let handle = match llm::runtime().block_on(async {
        use ksni::TrayMethods;
        tray.spawn().await
    }) {
        Ok(h) => {
            crate::logging::info("托盘图标已注册");
            Some(h)
        }
        Err(e) => {
            crate::logging::error(&format!(
                "托盘注册失败（面板可能没有提供 StatusNotifierWatcher）：{e}"
            ));
            None
        }
    };

    // 配置变了就把托盘菜单的快照刷新一遍
    let refresh_tray = {
        let handle = handle.clone();
        let ui = ui.clone();
        move || {
            let Some(h) = handle.clone() else { return };
            let snap = Snapshot::from_config(&ui.cfg.borrow());
            llm::runtime().spawn(async move {
                h.update(move |t: &mut SelTray| t.snap = snap).await;
            });
        }
    };

    let app = app.clone();
    let ui = ui.clone();
    glib::spawn_future_local(async move {
        while let Ok(cmd) = rx.recv().await {
            crate::logging::info(&format!("托盘命令：{cmd:?}"));
            match cmd {
                Cmd::Input => ui.open_input(),
                Cmd::Translate => {
                    ui.window.present();
                    ui.refresh(None);
                }
                Cmd::Settings(page) => open_settings(page),
                Cmd::SetProvider(id) => {
                    *ui.cfg.borrow_mut() = Config::load();
                    ui.cfg.borrow_mut().active_provider = id;
                    ui.save_cfg();
                    ui.sync_controls();
                    refresh_tray();
                }
                Cmd::SetPrompt(id) => {
                    *ui.cfg.borrow_mut() = Config::load();
                    ui.cfg.borrow_mut().active_prompt = id;
                    ui.save_cfg();
                    ui.sync_controls();
                    refresh_tray();
                }
                Cmd::SetAutostart(on) => {
                    if let Err(e) = crate::autostart::set_enabled(on) {
                        crate::logging::error(&format!("设置开机自启动失败：{e}"));
                    }
                    refresh_tray();
                }
                Cmd::OpenLog => {
                    let p = crate::logging::log_path();
                    if !p.exists() {
                        let _ = std::fs::write(&p, "");
                    }
                    let _ = std::process::Command::new("xdg-open").arg(&p).spawn();
                }
                Cmd::Quit => {
                    crate::logging::info("从托盘退出");
                    app.quit();
                    return;
                }
            }
        }
    });
}
