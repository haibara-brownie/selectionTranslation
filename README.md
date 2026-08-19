# selectionTranslation

niri / Wayland 下的全局划词翻译。**选中任意界面里的文字，按 `Mod+Shift+T`，右上角浮出译文。**

不想选中也行：点托盘图标打开输入框，手敲或粘贴。

- 七种翻译风格随时切换（通用 / GitHub / 科学论文 / 口语 / 术语解释 / 报错解读 / 中译英）
- 十家模型供应商预设，选中即自动填好 base_url，模型列表实时拉取
- 常驻托盘，Catppuccin 配色，跟随系统深浅色
- 单文件二进制约 9 MB，运行时只依赖桌面本来就有的 gtk4 / libadwaita

---

## 快速开始

```bash
# 1. 装依赖（Arch）
pac rust gtk4 libadwaita wl-clipboard ydotool
systemctl --user enable --now ydotool

# 2. 编译安装
git clone https://github.com/haibara-brownie/selectionTranslation.git
cd selectionTranslation
./install.sh
```

`install.sh` 会编译、装二进制和图标、写 niri 快捷键与窗口规则（**改 `config.kdl` 前先备份**）、开机自启并拉起托盘。加 `--no-niri` 不碰 niri 配置，加 `--no-autostart` 不设自启，`--uninstall` 卸载。

**3. 配一个模型**：按 `Mod+Alt+T` → 「供应商」页 → 点 **+** → 选预设（base_url 自动填好）→ 填 API key → 「拉取列表」挑模型 → 「测试连接」确认通了。

**4. 用起来**：选中任意文字，按 `Mod+Shift+T`。

---

## 怎么用

### 快捷键

| 按键 | 作用 |
|---|---|
| `Mod+Shift+T` | 翻译当前选中的文字 |
| `Mod+Alt+T` | 打开配置界面 |
| `Esc` | 收起弹窗 |
| `Ctrl+Enter` / `F5` | 在弹窗里翻译（输入框里回车是换行） |
| `Ctrl+Shift+C` | 复制译文 |

改键编辑 `~/.config/niri/selectiontranslation.kdl`。

### 弹窗

上下两张卡片：**原文**和**译文**。

原文那张**是可以编辑的** —— 取词取歪了直接改，改完 `Ctrl+Enter` 重译；也可以什么都不选，打开它手敲或粘贴。右上角实时显示字数，取到空内容一眼就能看出来。

顶部下拉换翻译风格，底部换供应商和模型，**换完立刻用新设置重译同一段文字**，不用重新选词。

### 托盘

| 操作 | 作用 |
|---|---|
| 左键点图标 | 打开弹窗并聚焦输入框 |
| 中键点图标 | 翻译当前选中的文字 |
| 右键 | 完整菜单：切风格、切供应商、设置、看日志、开关自启、退出 |
| 悬停 | 显示当前供应商 / 模型 / 风格 / 目标语言 |

常驻还有个实际好处：快捷键触发时是复用这个进程，省掉 GTK 冷启动，弹窗几乎瞬间出来。收起弹窗只是把窗口藏起来，进程和图标都还在。

### 翻译风格

| 风格 | 适合什么 |
|---|---|
| 🌐 通用翻译 | 默认，忠实又自然，只输出译文 |
| 💻 GitHub / 技术文档 | README、issue、commit message。代码、路径、命令、Markdown 结构原样保留 |
| 🔬 科学杂志 / 论文 | 学术书面语，术语给「中文（English）」对照，保留 LaTeX / 单位 / 引用标记 |
| 💬 日常口语 | 地道口语，保留语气和表情符号，俚语转成对应说法 |
| 📖 术语解释 | 先给译文，再补一段这个词是什么、用在哪、容易混淆在哪 |
| 🐞 报错 / 代码解读 | 讲清报错含义 + 常见原因 + 建议排查 |
| ✍️ 中译英润色 | 译成地道英文并润色 |

七条都能改，改坏了点「恢复内置提示词」就回来。也可以自己加。

### 命令行

