# 三平台可用 · 进度日志

## 2026-08-20

- **定级：重度**（待需求确认后开跑 HEAVY 九阶段）。理由：跨 core/tauri/ui 三层、跨三平台、
  多条工作线可并行（取词 / 窗口行为 / 动效 / 打包验证）。
- 前置核查已做完：
  - 装了 rustc 1.97.1 aarch64-apple-darwin。**mac 分支首次原生编译通过**
    （check / clippy / test 28 passed / fmt 全绿）。此前作者只在 Linux 上翻 cfg 强编过。
  - 读代码找出 6 个 mac 缺口：取词未真机验证、⌘⇧T 与浏览器冲突、弹窗无定位、
    无 Accessory 激活策略、方角无红绿灯、托盘非模板图标。
  - Cherry Studio 调研：划词助手**只支持 Win + mac，Linux 最早 2025Q4**；
    Windows 走 UI Automation + IAccessible + 低级鼠标/键盘钩子，剪贴板兜底。
    → 可借鉴点集中在 **Windows 加 UIA 取词路线**（seltrans 现在 Windows 只有 Ctrl+C 兜底）。
  - DS key：本地未找到可用凭据。**停止自行搜寻凭据，改为问用户**。
- 阻塞中：4 个需求分叉待用户确认（验收标准 / key 怎么给 / 动效范围 / Windows UIA 做不做）。
