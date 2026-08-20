//! 设置界面用得到的命令。
//!
//! 和 `cmds.rs` 一样是薄搬运层：业务逻辑在 `seltrans-core`，这里只做类型转换和
//! 错误转字符串。前端拿到的字段是 camelCase（见各结构的 `rename_all`）。

use serde::{Deserialize, Serialize};

use seltrans_core::config::{Config, Prompt, Provider};
use seltrans_core::{llm, logging, palette, presets, selection, system_fonts, typography};

// ---------- 配置读写 ----------

/// 前端看到的完整配置。
///
/// 不直接把 `core::Config` 序列化出去，是因为那边的字段名是 snake_case、还带
/// `#[serde(default)]` 这些只跟磁盘格式有关的东西。界面和磁盘格式解耦，改一边不会
/// 意外动到另一边。
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiConfig {
    pub providers: Vec<UiProvider>,
    pub active_provider: String,
    pub prompts: Vec<UiPrompt>,
    pub active_prompt: String,
    pub target_lang: String,
    pub selection_mode: String,
    pub theme: String,
    pub font_latin: String,
    pub font_cjk: String,
    pub font_fallback: String,
    pub popup_width: i32,
    pub popup_height: i32,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UiProvider {
    pub id: String,
    pub name: String,
    pub preset: String,
    pub kind: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub models: Vec<String>,
    pub extra_body: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UiPrompt {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub system: String,
}

impl From<&Provider> for UiProvider {
    fn from(p: &Provider) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            preset: p.preset.clone(),
            kind: p.kind.clone(),
            base_url: p.base_url.clone(),
            api_key: p.api_key.clone(),
            model: p.model.clone(),
            models: p.models.clone(),
            extra_body: p.extra_body.clone(),
        }
    }
}

impl From<UiProvider> for Provider {
    fn from(p: UiProvider) -> Self {
        Self {
            id: p.id,
            name: p.name,
            preset: p.preset,
            kind: p.kind,
            base_url: p.base_url,
            api_key: p.api_key,
            model: p.model,
            models: p.models,
            extra_body: p.extra_body,
        }
    }
}

impl From<&Prompt> for UiPrompt {
    fn from(p: &Prompt) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            icon: p.icon.clone(),
            system: p.system.clone(),
        }
    }
}

impl From<UiPrompt> for Prompt {
    fn from(p: UiPrompt) -> Self {
        Self {
            id: p.id,
            name: p.name,
            icon: p.icon,
            system: p.system,
        }
    }
}

#[tauri::command]
pub fn load_config() -> UiConfig {
    let c = Config::load();
    UiConfig {
        providers: c.providers.iter().map(UiProvider::from).collect(),
        active_provider: c.active_provider.clone(),
        prompts: c.prompts.iter().map(UiPrompt::from).collect(),
        active_prompt: c.active_prompt.clone(),
        target_lang: c.target_lang.clone(),
        selection_mode: c.selection_mode.clone(),
        theme: c.theme.clone(),
        font_latin: c.font_latin.clone(),
        font_cjk: c.font_cjk.clone(),
        font_fallback: c.font_fallback.clone(),
        popup_width: c.popup_width,
        popup_height: c.popup_height,
    }
}

#[tauri::command]
pub fn save_config(config: UiConfig) -> Result<(), String> {
    // 先读一遍再覆盖，保住界面上没有的字段（比如将来加的、或旧版本留下的）
    let mut c = Config::load();
    c.providers = config.providers.into_iter().map(Provider::from).collect();
    c.active_provider = config.active_provider;
    c.prompts = config.prompts.into_iter().map(Prompt::from).collect();
    c.active_prompt = config.active_prompt;
    c.target_lang = config.target_lang;
    c.selection_mode = config.selection_mode;
    c.theme = config.theme;
    c.font_latin = config.font_latin;
    c.font_cjk = config.font_cjk;
    c.font_fallback = config.font_fallback;
    c.popup_width = config.popup_width;
    c.popup_height = config.popup_height;
    c.save().map_err(|e| format!("配置写入失败：{e}"))?;

    // 托盘菜单里列着供应商和提示词，还带当前项的勾选 —— 不刷新的话，
    // 用户在设置页里改完，托盘上还是旧的，点一下就切回去了
    crate::tray::refresh_now();
    Ok(())
}

// ---------- 外观 ----------

#[tauri::command]
pub fn theme_css(system_dark: bool) -> String {
    crate::state::css_for(&Config::load(), system_dark)
}

#[tauri::command]
pub fn theme_choices() -> Vec<(String, String)> {
    palette::CHOICES
        .iter()
        .map(|(v, l)| (v.to_string(), l.to_string()))
        .collect()
}

#[tauri::command]
pub fn list_fonts() -> Vec<String> {
    system_fonts::families()
}

/// 这个字体家族自己有没有汉字字形。
///
/// 界面拿它来警告「你选的中文字体其实没有汉字」—— 真实踩过的坑是 `HarmonyOS Sans`
/// 是纯拉丁族，带汉字的是 `HarmonyOS Sans SC`。选错了汉字会落到系统默认字体。
/// 详见 docs/adr/0001。
#[tauri::command]
pub fn font_covers_cjk(family: String) -> bool {
    system_fonts::covers_cjk(&family)
}

/// 单独暴露一下，方便前端做「这段文字里有没有汉字」这类判断
#[tauri::command]
pub fn has_cjk(text: String) -> bool {
    text.chars().any(typography::is_cjk)
}

