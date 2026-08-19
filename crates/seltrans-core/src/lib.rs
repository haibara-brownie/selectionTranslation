//! seltrans 的核心逻辑：配置、预设、大模型调用、日志。
//!
//! 这里**不依赖任何 GUI 工具包** —— 界面层（现在是 GTK4，将来是 Tauri）都只是它的调用方。
//! 加东西进来前先问一句：mac 和 Windows 上这段代码还成立吗？平台相关的部分（取词、
//! 托盘、快捷键、自启）不属于这里。

pub mod config;
pub mod llm;
pub mod logging;
pub mod palette;
pub mod presets;
pub mod typography;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPO_URL: &str = "https://github.com/haibara-brownie/selectionTranslation";
