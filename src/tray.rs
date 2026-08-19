//! 托盘图标（StatusNotifierItem over D-Bus）。
//!
//! DMS 的 Quickshell 提供 `org.kde.StatusNotifierWatcher`，跟 FlClash / Cherry Studio /
//! cc-switch 用的是同一套协议。
//!
//! ksni 的回调跑在自己的 tokio 任务上，不能直接碰 GTK 控件，所以统一通过
//! `async_channel` 把命令送回 GTK 主线程执行。

use ksni::menu::{CheckmarkItem, StandardItem, SubMenu};
use ksni::{Icon, MenuItem, ToolTip, Tray};

/// 图标直接编进二进制，不依赖文件是否装到位
const ICON_SVG: &[u8] = include_bytes!("../data/xyz.brownie.SelectionTranslation.svg");

/// 把内置 SVG 光栅化成 SNI 要求的 ARGB32（网络字节序）位图。
///
/// 为什么不只给 icon_name：面板（这里是 Quickshell）在自己启动时就把图标主题缓存住了，
/// 之后新装的图标它按名字找不到，只会显示首字母兜底。直接把像素通过 D-Bus 递过去
/// 就绕开了整个主题查找。
///
/// 必须在 GTK 主线程上调用一次，结果存下来给 ksni 用。
pub fn render_icons() -> Vec<Icon> {
    [22, 32, 48, 64]
        .iter()
        .filter_map(|&size| render_one(size))
        .collect()
}

fn render_one(size: i32) -> Option<Icon> {
    use gtk::gdk_pixbuf::PixbufLoader;
    use gtk::prelude::*;

    let loader = PixbufLoader::with_type("svg").ok()?;
    loader.set_size(size, size);
    loader.write(ICON_SVG).ok()?;
    loader.close().ok()?;
    let pb = loader.pixbuf()?;

    let w = pb.width();
    let h = pb.height();
    let rowstride = pb.rowstride() as usize;
    let channels = pb.n_channels() as usize;
    let src = unsafe { pb.pixels() };

    // GdkPixbuf 给的是 RGBA，SNI 要的是大端 ARGB
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = y * rowstride + x * channels;
            let r = src[i];
            let g = src[i + 1];
            let b = src[i + 2];
            let a = if channels == 4 { src[i + 3] } else { 255 };
            data.extend_from_slice(&[a, r, g, b]);
        }
    }

    Some(Icon {
        width: w,
        height: h,
        data,
    })
}

use crate::config::Config;
use crate::{APP_ID, autostart, logging};

/// 托盘点出来的动作，送回 GTK 主线程处理
#[derive(Debug, Clone)]
pub enum Cmd {
    Translate,
    Settings(Option<&'static str>),
    SetProvider(String),
    SetPrompt(String),
    SetAutostart(bool),
    OpenLog,
    Quit,
}

/// 渲染菜单需要的配置快照（ksni 的 Tray 必须 Send，不能持有 Config 的引用）
#[derive(Clone, Default)]
pub struct Snapshot {
    pub provider_name: String,
    pub model: String,
    pub prompt_label: String,
    pub target_lang: String,
    pub providers: Vec<(String, String)>, // (id, 名称)
    pub prompts: Vec<(String, String)>,   // (id, 带图标的标签)
    pub active_provider: String,
    pub active_prompt: String,
    pub autostart: bool,
}

impl Snapshot {
    pub fn from_config(cfg: &Config) -> Self {
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
            autostart: autostart::is_enabled(),
        }
    }
}

pub struct SelTray {
    tx: async_channel::Sender<Cmd>,
    pub snap: Snapshot,
    icons: Vec<Icon>,
}

impl SelTray {
    pub fn new(tx: async_channel::Sender<Cmd>, snap: Snapshot, icons: Vec<Icon>) -> Self {
        SelTray { tx, snap, icons }
    }

    fn send(&self, cmd: Cmd) {
        // 通道是 unbounded，try_send 不会阻塞；失败只可能是 GTK 侧已经退出
        if self.tx.try_send(cmd.clone()).is_err() {
            logging::warn(&format!("托盘命令没送出去（主线程已退出？）：{cmd:?}"));
        }
    }
}

/// 图标装在这里；同时通过 icon_theme_path 告诉面板去哪找，避免图标缓存没刷新时显示不出来
fn icon_dir() -> String {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into()))
                .join(".local/share")
        });
    base.join("icons/hicolor/scalable/apps")
        .display()
        .to_string()
}

impl Tray for SelTray {
    fn id(&self) -> String {
        "seltrans".into()
    }

    fn title(&self) -> String {
        "划词翻译".into()
    }

    /// 内嵌位图渲染不出来时才退回按名字找图标
    fn icon_name(&self) -> String {
        if self.icons.is_empty() {
            APP_ID.into()
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
            title: "划词翻译".into(),
            description: format!(
                "{} · {}\n风格：{}　目标语言：{}\nMod+Shift+T 翻译选中的文本",
                self.snap.provider_name, self.snap.model, self.snap.prompt_label, self.snap.target_lang
            ),
        }
    }

    /// 左键点图标 = 直接翻译当前选中的文本
    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(Cmd::Translate);
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
                    activate: Box::new(move |t: &mut Self| t.send(Cmd::SetPrompt(id.clone()))),
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
                            t.send(Cmd::SetProvider(id.clone()))
                        }),
                        ..Default::default()
                    }
                    .into()
                })
                .collect()
        };

        vec![
            StandardItem {
                label: format!("{} · {}", s.provider_name, s.model),
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: format!("{}　→　{}", s.prompt_label, s.target_lang),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "翻译选中文本".into(),
                icon_name: "accessories-dictionary".into(),
                activate: Box::new(|t: &mut Self| t.send(Cmd::Translate)),
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
                activate: Box::new(|t: &mut Self| t.send(Cmd::Settings(None))),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "查看日志".into(),
                icon_name: "text-x-generic-symbolic".into(),
                activate: Box::new(|t: &mut Self| t.send(Cmd::OpenLog)),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "开机自启动".into(),
                checked: s.autostart,
                activate: Box::new(|t: &mut Self| {
                    let next = !t.snap.autostart;
                    t.send(Cmd::SetAutostart(next));
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "退出".into(),
                icon_name: "application-exit-symbolic".into(),
                activate: Box::new(|t: &mut Self| t.send(Cmd::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// 常驻进程是否已经在跑：查 GTK 在会话总线上注册的 app-id 有没有被占用。
pub fn is_running() -> bool {
    use gtk::gio;
    use gtk::glib::variant::ToVariant;

    let Ok(conn) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
        return false;
    };
    conn.call_sync(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameHasOwner",
        Some(&(crate::APP_ID_POPUP,).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        1000,
        gio::Cancellable::NONE,
    )
    .ok()
    .and_then(|v| v.child_value(0).get::<bool>())
    .unwrap_or(false)
}
