# 进度日志（滚动）

格式：`- [时间] 环节 | 做了什么(产物/结论) | 下一步`
一行一条，不粘贴产物内容。超约 300 行时把本月之前的条目挪进 `progress.YYYY-MM.log.md`。

---

- [2026-08-20 12:36] 环境 | 在 Arch 工作站接入个人技能库(agents-config)，装 Claude + Codex 两条通道，适配块落 CLAUDE.local.md，体检干净 | 切工作台为入库模式
- [2026-08-20 12:36] 环境 | 工作台从 `.scratch/`(本地)切到 `docs/workbench/`(入库)，为 Arch ↔ Mac 接续开发同步过程状态；占用锁仍留本地 `.worktrees/.claims/` | push 后到 Mac 上 pull
- [2026-08-20 12:36] 交接 | 代码侧最新为 26e6d07(修 Windows 编译：0600 权限是 Unix 专属)，之前两提交完成三平台打包与版本 0.2.0 | Mac 上先跑 install-skills + /setup-adapter 再继续
- [2026-08-20 18:00] 环境 | Mac 侧装 rustc 1.97.1(aarch64-apple-darwin)，mac 分支**首次原生编译通过**；注意本机没有 gtk4，所有 cargo 命令要带 `--exclude seltrans` | 见 cross-platform-ready/progress.log.md
- [2026-08-20 18:00] 定级 | 「三平台可用」定为重度，走 HEAVY；规格与 13 张票入库 `cross-platform-ready/` | 12 张 done，只剩 T-12
- [2026-08-20 18:00] 修 bug | mac 真机跑出 4 个只有运行时才暴露的缺陷：启动即 abort(缺 global-shortcut 插件注册)、常驻后首次触发开空弹窗、取词一次触发跑两遍、托盘模式关设置窗口整个程序退出 | **后三个都影响 Linux**，见 T-12
- [2026-08-20 18:00] 功能 | 快捷键改默认值并做成可配置(T-07)、Windows 加 UIA 取词(T-06)、下拉全部改自绘并挂 body 做浮层、动效四项、README 三平台化 | Linux 侧全部待回归
- [2026-08-20 18:00] 图标 | 新图标在 22px 托盘尺寸下字形糊掉，补了 `data/tray-small.svg` 供 Linux 托盘 22/32 档用，单色版重做 | Arch 上要肉眼确认托盘观感
- [2026-08-20 18:00] 交接 | 代码侧最新 ea36869；检查全绿(fmt/clippy -D warnings/33 测试/pnpm build/Windows 目标 0 问题) | **Arch 上先做 T-12，清单已可直接执行**
- [2026-08-20 18:40] 发布 | 版本号三处对齐到 0.3.0；**只在本机打了 macOS 包**（.app + .dmg，arm64，5MB），没打 tag、没走发版 CI、其他平台不发布 | 要发正式版先过 T-12
- [2026-08-20 18:40] 修 CI | 上一次 push 把 ci.yml 弄挂了：python 切片 end<start 把 tray.rs 的 Linux 段复制了一遍，加一处 dead_code。mac 上发现不了（那段是 cfg(linux)，本机不编译）。已修并绿 | 教训：改编不了的代码前先用 rustfmt 验结构 + /tmp 壳子做交叉检查
- [2026-08-20 18:40] 文档 | README 加三张截图（弹窗/风格下拉/设置页），用 screencapture -o -l <windowid> 只截窗口带 alpha，GitHub 浅深两种主题都验过 | —
- [2026-08-20 18:45] 交接 | **T-13（首次使用提示）做到一半**：后端全完成但前端还没用（字段惰性，不影响现有行为），前端只抽出了 ui/lib/keys.ts | **剩余三处都在前端，T-13 票里写了逐步骤的接续说明**
- [2026-08-20 21:22] 回归 | **T-12 Linux 回归九条全过**（真机 niri）：常驻首次触发、单次取词、关设置不掉托盘、niri 摆位、自绘下拉、动效、托盘图标、快捷键只读列表、依赖自检。详见 cross-platform-ready/ | 只剩 T-13
- [2026-08-20 21:22] 坑 | **Tauri 二进制必须用 `pnpm tauri build`，`cargo build --release` 编出来的跑不起来**（`dev = !has_feature("custom-protocol")`，只有 tauri build 会加，跟 profile 无关）。已写进 README 与 docs/切到-tauri-版.md | —
- [2026-08-20 21:22] 环境 | Tauri 的 niri 窗口规则此前从没装到 Arch 上（install.sh 是安装动作，验证时不跑）。已备份后手工加进 ~/.config/niri/selectiontranslation.kdl | —
- [2026-08-20 21:22] 工具 | safekey.sh（带 trap 的安全按键）与 shoot.sh（无头嵌套会话截图）从 .scratch 挪进 docs/workbench/tools/，两台机器共用 | —
