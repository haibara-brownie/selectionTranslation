//! 取词：把用户当前选中的文本抓过来。
//!
//! 这是全项目最依赖平台的一块，三家的路子完全不同：
//!
//! | 平台 | 首选 | 兜底 |
//! |---|---|---|
//! | Linux / Wayland | 主选区（`wl-paste --primary`），划完词就能读 | 模拟 Ctrl+C（ydotool） |
//! | macOS | 辅助功能 API 读 `kAXSelectedTextAttribute` | 模拟 ⌘C |
//! | Windows | 无通用接口 | 模拟 Ctrl+C |
//!
//! **三家共通的坑**：用户按快捷键的那一刻手还按着修饰键，这时候直接发 Ctrl+C，应用
//! 收到的是 `Super+Shift+Ctrl+C` —— 复制不到东西，还可能把合成器的修饰键状态搞乱
//! （表现为键盘"卡住"，Linux 上真踩过一次，整台电脑没法操作）。所以每个平台的兜底
//! 路径都必须：**先显式抬起所有修饰键，且无论中途出什么岔子都要再抬一次**。
//! 别因为"那个平台好像没这个问题"就省掉。
//!
//! 剪贴板还原都只还原纯文本 —— 富文本、图片还不回去，这是已知取舍。

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{deps_report, grab};

#[cfg(not(target_os = "linux"))]
mod unsupported;
#[cfg(not(target_os = "linux"))]
pub use unsupported::{deps_report, grab};
