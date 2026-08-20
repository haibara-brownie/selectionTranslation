# cross-platform-ready · 进度日志

目标：把只在 niri/Wayland 上真正跑通的 seltrans，做成 Linux / macOS / Windows 三家都能日常用。

> 本目录随仓库公开。凭据、key 位置、内网地址端口一律不写这里。

## 2026-08-20（Mac 工作站）

### 定级与需求

**定级：重度**，走 HEAVY 九阶段。理由：跨 core/tauri/ui 三层、跨三平台、多条工作线可并行。

用户拍板的四个岔路：

| 岔路 | 结论 |
|---|---|
| 验收标准 | mac 在本机真机跑通；Linux / Windows 保证代码正确 + 跨平台编译过 + CI 三家绿，运行时由用户到各自机器上验 |
| 动效范围 | 四项全要：弹窗进场/退场、译文流式打字感、加载态与骨架屏、切风格/供应商过渡 |
| Windows UIA 取词 | **做**，用户有 Windows 机器能验 |
| 快捷键 | 授权自行改默认键 |

### 环境（Mac 侧首次搭起来）

- 装 rustc 1.97.1 aarch64-apple-darwin（此前本机无 Rust 工具链）。
- **mac 分支首次原生编译通过**：`cargo check/clippy --workspace --exclude seltrans --all-targets`
  零错误零警告、`cargo test` 28 passed、`cargo fmt --all --check` 干净。
  此前作者只在 Linux 上翻 cfg 强编过（见 620d607 提交信息）。
- `pnpm install` 通过，esbuild postinstall 正常（印证 pnpm 11 的 allowBuilds 有效）。
- 注意：根 package `seltrans` 是 GTK4 版，mac 上编不过，本机所有 cargo 命令必须带
  `--exclude seltrans`；`cargo build --bin` 要写 `-p seltrans-tauri`，否则 cargo 找的是根包。

### 🐞 首次真机启动就抓到硬 bug：mac 上应用直接 abort（已修，待提交）

- 现象：启动即 panic
  `state() called before manage() for tauri_plugin_global_shortcut::GlobalShortcut<...>`，
  panic 点 `tao .../macos/app_delegate.rs did_finish_launching`，
  `thread caused non-unwinding panic. aborting.`
- 根因：`main.rs` 的 Builder 链**从未注册 global-shortcut 插件**，而 `hotkey.rs` 非 Linux
  分支里 `app.global_shortcut()` 底下是 `state::<GlobalShortcut>()`，没 manage 过直接 panic。
  且它跑在 `extern "C"` 的 `did_finish_launching` 回调里 → non-unwinding panic → 当场 abort，
  `register()` 返回 `Result` 那层「失败也继续跑」的兜底**完全够不着**。
- 为什么一直没暴露：Linux 走的是 `register` 的空实现分支，缺插件永远碰不到。
  620d607 说的「在 Linux 上翻 cfg 强编一遍，零错误」只能证明类型对——这个 bug 是活反例。
- 修法：`hotkey.rs` 新增 `pub fn plugin(builder) -> builder`，非 Linux 挂插件、Linux 恒等；
  `main.rs` 拆开 Builder 链，在 single-instance 之后调它。平台分叉留在 hotkey.rs，
  没让 main.rs 再长一段 cfg。
- **回归测试缺口**：Tauri 启动期 panic 没法单测覆盖，只能靠各平台真机启动。记为已知限制。

### 读代码找出的 mac 缺口（6 条）

1. ~~取词从未真机运行~~ → 降级链路已验证正确（见下），真实取词待验
2. `⌘⇧T` 与浏览器「重新打开关闭的标签页」全局冲突（**Windows 上 Ctrl+Shift+T 同样冲突**）
3. 弹窗在 mac 上无人定位（Linux 靠 niri window-rule，mac 没有对应机制）
4. 无 `ActivationPolicy::Accessory`，托盘常驻会占 Dock 图标
5. `decorations(false)` + mac 不能透明 → 方角、无红绿灯
6. 托盘图标非模板图标（`icon_as_template(false)`，注释已说明要另配 `tray-mono.png`）

### 取词降级链路已验证（无授权状态）

日志显示：AX 没授权 → 自动转 ⌘C 兜底 → 也没授权 → 干净报错并带排查路径。
设计的降级行为是对的。**还差「有授权之后到底能不能取到词」这一步**——需要人工授权 + 按键。

### Cherry Studio 调研

