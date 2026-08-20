#!/usr/bin/env bash
# 安全按键：给验证用。无论如何退出，都保证把所有修饰键抬起来。
#
# 为什么必须有这个东西：2026-08-19 那次事故就是模拟按键之后修饰键卡在按下状态，
# 整台电脑没法操作。用户此刻在睡觉，卡住了没人能救。所以：
#   - trap 覆盖正常退出、出错退出、被中断、被 kill
#   - 按键之前先抬一次（用户可能正按着别的键）
#   - 按键之后再抬一次
#
# 用法：
#   safekey.sh key 1:1 1:0                # 直接透传给 ydotool key
#   safekey.sh esc                        # 常用键的别名
#   safekey.sh ctrl+c / ctrl+enter / tab
#   safekey.sh release                    # 只抬修饰键，什么都不按

set -uo pipefail

# 左右 Ctrl / Shift / Alt / Super 的 evdev 键码
MODS=(29 97 42 54 56 100 125 126)

release_all() {
    local args=()
    for k in "${MODS[@]}"; do args+=("$k:0"); done
    ydotool key "${args[@]}" >/dev/null 2>&1 || true
}

# 正常退出、报错、被打断、被终止 —— 全都要抬
trap release_all EXIT INT TERM HUP

# 键码
K_ESC=1 K_ENTER=28 K_CTRL=29 K_C=46 K_TAB=15 K_A=30

press() { ydotool key "$@"; }

release_all
sleep 0.15

case "${1:-}" in
    release)     ;;                                    # trap 会处理
    esc)         press $K_ESC:1 $K_ESC:0 ;;
    tab)         press $K_TAB:1 $K_TAB:0 ;;
    enter)       press $K_ENTER:1 $K_ENTER:0 ;;
    ctrl+c)      press $K_CTRL:1 $K_C:1 $K_C:0 $K_CTRL:0 ;;
    ctrl+a)      press $K_CTRL:1 $K_A:1 $K_A:0 $K_CTRL:0 ;;
    ctrl+enter)  press $K_CTRL:1 $K_ENTER:1 $K_ENTER:0 $K_CTRL:0 ;;
    key)         shift; press "$@" ;;
    *)           echo "用法: safekey.sh {release|esc|tab|enter|ctrl+c|ctrl+a|ctrl+enter|key <码...>}" >&2
                 exit 2 ;;
esac

sleep 0.1
# trap 在这里再抬一次
