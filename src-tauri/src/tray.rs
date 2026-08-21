//! 托盘图标。
//!
//! # 为什么 Linux 上不用 Tauri 内置的托盘
//!
//! `tauri::tray` 在 Linux 上走的是 `libayatana-appindicator`（C 库 + GTK），而且有
//! 已知缺陷：Wayland 下 `.deb` 安装和 dev 模式的图标根本不显示，只有 AppImage 正常
//! （tauri#14234）。GTK 版用的 `ksni` 是纯 Rust + 纯 D-Bus 的 StatusNotifierItem
//! 实现，没有任何 C 依赖，而且在这台 niri 机器上是实测能用的。所以这里按平台切：
//! **Linux 留着 ksni，mac / Windows 才用 Tauri 内置托盘** —— 那两个平台上内置实现
//! 没毛病，再自己造轮子不划算。
//!
//! 配合的是 Cargo.toml 里的 feature 分段：`tray-icon` 只在非 Linux target 上打开，
//! Linux 构建压根不会把 appindicator 链进来。
//!
//! # 为什么图标要内嵌进二进制、启动时自己光栅化
//!
//! SNI 允许只报一个 `icon_name` 让面板去图标主题里找。实测不行：面板（这里是 DMS 的
//! Quickshell）在**自己启动时**就把图标主题缓存住了，之后新装进 hicolor 的图标它按
//! 名字查不到，只会显示一个首字母兜底方块。把 SVG 编进二进制、启动时光栅化成 ARGB32
//! 位图直接经 D-Bus 递过去，就绕开了整个主题查找，也不依赖安装脚本有没有把图标文件
//! 放对地方。
//!
//! # 托盘动作怎么回到主线程
//!
//! GTK 版必须用 `async_channel`：ksni 的回调在别的线程上，而 UI 状态是 `Rc<Ui>`
//! （`!Send`），除了发消息给 GTK 主循环没有第二条路。
//!
//! Tauri 这边不一样：`AppHandle` 本身是 `Send + Sync`，而且自带 `run_on_main_thread`
//! 这个主线程跳板。再架一层 channel 等于多一个冗余队列，还会逼着 `main.rs` 养一个
//! 接收循环 —— 托盘的管线就漏进了不该管这事的文件。所以这里**不用 channel**：回调
//! 直接拿 `AppHandle`，要碰窗口时跳一下主线程，落到 `windows::dispatch` 上（开窗的
//! 规矩、取词时序都归它管，托盘不该有第二套）。
//!
//! 纯读写配置文件的动作（切供应商 / 切提示词 / 自启）不碰 UI，就地同步做完，这样紧接着
//! 重取的菜单快照一定是最新的。

use std::sync::OnceLock;

use tauri::AppHandle;

use seltrans_core::config::Config;
use seltrans_core::logging;

use crate::windows::{self, TranslateRequest};

/// 内置图标的矢量源。三份各有各的用途，别互相替代：
///
/// | 文件 | 谁用 | 为什么不能共用 |
/// |---|---|---|
/// | `xyz.brownie.SelectionTranslation.svg` | Linux 托盘 48/64 档、以及打包出的各种 PNG | 按 128 设计，细节最全 |
/// | `tray-small.svg` | Linux 托盘 22/32 档 | 应用图标缩到 22px 时每个字形只剩约 4 像素，必糊 |
/// | `tray-mono.svg` | macOS 菜单栏 | 模板图标只认 alpha，彩色图压成单色会丢层级 |
/// 三份都对两个平台可见（而不是各自 cfg 到自己那边），是为了让**测试能覆盖到全部三份**
/// —— 测试跑在 mac 上，而 `SMALL` 只有 Linux 构建会用；分别 cfg 的话它就成了一段只能
/// 盲改的资源。非 Linux 构建下用不到的那两个标记为允许未使用。
#[cfg(any(target_os = "linux", target_os = "macos"))]
// 每个平台只用得到其中一部分（Linux 用 APP + SMALL，mac 用 MONO），但三份都要对两个
// 平台可见 —— 只有这样测试才能覆盖到全部三份。所以整体允许未使用，而不是逐个 cfg：
// 逐个 cfg 的话，Linux 专属的 SMALL 就成了一段谁也验不了的资源。
#[allow(dead_code)]
mod icons {
    pub const APP: &[u8] = include_bytes!("../../data/xyz.brownie.SelectionTranslation.svg");
    pub const SMALL: &[u8] = include_bytes!("../../data/tray-small.svg");
    pub const MONO: &[u8] = include_bytes!("../../data/tray-mono.svg");
}