- 划词助手**只支持 Windows + macOS，Linux 最早 2025Q4**。
- Windows：UI Automation + IAccessible + 低级鼠标/键盘钩子，剪贴板模拟只是兜底。
- macOS：辅助功能 API。已知限制：全屏应用、与 Raycast/AeroSpace 等窗口管理器冲突、
  需要 I-beam 光标状态才能可靠触发。
- **可借鉴点集中在 Windows 加 UIA 取词路线**——seltrans 现在 Windows 只有 Ctrl+C 一条路。
  Linux 上 seltrans 反而比他们强（主选区已可用）。

### 运行方式备忘（Mac 侧）

`pnpm tauri dev` 起的进程活不过一个 agent 回合。要长驻验证就先 `pnpm build` 出 dist、
再 `cargo build -p seltrans-tauri`，然后脱离终端跑 `nohup ./target/debug/seltrans-tauri tray &`。

### 下一步

1. 改 mac / Windows 的默认快捷键（用户已授权），避开浏览器的 reopen-closed-tab。
2. 人工验证真实取词：Safari（应走 AX）→ VS Code（应掉 ⌘C 兜底）→ 终端，
   并确认兜底后剪贴板还原。
3. 据验证结果写规格（阶段 3）→ 拆票（阶段 4，⏸ 等审批）→ 分波并行实现。

### 快捷键换掉了（用户拍板：mac / Windows 用 Alt+Shift+T，Linux 不动）

- 原来是 `CmdOrCtrl+Shift+T`（翻译）/ `CmdOrCtrl+Alt+T`（设置），理由是「换平台肌肉记忆还在」。
  这个理由撑不住：**全局快捷键在 mac / Windows 上是系统级独占的**，而 `⌘⇧T`（mac）和
  `Ctrl+Shift+T`（Windows）恰好是所有浏览器的「重新打开关闭的标签页」，`⌘⌥T` 是
  Safari / 访达 / 邮件的「显示或隐藏工具栏」。为了对齐肌肉记忆去废掉用户已有的肌肉记忆，
  不划算。
- 现在：`Alt+Shift+T`（翻译）/ `Alt+Shift+Comma`（设置），mac 上即 `⌥⇧T` / `⌥⇧,`。
  跟 Linux 的 `Mod+Shift+T` 只差一个修饰键，在不抢别人按键的前提下尽量对齐。
- **Linux 完全不受影响**：那边快捷键归合成器，`data/niri-snippet.kdl` 一行没动。
- 同步更新了 `describe()`，否则「关于」页和 `--help` 会说谎。改动只在 `hotkey.rs`。

### 🐞 又发现一个：取词在快捷键路径上跑了两遍

日志实证（12:44 那次）：
```
[INFO] ===== 启动，子命令=tauri:popup pid=81554 =====
[INFO] 开始取词，方式=auto            ← 第一次（新进程，结果最终被丢弃）
[INFO] 常驻实例收到第二次启动：popup
[INFO] 开始取词，方式=auto            ← 第二次（常驻实例，这次的结果才被用）
```
新进程在 `tauri::Builder` 之前 `prepare()` 取一次，single-instance 把 argv 递给常驻实例后，
`dispatch()` 里又 `prepare()` 一次。第一次的结果随进程退出丢掉。

- 代价在**兜底路径上翻倍**：两轮「抬修饰键 → 模拟复制 → 等剪贴板 → 还原」，
  每轮固定成本 120ms + 最多 800ms，剪贴板被改写两次、被搞坏的风险也翻倍。
- **Linux 上这是日常主路径**（合成器每次按键 spawn 新进程），只是那边首选主选区廉价，
  一旦掉进 Ctrl+C 兜底就是同样的问题。
- 难点：进程在 Tauri 初始化前无法知道自己是次要实例，而取词又必须赶在那之前
  （mac 上 NSApplication 激活会抢焦点）。**需要设计，没当场改。**

### 打 .app 包（进行中）

裸二进制在 macOS 的 TCC 面前很难授权（每次重编哈希都变），改用 `pnpm tauri build --bundles app`
出正经 .app，bundle id 稳定为 `xyz.brownie.SelectionTranslation`。顺带首次验证 mac 打包链路。

---

## 真机验证结果（mac，2026-08-20 下午）

### 关键发现：我的终端有辅助功能权限，权限会被子进程继承

