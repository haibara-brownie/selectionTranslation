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

---

## 样式修复（mac，2026-08-20 下午晚些）

用户反馈：mac 上「缺少圆角、下拉选项样式很怪、并且不统一」。
拿到屏幕录制权限后能自己截图验证了，以下都是看着改的。

### 关键能力解锁：截图

宿主终端（kitty）授了屏幕录制权限后，`screencapture` 可用，从此**视觉问题我能自己看、
自己验**，不用每改一次都请人看一眼。

注意：**AppleScript 的 `click at` 打不进 WKWebView**（合成点击不移光标），
验证网页内交互要走键盘（`key code` + Tab/方向键/空格），或者别的注入手段。
这一条卡了好几轮才想明白，记下来省得再踩。

### ✅ 圆角（mac）

- 现象：mac 上窗口是硬方角，Linux 上是圆的。
- 根因：Linux/Windows 靠「无边框 + 透明窗口 + CSS 圆角」，而 mac 的 `transparent()`
  被 Tauri 挡在 `macos-private-api` 后面（苹果私有 API）。620d607 拒过一次，不推翻。
  无边框 NSWindow 是硬方角，CSS 圆角只会让四角露底色。
- 解法（全程公开 API）：`TitleBarStyle::Overlay` + `hidden_title` 让系统画圆角和投影，
  网页内容铺到标题栏底下，再用 objc2 把三颗红绿灯藏掉。打扮逻辑收进 `decorate()`，
  弹窗和设置页共用。
- **两个坑**：
  1. 藏红绿灯必须 `run_on_main_thread`。single-instance 的回调不在主线程，
     在别的线程调 AppKit 会抛 ObjC 异常，Rust 接不住外来异常 → 进程 abort
     （`fatal runtime error: Rust cannot catch foreign exceptions`）。
     表现很迷惑：直接启动没事，一走常驻实例就崩。
  2. 单测里要整个跳过。MockRuntime 不实现 `ns_window()`，兜底返回非空但无效的指针，
     `NonNull` 拦不住，解引用即 SIGSEGV。

### ✅ 下拉全部改成自绘

- 现象：mac 上下拉展开是个 NSMenu——系统蓝高亮、系统字体、从窗口顶部溢出盖住正文。
- 根因：原生 `<select>` 的弹层由系统画。三平台三种画法，**只要还用原生弹层，
  「三平台长得一样」就永远做不到**。
- 应用内也不一致：项目本来就有自绘弹层（`.combo-list`/`.combo-item`），
  但当初只给字体那个带搜索的下拉用了。
- 解法：把那套视觉语言抽到 `ui/dropdown.css`（两窗口共用），新增 `ui/lib/dropdown.ts`
  **接管**原生 select——select 留在 DOM 当数据源，调用方的 `.value`/`replaceChildren`/
  `change` 一行没改，选项变化靠 `MutationObserver` 盯。
- 把 CSS 注释里当初记的债还了：键盘导航（↑↓/Enter/Esc/Home/End/首字母跳转）、
  列表滚动、纵向避让（往上翻）、横向避让（贴右边往左长）。
- **两个坑**：
  1. 触发器是 `<button>`，外面那层 `<label>` 会把点击再转发给它，toggle 两次等于没反应。
  2. 只做纵向避让不够：设置页的下拉都在行右端，`left:0 + width:max-content` 向右生长会
     撑出横向滚动条，容器一滚行标题就被推出可视区（"目标语言"只剩"…哪种语言"）。

### 真机验证过的

- 弹窗、设置页都是圆角 + 原生投影，无红绿灯，常驻路径不再崩
- 下拉在两个窗口里长相一致，开在窗口内不溢出
- 键盘：Tab 聚焦、空格展开、↑↓ 移动、Enter 选中
- 选中后自动重译：提示词从「通用」换到「科学杂志/论文」，system 从 174 → 243 字符，
  译文变成带英文对照的学术风格
- 设置页四个 comboRow 全部换新，行标题不再被挤掉

### 未验 / 已知

- 底部「模型」下拉的**向上翻**没能在真机上摆拍到（Tab 序没数准，只有 2 个选项）。
  逻辑写了、纵向避让代码路径和顶部那个共用，但没有截图为证。
- 字体那三个搜索框没有 caret，和其余下拉的 affordance 略不同——是搜索 vs 选择的
  差异，暂按有意为之处理。

---

## 用户指出的漏检 + 补流程债（2026-08-20 傍晚）

### 🐞 我漏检了：下拉根本没浮起来