/// 把内置 SVG 光栅化成**不预乘**的 RGBA。
///
/// 两个平台的托盘都从这里出发，之后各自重排通道：Linux 的 SNI 要大端 ARGB，
/// Tauri 的 `Image` 要 RGBA。合并到一处是为了让「图标画没画出来」这件事**只有一个
/// 地方会错**，也让 Linux 专属的那份图能在别的平台上被单测覆盖 —— 否则它就成了
/// 一段只能盲改的代码。
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn rasterize(svg: &[u8], size: u32) -> Option<Vec<u8>> {
    use resvg::tiny_skia;

    let opt = resvg::usvg::Options::default();
    let tree = match resvg::usvg::Tree::from_data(svg, &opt) {
        Ok(t) => t,
        Err(e) => {
            logging::error(&format!("内置托盘图标解析失败：{e}"));
            return None;
        }
    };
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    let scale = tiny_skia::Transform::from_scale(
        size as f32 / tree.size().width(),
        size as f32 / tree.size().height(),
    );
    resvg::render(&tree, scale, &mut pixmap.as_mut());

    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for px in pixmap.pixels() {
        let c = px.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Some(rgba)
}

/// 托盘句柄。克隆很便宜（内部就是一个服务句柄）。
#[derive(Clone)]
pub struct TrayHandle(imp::Inner);

/// 全局那一份。
///
/// 两个理由：
/// 1. ksni 的服务句柄掉了托盘就没了，而调用方很可能只关心 `spawn` 有没有报错、
///    顺手把返回值丢掉。存一份在这里，丢了也不至于把托盘弄没。
/// 2. 设置页保存完配置要刷新托盘，那儿只有 `AppHandle`，拿不到 `TrayHandle`。
static TRAY: OnceLock<TrayHandle> = OnceLock::new();

/// 起托盘。
///
/// 失败不该拖垮整个程序（面板不提供 StatusNotifierWatcher 是常见情况），
/// 所以错误是 `String` 让调用方记一笔日志继续跑。
pub fn spawn(app: &AppHandle) -> Result<TrayHandle, String> {
    let handle = TrayHandle(imp::spawn(app)?);
    let _ = TRAY.set(handle.clone());
    Ok(handle)
}

/// 配置变了（切了供应商 / 提示词 / 目标语言）之后刷新菜单和悬停提示。
/// 从任意线程调都安全。
///
/// 配置变了之后重取快照重画菜单。
pub fn refresh(handle: &TrayHandle) {
    imp::refresh(&handle.0);
}

/// 同上，给拿不到 `TrayHandle` 的地方用（比如只有 `AppHandle` 的 tauri 命令里）。
/// 托盘没起来时静默跳过。
pub fn refresh_now() {
    if let Some(h) = TRAY.get() {
        refresh(h);
    }
}

// ---------------------------------------------------------------- 菜单快照

/// 画菜单要的配置快照。
///
/// 之所以是快照而不是每次现读 `Config`：ksni 的 `Tray` 必须 `Send + 'static`，
/// 没法持有 `&Config`；而且画一次菜单要读十几个字段，现读等于反复敲磁盘。
#[derive(Clone, Default)]
struct Snapshot {
    provider_name: String,
    model: String,
    prompt_label: String,
    target_lang: String,
    /// (id, 显示名)
    providers: Vec<(String, String)>,
    /// (id, 带图标的标签)
    prompts: Vec<(String, String)>,
    active_provider: String,
    active_prompt: String,
    autostart: bool,
}

impl Snapshot {
    fn load(app: &AppHandle) -> Self {
        let cfg = Config::load();
        let active = cfg.active_provider();
        Snapshot {
            provider_name: active
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "未配置供应商".into()),
            model: active
                .map(|p| p.model.clone())
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| "未选模型".into()),
            prompt_label: cfg
                .active_prompt()
                .map(|p| p.label())
                .unwrap_or_else(|| "—".into()),
            target_lang: cfg.target_lang.clone(),
            providers: cfg
                .providers
                .iter()
                .map(|p| (p.id.clone(), p.name.clone()))
                .collect(),
            prompts: cfg
                .prompts
                .iter()
                .map(|p| (p.id.clone(), p.label()))
                .collect(),
            active_provider: active.map(|p| p.id.clone()).unwrap_or_default(),
            active_prompt: cfg
                .active_prompt()
                .map(|p| p.id.clone())
                .unwrap_or_default(),
            autostart: autostart::is_enabled(app),
        }
    }

    /// 悬停提示的正文：当前供应商 / 模型 / 提示词 / 目标语言
    fn tooltip(&self) -> String {
        format!(
            "{} · {}\n风格：{}　目标语言：{}\n按快捷键翻译选中的文本",
            self.provider_name, self.model, self.prompt_label, self.target_lang
        )
    }

    /// 菜单顶上那两行只读信息
    fn headline(&self) -> String {
        format!("{} · {}", self.provider_name, self.model)
    }

    fn subhead(&self) -> String {
        format!("{}　→　{}", self.prompt_label, self.target_lang)
    }
}

