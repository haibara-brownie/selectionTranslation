/**
 * Rust 命令的类型化封装。
 *
 * 前端**只**通过这里跟后端说话 —— 直接散着写 `invoke("...")` 的话，命令改名字时
 * TypeScript 一点忙都帮不上。类型定义要和 `src-tauri/src/state.rs`、`settings_cmds.rs`
 * 里的 `#[derive(Serialize)]` 结构一一对应（那边是 `rename_all = "camelCase"`）。
 */

import { invoke } from "@tauri-apps/api/core";

// ---------- 与 Rust 一一对应的类型 ----------

export type Provider = {
  id: string;
  name: string;
  preset: string;
  /** "openai" | "anthropic" */
  kind: string;
  baseUrl: string;
  apiKey: string;
  model: string;
  models: string[];
  extraBody: string;
};

export type Prompt = {
  id: string;
  name: string;
  icon: string;
  system: string;
};

export type ProviderPreset = {
  id: string;
  name: string;
  kind: string;
  baseUrl: string;
  /** 去哪儿申请 API key */
  keysUrl: string;
  /** 界面上的补充说明（哪个模型适合划词、哪个已停用之类） */
  hint: string;
};

export type Config = {
  providers: Provider[];
  activeProvider: string;
  prompts: Prompt[];
  activePrompt: string;
  targetLang: string;
  selectionMode: string;
  theme: string;
  fontLatin: string;
  fontCjk: string;
  fontFallback: string;
  popupWidth: number;
  popupHeight: number;
};

/** 「关于」页依赖自检的一行 */
export type DepCheck = { name: string; ok: boolean; note: string };

export type AboutInfo = {
  version: string;
  repoUrl: string;
  configPath: string;
  logPath: string;
  logSizeKb: number;
  os: string;
  deps: DepCheck[];
};

// ---------- 命令 ----------

export const api = {
  /** 当前生效的两组全局快捷键：[翻译, 设置]。返回的是生效值，不是配置里的原始空串。 */
  hotkeys: () => invoke<[string, string]>("hotkeys"),
  /** 改键。传空串恢复内置默认。设不成会 reject，文案可以直接显示给用户。 */
  setHotkeys: (translate: string, settings: string) =>
    invoke<void>("set_hotkeys", { translate, settings }),

  loadConfig: () => invoke<Config>("load_config"),
  saveConfig: (config: Config) => invoke<void>("save_config", { config }),

  /** 生成主题 CSS（调色板变量 + 字体分档），改完设置立刻拿它刷新界面 */
  themeCss: (systemDark: boolean) => invoke<string>("theme_css", { systemDark }),

  /** 当前平台："linux" | "macos" | "windows" */
  platform: () => invoke<string>("platform"),

  /** 系统已装的字体家族 */
  listFonts: () => invoke<string[]>("list_fonts"),
  /** 这个字体自己有没有汉字字形 —— 用来警告"你选的中文字体其实没有汉字" */
  fontCoversCjk: (family: string) => invoke<boolean>("font_covers_cjk", { family }),

  /** 内置的供应商预设 */
  providerPresets: () => invoke<ProviderPreset[]>("provider_presets"),
  /** 内置的提示词预设，用来「恢复内置」 */
  promptPresets: () => invoke<Prompt[]>("prompt_presets"),
  /** 可选目标语言 [值, 显示文字][] */
  targetLangs: () => invoke<[string, string][]>("target_langs"),
  /** 可选配色 [值, 显示文字][] */
  themeChoices: () => invoke<[string, string][]>("theme_choices"),

  /** 实时拉取该供应商的模型列表 */
  listModels: (provider: Provider) => invoke<string[]>("list_models", { provider }),
  /** 发一条最短请求验证 key / base_url / 模型名 */
  testConnection: (provider: Provider) => invoke<string>("test_connection", { provider }),

  about: () => invoke<AboutInfo>("about_info"),
  openPath: (path: string) => invoke<void>("open_path", { path }),

  autostartEnabled: () => invoke<boolean>("autostart_enabled"),
  setAutostart: (on: boolean) => invoke<void>("set_autostart", { on }),
};

/** 生成一个新的 id（供应商 / 提示词新建时用），和 Rust 侧的规则保持一致 */
export function newId(prefix: string): string {
  return `${prefix}-${Math.random().toString(36).slice(2, 10)}`;
}
