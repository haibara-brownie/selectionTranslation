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
//!
//! **调用时机也有硬约束**：取词必须发生在译文窗口抢到焦点**之前**，否则模拟出来的
//! 复制键发给的是我们自己的窗口。所以上层是「先取词，再开窗」，不是「开窗后让前端
//! 去取」—— 见 `src-tauri/src/windows.rs`。

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{deps_report, grab};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{deps_report, grab};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{deps_report, grab};

// 三家之外（BSD 之类）：不假装能取词，明确说没实现
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub use unsupported::{deps_report, grab};