// ---------------------------------------------------------------- 要碰窗口的动作

/// 要碰窗口、必须在主线程上做的动作。
///
/// 只读写配置的动作不在这里 —— 那些就地做完更简单，见模块头。
#[derive(Debug, Clone, Copy)]
enum Action {
    /// 打开弹窗并聚焦输入框
    Input,
    /// 取词并翻译
    Translate,
    Settings,
    OpenLog,
    Quit,
}

/// 把动作送到主线程执行。
///
/// 在主线程上调也安全：`run_on_main_thread` 只是往事件循环塞一条消息，不会自己等自己。
fn dispatch(app: &AppHandle, action: Action) {
    logging::info(&format!("托盘动作：{action:?}"));
    let target = app.clone();
    if let Err(e) = app.run_on_main_thread(move || apply(&target, action)) {
        logging::error(&format!("托盘动作没能派发到主线程：{action:?}：{e}"));
    }
}

fn apply(app: &AppHandle, action: Action) {
    match action {
        // 开窗一律走 windows::dispatch：取词必须赶在窗口抢到焦点之前，那套时序归它管
        Action::Input => windows::dispatch(
            app,
            "popup",
            TranslateRequest {
                input_mode: true,
                ..Default::default()
            },
            None,
        ),
        Action::Translate => windows::dispatch(app, "popup", TranslateRequest::default(), None),
        Action::Settings => windows::dispatch(app, "settings", TranslateRequest::default(), None),
        Action::OpenLog => open_log(),
        Action::Quit => {
            logging::info("从托盘退出");
            app.exit(0);
        }
    }
}

/// 用系统默认程序打开日志。
///
/// 走 opener 插件而不是写死 `xdg-open`：这份代码 mac / Windows 上也要跑。
fn open_log() {
    let path = logging::log_path();
    // 一次都没写过日志时文件不存在，直接打开会报错；先落个空文件
    if !path.exists()
        && let Err(e) = std::fs::write(&path, "")
    {
        logging::error(&format!("创建日志文件失败：{e}"));
        return;
    }
    if let Err(e) = tauri_plugin_opener::open_path(&path, None::<&str>) {
        logging::error(&format!("打开日志失败：{e}"));
    }
}

// ---------------------------------------------------------------- 只写配置的动作

fn set_provider(id: &str) {
    // 先重新 load 再改：设置页可能刚在别处动过配置，拿旧快照整个写回去会把它盖掉
    let mut cfg = Config::load();
    if !cfg.providers.iter().any(|p| p.id == id) {
        logging::warn(&format!("托盘要切到不存在的供应商：{id}"));
        return;
    }
    cfg.active_provider = id.to_string();
    save(&cfg);
}

fn set_prompt(id: &str) {
    let mut cfg = Config::load();
    if cfg.prompt_by_id(id).is_none() {
        logging::warn(&format!("托盘要切到不存在的提示词：{id}"));
        return;
    }
    cfg.active_prompt = id.to_string();
    save(&cfg);
}

fn save(cfg: &Config) {
    if let Err(e) = cfg.save() {
        logging::error(&format!("托盘写配置失败：{e}"));
    }
}

/// 开机自启，直接用 `main.rs` 已经注册好的 autostart 插件。
///
/// 不自己写 desktop 文件：插件三平台各走各的原生机制（XDG autostart / LaunchAgent /
/// 注册表 Run 键），而且自启参数（`tray`）已经在注册时定好了，托盘再定义一遍迟早对不上。
mod autostart {
    use tauri::AppHandle;
    use tauri_plugin_autostart::ManagerExt;

    use seltrans_core::logging;

    pub fn is_enabled(app: &AppHandle) -> bool {
        match app.autolaunch().is_enabled() {
            Ok(v) => v,
            Err(e) => {
                // 读不到就当没开：菜单上少个勾比整个托盘起不来强
                logging::warn(&format!("读取自启状态失败：{e}"));
                false
            }
        }
    }

    pub fn set_enabled(app: &AppHandle, on: bool) {
        let m = app.autolaunch();
        let r = if on { m.enable() } else { m.disable() };
        match r {
            Ok(()) => logging::info(if on {
                "已开启开机自启动"
            } else {
                "已关闭开机自启动"
            }),
            Err(e) => logging::error(&format!("设置开机自启动失败：{e}")),
        }
    }
}