早期看到的「取词成功」有一部分是假象：直接执行 `seltrans.app/Contents/MacOS/seltrans-tauri`
时，TCC 把权限算在**父进程**（终端）头上，于是子进程白蹭到了权限；而用 `open` 正经启动的
实例没有。**app 自身至今未被授权。**

推论（对用户有影响）：**ad-hoc 签名的 app 每次重新打包，cdhash 变了，TCC 授权就失效**。
用户每次升级都得重勾一次辅助功能。签名+公证的 app 不会有这个问题（designated requirement
基于 Team ID）。这条要写进发版说明。

### ✅ 取词全链路验证通过

| 场景 | AX 路线 | 兜底 | 结果 |
|---|---|---|---|
| TextEdit（原生 AppKit） | 命中 | 不需要 | 取到 84 字符 ✅ |
| TextEdit（新进程首次） | AXError -25204 通信失败 | ⌘C | 取到 84 字符 ✅ |
| VS Code（Electron） | `AXSelectedText 有 0 字符` | ⌘C | 取到 79 字符 ✅ |

**剪贴板还原每次都正确**（用哨兵字符串验的，三轮全过）。
Electron 那一路的 AX 失败原因和 `selection/macos.rs` 模块头预判的完全一致。

### ✅ 端到端翻译验证通过（两条后端都验了）

- **openai 后端**（DeepSeek `/v1/chat/completions`）：`translate --text` 1.24s 出结果；
  管道输入正常；弹窗里 `发起翻译 → 翻译完成，共 26 字符`。
- **anthropic 后端**（DeepSeek `/anthropic/v1/messages`）：也通。这条路径此前从未跑过。
  为此在配置里留了第二个 provider `ds-anthropic`，active 仍是 deepseek。

### 🐞 修掉的第二个 bug：常驻后第一次触发必定开空弹窗

- 现象：托盘常驻起来后**第一次**按快捷键，取词日志显示成功，但没有任何翻译请求，
  弹窗是空的。第二次起才正常。
- 根因：`open_popup` 在**新建窗口**那条路上 **`req` 参数压根没被用到**——取到的文本
  掉在地上。前端新建时收不到 `EVENT_TRANSLATE`（还没挂监听器），走的是 `launch_args`，
  而 `Launch` 里存的是**进程启动时**那份，托盘模式下是空的。复用窗口那条路才发事件，
  所以第二次起正常。
- **Linux 上同样成立，而且那边正是日常主路径。**
- 修法：`open_popup` 新建分支里把当轮 `req` 写进 `Launch` 槽位；`Launch` 的文档改成
  「会被改写的槽位，不是进程启动参数」。
- 验证：全新起常驻实例 → 第一次触发 → `取词成功 → 发起翻译 → 翻译完成，共 26 字符` ✅

### ✅ 窗口定位（3 号待办的一部分）

`windows.rs` 加 `place_top_right`：按**鼠标所在那块屏**的 `work_area()` 右上角摆，留白 24pt。

实测验算：
- 内置屏（逻辑宽 1800）：窗口落在 `(1216, 63)`，`1800−560−24 = 1216` ✓，`菜单栏39+24 = 63` ✓
- 外接屏（桌面拼接宽 3720）：落在 `(3136, 143)`，`3720−560−24 = 3136` ✓
- 改之前是 `(620, 188)`，偏中间。

只在**创建时**摆一次，不在复用时强行拽回——用户把它拖到别处说明他想让它待在那儿。

### ✅ Accessory 激活策略（3 号待办的一部分）

`main.rs` setup 里加 `set_activation_policy(Accessory)`（仅 macOS）。
实测 `System Events` 报 `background only = true`，Dock 里不再出现图标。

### ✅ mac 打包链路首次验证

`pnpm tauri build --bundles app` 通过，release 编译 1m08s，产出
`target/release/bundle/macos/seltrans.app`，bundle id `xyz.brownie.SelectionTranslation`，
签名 adhoc / TeamIdentifier not set。已装到 `/Applications/seltrans.app`。

### 仍待办

1. **只有用户能做**：给 `/Applications/seltrans.app` 授权辅助功能（app 自身至今未授权，
   之前的验证是靠终端权限继承绕过去的）。
2. 双重取词（Electron 场景下剪贴板被改写两遍，changeCount 79→81 实证）——需设计。
3. 窗口 chrome：方角无红绿灯，是否改用 `TitleBarStyle::Overlay`。**需要肉眼判断**，
   我截不了图（缺屏幕录制权限）。
4. 动效四项、Windows UIA、快捷键可配置、托盘模板图标。