```bash
seltrans popup                     # 取词并弹窗（快捷键调用的就是这个）
seltrans popup --input             # 打开输入框，不取词
seltrans settings [页面]           # 配置界面，页面可选 general/providers/prompts/about
seltrans tray                      # 常驻托盘（开机自启跑的就是这个）
seltrans translate --text "hello"  # 终端里翻译，不开窗口
echo "hello" | seltrans translate  # 也吃管道
seltrans autostart on|off          # 开关开机自启
seltrans log -f                    # 跟踪运行日志
```

---

## 配置

配置存在 `~/.config/seltrans/config.json`，权限 `0600`（里面有 API key）。

### 供应商与模型

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

**模型名不写死在程序里**，一律点「拉取列表」从 `/v1/models` 实时取 —— 各家迭代太快，写死很快就失效（`deepseek-chat` 已在 2026-07-24 停用，`moonshot-v1` 系列 2026-08-31 退役）。

> 划词翻译重延迟和成本，**建议挑各家的快档而不是旗舰**。有些旗舰（如 `kimi-k3`、`qwen3.8-max`）默认强制开启思考，会又慢又贵。

可以配多个供应商，在弹窗底部或托盘菜单里一键切换。

### 目标语言

「通用」页的下拉，内置 21 种主流大模型翻译质量都比较可靠的语种。选「自定义…」可以填任何模型看得懂的语言名。

存的是**语言名本身**而不是 ISO 代码 —— 它会直接替换提示词里的 `{target_lang}`，写「简体中文」比写 `zh-Hans` 稳定得多。

### 配色

