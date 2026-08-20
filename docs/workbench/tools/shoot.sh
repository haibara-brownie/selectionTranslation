#!/usr/bin/env bash
# 在一个**无头**的嵌套 Wayland 会话里跑起界面并截图。
#
# 为什么要这么绕：验证的时候用户可能不在（显示器休眠就没有 wl_output，grim 截不到），
# 或者根本不该往人家正在用的桌面上弹窗口。无头会话跟真实桌面完全隔离，
# 想弹多少窗口、按多少键都不会打扰到人。
#
# 用法：shoot.sh <输出png> <要跑的命令...>

set -uo pipefail

OUT="$1"; shift
W=${SHOOT_W:-1100}
H=${SHOOT_H:-820}
WAIT=${SHOOT_WAIT:-9}

RUNDIR=$(mktemp -d /tmp/shoot.XXXXXX)
trap 'rm -rf "$RUNDIR"' EXIT

# 嵌套会话起来后干的事：跑目标命令 → 等它画完 → 截图 → 关掉合成器
# 把要跑的命令**逐个参数**转义存成脚本。
# 别图省事写 `labwc -S "$RUNDIR/inside.sh $*"` —— $* 拼接时会丢掉引号，
# `--text "a b c"` 会被拆成三个参数，程序只收到 "a"。踩过一次了。
{
    printf '#!/usr/bin/env bash\n'
    printf 'exec '
    printf '%q ' "$@"
    printf '&\n'
} > "$RUNDIR/app.sh"

cat > "$RUNDIR/inside.sh" <<INNER
#!/usr/bin/env bash
"$RUNDIR/app.sh"
sleep $WAIT
grim "$OUT" 2>"$RUNDIR/grim.err" || echo "grim 失败：\$(cat "$RUNDIR/grim.err")" >&2
labwc --exit 2>/dev/null
INNER
chmod +x "$RUNDIR/inside.sh" "$RUNDIR/app.sh"

# WLR_BACKENDS=headless 造一个虚拟输出，不需要真实显示器
# WLR_LIBINPUT_NO_DEVICES=1 别去抓真实的键盘鼠标
WLR_BACKENDS=headless \
WLR_LIBINPUT_NO_DEVICES=1 \
WLR_HEADLESS_OUTPUTS=1 \
WLR_RENDER_DRM_DEVICE=${WLR_RENDER_DRM_DEVICE:-/dev/dri/renderD128} \
XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}" \
    labwc -S "$RUNDIR/inside.sh" >"$RUNDIR/labwc.log" 2>&1

if [[ -f "$OUT" ]]; then
    echo "截图已存：$OUT"
else
    echo "没截到图。labwc 日志：" >&2
    tail -20 "$RUNDIR/labwc.log" >&2
    exit 1
fi