用户指出「设置界面下拉选项依旧有问题，是显示在那个小框里面的，不是浮层」。
**他是对的，而且线索早就在我自己的截图里**——「目标语言」有 21 个语种，展开后只露出 2 个，
我当时只核对了「行标题回来了」就宣布成功，没问一句「为什么只有 2 项」。

裁剪源头两层，都是设置页本来就有的：`.rows { overflow: hidden }`（为了卡片圆角）+
`.pane { overflow-y: auto }`（面板滚动）。绝对定位的弹层跑不出去。

修法：浮层挂到 `document.body` 用 fixed 定位，位置由 JS 按锚点算（`placeFloating`）。
配套：捕获阶段的 scroll 监听重新定位（fixed 浮层不会跟着内层容器滚）；关闭时把浮层挪回
原宿主（设置页切标签会 `replaceChildren` 重建整页，留在 body 上会堆孤儿）。
字体那个带搜索的下拉有同样的毛病，一并改。

**教训**：视觉验证不能只核对「我改的那一处」，要问「画面上每一处不寻常的地方是为什么」。

### 🐞 顺带撞出来的：托盘模式下关设置窗口 → 整个程序退出

排查上面那个问题时，进程莫名消失。不是崩溃——没有崩溃报告，stderr 只有一条无害的输入法
消息，是**干净退出**。

根因：Tauri 默认「所有窗口都关了就退出」。弹窗靠 `prevent_close` 只藏不销毁绕开了；
设置窗口是真销毁的，一关就触发 `ExitRequested`。表现是：在设置页按一次 Esc，
托盘图标没了、全局快捷键失效，用户完全不知道发生了什么。**三平台通用。**

修法：`run()` 的事件回调里拦 `ExitRequested`，仅托盘模式 `prevent_exit`。
设置窗口刻意不学弹窗「只藏不销毁」——托盘菜单能直接打开到某个标签页，复用窗口时没法换页。

### 流程债已补：规格 + 票

用户要求「流程该补的急须补」。已补：

- `spec.md`（阶段 3）：目标、验收标准、范围、8 条硬约束、**跨平台一致性的接缝在哪**、
  未决问题、已知限制。
  其中一条一般规律值得记住：**凡是「由操作系统画」的东西，都是一致性的漏点**
  （原生 select 弹层就是这么暴露的）。
- `tickets/`（阶段 4）：T-00 归档已完成的 6 个提交，T-01..T-12 是剩余工作，
  带 `blocked-by` / `files`。

**并行波次**（`files` 两两不重叠的可同时开工）：

| 波 | 票 | 说明 |
|---|---|---|
| 1 | T-01（Rust 窗口层）、T-02/T-03/T-04（前端动效）、T-06（Windows 取词）、T-08（图标）、T-09（文档）、T-10（验证） | 互不重叠，可并行 |
| 2 | T-05（依赖 T-03）、T-07（快捷键可配置，跨 Rust+前端，和 T-01 都碰 hotkey/main 附近，建议错开） | |
| 3 | T-11（依赖 T-07）、T-12（Linux 回归，依赖前面所有改动落地） | |

T-12 必须最后做，而且**只能在 Arch 工作站上做**。

### 事故

写票的脚本里 `cd tickets` 失败（那个目录在早先 `rm -rf .scratch` 时没了，一直没在
`docs/workbench/` 下重建），脚本没用 `set -e` 继续跑，13 个票文件全写进了仓库根。
已归位。同时把 `T-00-已完成.md` 改成 ASCII 名 `T-00-done.md`——中文路径这轮已经坑过一次。

---

## 波次 1 执行（2026-08-20 傍晚—夜）

串行做完（宿主不支持 sub-agent，按 CLAUDE.md 的规定在当前会话内串行，产出格式不变）。

| 票 | 结果 | 验证 |
|---|---|---|
| T-01 取词双跑 | ✅ | 三场景真机验：常驻+AX、独立启动、常驻+⌘C 兜底，「开始取词」都只出现一次 |
| T-02 进退场 | ✅ | 部分——录像逐帧能看到窗口从无到有，150ms 的位移在抽帧尺度分辨不出 |
| T-03 流式渐现 | ✅ | 部分——译文渲染正确，120ms 的分片渐现没分辨出来 |
| T-04 骨架屏 | ✅ | 截图确认 |
| T-05 交叉淡出 | ✅ | 录像逐帧确认（旧译文逐帧变淡、骨架屏同时铺开） |
| T-08 托盘模板图标 | ✅ | 光栅化有单测；外观导出到浅/深底看过；**菜单栏实景没验成** |
| T-09 发版说明 | ✅ | 顺带修掉 release.yml 一句假话 |
| T-10 补验向上翻 | ✅ | 截图确认 |
| T-06 Windows UIA | ⬜ | 未开工 |