// ================================================================ Linux：ksni

#[cfg(target_os = "linux")]
mod imp {
    use ksni::menu::{CheckmarkItem, StandardItem, SubMenu};
    use ksni::{Icon, MenuItem, ToolTip, Tray, TrayMethods};
    use tauri::AppHandle;

    use seltrans_core::logging;

    use super::{Action, Snapshot, autostart, dispatch, set_prompt, set_provider};

    pub type Inner = ksni::Handle<SelTray>;

    const TITLE: &str = "划词翻译";

    pub fn spawn(app: &AppHandle) -> Result<Inner, String> {
        let icons = render_icons();
        if icons.is_empty() {
            logging::warn("托盘图标光栅化失败，退回按名字查找主题图标");
        }
        let tray = SelTray {
            app: app.clone(),
            snap: Snapshot::load(app),
            icons,
        };

        // 在 Tauri 的全局 tokio 运行时上注册。用 block_on 而不是 spawn，是为了让
        // 「面板没有 StatusNotifierWatcher」这种失败当场返回给调用方；这一步只是一次
        // D-Bus 握手，不会明显拖慢启动。block_on 里 spawn 出来的 ksni 后台任务会留在
        // 同一个运行时上继续跑。
        let handle = tauri::async_runtime::block_on(tray.spawn())
            .map_err(|e| format!("托盘注册失败（面板可能没有提供 StatusNotifierWatcher）：{e}"))?;
        logging::info("托盘图标已注册（ksni / StatusNotifierItem）");
        Ok(handle)
    }

    pub fn refresh(handle: &Inner) {
        let h = handle.clone();
        // update 是异步的，丢给运行时就行；调用方不需要等它画完。
        // 快照在闭包里现取 —— 那时候才是真正写进托盘的时刻。
        tauri::async_runtime::spawn(async move {
            h.update(move |t: &mut SelTray| t.reload()).await;
        });
    }

    pub struct SelTray {
        app: AppHandle,
        snap: Snapshot,
        icons: Vec<Icon>,
    }

    impl SelTray {
        /// 改完配置后立刻重取快照。
        /// ksni 会在回调返回后 diff 菜单，所以这里改完就等于菜单跟着变了。
        fn reload(&mut self) {
            self.snap = Snapshot::load(&self.app);
        }
    }

    impl Tray for SelTray {
        fn id(&self) -> String {
            "seltrans".into()
        }

        fn title(&self) -> String {
            TITLE.into()
        }

        /// 只有内嵌位图渲染不出来时才退回按名字找主题图标
        fn icon_name(&self) -> String {
            if self.icons.is_empty() {
                "xyz.brownie.SelectionTranslation".into()
            } else {
                String::new()
            }
        }

        fn icon_pixmap(&self) -> Vec<Icon> {
            self.icons.clone()
        }

        fn icon_theme_path(&self) -> String {
            icon_dir()
        }

        fn tool_tip(&self) -> ToolTip {
            ToolTip {
                icon_name: String::new(),
                icon_pixmap: self.icons.clone(),
                title: TITLE.into(),
                description: self.snap.tooltip(),
            }
        }

        /// 左键点图标 = 打开输入框。
        /// 不是「翻译选中文本」：会去点托盘图标，通常就意味着当下没选中任何东西，
        /// 那样只会得到一句「没取到文本」。
        fn activate(&mut self, _x: i32, _y: i32) {
            dispatch(&self.app, Action::Input);
        }