「通用 → 外观 → 配色」，用的是 [Catppuccin](https://github.com/catppuccin/palette) 官方四个风味：

| 选项 | |
|---|---|
| 跟随系统 | 浅色用 Latte、深色用 Mocha，系统切换自动跟着换 |
| Latte | 浅色 |
| Frappé / Macchiato / Mocha | 三档深色，由浅到深 |

改完立刻生效，不用重开窗口。

### 字体

「通用 → 字体」有三档，都是带搜索的下拉（系统上几百个字体家族，不搜没法选）。搜索是**子串匹配**，搜 `maple` 能搜到 `JetBrains Maple Mono`：

| 档位 | 管什么 |
|---|---|
| 拉丁字体 | 英文、数字、代码 |
| 中文字体 | 汉字、中文标点、假名、谚文 |
| 后备字体 | 前两档都没有的字形，比如 emoji、西里尔字母 |

**整个界面都按字符脚本直接分配**：汉字一定用中文字体，其余按 拉丁 → 中文 → 后备 的顺序回退。

这一点很重要，因为 CSS 的 `font-family` 是**逐字符**回退的 —— 列表里第一个有该字形的字体就赢了。而 JetBrains Maple Mono 这类字体自带汉字，光靠回退顺序的话选它当拉丁字体就会把「中文字体」那档彻底架空。

所以不靠回退顺序猜。GTK 版：译文和原文区给汉字范围打 `GtkTextTag`，界面上的按钮、标签则遍历控件树逐个挂 Pango 属性。Tauri 版：用 `@font-face` 的 `unicode-range` 把两档**各自钉死**在自己的字符区间。结果都是同一行里 `lockfile` 用 Maple、汉字用衬线，泾渭分明。

> **中文字体那一档，要选真的有汉字的字体。**
> 举个真实的坑：`HarmonyOS Sans` 是纯拉丁族，**没有汉字**，带汉字的是 `HarmonyOS Sans SC`。
> 选错了的话，汉字既进不了中文档（它没有字形），也不会退给拉丁档（拉丁档被挡在汉字区外），
> 最后落到系统默认字体 —— 不会显示成豆腐块，但也不是你选的那个字体。

三档都选「系统默认」就完全不发 `font-family`，用系统的。只填拉丁字体不填中文字体时，
拉丁字体不受限制、管全部字符（它自带汉字这时候是好事）。

### 附加请求体

供应商配置里的「附加请求体」是一段 JSON，会合并进请求体，用来塞各家特有的参数：

```json
{"reasoning_effort": "none"}
```

程序**默认不发送 `temperature`** —— Claude 当前世代模型收到它会直接 400，部分推理模型也一样。需要的话在这里自己加。

---

## 排查

配置界面「关于」页有依赖自检，会告诉你 wl-clipboard、ydotool 服务、niri 规则各自的状态。

| 症状 | 怎么办 |
|---|---|
| 模型回「请提供需要翻译的内容」 | 发出去的用户消息是空的。先看弹窗里「原文」卡片的字数，再看日志里「发起翻译」那行的 `user 字符数`。多半是取到了空行或一串零宽字符 |
| 取不到词 | 确认按快捷键前文字确实选中着；某些 Electron / Java 应用不提供主选区，把取词方式改成「自动」或「仅模拟 Ctrl+C」 |
| 模拟 Ctrl+C 无效 | `systemctl --user status ydotool` 看服务在不在 |
| 键盘像卡住了 | 修饰键被卡在按下状态，执行 `ydotool key 29:0 97:0 42:0 54:0 56:0 100:0 125:0 126:0` |
| 快捷键没反应 | `niri validate` 看配置过没过；确认 `~/.local/bin` 在 PATH 里 |
| 托盘没图标 | `seltrans tray` 手动跑一下看报什么；面板需要提供 `org.kde.StatusNotifierWatcher` |
| HTTP 401 / 404 | 弹窗里会直接显示服务端返回的原文，多半是 key 错了或 base_url 少了 `/v1` |

### 日志

```
~/.local/state/seltrans/seltrans.log
```

超 1 MB 自动轮转。`seltrans log -f` 跟踪，`SELTRANS_DEBUG=1` 可同时打到 stderr。**日志不记录 API key。**

翻译结果不对时先看这一行：

```
[INFO] 发起翻译 | 供应商=DeepSeek 模型=deepseek-v4-flash | system=315 字符 | user=53 字符
       | user 预览: The build failed because the lockfile is out of date.
```

`user 字符数` 和 `user 预览` 就是**真正发给模型的内容**。预览里会把肉眼看不见的字符标出来（`<ZWSP>`、`<BOM>`、`<NBSP>`）—— 网页上选到空行、图标字体时经常拿到一串零宽字符，看着像有内容其实什么都没有。这种情况程序会直接拦下不发请求。

---

## 原理与取舍

<details>
<summary>Wayland 下怎么取词</summary>

没有 X11 那样的全局取词 API，所以分两条路：

1. **主选区**（`wl-paste --primary`）—— 选中就生效，零侵入，不碰剪贴板。绝大多数 GTK / Qt / 终端应用都支持。
2. **模拟 Ctrl+C**（ydotool）—— 主选区拿不到时的兜底。会先存下当前剪贴板，复制、读取，然后**还原回去**。

第 2 条有个坑：快捷键是 `Mod+Shift+T`，程序跑起来那一刻你手还按着 Super+Shift，这时直接发 Ctrl+C，应用收到的是 `Super+Shift+Ctrl+C` —— 既复制不到，还可能让合成器的修饰键状态和物理按键脱节，**表现就是键盘像卡住了**。所以发 Ctrl+C 前会先把左右 Ctrl / Shift / Alt / Super 全部显式抬起并等 120 ms，且用 RAII 守卫保证提前返回 / panic / 出错时一定再抬一次。

取词方式可以在设置里锁死成其中一种，默认「自动」。
</details>

<details>
<summary>为什么弹窗不跟随鼠标</summary>

Wayland 下拿不到全局光标坐标，这是没法绕开的。弹窗固定在屏幕右上角，位置和尺寸可以改 `~/.config/niri/selectiontranslation.kdl` 里的窗口规则。
</details>

<details>
<summary>托盘图标为什么内嵌在二进制里</summary>

走的是 StatusNotifierItem over D-Bus，和 FlClash / Cherry Studio / cc-switch 同一套协议，任何提供 `org.kde.StatusNotifierWatcher` 的面板都能显示。

图标是**内嵌进二进制、启动时光栅化成 ARGB32 再经 D-Bus 递给面板**的，不走图标主题查找 —— 面板通常在自己启动时就把图标主题缓存住了，之后新装的图标按名字找不到，只会显示个首字母兜底（实测就是这样）。
</details>

<details>
<summary>写主题 CSS 踩过的坑</summary>

**别用裸的 `.background` 选择器** —— GtkPopover 自己也带这个样式类，一刷就会在弹层外面露出一圈窗口底色的方块。要用 `window`。

GTK4 的 popover **默认没有任何出现动画**（GTK3 有，GTK4 去掉了），得自己写 `@keyframes`。

CssProvider 默认静默忽略解析错误，接上 `parsing-error` 信号写进日志，才不会改坏了自己不知道。

**CSS 动画只在样式变化时重播**，而 popover 的节点是复用的 —— 第二次打开样式没变就不播了。解法是准备两套一模一样、只是名字不同的 `@keyframes`，每次 `map` 时在两个类之间来回切，强制 `animation-name` 变化。

**下拉搜索默认是前缀匹配**，`GtkDropDown` / `AdwComboRow` 的 `search-match-mode` 要显式设成 `Substring`（该属性需要 libadwaita 1.6+）。
</details>

---

## 开发

仓库是一个 cargo workspace，分**核心**与**界面**两层：核心不碰任何 GUI 工具包。
这条线是为支持 mac / Windows 划的 —— 换界面时核心那半边原样搬走。

界面层目前**有两套并存**：能用的 GTK4 版，和迁移中的 Tauri 版。
日常用的是 GTK 版；Tauri 版还只有弹窗，追平之后会取代它。

```
crates/seltrans-core/src/     核心逻辑（零 GUI 依赖，三平台通用）
├── config.rs       配置读写（原子写 + 0600）
├── presets.rs      供应商预设 + 目标语言列表 + 七条内置提示词
├── llm.rs          OpenAI 兼容与 Anthropic 两套流式后端 + SSE 解码器
├── logging.rs      日志、文本预览、零宽字符判空
├── palette.rs      Catppuccin 四风味色值
├── typography.rs   汉字区间表 + 按脚本分档的字体 CSS
└── selection/      取词，按平台切（mod.rs / linux.rs / unsupported.rs）

src/                          界面层 ①：GTK4 / libadwaita（当前可用）
├── main.rs         CLI 分发：popup / tray / settings / translate / log / autostart
├── popup.rs        翻译弹窗 + 常驻模式 + 托盘命令派发
├── settings_ui.rs  配置界面
├── theme.rs        把 palette 翻译成 GTK 认识的 CSS
├── fonts.rs        字体家族枚举 + Pango 属性按脚本分字体
├── tray.rs         StatusNotifierItem 托盘
└── autostart.rs    ~/.config/autostart 里的 desktop 文件读写

src-tauri/src/                界面层 ②：Tauri 2（迁移中，只有弹窗）
├── main.rs         CLI 分流 + 窗口创建 + Wayland app-id
├── cmds.rs         前端能调的命令（薄搬运层，不放业务逻辑）
└── state.rs        首屏状态打包

ui/                           Tauri 的前端（Vite + TypeScript，无框架）
├── index.html      弹窗版面
├── popup.ts        渲染与交互
└── style.css       只用 CSS 变量，不写死任何色值
```

往 core 里加东西之前先问一句：**这段代码在 mac 和 Windows 上还成立吗？**
平台相关的部分（取词、托盘、快捷键、自启）不属于 core。

```bash
cargo build                              # 编译（GTK 版 + Tauri 版）
cargo clippy --workspace --all-targets -- -D warnings   # 静态检查
cargo fmt --check
cargo test --workspace                   # 测试

pnpm install && pnpm tauri dev           # 跑 Tauri 版（会同时起 vite）
```

> **`--workspace` 不能省。** 仓库根的 `Cargo.toml` 既是 workspace 根又是一个 package，
> 裸跑 `cargo test` / `cargo clippy` **只作用于根包**，会静默跳过 core 和 Tauri 那两个
> crate 然后报 ok。

技术栈：Rust 2024 + GTK4 / libadwaita（`gtk4` 0.11、`libadwaita` 0.9）+ Tauri 2.11、tokio、reqwest（rustls）、ksni（托盘）、jiff（时间戳）；前端 Vite 7 + TypeScript，不上框架。无数据库，配置落 XDG 目录。

### Tauri 版的已知限制（迁移中）

- 只有弹窗。设置页、托盘、开机自启还得用 `seltrans` 那个二进制。
- 全局快捷键在 Linux 上**只能**由合成器 spawn（Wayland 没有对应协议，Tauri 的
  global-shortcut 插件在 Wayland 下注册会"成功"但回调永不触发）。mac / Windows 上
  才用得了插件。
- 窗口浮动位置靠 niri 的窗口规则，按 app-id + 标题匹配，见 `data/niri-snippet.kdl`。

## 许可

MIT