### 这一轮学到的验证手法（下次直接用）

- **`popup --input` 是进入弹窗焦点链的确定入口**（`apply()` 会 focus 原文框），
  从那儿数 Tab 比从窗口激活状态数可靠得多。
- **动画要用录像抽帧验，不能用截图**。`screencapture` 自己就要 200ms+，抓不到
  120~150ms 的动画。`screencapture -v -V <秒>` 录像 + `ffmpeg fps=N,tile=RxC` 拼联络表，
  再按时间窗 `-ss/-t` 密集抽帧。注意 `tile` + `-frames:v 1` 只取**开头** N 帧，
  要先粗看一遍定位时间窗。
- **看不见的东西要导出来看**。托盘图标在本机菜单栏被挤掉了，就把光栅化结果 dump 成
  RGBA、用 ffmpeg 叠到浅色/深色底上看——比在菜单栏里找它可靠。
- **凡是「只表现成看不见」的失败，都该有测试兜底**。托盘图标画成空白、或 mask 没生效
  变黑块，在菜单栏上都只是"图标不见了/很怪"，很难往光栅化上想，所以补了单测卡两头。

### 顺带修掉的问题（不在票里）

- 光标原本只看 `.streaming`，等首个 token 的几秒里会和骨架屏一起出现，两个占位叠着。
- `release.yml` 写着「macOS / Windows 上在设置里直接配」快捷键——设置界面根本没有改键
  入口（那是 T-07）。改成如实说明当前写死。

### 下一步

T-06（Windows UIA 取词）是波次 1 最后一张，也是**本机完全验不了**的一张：
`cargo check --target x86_64-pc-windows-msvc` 卡在 `aws-lc-sys`（要能编 Windows 的 C 工具链），
连类型检查都做不了。只能盲写 + CI + 用户的 Windows 机器。

## 2026-08-20 晚（Arch 侧，T-12 Linux 回归）