        /// 中键 = 翻译当前选中的文本
        fn secondary_activate(&mut self, _x: i32, _y: i32) {
            dispatch(&self.app, Action::Translate);
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            let s = &self.snap;

            let prompt_items: Vec<MenuItem<Self>> = s
                .prompts
                .iter()
                .map(|(id, label)| {
                    let id = id.clone();
                    CheckmarkItem {
                        label: label.clone(),
                        checked: *id == s.active_prompt,
                        activate: Box::new(move |t: &mut Self| {
                            set_prompt(&id);
                            t.reload();
                        }),
                        ..Default::default()
                    }
                    .into()
                })
                .collect();

            let provider_items: Vec<MenuItem<Self>> = if s.providers.is_empty() {
                vec![
                    StandardItem {
                        label: "还没有配置供应商".into(),
                        enabled: false,
                        ..Default::default()
                    }
                    .into(),
                ]
            } else {
                s.providers
                    .iter()
                    .map(|(id, name)| {
                        let id = id.clone();
                        CheckmarkItem {
                            label: name.clone(),
                            checked: *id == s.active_provider,
                            activate: Box::new(move |t: &mut Self| {
                                set_provider(&id);
                                t.reload();
                            }),
                            ..Default::default()
                        }
                        .into()
                    })
                    .collect()
            };

            vec![
                StandardItem {
                    label: s.headline(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
                StandardItem {
                    label: s.subhead(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
                MenuItem::Separator,
                StandardItem {
                    label: "打开输入框翻译".into(),
                    icon_name: "document-edit-symbolic".into(),
                    activate: Box::new(|t: &mut Self| dispatch(&t.app, Action::Input)),
                    ..Default::default()
                }
                .into(),
                StandardItem {
                    label: "翻译选中文本".into(),
                    icon_name: "accessories-dictionary".into(),
                    activate: Box::new(|t: &mut Self| dispatch(&t.app, Action::Translate)),
                    ..Default::default()
                }
                .into(),
                SubMenu {
                    label: "翻译风格".into(),
                    submenu: prompt_items,
                    ..Default::default()
                }
                .into(),
                SubMenu {
                    label: "供应商".into(),
                    submenu: provider_items,
                    ..Default::default()
                }
                .into(),
                MenuItem::Separator,
                StandardItem {
                    label: "设置…".into(),
                    icon_name: "emblem-system-symbolic".into(),
                    activate: Box::new(|t: &mut Self| dispatch(&t.app, Action::Settings)),
                    ..Default::default()
                }
                .into(),
                StandardItem {
                    label: "查看日志".into(),
                    icon_name: "text-x-generic-symbolic".into(),
                    activate: Box::new(|t: &mut Self| dispatch(&t.app, Action::OpenLog)),
                    ..Default::default()
                }
                .into(),
                CheckmarkItem {
                    label: "开机自启动".into(),
                    checked: s.autostart,
                    activate: Box::new(|t: &mut Self| {
                        autostart::set_enabled(&t.app, !t.snap.autostart);
                        t.reload();
                    }),
                    ..Default::default()
                }
                .into(),
                MenuItem::Separator,
                StandardItem {
                    label: "退出".into(),
                    icon_name: "application-exit-symbolic".into(),
                    activate: Box::new(|t: &mut Self| dispatch(&t.app, Action::Quit)),
                    ..Default::default()
                }
                .into(),
            ]
        }
    }

    /// 位图渲染不出来时的兜底：告诉面板去哪个目录按名字找。
    ///
    /// 用 `dirs` 而不是手写 XDG —— 同一份代码里 `config.rs` / `logging.rs` 都是这个规矩。
    fn icon_dir() -> String {
        dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("icons/hicolor/scalable/apps")
            .display()
            .to_string()
    }

    /// 把内置 SVG 光栅化成 SNI 要的几档尺寸（理由见模块头）。
    ///
    /// 用 resvg 而不是 GTK 版那样用 GdkPixbuf：这个 crate 里没有 GTK，也不值得为一个
    /// 图标把它拖回来。
    fn render_icons() -> Vec<Icon> {
        // 面板从这几档里挑最合适的，多给几个免得被拉伸糊掉。
        //
        // **22 / 32 用另一套构图**：应用图标是按 128 设计的，缩到 22px 时外层安全边距
        // 吃掉两侧各 1.4 像素，选区条里那三个字形总共只剩约 12 个物理像素 —— 每字 4 像素，
        // 「文」必然糊成一团，四角括号退化成噪点。实测确认过。理由见 data/tray-small.svg。
        [22u32, 32, 48, 64]
            .iter()
            .filter_map(|&size| {
                let svg = if size <= 32 {
                    super::icons::SMALL
                } else {
                    super::icons::APP
                };
                render_one(svg, size)
            })
            .collect()
    }

    fn render_one(svg: &[u8], size: u32) -> Option<Icon> {
        let rgba = super::rasterize(svg, size)?;
        // 共享的光栅化给的是不预乘 RGBA；SNI 要的是大端 ARGB，重排一下
        // （GTK 版喂给面板的 GdkPixbuf 数据就是不预乘的，面板按那个来）
        let mut data = Vec::with_capacity(rgba.len());
        for px in rgba.as_chunks::<4>().0 {
            data.extend_from_slice(&[px[3], px[0], px[1], px[2]]);
        }
        Some(Icon {
            width: size as i32,
            height: size as i32,
            data,
        })
    }
}

// ================================================ mac / Windows：Tauri 内置托盘

#[cfg(not(target_os = "linux"))]
mod imp {
    use tauri::AppHandle;
    use tauri::image::Image;
    use tauri::menu::{
        CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
    };
    use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    use seltrans_core::logging;

    use super::{Action, Snapshot, autostart, dispatch, set_prompt, set_provider};

    /// 不存 `TrayIcon` 本身：它是 `!Send`，而且 Tauri 的 manager 已经替我们持有一份
    /// （`build()` 会把它登记进去，`tray_by_id` 就是从那儿取的）。存 `AppHandle` 反而
    /// 让句柄是 `Send + Sync`，能塞进全局槽。
    pub type Inner = AppHandle;

    const TRAY_ID: &str = "seltrans";

    /// Windows 用彩色位图，`icons/32x32.png` 打包时本来就在。
    #[cfg(not(target_os = "macos"))]
    const ICON_PNG: &[u8] = include_bytes!("../icons/32x32.png");

    // 菜单项 id。带前缀的两类是动态项，id 里带着要切到哪个 provider / prompt。
    const ID_INPUT: &str = "input";
    const ID_TRANSLATE: &str = "translate";
    const ID_SETTINGS: &str = "settings";
    const ID_LOG: &str = "log";
    const ID_AUTOSTART: &str = "autostart";
    const ID_QUIT: &str = "quit";
    const PREFIX_PROMPT: &str = "prompt:";
    const PREFIX_PROVIDER: &str = "provider:";

    /// 托盘图标。Windows 直接解 PNG；macOS 现场把单色 SVG 光栅化成 RGBA。
    ///
    /// mac 这边不预先烤一张 PNG 进仓库：几何数据已经在 SVG 里了，再存一份位图就是两个
    /// 事实源，改图标时必然漏改一个。Linux 那边（ksni）本来就是现场光栅化的，这里用的
    /// 是同一套 resvg，只是终点从 SNI 的 ARGB 换成 Tauri 的 RGBA。
    #[cfg(not(target_os = "macos"))]
    fn tray_image() -> Result<Image<'static>, String> {
        Image::from_bytes(ICON_PNG).map_err(|e| format!("托盘图标解码失败：{e}"))
    }

    /// 菜单栏图标按 22pt 排版，Retina 下要 2 倍像素才不糊
    #[cfg(target_os = "macos")]
    const MONO_SIZE: u32 = 44;

    /// 把单色 SVG 光栅化成不预乘的 RGBA。
    ///
    /// 薄薄一层包装，为的是给它一个能被单测抓住的名字 —— 模板图标画糊了
    /// （mask 没生效导致全透明之类）在菜单栏上看不出来，只会"图标消失"。
    #[cfg(target_os = "macos")]
    fn mono_rgba() -> Result<Vec<u8>, String> {
        super::rasterize(super::icons::MONO, MONO_SIZE).ok_or("单色托盘图标光栅化失败".to_string())
    }

    #[cfg(target_os = "macos")]
    fn tray_image() -> Result<Image<'static>, String> {
        Ok(Image::new_owned(mono_rgba()?, MONO_SIZE, MONO_SIZE))
    }

    pub fn spawn(app: &AppHandle) -> Result<Inner, String> {
        let snap = Snapshot::load(app);
        let menu = build_menu(app, &snap)?;
        let icon = tray_image()?;

        TrayIconBuilder::with_id(TRAY_ID)
            .icon(icon)
            .tooltip(snap.tooltip())
            .menu(&menu)
            // 左键留给「打开输入框」，右键才弹菜单
            .show_menu_on_left_click(false)
            // macOS 上用简化模板图标，系统会按菜单栏明暗自己上色；另两家用彩色图。
            // 不能拿彩色应用图标直接当模板：模板只认 alpha，会丢失内部颜色层级。
            .icon_as_template(cfg!(target_os = "macos"))
            .on_menu_event(on_menu_event)
            .on_tray_icon_event(on_icon_event)
            .build(app)
            .map_err(|e| format!("托盘注册失败：{e}"))?;

        logging::info("托盘图标已注册（Tauri 内置）");
        Ok(app.clone())
    }

    pub fn refresh(handle: &Inner) {
        let app = handle.clone();
        // 菜单在 Windows 上必须在主线程上动，统一跳一下最省心
        if let Err(e) = handle.run_on_main_thread(move || rebuild(&app)) {
            logging::error(&format!("托盘刷新没能派发到主线程：{e}"));
        }
    }

    /// 整张菜单重建，而不是只改勾选状态 —— 供应商 / 提示词列表本身也会变，
    /// 差量更新要写的判断比重建还多，而菜单统共十几项。
    fn rebuild(app: &AppHandle) {
        let Some(tray) = app.tray_by_id(TRAY_ID) else {
            logging::warn("托盘刷新时找不到托盘图标");
            return;
        };
        let snap = Snapshot::load(app);
        match build_menu(app, &snap) {
            Ok(m) => {
                if let Err(e) = tray.set_menu(Some(m)) {
                    logging::error(&format!("托盘菜单更新失败：{e}"));
                }
            }
            Err(e) => logging::error(&format!("托盘菜单重建失败：{e}")),
        }
        if let Err(e) = tray.set_tooltip(Some(snap.tooltip())) {
            logging::error(&format!("托盘提示更新失败：{e}"));
        }
    }

    fn build_menu(app: &AppHandle, s: &Snapshot) -> Result<Menu<tauri::Wry>, String> {
        let err = |e: tauri::Error| format!("构造托盘菜单失败：{e}");

        // 顶上两行只是信息展示，禁用掉免得看起来能点
        let headline = MenuItem::new(app, s.headline(), false, None::<&str>).map_err(err)?;
        let subhead = MenuItem::new(app, s.subhead(), false, None::<&str>).map_err(err)?;

        let input =
            MenuItem::with_id(app, ID_INPUT, "打开输入框翻译", true, None::<&str>).map_err(err)?;
        let translate = MenuItem::with_id(app, ID_TRANSLATE, "翻译选中文本", true, None::<&str>)
            .map_err(err)?;

        let prompt_items = s
            .prompts
            .iter()
            .map(|(id, label)| {
                CheckMenuItem::with_id(
                    app,
                    format!("{PREFIX_PROMPT}{id}"),
                    label,
                    true,
                    *id == s.active_prompt,
                    None::<&str>,
                )
            })
            .collect::<tauri::Result<Vec<_>>>()
            .map_err(err)?;
        let prompt_refs: Vec<&dyn IsMenuItem<tauri::Wry>> = prompt_items
            .iter()
            .map(|i| i as &dyn IsMenuItem<tauri::Wry>)
            .collect();
        let prompt_menu = Submenu::with_items(app, "翻译风格", true, &prompt_refs).map_err(err)?;

        let provider_items = s
            .providers
            .iter()
            .map(|(id, name)| {
                CheckMenuItem::with_id(
                    app,
                    format!("{PREFIX_PROVIDER}{id}"),
                    name,
                    true,
                    *id == s.active_provider,
                    None::<&str>,
                )
            })
            .collect::<tauri::Result<Vec<_>>>()
            .map_err(err)?;
        let empty_hint =
            MenuItem::new(app, "还没有配置供应商", false, None::<&str>).map_err(err)?;
        let provider_refs: Vec<&dyn IsMenuItem<tauri::Wry>> = if provider_items.is_empty() {
            vec![&empty_hint]
        } else {
            provider_items
                .iter()
                .map(|i| i as &dyn IsMenuItem<tauri::Wry>)
                .collect()
        };
        let provider_menu =
            Submenu::with_items(app, "供应商", true, &provider_refs).map_err(err)?;

        let settings =
            MenuItem::with_id(app, ID_SETTINGS, "设置…", true, None::<&str>).map_err(err)?;
        let log = MenuItem::with_id(app, ID_LOG, "查看日志", true, None::<&str>).map_err(err)?;
        let auto = CheckMenuItem::with_id(
            app,
            ID_AUTOSTART,
            "开机自启动",
            true,
            s.autostart,
            None::<&str>,
        )
        .map_err(err)?;
        let quit = MenuItem::with_id(app, ID_QUIT, "退出", true, None::<&str>).map_err(err)?;
        let sep1 = PredefinedMenuItem::separator(app).map_err(err)?;
        let sep2 = PredefinedMenuItem::separator(app).map_err(err)?;
        let sep3 = PredefinedMenuItem::separator(app).map_err(err)?;

        Menu::with_items(
            app,
            &[
                &headline,
                &subhead,
                &sep1,
                &input,
                &translate,
                &prompt_menu,
                &provider_menu,
                &sep2,
                &settings,
                &log,
                &auto,
                &sep3,
                &quit,
            ],
        )
        .map_err(err)
    }

    /// 菜单事件本来就是事件循环派发的，已经在主线程上；改配置的分支就地做完再重建菜单。
    fn on_menu_event(app: &AppHandle, ev: MenuEvent) {
        let id = ev.id().as_ref();
        match id {
            ID_INPUT => dispatch(app, Action::Input),
            ID_TRANSLATE => dispatch(app, Action::Translate),
            ID_SETTINGS => dispatch(app, Action::Settings),
            ID_LOG => dispatch(app, Action::OpenLog),
            ID_QUIT => dispatch(app, Action::Quit),
            ID_AUTOSTART => {
                // 菜单上勾没勾不能当真（点击已经把它翻过来了），以磁盘状态为准
                let on = !autostart::is_enabled(app);
                autostart::set_enabled(app, on);
                rebuild(app);
            }
            _ => {
                if let Some(pid) = id.strip_prefix(PREFIX_PROMPT) {
                    set_prompt(pid);
                    rebuild(app);
                } else if let Some(pid) = id.strip_prefix(PREFIX_PROVIDER) {
                    set_provider(pid);
                    rebuild(app);
                } else {
                    logging::warn(&format!("托盘收到不认识的菜单项：{id}"));
                }
            }
        }
    }

    /// 左键 = 打开输入框，中键 = 翻译选中文本。理由同 Linux 分支的 `activate`。
    /// 只认「松开」那一下：按下就触发的话，拖动托盘图标会误开窗口。
    fn on_icon_event(tray: &TrayIcon, ev: TrayIconEvent) {
        if let TrayIconEvent::Click {
            button,
            button_state: MouseButtonState::Up,
            ..
        } = ev
        {
            match button {
                MouseButton::Left => dispatch(tray.app_handle(), Action::Input),
                MouseButton::Middle => dispatch(tray.app_handle(), Action::Translate),
                MouseButton::Right => {}
            }
        }
    }
}

/// 图标的光栅化。
///
/// 为什么值得单测：这三份图**画坏了都只表现成「图标不见了 / 很怪」**，在托盘或菜单栏
/// 上根本看不出是哪一步出的问题，很难往光栅化上想。而且 `SMALL` 只有 Linux 构建会用，
/// 没有测试的话它在别的机器上就是一段谁也验不了的资源。
///
/// 尺寸取的是**真实使用尺寸**，不是随便挑的：22 是 Linux 面板和 mac 菜单栏的实际点数，
/// 44 是 Retina 下菜单栏的像素数。在放大图上什么都好看，问题只在真实尺寸下暴露。
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod icon_tests {
    use super::*;

    /// 有多少像素基本不透明 / 基本透明
    fn profile(rgba: &[u8]) -> (usize, usize, usize) {
        let total = rgba.len() / 4;
        let opaque = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 128)
            .count();
        let clear = rgba.as_chunks::<4>().0.iter().filter(|p| p[3] < 32).count();
        (total, opaque, clear)
    }

