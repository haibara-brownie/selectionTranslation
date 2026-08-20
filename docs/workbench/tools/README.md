# 验证用的小工具

开发过程里攒下的、验证界面行为要用的脚本。**只在开发时用，不参与构建也不随产品分发。**

放进入库的工作台是因为两台机器都要用，而且里面沉淀的是踩过的坑，不是一次性代码。

## `safekey.sh`（Linux）

给界面验证用的**安全按键**包装。

```bash
docs/workbench/tools/safekey.sh esc
docs/workbench/tools/safekey.sh ctrl+enter
docs/workbench/tools/safekey.sh key 15:1 15:0     # 透传给 ydotool
docs/workbench/tools/safekey.sh release           # 只抬修饰键
```

**为什么必须包一层**：模拟按键之后修饰键卡在按下状态，会让整台电脑没法操作
（2026-08-19 真出过一次）。所以脚本用 `trap` 覆盖正常退出、报错、中断、被 kill
四种情况，无论如何都保证把左右 Ctrl / Shift / Alt / Super 全部抬起来；
按键之前也先抬一次，因为用户可能正按着别的键。

**发键之前一定要先确认焦点**：

```bash
niri msg -j windows | jq '.[] | select(.is_focused) | .app_id'
```

显示器休眠或会话锁定时，**整个会话没有任何窗口持有焦点** —— 那时候发键既证明不了
什么，又要承担卡键风险。确认不了焦点就别发。

## `shoot.sh`（Linux）

在**无头的嵌套 Wayland 会话**里跑起界面并截图，跟真实桌面完全隔离。

```bash
docs/workbench/tools/shoot.sh /tmp/out.png ./target/release/seltrans-tauri settings
SHOOT_W=1280 SHOOT_H=720 SHOOT_WAIT=16 docs/workbench/tools/shoot.sh /tmp/out.png <命令...>
```

**什么时候用**：用户不在、显示器休眠（没有 wl_output，`grim` 直接失败），
或者不想往人家正在用的桌面上弹窗口。

**注意**：single-instance 插件会让嵌套会话里新起的实例把 argv 递给真实桌面上
那个常驻实例然后自己退出 —— 截出来会是黑屏。先 `pkill -x seltrans-tauri`。

## 直接在真实桌面上验证时

- 造「划过词」的状态：`wl-copy --primary "..."`，不用真去选中文字，也不碰剪贴板
- 录动画：`wf-recorder -f x.mp4 -r 60`，再用 ffmpeg 抽帧拼联络表。
  截图抓不到 120~150ms 的动画。
- **不要用 `pkill -f`** 关自己起的进程：模式会匹配到 Bash 工具自己的 shell，
  退出码 144。用 `pkill -x <名字>` 或 `niri msg action close-window --id <id>`。