- [21:00] 基线 | pull 后 fmt / clippy -D warnings / 33 测试 / pnpm build 在 Arch 上全绿，三个 package 都能编 | 跑起来
- [21:02] 坑 | **`cargo build --release` 编出的 Tauri 二进制跑不起来**：窗口空白 + `Connection refused`，它去连 devUrl 了。根因是 tauri 的 build.rs `let dev = !has_feature("custom-protocol")`，只有 `tauri build` 会加这个特性，跟 profile 无关。票里写的命令是错的，已订正，并写进 README 与 docs/切到-tauri-版.md | 用 pnpm tauri build --no-bundle 重编
- [21:03] 回归 1、2 | 常驻后**第一次**触发就走完取词→翻译→完成，无空弹窗；「开始取词」只出现 1 次。用 `wl-copy --primary` 造划词状态驱动整条快捷键通路 | 第 3 条
- [21:12] 回归 3 | Esc 关设置窗口后进程存活、SNI 仍注册、后续 popup 照常翻译。后来误触 ✕ 时又复现一次，两次都对 | 第 4 条
- [21:05] 坑 | **Tauri 的 niri 窗口规则从没装到本机**：仓库 data/niri-snippet.kdl 里有，用户 ~/.config/niri/ 下只有 GTK 那两条（install.sh 是安装动作，验证时不该跑）。**已备份后手工加上**（`selectiontranslation.kdl.bak-20260820-210514`），niri validate 通过。install.sh 是整份覆盖该文件的，重跑不会漂移 | 第 4 条
- [21:06] 回归 4 | 规则生效后弹窗 is_floating=True、560×480、右上角；设置窗口按标题匹配到 900×700 | 第 5 条
- [21:08] 回归 5 | 自绘下拉在 WebKitGTK 上全通：顶栏展开不被裁、底栏**向上翻**、设置页目标语言下拉盖过好几行没被滚动容器裁；空格展开 / ↑↓ / Enter / Esc 只收下拉不关窗；标签栏也能 Tab+空格切页。「浮层改挂 body」那个修复生效 | 第 6 条
- [21:16] 回归 6 | wf-recorder 60fps + ffmpeg 抽帧：骨架屏三条微光在扫、spinner 在转、首 token 到达后换成文字并出现闪烁光标。**光标只在有文字后才出现**，那个修复生效。无卡顿 | 第 7 条
- [21:01] 回归 7 | 托盘图标放大 8 倍看：深色圆角方块 + 薄荷绿选区条 + 深色箭头镂空，箭头清晰可辨。（一开始没认出来，是用户提醒「图标变了而已」） | 第 8、9 条
- [21:19] 回归 8、9 | 快捷键分组是只读列表无录制按钮；依赖自检四项全绿 + 绿色汇总条 | 收尾
- [21:22] 收尾 | T-12 status→done，九条结果与 Linux 侧验证手法写进票里。safekey.sh / shoot.sh 从不入库的 .scratch 挪进 docs/workbench/tools/（票是入库的，引用 .scratch 到了 Mac 上就是断的），配 README 说明理由 | 只剩 T-13
- [2026-08-20] Windows 收尾 | 按 source-command-autopilot 读取交接/发版文档；项目未提供 AGENTS.md/项目适配块，先以文档中的 Windows 验收清单为准，准备核对 GitHub v0.3.0 草稿 Release 并在本机实测安装/运行 | 先确认 Release 产物与 Actions 状态
- [2026-08-21] 定级与适配 | 任务按中度交付验证推进；确认仓库无 AGENTS.md/CLAUDE.local.md，改以 docs/发版.md、T-06 和 CI 定义为验收依据 | 检查 GitHub Release 与 Actions
- [2026-08-21] Windows 验收准备 | 将验收落到安装器完整性、启动/单实例/托盘、首次引导、设置与快捷键、UIA/剪贴板兜底及剪贴板还原 | 开始 GitHub 与 Windows 真机操作
- [2026-08-21] GitHub 发版核对 | v0.3.0 workflow run 32393582175 成功，Linux/macOS/Windows 三个矩阵 job 全绿；草稿 Release 有 9 个资产 | 下载 Windows 两种安装包
- [2026-08-21] 安装包完整性 | MSI 5,402,624 bytes、NSIS 3,857,172 bytes，两个 SHA-256 均与 GitHub digest 完全一致 | 读取包元数据与签名状态
- [2026-08-21] 包元数据 | 两个包均 NotSigned；NSIS x64、ProductVersion/FileVersion 0.3.0；MSI ProductName=seltrans、Manufacturer=brownie、ALLUSERS=1、UpgradeCode 稳定 | 需要用户确认后安装并运行新下载的 Windows 软件
- [2026-08-21] 人工介入点 | 用户明确确认安装并运行，要求持续验证到结束并修复发现的问题 | 静默安装 NSIS 后核对落盘结果
- [2026-08-21] Windows 安装首轮 RED | NSIS /S 返回 0 且写入 HKCU 卸载项，但 %LOCALAPPDATA%/seltrans 为空，预期的应用与 uninstall.exe 均不存在 | 用包内容/安全事件/路径定位排查，再交互复现
- [2026-08-21] 安装排查结论 | 排除包漏载荷/Defender 隔离/路径错误；NSIS 父进程退出后仍有延迟落盘，最终 seltrans-tauri.exe 14,868,480 bytes 与 uninstall.exe 均存在 | 后续验收等待目标文件出现，继续启动验证
- [2026-08-21] 启动前检查 | 已安装二进制 x64 0.3.0、NotSigned；--help 正确打印 Windows 配置/日志路径和可改快捷键说明 | 启动 tray，检查进程/日志/首次提示
- [2026-08-21] Windows 基础启动 | tray 常驻、托盘注册、两枚全局快捷键注册成功；第二实例 popup --input 0 退出并由常驻实例打开主窗口，首次七步指引显示正常 | 从指引进入设置页
- [2026-08-21] Windows 设置页 RED | “划词翻译 · 设置”窗口能创建但永久纯白，无 RootWebArea；主弹窗和 WebView2 Runtime 正常，阻断配置供应商/改键 | 检查设置窗口 URL 与前端启动异常，补回归测试后修复
- [2026-08-21] 设置白屏根因 | 仅 settings providers/引导 page=providers 白屏，settings 无参数正常；WebviewUrl::App(PathBuf) 被拼入 ?page，Windows 将其当非法资源文件名，Linux/mac 未暴露 | 固定资源路径并用初始化数据传目标页
- [2026-08-21] 设置修复 GREEN | 新增资源路径回归测试与平台文案测试；URL 固定 settings.html，前端读初始化页；Windows/mac 零副作用模式恢复可选并换成 UIA/辅助功能文案；前端测试 2/2、pnpm build 通过 | 本机无 Rust/MSVC，先验原 v0.3.0 其余 Windows 运行时
- [2026-08-21] Windows UIA 首验 | 记事本选中固定 73 字符后 Alt+Shift+T 成功；日志为“UIA 命中：焦点元素直接持有选区”，剪贴板哨兵前后不变；首次指引完成并可收起弹窗，常驻进程存活 | 安装本机构建环境，重打修复包
- [2026-08-21] 构建环境准备 | 用户授权安装需要的环境、打包、继续全量 Windows 验证并在全绿后提交；即将安装 Rust stable MSVC 与 VS 2022 C++ Build Tools/Windows SDK | 安装后跑 Rust/前端检查
- [2026-08-21] 构建环境完成 | VS Build Tools 17.14.39 + VC x64/Windows SDK、rustup 1.29 + stable x86_64-pc-windows-msvc 已安装，系统无重启要求 | 先跑设置白屏与平台文案的最窄回归测试
- [2026-08-21] 最窄回归 GREEN | Rust `指定设置标签页不会污染打包资源路径` 通过，前端 Windows/Linux 平台文案测试 2/2 通过 | 扩大到 fmt、clippy、workspace 测试与前端生产构建
- [2026-08-21] 扩大检查第 1 轮 | 前端测试 2/2、pnpm build 通过；cargo fmt --check 仅报 settings_page_script 链式调用需换行 | 执行 rustfmt 后重验，再跑 clippy/workspace tests
- [2026-08-21] 扩大检查 GREEN | rustfmt 通过；clippy core+Tauri all-targets -D warnings 通过；workspace 测试 32/32（core 28 + Tauri 4）通过；前端测试 2/2 与 pnpm build 通过 | 本机 tauri build 打 MSI/NSIS
- [2026-08-21] 打包第 1 轮 | Tauri CLI 在 cargo metadata 前失败：桌面会话旧 PATH 未包含刚安装的 ~/.cargo/bin，未进入产品编译 | 给构建进程显式补 Cargo PATH 后重跑
- [2026-08-21] Windows 打包 GREEN | `pnpm tauri build --bundles msi,nsis` 成功，release 编译 3m05s，产出 target/release/bundle/msi 与 nsis 两个 0.3.0 x64 安装包 | 校验本地包后覆盖安装修复版
- [2026-08-21] 修复包覆盖安装 | NSIS /S 返回 0，安装 exe 14,895,616 bytes、0.3.0；与 target 二进制唯一差异为 bundler 将 UNKNOWN 标记写成 NSIS，符合 Tauri 打包行为 | 启动修复版复验 settings providers 与平台文案
- [2026-08-21] 设置修复真机 GREEN | 修复包 `settings providers` 直接进入供应商页且非白屏；通用页显示 Windows UIA/Ctrl+Insert 文案，无 Wayland/niri；“仅 UI Automation”可选择落盘，已恢复自动；关设置后常驻进程仍在 | 重验 UIA 与模拟复制两条取词路线
- [2026-08-21] Windows 取词真机 GREEN | 修复包记事本 auto 走 UIA 直取 73 字符且剪贴板不变；clipboard 模式日志确认 Ctrl+Insert 取到 73 字符且哨兵还原，已恢复 auto；Chrome example.com UIA 直取 101 字符且剪贴板不变 | 用 VS Code/Electron 尝试覆盖祖先链回退
- [2026-08-21] Electron/Word 真机 GREEN | VS Code UIA 扫 10 层无选区后按设计降级 Ctrl+Insert，取到 87 字符且剪贴板还原；Word UIA 直取 75 字符且剪贴板不变；测试窗口均已关闭 | 对 MSI 做 administrative extraction 验证
- [2026-08-21] MSI 验证 GREEN | administrative extraction 退出 0，完整解出 14,895,616 bytes 的 seltrans-tauri.exe 0.3.0 | 更新 README/规格/T-06/Release 文案并写 delivery.md
- [2026-08-21] 文档收干 | README 与规格移除 Windows/CI 未验证说法；T-06 记录五类真机结果；release workflow 改为“mac/Win 可在设置改键”；delivery.md 汇总交付与限制 | 跑文案相关检查并自审 diff
- [2026-08-21] 最终检查 | pnpm build（含平台文案 3/3）通过、cargo fmt 通过、diff check 通过、无 DEBUG 标记；此前 clippy/32 项 Rust 测试/Windows 双安装包构建均通过 | 自审后提交
- [2026-08-21] 自审与国际化 | 资源路径有白名单与 JSON 转义，模式值保持后端协议，测试已接入 CI；无新发现；项目无适配块且未声明 I18N 流程，沿用现有中文直写 | 精确暂存本次文件并提交