    /// 每份图在自己的真实尺寸下都要「有内容」，不能是一片空白。
    #[test]
    fn 三份图标在真实尺寸下都画得出内容() {
        for (name, svg, size) in [
            ("应用图标", icons::APP, 48u32),
            ("托盘小尺寸", icons::SMALL, 22),
            ("菜单栏单色", icons::MONO, 44),
        ] {
            let rgba = rasterize(svg, size).unwrap_or_else(|| panic!("{name} 光栅化失败"));
            let (total, opaque, _) = profile(&rgba);
            assert!(
                opaque * 10 > total,
                "{name} 在 {size}px 下不透明像素只有 {opaque}/{total}，基本是空白的"
            );
        }
    }

    /// 模板图标只认 alpha：全不透明说明 mask 没生效，菜单栏上会变成一坨黑块。
    #[test]
    fn 菜单栏单色图的箭头必须是镂空的() {
        let rgba = rasterize(icons::MONO, 44).expect("光栅化失败");
        let (total, _, clear) = profile(&rgba);
        assert!(
            clear * 3 > total,
            "透明像素只有 {clear}/{total}，箭头没被挖空，模板图标会变成黑块"
        );
    }

    /// 小尺寸那份的存在理由就是「22px 下还读得清」。箭头镂空低于 2 像素就不再是洞，
    /// 只是一道灰边 —— 用镂空面积占比兜住这条，别让后来的改动把它调回细线。
    #[test]
    fn 托盘小尺寸图在_22px_下箭头仍是可辨的镂空() {
        let rgba = rasterize(icons::SMALL, 22).expect("光栅化失败");
        let total = rgba.len() / 4;
        // 深色底 + 薄荷色选区条 + 深色箭头：数"接近底色的暗像素"太脆，
        // 改数选区条那片亮像素 —— 箭头挖得越实，亮像素越少。给一个上下界。
        let mint = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 128 && p[1] > 150 && p[0] < 180)
            .count();
        assert!(
            mint * 12 > total,
            "选区条只剩 {mint}/{total} 像素，图形太小了"
        );
        assert!(
            mint * 3 < total,
            "选区条占了 {mint}/{total}，箭头没挖出来或太细"
        );
    }
}
