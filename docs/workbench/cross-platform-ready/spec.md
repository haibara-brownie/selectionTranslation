# 规格：三平台可用（cross-platform-ready）

- 状态：进行中
- 定级：重度
- 起草：2026-08-20（Mac 工作站）

## 一、要解决什么

seltrans 现在**只在 niri/Wayland 上真正跑通**。mac 和 Windows 的代码写完了，但在本轮之前
**一行都没在真机上运行过**——首次在 Mac 真机启动就抓到「应用直接 abort」，正说明
「在 Linux 上翻 cfg 强编一遍，零错误」证明不了运行时。

目标：**Linux / macOS / Windows 三家都能当日常工具用**，且三家的观感一致。

## 二、验收标准

用户拍板：

| 平台 | 标准 |
|---|---|
| macOS | 在本机真机验证到能日常使用 |
| Linux / Windows | 代码正确、跨平台编译过、CI 三家绿；运行时由用户到各自机器上验 |

「macOS 能日常使用」的具体样子（一个可勾的例子）：

> 在 VS Code 里选中一段英文 → 按 `⌥⇧T` → 不到一秒，卡片**带淡入动画**出现在**鼠标那块屏
> 的右上角** → 原文卡片里是刚才选中的那段（不是剪贴板里的旧内容）→ 译文逐字流式吐出
> → 按 Esc 卡片淡出 → **VS Code 的剪贴板和按快捷键之前一模一样** → 全程 Dock 里不多出
> 图标、菜单栏图标是单色的、刚按的快捷键**没有抢走任何应用的既有功能**。

## 三、范围

**做**：取词、窗口行为与观感、交互动效、全局快捷键、托盘、打包与发版说明。

**不做**：
- 删 GTK 版代码（那是迁移 P5，见 `docs/切到-tauri-版.md`，前提是 Tauri 版先站稳）
- 给 app 做代码签名与公证（要 Apple 开发者账号，另一件事）
- 新翻译功能、新供应商

## 四、约束（违反了就是 bug）

这些是本项目的硬约束，改任何一处都要重新核对：

1. **取词必须赶在窗口拿到焦点之前**。模拟的复制键发给的是当前有焦点的窗口。
2. **Wayland 下应用注册不了全局快捷键**。Linux 走「合成器 spawn → single-instance 递 argv
   → 常驻实例派活」，所以 argv 会被解析两次。
3. **无窗口子命令必须在 Tauri 初始化前跑完退出**，否则会被 single-instance 转交，
   `translate` 会从「打印译文」变成「弹个窗」。
4. **取词兜底路径必须先抬起所有修饰键**，且出岔子也要再抬一次。
5. **业务逻辑只在 `seltrans-core`**，界面层只做搬运。
6. **不引入 macOS 私有 API**（`macos-private-api`）。620d607 已决策，本轮沿用：
   圆角走「原生标题栏 Overlay + 藏红绿灯」，不走 `transparent()`。
7. **AppKit 只能在主线程调**。single-instance 的回调不在主线程，跨线程调会抛 ObjC 异常，
   Rust 接不住，进程直接 abort。
8. **`docs/workbench/` 是公开目录**，凭据、key 位置、内网地址端口不写进去。

## 五、跨平台一致性的接缝在哪

一致性不能靠「三边各自调样式」，要靠**把平台差异挡在一层里**：

| 关注点 | 挡在哪 | 决策 |
|---|---|---|
| 取词 | `core/selection/{linux,macos,windows}.rs` | 各平台各一条首选路 + 统一的模拟复制兜底 |
| 窗口打扮 | `windows.rs::decorate()` | 两条路：非 mac 走「无边框+透明+CSS 圆角」，mac 走「原生标题栏 Overlay + 藏红绿灯」 |
| 弹窗定位 | `windows.rs::place_top_right()` | Linux 归合成器，另两家自己算鼠标所在屏的右上角 |
| 快捷键 | `hotkey.rs` | Linux 归合成器；另两家 `Alt+Shift+T` / `Alt+Shift+Comma` |
| 托盘 | `tray.rs` 的 `imp` 模块 | Linux 用 ksni，另两家用 Tauri 内置 |
| **下拉** | `ui/lib/dropdown.ts` | **一律自绘**。原生 `<select>` 的弹层由系统画，三平台三种长相，用它就不可能一致 |

最后一条是本轮学到的一般规律：**凡是「由操作系统画」的东西，都是一致性的漏点**。

## 六、未决 / 需要设计的

- **取词在快捷键路径上跑两遍**。难点：进程在 Tauri 初始化前无法知道自己是次要实例，
  而取词又必须赶在那之前。见 `tickets/T-01.md`。
- **快捷键写死**。撞上别人的绑定时用户没有任何办法改。见 `tickets/T-07.md`。

## 六点五、两台机器的差异（接续开发前先看这个）

| | Arch / niri 工作站 | Mac |
|---|---|---|
| 根 package `seltrans`（GTK4 版） | **能编** | 编不过（没有 gtk4/libadwaita，且要求 ≥4.18） |
| cargo 命令 | `cargo test --workspace` | `cargo test --workspace --exclude seltrans` |
| 指定二进制 | `cargo build -p seltrans-tauri` | 同左（写 `--bin` 会去找根包，报 no bin target） |
| Windows 代码 | 同样编不了 | 用 `/tmp` 里的一次性壳子交叉检查，见 T-06 |
| 跑起来验 | `./target/release/seltrans-tauri tray &`，**别覆盖 `~/.local/bin/seltrans`**（那是 GTK 版） | `pnpm tauri build --bundles app` 出 .app 再装到 /Applications |

### Mac 侧攒下的验证手法（Linux 上大多也适用）

- **动画要录像抽帧，不能截图**：`screencapture` 自己就要 200ms+，抓不到 120~150ms 的动画。
  用 `screencapture -v -V <秒>` 录像 + `ffmpeg fps=N,tile=RxC` 拼联络表，再按时间窗
  `-ss/-t` 密集抽帧。注意 `tile` + `-frames:v 1` 只取**开头** N 帧，要先粗看定位时间窗。
- **看不见的东西导出来看**：托盘图标在 mac 上被拥挤的菜单栏挤到了屏幕外，就把光栅化结果
  dump 成 RGBA、用 ffmpeg 叠到浅色/深色底上看。
- **`popup --input` 是进入弹窗焦点链的确定入口**（`apply()` 会 focus 原文框），
  从那儿数 Tab 比从窗口激活状态数可靠。
- **凡是「只表现成看不见」的失败都该有测试兜底**：托盘图标画成空白、mask 没生效变黑块，
  在托盘上都只是「图标不见了/很怪」，很难往光栅化上想。

## 七、已知限制（本轮不解决，但要写进发版说明）

- **ad-hoc 签名的 app 每次升级都要重勾辅助功能**。cdhash 变了 TCC 就不认。
  签名+公证才能根治。
- Tauri 启动期的 panic 没法单测覆盖，只能靠各平台真机启动。
- Windows 代码在 mac 上连交叉类型检查都做不了（`aws-lc-sys` 需要能编 Windows 的 C 工具链），
  只能靠 CI 和用户的 Windows 机器。
