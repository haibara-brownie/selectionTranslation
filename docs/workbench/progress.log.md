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
