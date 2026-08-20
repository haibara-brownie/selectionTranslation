//! 前端渲染首屏要的全部东西，一次性打包给它。
//!
//! 刻意做成"一个命令拿全"而不是让前端分五次问：窗口是按快捷键弹出来的，多一次
//! IPC 往返就多一分肉眼可见的延迟。

use serde::Serialize;

use seltrans_core::config::Config;
use seltrans_core::{palette, typography};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptOption {
    pub id: String,
    pub name: String,
    pub icon: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderOption {
    pub id: String,
    pub name: String,
    pub model: String,
    /// 上次拉取缓存下来的模型列表，给底部下拉用
    pub models: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    /// 直接可以塞进 <style> 的 CSS：调色板变量 + 字体分档
    pub css: String,
    pub prompts: Vec<PromptOption>,
    pub active_prompt: String,
    pub providers: Vec<ProviderOption>,
    pub active_provider: String,
    pub target_lang: String,
    /// 一个供应商都没配时前端要引导用户去设置，而不是干等
    pub configured: bool,
    /// 首次使用提示还没被关掉。前端据此决定要不要盖那一层。
    pub onboarding: bool,
    /// 当前生效的两组全局快捷键，(翻译, 设置)。
    ///
    /// **首次提示里的快捷键必须来自这里，不能在前端写死**：三个平台默认值不同，
    /// 用户还能自己改 —— 一份说谎的教程比没有更糟。
    pub hotkeys: (String, String),
}

/// 主题设成"跟随系统"时，用哪个风味。
///
/// GTK 版靠 libadwaita 的 StyleManager 拿系统明暗，Tauri 这边前端能直接用
/// `prefers-color-scheme`，所以由前端把结果回传，Rust 只管按 id 出 CSS。
pub fn css_for(cfg: &Config, system_dark: bool) -> String {
    let flavor = match cfg.theme.as_str() {
        "system" | "" => palette::for_system(system_dark),
        id => palette::flavor_by_id(id).unwrap_or_else(|| palette::for_system(system_dark)),
    };

    let mut css = palette::css_variables(flavor);
    css.push('\n');
    css.push_str(&typography::font_css(
        &cfg.font_latin,
        &cfg.font_cjk,
        &cfg.font_fallback,
    ));
    css
}

impl UiState {
    pub fn build(cfg: &Config, system_dark: bool) -> Self {
        Self {
            css: css_for(cfg, system_dark),
            prompts: cfg
                .prompts
                .iter()
                .map(|p| PromptOption {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    icon: p.icon.clone(),
                })
                .collect(),
            active_prompt: cfg.active_prompt.clone(),
            providers: cfg
                .providers
                .iter()
                .map(|p| ProviderOption {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    model: p.model.clone(),
                    models: p.models.clone(),
                })
                .collect(),
            active_provider: cfg.active_provider.clone(),
            target_lang: cfg.target_lang.clone(),
            configured: !cfg.providers.is_empty(),
            onboarding: !cfg.onboarded,
            hotkeys: crate::hotkey::current(),
        }
    }
}
