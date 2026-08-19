//! 开机自启动：往 `~/.config/autostart/` 里写一个 desktop 文件。
//!
//! 这台机器上 FlClash / Cherry Studio / cc-switch 用的就是这个机制
//! （systemd 的 `xdg-desktop-autostart.target` 会在图形会话起来后拉起它们），
//! 所以跟着走最省事，也不用改 niri 配置。

use std::path::PathBuf;

use crate::logging;

pub const FILE_NAME: &str = "xyz.brownie.SelectionTranslation.desktop";

pub fn autostart_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into())).join(".config")
        });
    base.join("autostart")
}

pub fn desktop_path() -> PathBuf {
    autostart_dir().join(FILE_NAME)
}

pub fn is_enabled() -> bool {
    match std::fs::read_to_string(desktop_path()) {
        // 有些工具用 X-GNOME-Autostart-enabled=false 来"软禁用"，这里也认
        Ok(s) => !s.contains("X-GNOME-Autostart-enabled=false") && !s.contains("Hidden=true"),
        Err(_) => false,
    }
}

/// 用绝对路径而不是裸 `seltrans` —— 自启动时的 PATH 未必包含 ~/.local/bin
fn exe_path() -> String {
    std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "seltrans".to_string())
}

pub fn set_enabled(on: bool) -> std::io::Result<()> {
    let path = desktop_path();
    if on {
        std::fs::create_dir_all(autostart_dir())?;
        let content = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=划词翻译（托盘常驻）\n\
             Name[en]=Selection Translation (tray)\n\
             Comment=常驻托盘，按 Mod+Shift+T 立即翻译选中的文本\n\
             Exec={exe} tray\n\
             Icon=xyz.brownie.SelectionTranslation\n\
             Terminal=false\n\
             NoDisplay=true\n\
             X-GNOME-Autostart-enabled=true\n",
            exe = exe_path()
        );
        std::fs::write(&path, content)?;
        logging::info(&format!("已开启开机自启动：{}", path.display()));
    } else if path.exists() {
        std::fs::remove_file(&path)?;
        logging::info("已关闭开机自启动");
    }
    Ok(())
}
