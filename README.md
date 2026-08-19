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
seltrans log -f                    # 跟踪运行日志
```

## 日志

niri 用 `spawn` 启动程序时 stderr 会进 niri 自己的日志，所以关键节点都落到自己的文件里：

```
~/.local/state/seltrans/seltrans.log
```

超过 1 MB 自动轮转成 `seltrans.log.1`。设 `SELTRANS_DEBUG=1` 可以同时打到 stderr。

翻译结果不对时先看这一行：

```
21:15:56 [INFO] 发起翻译 | 供应商=DeepSeek kind=openai 模型=deepseek-v4-flash
  端点=https://api.deepseek.com/v1/chat/completions | system=315 字符 | user=53 字符
  | user 预览: The build failed because the lockfile is out of date.
```

`user 字符数` 和 `user 预览` 就是**真正发给模型的内容**。如果模型回你"请提供需要翻译的内容"，看这里立刻就知道是取词取空了还是别的问题。预览里会把肉眼看不见的字符标出来（`<ZWSP>`、`<BOM>`、`<NBSP>` 等）——网页上选到空行、图标字体时经常拿到一串零宽字符，看着像有内容其实什么都没有。这种情况程序会直接拦下不发请求。

日志**不记录 API key**。

## 配置

首次使用：`Mod+Alt+T` → 「供应商」页 → 点 **+** → 选一个预设（base_url 自动填好）→ 填 API key → 点「拉取列表」挑模型 → 点「测试连接」确认通了。

配置存在 `~/.config/seltrans/config.json`，权限 `0600`（里面有 API key）。

### 目标语言

在「通用」页的下拉里选，内置 21 种主流大模型翻译质量都比较可靠的语种（简繁中英日韩、法德西葡意、俄乌荷波瑞土、阿拉伯、印地、泰、越、印尼）。选最后一项「自定义…」会露出输入框，可以填任何模型看得懂的语言名。

存的是**语言名本身**而不是 ISO 代码 —— 它会直接替换掉提示词里的 `{target_lang}`，写「简体中文」比写 `zh-Hans` 稳定得多。

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

第 2 条路有个必须处理的坑：快捷键是 `Mod+Shift+T`，程序跑起来那一刻你**手还按着 Super+Shift**，这时直接发 Ctrl+C，应用收到的其实是 `Super+Shift+Ctrl+C` —— 既复制不到东西，还可能让合成器的修饰键状态和物理按键脱节，表现就是**键盘像卡住了一样没法操作**。所以发 Ctrl+C 之前会先把左右 Ctrl / Shift / Alt / Super 全部显式抬起并等 120 ms，且用 RAII 守卫保证无论中途提前返回、panic 还是出错，退出时一定会再抬一次。

取词方式在设置里可以锁死成其中一种。默认「自动」，即先试主选区，失败再兜底。

弹窗固定在屏幕右上角，不跟随鼠标 —— Wayland 下拿不到全局光标坐标，这是没法绕开的取舍。位置和尺寸可以改 `~/.config/niri/selectiontranslation.kdl` 里的窗口规则。

## 项目结构

```
src/
├── main.rs         CLI 分发：popup / settings / translate
├── config.rs       ~/.config/seltrans/config.json 读写（原子写 + 0600）
├── logging.rs      日志、文本预览、零宽字符判空
├── presets.rs      供应商预设目录 + 目标语言列表 + 七条内置提示词
├── selection.rs    取词：主选区 / 模拟 Ctrl+C / 修饰键守卫 / 依赖自检
├── llm.rs          OpenAI 兼容与 Anthropic 两套流式后端
├── popup.rs        翻译弹窗
└── settings_ui.rs  配置界面
data/
├── niri-snippet.kdl                        快捷键与窗口规则
└── xyz.brownie.SelectionTranslation.desktop 桌面项
```

## 排查

界面「关于」页有依赖自检，会告诉你 wl-clipboard、ydotool 服务、niri 规则各自的状态。

- **模型回"请提供需要翻译的内容"** —— 说明发出去的用户消息是空的。弹窗里的「原文」默认展开且带字数，先看那里；再看日志里「发起翻译」那行的 `user 字符数`。多半是取词取到了空行或一串零宽字符
- **取不到词** —— 先确认按快捷键前文字确实选中着；某些 Electron / Java 应用不提供主选区，把取词方式改成「自动」或「仅模拟 Ctrl+C」
- **模拟 Ctrl+C 无效** —— `systemctl --user status ydotool` 看服务在不在
- **键盘像卡住了** —— 修饰键被卡在按下状态。手动解除：`ydotool key 29:0 97:0 42:0 54:0 56:0 100:0 125:0 126:0`
- **快捷键没反应** —— `niri validate` 看配置有没有过；确认 `~/.local/bin` 在 PATH 里
- **报 HTTP 401 / 404** —— 弹窗里会直接显示服务端返回的原文，多半是 key 错了或 base_url 少了 `/v1`

## 许可

MIT