/// 当前平台（"linux" / "macos" / "windows"）。
///
/// 界面上好几处要按平台分叉（mac/Windows 没有主选区、快捷键来路不同）。
/// 单独开一个命令是因为 `about_info` 会顺带做依赖自检和 stat 日志文件，
/// 为了拿一个字符串付那个成本不划算。
#[tauri::command]
pub fn platform() -> &'static str {
    std::env::consts::OS
}

// ---------- 预设 ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiProviderPreset {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub keys_url: String,
    pub hint: String,
}

#[tauri::command]
pub fn provider_presets() -> Vec<UiProviderPreset> {
    presets::PROVIDER_PRESETS
        .iter()
        .map(|p| UiProviderPreset {
            id: p.id.to_string(),
            name: p.name.to_string(),
            kind: p.kind.to_string(),
            base_url: p.base_url.to_string(),
            keys_url: p.keys_url.to_string(),
            hint: p.hint.to_string(),
        })
        .collect()
}

#[tauri::command]
pub fn prompt_presets() -> Vec<UiPrompt> {
    presets::PROMPT_PRESETS
        .iter()
        .map(|p| UiPrompt {
            id: p.id.to_string(),
            name: p.name.to_string(),
            icon: p.icon.to_string(),
            system: p.system.to_string(),
        })
        .collect()
}

#[tauri::command]
pub fn target_langs() -> Vec<(String, String)> {
    presets::TARGET_LANGS
        .iter()
        .map(|(v, l)| (v.to_string(), l.to_string()))
        .collect()
}

// ---------- 供应商联调 ----------

#[tauri::command]
pub async fn list_models(provider: UiProvider) -> Result<Vec<String>, String> {
    llm::list_models(Provider::from(provider)).await
}

#[tauri::command]
pub async fn test_connection(provider: UiProvider) -> Result<String, String> {
    llm::test_connection(Provider::from(provider)).await
}

// ---------- 关于 ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DepCheck {
    pub name: String,
    pub ok: bool,
    pub note: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutInfo {
    pub version: String,
    pub repo_url: String,
    pub config_path: String,
    pub log_path: String,
    pub log_size_kb: u64,
    pub os: String,
    pub deps: Vec<DepCheck>,
}

#[tauri::command]
pub fn about_info() -> AboutInfo {
    let log = logging::log_path();
    AboutInfo {
        version: seltrans_core::VERSION.to_string(),
        repo_url: seltrans_core::REPO_URL.to_string(),
        config_path: seltrans_core::config::config_path().display().to_string(),
        log_path: log.display().to_string(),
        log_size_kb: std::fs::metadata(&log).map(|m| m.len() / 1024).unwrap_or(0),
        os: std::env::consts::OS.to_string(),
        deps: selection::deps_report()
            .into_iter()
            .map(|(name, ok, note)| DepCheck { name, ok, note })
            .collect(),
    }
}

/// 用系统默认程序打开一个路径或 URL（日志文件、仓库主页、申请 key 的页面）
#[tauri::command]
pub fn open_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| format!("打不开 {path}：{e}"))
}

// ---------- 开机自启 ----------

#[tauri::command]
pub fn autostart_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .is_enabled()
        .map_err(|e| format!("读取自启状态失败：{e}"))
}

#[tauri::command]
pub fn set_autostart(app: tauri::AppHandle, on: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let m = app.autolaunch();
    let r = if on { m.enable() } else { m.disable() };
    r.map_err(|e| format!("设置开机自启失败：{e}"))
}

/// 当前生效的两组全局快捷键：(翻译, 设置)。
///
/// 返回的是**生效值**而不是配置里的原始值 —— 配置留空表示"用内置默认"，
/// 界面上要显示的是用户实际按什么键，不是一个空框。
#[tauri::command]
pub fn hotkeys() -> (String, String) {
    crate::hotkey::current()
}

/// 改键：校验 → 落盘 → 立刻重新注册。
///
/// **刻意不走 `save_config`。** 那条路是防抖的（前端改一个字段 250ms 后才写），
/// 而改键需要当场知道成不成 —— 组合被别的程序占了、或者写法不合法，用户得马上看到，
/// 而不是过一会儿发现快捷键不好使。所以这里是同步的、带返回值的独立命令。
///
/// 传空串表示"恢复内置默认"。
#[tauri::command]
pub fn set_hotkeys(
    app: tauri::AppHandle,
    translate: String,
    settings: String,
) -> Result<(), String> {
    if !translate.trim().is_empty() && translate.trim() == settings.trim() {
        return Err("两个快捷键不能设成同一个组合".into());
    }

    let mut cfg = Config::load();
    let (old_t, old_s) = (cfg.hotkey_translate.clone(), cfg.hotkey_settings.clone());
    cfg.hotkey_translate = translate.trim().to_string();
    cfg.hotkey_settings = settings.trim().to_string();
    cfg.save().map_err(|e| format!("配置写入失败：{e}"))?;

    // 注册失败就把配置回滚。留着一份注册不上的配置，下次启动照样起不来，
    // 而那时候用户已经不记得自己改过什么了
    if let Err(e) = crate::hotkey::reload(&app) {
        let mut back = Config::load();
        back.hotkey_translate = old_t;
        back.hotkey_settings = old_s;
        let _ = back.save();
        let _ = crate::hotkey::reload(&app);
        return Err(e);
    }
    Ok(())
}
