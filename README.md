# selectionTranslation

niri / Wayland 下的全局划词翻译。选中任意界面里的文字，按一下快捷键，右上角浮出译文。

- **任何界面都能用** —— 主选区取词为主，取不到时自动回退到模拟 Ctrl+C（读完会还原剪贴板）
- **翻译风格可切** —— 内置通用、GitHub / 技术文档、科学杂志 / 论文、日常口语、术语解释、报错解读、中译英润色七种，可自由增删改
- **模型随便换** —— 内置十家供应商预设，选中即自动填好 base_url；模型列表实时从 `/v1/models` 拉取，不写死在程序里
- **单文件二进制** —— Rust + GTK4/libadwaita，约 6 MB，运行时只依赖桌面本来就有的 gtk4 / libadwaita

## 安装

```bash
git clone https://github.com/haibara-brownie/selectionTranslation.git
cd selectionTranslation
./install.sh
```

`install.sh` 会做四件事：编译 release 版、装到 `~/.local/bin/seltrans`、装桌面项、把快捷键和窗口规则写进 niri（**改 `config.kdl` 前会先备份**）。

不想让它碰 niri 配置就加 `--no-niri`，卸载用 `--uninstall`。

### 依赖

| 包 | 用途 | 必需 |
|---|---|---|
| `rust` | 编译 | 是（仅编译时） |
| `gtk4`、`libadwaita` | 界面 | 是 |
| `wl-clipboard` | 主选区取词 | 是 |
| `ydotool` | 模拟 Ctrl+C 兜底取词 | 否，但强烈建议 |

ydotool 需要跑着守护进程：

```bash
systemctl --user enable --now ydotool
```

Arch 上一次装齐：`pac rust gtk4 libadwaita wl-clipboard ydotool`

## 用法

| 快捷键 | 作用 |
|---|---|
| `Mod+Shift+T` | 翻译当前选中的文本 |
| `Mod+Alt+T` | 打开配置界面 |
| `Esc` | 关闭翻译弹窗 |
| `F5` | 在弹窗里重新翻译 |
| `Ctrl+Shift+C` | 在弹窗里复制译文 |

弹窗顶部的下拉框可以随时换翻译风格，底部可以换供应商和模型，**换完立刻用新设置重译同一段文字**。

命令行：

```bash
seltrans popup                     # 取词并弹窗（快捷键调用的就是这个）
seltrans settings [页面]           # 配置界面，页面可选 general/providers/prompts/about
seltrans translate --text "hello"  # 在终端里翻译，不开窗口
echo "hello" | seltrans translate  # 也吃管道
```

## 配置

首次使用：`Mod+Alt+T` → 「供应商」页 → 点 **+** → 选一个预设（base_url 自动填好）→ 填 API key → 点「拉取列表」挑模型 → 点「测试连接」确认通了。

配置存在 `~/.config/seltrans/config.json`，权限 `0600`（里面有 API key）。

### 内置供应商预设

| 预设 | 接口 | base_url |
|---|---|---|
| DeepSeek | OpenAI 兼容 | `https://api.deepseek.com/v1` |
| 智谱 GLM | OpenAI 兼容 | `https://open.bigmodel.cn/api/paas/v4` |
| Kimi（Moonshot） | OpenAI 兼容 | `https://api.moonshot.ai/v1` |
| 硅基流动 | OpenAI 兼容 | `https://api.siliconflow.cn/v1` |
| 阿里百炼（通义千问） | OpenAI 兼容 | `https://dashscope.aliyuncs.com/compatible-mode/v1` |
| OpenRouter | OpenAI 兼容 | `https://openrouter.ai/api/v1` |
| OpenAI | OpenAI 兼容 | `https://api.openai.com/v1` |
| Anthropic Claude | Messages API | `https://api.anthropic.com` |
| Ollama（本地） | OpenAI 兼容 | `http://localhost:11434/v1` |
| 自定义 | OpenAI 兼容 | 自己填 |

**模型名不写死在程序里** —— 各家迭代太快（`deepseek-chat` 已在 2026-07-24 停用、`moonshot-v1` 系列 2026-08-31 退役），所以一律点「拉取列表」实时获取。划词翻译重延迟和成本，建议挑各家的快档而不是旗舰：旗舰里有几个（如 `kimi-k3`、`qwen3.8-max`）默认强制开启思考，会又慢又贵。

### 提示词

每条提示词就是一段 system prompt，里面写 `{target_lang}` 会被替换成设置里的目标语言。内置七条可以随意改，改坏了点「恢复内置提示词」就回来了。

### 附加请求体

供应商配置里的「附加请求体」是一段 JSON 对象，会合并进请求体，用来塞各家特有的参数，例如：

```json
{"reasoning_effort": "none"}
```

程序**默认不发送 `temperature`** —— Claude 当前世代模型收到它会直接 400，部分推理模型也一样。需要的话在这里自己加。

## 取词原理

Wayland 没有 X11 那样的全局取词 API，所以分两条路：

1. **主选区**（`wl-paste --primary`）—— 选中就生效，零侵入，不碰剪贴板。绝大多数 GTK / Qt / 终端应用都支持。
2. **模拟 Ctrl+C**（ydotool）—— 主选区拿不到时的兜底。会先存下当前剪贴板内容，复制、读取，然后**还原回去**。

取词方式在设置里可以锁死成其中一种。默认「自动」，即先试主选区，失败再兜底。

弹窗固定在屏幕右上角，不跟随鼠标 —— Wayland 下拿不到全局光标坐标，这是没法绕开的取舍。位置和尺寸可以改 `~/.config/niri/selectiontranslation.kdl` 里的窗口规则。

## 项目结构

```
src/
├── main.rs         CLI 分发：popup / settings / translate
├── config.rs       ~/.config/seltrans/config.json 读写（原子写 + 0600）
├── presets.rs      供应商预设目录 + 七条内置提示词
├── selection.rs    取词：主选区 / 模拟 Ctrl+C / 依赖自检
├── llm.rs          OpenAI 兼容与 Anthropic 两套流式后端
├── popup.rs        翻译弹窗
└── settings_ui.rs  配置界面
data/
├── niri-snippet.kdl                        快捷键与窗口规则
└── xyz.brownie.SelectionTranslation.desktop 桌面项
```

## 排查

界面「关于」页有依赖自检，会告诉你 wl-clipboard、ydotool 服务、niri 规则各自的状态。

- **取不到词** —— 先确认按快捷键前文字确实选中着；某些 Electron / Java 应用不提供主选区，把取词方式改成「自动」或「仅模拟 Ctrl+C」
- **模拟 Ctrl+C 无效** —— `systemctl --user status ydotool` 看服务在不在
- **快捷键没反应** —— `niri validate` 看配置有没有过；确认 `~/.local/bin` 在 PATH 里
- **报 HTTP 401 / 404** —— 弹窗里会直接显示服务端返回的原文，多半是 key 错了或 base_url 少了 `/v1`

## 许可

MIT
