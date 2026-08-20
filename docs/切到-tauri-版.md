# 从 GTK 版切到 Tauri 版

迁移的最后一步（方案里的 P5）**还没做**，因为它的前提是「Tauri 版在你日常使用中站住脚」——
那要你自己用过才算数，不是跑几个测试能证明的。

这份文档写清楚怎么切、怎么退回去。

---

## 先试用，别急着切

两个二进制**共用同一份配置和日志**，可以随时来回切，互不影响。

```bash
pnpm tauri build --no-bundle                  # 不要用 cargo build --release，见下
./target/release/seltrans-tauri settings      # 设置界面
./target/release/seltrans-tauri popup --text "hello"
./target/release/seltrans-tauri tray          # 常驻托盘
```

> **别用 `cargo build --release` 编 Tauri 版。** 那样编出来的二进制**跑不起来**：
> 窗口一片空白，左上角写着 `Could not connect to localhost. Connection refused`。
>
> 原因是 Tauri 靠 `tauri` 依赖有没有开 `custom-protocol` 特性来判断 dev / 生产
> （`tauri` 的 build.rs：`let dev = !has_feature("custom-protocol")`）。
> 只有 `tauri build` 会加上这个特性；不加就走 `devUrl`，去连一个没起的 vite。
>
> 这个坑不挑构建配置 —— `--release` 一样中招，因为它跟 profile 无关。

日常那个 `~/.local/bin/seltrans` **不会**被动到。

### 值得重点试的几件事

| 试什么 | 怎么算通过 |
|---|---|
| 划词取词 | 在浏览器、终端、PyCharm 各选一段按快捷键，译文对得上 |
| Ctrl+C 兜底 | 找个不支持主选区的应用（某些 Electron）选中再按快捷键；**翻完之后剪贴板内容要跟之前一样** |
| 修饰键 | 按住 `Mod+Shift` 不松手触发翻译，之后键盘不能有卡住的感觉 |
| Esc / Ctrl+C / Ctrl+Enter | 关窗、复制译文、重译 —— **这三个我没能验证**（验证时你的显示器休眠了，会话没有焦点，往没焦点的会话注入按键既证明不了什么又有卡键风险） |
| 托盘 | 图标在不在、左键开输入框、中键取词翻译、右键菜单切供应商 |
| 设置页四页 | 加供应商、拉模型列表、测试连接、改字体、恢复内置提示词 |
| 常驻速度 | 连按两次快捷键，第二次应该几乎瞬间出来（窗口是复用的） |

---

## 确认没问题之后再切

### 1. 先留退路

```bash
git tag -a v0.1.0-gtk -m "GTK4 版最后一个可用状态，Tauri 版出问题时回到这里"
git push origin v0.1.0-gtk
```

### 2. 换掉安装的二进制

`install.sh` 目前装的是 GTK 版。切换时要改的地方：

- 编译目标从 `seltrans` 换成 `seltrans-tauri`
- 装进 `~/.local/bin/seltrans` 的换成 Tauri 那个二进制（**名字保持 `seltrans`**，
  这样 niri 配置里的 `spawn "seltrans" "popup"` 不用改）
- 运行时依赖从 `gtk4 libadwaita` 换成 `webkit2gtk-4.1 gtk3 libayatana-appindicator`
- 构建依赖多了 `nodejs` 和 `pnpm`

### 3. niri 规则

`data/niri-snippet.kdl` 里两套规则**已经都写好了**，GTK 版按 `.Popup` / `.Settings`
后缀匹配，Tauri 版按 app-id + 标题匹配。切换后 GTK 那两条不再匹配到任何窗口，
留着无害，也可以删掉。

### 4. 删掉 GTK 版代码

确认稳定运行一段时间之后：

```
src/popup.rs  src/settings_ui.rs  src/theme.rs  src/fonts.rs
src/tray.rs   src/autostart.rs    src/main.rs
```

连带根 `Cargo.toml` 里的 `[package]` 段和 gtk4 / libadwaita / ksni 依赖。
删完 workspace 就只剩 `crates/seltrans-core` 和 `src-tauri` 两个成员，
`cargo test` 那个「必须带 `--workspace`」的坑也就自动消失了。

`.github/workflows/ci.yml` 里那个单独跑 GTK 版的 job（`ubuntu-26.04` 那个）也一起删。

---

## 出问题怎么退

```bash
git checkout v0.1.0-gtk
./install.sh
```

配置文件格式没变，退回去之后设置全在。

---

## 已知的、切换前应该知道的事

- **mac / Windows 的取词代码没有真机验证过**。写完了、类型检查过了，但模拟按键、
  辅助功能授权、剪贴板还原这些必须实机试。相关待验证项列在
  `crates/seltrans-core/src/selection/{macos,windows}.rs` 的模块头里。
- **CI 一次都没真跑过**。所有 action 的版本号都现查核对过，YAML 语法验证过，
  但 runner 上的系统依赖是否完备、mac universal 合成、Windows 的 NASM 问题，
  只能等第一次打 tag 才知道。
- **mac 托盘图标不是模板图标**。现有图标是彩色的，切成单色会变成两坨看不出内容的
  黑块，需要另配一张 `icons/tray-mono.png`。
- Windows 上传统控制台（conhost）里 Ctrl+C 是中断信号不是复制，那类窗口取不到词。
- Windows 的 UIPI 会挡住提权窗口 —— 目标程序以管理员身份运行时取不到词。
