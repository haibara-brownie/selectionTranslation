//! 配置读写：`~/.config/seltrans/config.json`，权限 0600（里面存 API key）。

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::presets;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Provider {
    pub id: String,
    pub name: String,
    /// 来源预设 id，见 presets::PROVIDER_PRESETS
    pub preset: String,
    /// "openai" | "anthropic"
    pub kind: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    /// 上次「拉取模型」的结果，仅作下拉缓存
    #[serde(default)]
    pub models: Vec<String>,
    /// 附加请求体（JSON 对象字符串），会合并进请求。例如 {"reasoning_effort":"none"}
    #[serde(default)]
    pub extra_body: String,
}

impl Provider {
    pub fn is_anthropic(&self) -> bool {
        self.kind == "anthropic"
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Prompt {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub system: String,
}

impl Prompt {
    pub fn label(&self) -> String {
        if self.icon.is_empty() {
            self.name.clone()
        } else {
            format!("{} {}", self.icon, self.name)
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub providers: Vec<Provider>,
    #[serde(default)]
    pub active_provider: String,
    #[serde(default = "builtin_prompts")]
    pub prompts: Vec<Prompt>,
    #[serde(default = "default_prompt_id")]
    pub active_prompt: String,
    #[serde(default = "default_lang")]
    pub target_lang: String,
    /// "auto" | "primary" | "clipboard"
    #[serde(default = "default_sel_mode")]
    pub selection_mode: String,
    /// system / latte / frappe / macchiato / mocha
    #[serde(default = "default_theme")]
    pub theme: String,
    /// 三档字体，留空表示用系统默认
    #[serde(default)]
    pub font_latin: String,
    #[serde(default)]
    pub font_cjk: String,
    #[serde(default)]
    pub font_fallback: String,
    #[serde(default = "default_w")]
    pub popup_width: i32,
    #[serde(default = "default_h")]
    pub popup_height: i32,
}

fn default_version() -> u32 {
    1
}
fn default_prompt_id() -> String {
    "general".into()
}
fn default_lang() -> String {
    "简体中文".into()
}
fn default_sel_mode() -> String {
    "auto".into()
}
fn default_theme() -> String {
    "system".into()
}
fn default_w() -> i32 {
    560
}
fn default_h() -> i32 {
    480
}

pub fn builtin_prompts() -> Vec<Prompt> {
    presets::PROMPT_PRESETS
        .iter()
        .map(|p| Prompt {
            id: p.id.to_string(),
            name: p.name.to_string(),
            icon: p.icon.to_string(),
            system: p.system.to_string(),
        })
        .collect()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: default_version(),
            providers: Vec::new(),
            active_provider: String::new(),
            prompts: builtin_prompts(),
            active_prompt: default_prompt_id(),
            target_lang: default_lang(),
            selection_mode: default_sel_mode(),
            theme: default_theme(),
            font_latin: String::new(),
            font_cjk: String::new(),
            font_fallback: String::new(),
            popup_width: default_w(),
            popup_height: default_h(),
        }
    }
}

impl Config {
    pub fn active_provider(&self) -> Option<&Provider> {
        self.providers
            .iter()
            .find(|p| p.id == self.active_provider)
            .or_else(|| self.providers.first())
    }

    pub fn prompt_by_id(&self, id: &str) -> Option<&Prompt> {
        self.prompts.iter().find(|p| p.id == id)
    }

    pub fn active_prompt(&self) -> Option<&Prompt> {
        self.prompt_by_id(&self.active_prompt)
            .or_else(|| self.prompts.first())
    }

    /// 把提示词里的 {target_lang} 占位符替换掉
    pub fn render_system(&self, prompt: &Prompt) -> String {
        prompt.system.replace("{target_lang}", &self.target_lang)
    }

    pub fn load() -> Config {
        match std::fs::read_to_string(config_path()) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                eprintln!("seltrans: 配置文件解析失败（将使用默认配置）: {e}");
                Config::default()
            }),
            Err(_) => Config::default(),
        }
    }

    /// 原子写入 + 0600 权限
    pub fn save(&self) -> std::io::Result<()> {
        let path = config_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            f.write_all(serde_json::to_string_pretty(self)?.as_bytes())?;
            f.write_all(b"\n")?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }
}

/// 配置目录。三平台各按各自的规矩：
///
/// | 平台 | 位置 |
/// |---|---|
/// | Linux | `$XDG_CONFIG_HOME/seltrans`，默认 `~/.config/seltrans` |
/// | macOS | `~/Library/Application Support/seltrans` |
/// | Windows | `%APPDATA%\seltrans` |
///
/// 早先这里是手写的 XDG 逻辑，`HOME` 取不到就落到 `/tmp` —— Windows 上通常没有
/// `HOME`，配置会静静地写进临时目录，重启就没了。交给 `dirs` 去查各平台的正解。
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        // 一个能查到用户目录的系统上这里不会发生；真发生了也别 panic，
        // 落到临时目录至少还能跑起来让用户看见错误信息
        .unwrap_or_else(std::env::temp_dir)
        .join("seltrans")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

/// 生成一个够用的唯一 id（不引入 uuid 依赖）
pub fn new_id() -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("p{n:x}")
}
