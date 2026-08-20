#!/usr/bin/env bash
# selectionTranslation 安装脚本
#
#   ./install.sh                 编译 + 安装二进制/图标/桌面项 + niri 快捷键与窗口规则
#                                + 开启开机自启动并立刻拉起托盘
#   ./install.sh --no-niri       不动 niri 配置
#   ./install.sh --no-autostart  不设开机自启动、不启动托盘
#   ./install.sh --uninstall     卸载
#
# 改动 niri 配置前会先备份 config.kdl。全程不需要 root。

set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"
NIRI_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/niri"
NIRI_SNIPPET="${NIRI_DIR}/selectiontranslation.kdl"
NIRI_CONFIG="${NIRI_DIR}/config.kdl"
INCLUDE_LINE='include "selectiontranslation.kdl"'
APP_ID="xyz.brownie.SelectionTranslation"
DESKTOP_FILE="${APP_ID}.desktop"
ICON_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"
AUTOSTART_FILE="${XDG_CONFIG_HOME:-$HOME/.config}/autostart/${DESKTOP_FILE}"

info() { printf '\033[1;34m::\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!!\033[0m %s\n' "$*"; }
ok()   { printf '\033[1;32m✓\033[0m %s\n' "$*"; }

uninstall() {
    info "卸载 selectionTranslation"
    for p in $(pgrep -x seltrans 2>/dev/null); do kill "$p" 2>/dev/null; done
    rm -f "${BIN_DIR}/seltrans" && ok "已删除 ${BIN_DIR}/seltrans"
    rm -f "${APP_DIR}/${DESKTOP_FILE}" && ok "已删除桌面项"
    rm -f "${ICON_DIR}/${APP_ID}.svg" && ok "已删除图标"
    rm -f "$AUTOSTART_FILE" && ok "已关闭开机自启动"
    if [[ -f "$NIRI_SNIPPET" ]]; then
        rm -f "$NIRI_SNIPPET"
        ok "已删除 $NIRI_SNIPPET"
    fi
    if [[ -f "$NIRI_CONFIG" ]] && grep -qF "$INCLUDE_LINE" "$NIRI_CONFIG"; then
        cp -a "$NIRI_CONFIG" "${NIRI_CONFIG}.bak-$(date +%Y%m%d-%H%M%S)"
        grep -vF "$INCLUDE_LINE" "$NIRI_CONFIG" > "${NIRI_CONFIG}.tmp"
        mv "${NIRI_CONFIG}.tmp" "$NIRI_CONFIG"
        ok "已从 config.kdl 移除 include 行（原文件已备份）"
    fi
    warn "配置文件 ${XDG_CONFIG_HOME:-$HOME/.config}/seltrans/ 保留着（里面有你的 API key），如需一并删除请手动 rm -rf"
    exit 0
}

DO_NIRI=1
DO_AUTOSTART=1
for arg in "$@"; do
    case "$arg" in
        --uninstall)    uninstall ;;
        --no-niri)      DO_NIRI=0 ;;
        --no-autostart) DO_AUTOSTART=0 ;;
        -h|--help)      sed -n '2,11p' "$0"; exit 0 ;;
        *) warn "未知参数：$arg"; exit 2 ;;
    esac
done

# ---- 依赖检查 ----
# 装的是 Tauri 版：界面是 HTML/CSS/TS，由 WebKitGTK 渲染。
# libgtk-3 不是"界面还是 GTK"，那是 WebKitGTK 的开窗层（mac 上对应 AppKit）。
command -v cargo >/dev/null || { warn "没有 cargo，请先装 rust：pac rust"; exit 1; }
command -v pnpm  >/dev/null || { warn "没有 pnpm，前端编不了：pac pnpm"; exit 1; }
pkg-config --exists webkit2gtk-4.1 || { warn "缺少 webkit2gtk-4.1：pac webkit2gtk-4.1"; exit 1; }
pkg-config --exists gtk+-3.0 || { warn "缺少 gtk3：pac gtk3"; exit 1; }
command -v wl-paste >/dev/null || warn "没有 wl-paste，主选区取词会不可用：pac wl-clipboard"
command -v ydotool  >/dev/null || warn "没有 ydotool，Ctrl+C 兜底取词会不可用：pac ydotool"

# ---- 编译 ----
#
# **必须走 tauri build，不能用 cargo build --release。**
# Tauri 靠 `tauri` 依赖有没有开 custom-protocol 特性判断 dev / 生产
# （它的 build.rs 里就一行 `let dev = !has_feature("custom-protocol")`），
# 只有 tauri build 会加。cargo 编出来的会去连 devUrl，窗口一片空白。
info "编译 release 版本（第一次会久一点）"
( cd "$REPO_DIR" && pnpm install --frozen-lockfile && pnpm tauri build --no-bundle )

