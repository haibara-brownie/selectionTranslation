# 交付：三平台可用

- 完成日期：2026-08-21
- Windows 修复提交基线：`218db61`
- 状态：Linux / macOS / Windows 真机验证完成

## Windows 收尾

- GitHub v0.3.0 发版 workflow `32393582175` 成功，Windows MSI / NSIS 产物齐全。
- 原 v0.3.0 包发现 `settings providers` 在 Windows 永久白屏：
  `WebviewUrl::App` 的打包资源 PathBuf 被拼入 `?page=providers`，Windows 将其当非法文件名。
- 设置资源路径固定为 `settings.html`，目标页改由初始化数据传给前端。
- 设置页取词说明按平台分开：Windows 显示 UI Automation / `Ctrl+Insert`，macOS 显示
  辅助功能 / `⌘C`，Linux 保留主选区 / niri。
- Windows 与 macOS 的零副作用模式恢复可选；Windows 显示为“仅 UI Automation”。

## 真机结果

| 项目 | 结果 |
|---|---|
| NSIS 安装与覆盖升级 | 通过，安装后 exe 为 0.3.0 x64 |
| MSI administrative extraction | 通过，退出码 0，解出 0.3.0 x64 exe |
| tray / 单实例 / 默认全局快捷键 | 通过 |
| `settings providers` | 直接进入供应商页，无白屏 |
| 关设置窗口后继续常驻 | 通过 |
| 记事本 | UIA 直取 73 字符，剪贴板不变 |
| Chrome | UIA 直取 101 字符，剪贴板不变 |
| VS Code | 扫 10 层后 `Ctrl+Insert` 兜底，剪贴板还原 |
| Word | UIA 直取 75 字符，剪贴板不变 |
| 强制模拟复制 | `Ctrl+Insert` 命中，剪贴板还原 |

## 检查

- `cargo fmt --all --check`：通过
- `cargo clippy --workspace --exclude seltrans --all-targets -- -D warnings`：通过
- `cargo test --workspace --exclude seltrans`：32/32 通过
- `node --test ui/settings/platform-copy.test.mjs`：3/3 通过
- `pnpm build`：通过
- `pnpm tauri build --bundles msi,nsis`：通过

Windows 本机不能编译 Linux GTK 根包 `seltrans`，因此按 CI 的 Windows job 口径排除它；
Linux GTK 与 macOS 已在各自工作站通过真机回归。

## 已知限制

- 安装包未签名，SmartScreen 会提示；需要 OV/EV 证书才能根治。
- UIPI 会阻止普通权限的 seltrans 读取提权窗口；游戏等 Raw Input / DirectInput 场景可能
  忽略模拟按键。
- 模拟复制只能还原纯文本剪贴板，图片、文件列表和富文本不在保证范围内。
