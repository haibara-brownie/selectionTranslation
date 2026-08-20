---
id: T-00
title: 本轮已完成的修复（归档）
blocked-by: []
owner: mac
status: done
files: [src-tauri/src/*, ui/*]
---

按时间顺序，全部已提交并在 mac 真机验证：

| 提交 | 内容 |
|---|---|
| `e44c260` | mac 启动即 abort（缺 global-shortcut 插件注册）；常驻后首次触发开空弹窗；快捷键抢浏览器功能；弹窗定位；Accessory |
| `3f1f8e6` | 补上回归测试（MockRuntime），窗口层改为对运行时泛型 |
| `d76efb3` | mac 窗口圆角（原生标题栏 Overlay + 藏红绿灯，不碰私有 API） |
| `910a4cf` | 下拉全部改成自绘 |
| `6b7d5a1` | 下拉浮层改挂 body（修祖先裁剪） |
| `4083345` | 托盘常驻时关设置窗口不再退出程序 |