# ---- 安装二进制 ----
#
# 装成 `seltrans` 这个名字，niri 里的 `spawn "seltrans"` 就不用改。
mkdir -p "$BIN_DIR"
install -m755 "${REPO_DIR}/target/release/seltrans-tauri" "${BIN_DIR}/seltrans"
ok "已安装 ${BIN_DIR}/seltrans（$(du -h "${BIN_DIR}/seltrans" | cut -f1)）"

case ":${PATH}:" in
    *":${BIN_DIR}:"*) ;;
    *) warn "${BIN_DIR} 不在 PATH 里，niri 的 spawn 可能找不到 seltrans" ;;
esac

# ---- 图标 ----
mkdir -p "$ICON_DIR"
install -m644 "${REPO_DIR}/data/${APP_ID}.svg" "${ICON_DIR}/${APP_ID}.svg"
command -v gtk-update-icon-cache >/dev/null \
    && gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" >/dev/null 2>&1 || true
ok "已安装图标"

# ---- 桌面项 ----
mkdir -p "$APP_DIR"
install -m644 "${REPO_DIR}/data/${DESKTOP_FILE}" "${APP_DIR}/${DESKTOP_FILE}"
command -v update-desktop-database >/dev/null && update-desktop-database "$APP_DIR" 2>/dev/null || true
ok "已安装桌面项"

# ---- niri ----
if [[ $DO_NIRI -eq 1 ]]; then
    if [[ ! -d "$NIRI_DIR" ]]; then
        warn "没找到 $NIRI_DIR，跳过 niri 配置"
    else
        install -m644 "${REPO_DIR}/data/niri-snippet.kdl" "$NIRI_SNIPPET"
        ok "已写入 $NIRI_SNIPPET"

        if [[ -f "$NIRI_CONFIG" ]]; then
            if grep -qF "$INCLUDE_LINE" "$NIRI_CONFIG"; then
                ok "config.kdl 里已经有 include 行，不重复添加"
            else
                backup="${NIRI_CONFIG}.bak-$(date +%Y%m%d-%H%M%S)"
                cp -a "$NIRI_CONFIG" "$backup"
                ok "已备份 config.kdl → $(basename "$backup")"
                printf '\n// selectionTranslation 划词翻译\n%s\n' "$INCLUDE_LINE" >> "$NIRI_CONFIG"
                ok "已在 config.kdl 末尾追加 include 行"
            fi

            if command -v niri >/dev/null && niri validate --config "$NIRI_CONFIG" >/dev/null 2>&1; then
                ok "niri validate 通过（niri 会自动热重载，无需重启）"
            else
                warn "niri validate 没通过，请手动检查：niri validate"
            fi
        else
            warn "没找到 $NIRI_CONFIG，请自行 include 一下 selectiontranslation.kdl"
        fi
    fi
fi

# ---- 托盘常驻 / 开机自启 ----
if [[ $DO_AUTOSTART -eq 1 ]]; then
    "${BIN_DIR}/seltrans" autostart on >/dev/null && ok "已开启开机自启动（登录后自动常驻托盘）"
    for p in $(pgrep -x seltrans 2>/dev/null); do kill "$p" 2>/dev/null; done
    sleep 0.5
    setsid "${BIN_DIR}/seltrans" tray >/dev/null 2>&1 </dev/null &
    sleep 1.5
    if pgrep -x seltrans >/dev/null; then
        ok "托盘已启动，顶栏右侧应该出现一个蓝色的「A文」图标"
    else
        warn "托盘没起来，手动跑一下看报什么：seltrans tray"
    fi
fi

cat <<'EOF'

安装完成。下一步：

  1. 按 Mod+Alt+T 打开配置界面（或运行 seltrans settings）
  2. 到「供应商」页点 +，选一个预设 —— base_url 会自动填好
  3. 填上 API key，点「拉取列表」挑一个模型，再点「测试连接」确认通了
  4. 随便选中一段文字，按 Mod+Shift+T

托盘图标：左键点一下 = 翻译当前选中的文本；右键是完整菜单
（切风格、切供应商、设置、看日志、开机自启动、退出）。

弹窗里也可以随时切换翻译风格（通用 / GitHub / 科学杂志 / 口语 / 术语解释 / 报错解读 / 中译英），
切换后会立刻用新风格重译。

EOF
