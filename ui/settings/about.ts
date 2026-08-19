/**
 * 「关于」页：版本、日志、依赖自检（对照 GTK 版的 `settings_ui.rs::build_about`）。
 *
 * 这一页的真正用处不是显示版本号，而是**出问题时的自助排查入口** ——
 * 日志在哪、怎么看、缺哪个依赖，都在这里一次讲清楚，省掉一轮来回。
 */

import { api, type AboutInfo } from "../lib/api";
import { actionRow, addRow, button, group, notice, statusRow } from "../lib/dom";
import type { Ctx } from "../lib/shell";

/** 平台标识给个人话的说法，`std::env::consts::OS` 那几个字母不是给人看的 */
const OS_LABEL: Record<string, string> = {
  linux: "Linux",
  macos: "macOS",
  windows: "Windows",
};

/** 取所在目录。文件管理器里定位配置文件比直接用编辑器打开它更安全 */
function parentDir(path: string): string {
  const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return i > 0 ? path.slice(0, i) : path;
}

export async function render(pane: HTMLElement, ctx: Ctx): Promise<void> {
  let info: AboutInfo;
  try {
    info = await api.about();
  } catch (e) {
    ctx.status(`读取版本信息失败：${e}`, "error");
    pane.append(notice(`读取版本信息失败：${e}`, "warn"));
    return;
  }

  // 打开动作统一走这里：openPath 走的是系统默认程序，失败原因五花八门（没装
  // xdg-open、路径不存在……），每处都得把原因说出来，不能静悄悄没反应
  const open = (path: string, what: string) => async () => {
    try {
      await api.openPath(path);
    } catch (e) {
      ctx.status(`打不开${what}：${e}`, "error");
    }
  };

  // ---------- 程序本身 ----------

  const g = group("selectionTranslation");
  addRow(g, actionRow("版本", info.version));
  addRow(g, actionRow("运行平台", OS_LABEL[info.os] ?? info.os));
  addRow(
    g,
    actionRow("仓库地址", info.repoUrl, button("打开", open(info.repoUrl, "仓库主页"))),
  );
  addRow(
    g,
    actionRow(
      "配置文件",
      `${info.configPath}　里面有 API key，文件权限是 0600（只有你自己能读），发日志或截图求助时别把它带上。`,
      button("打开所在目录", open(parentDir(info.configPath), "配置目录")),
    ),
  );
  pane.append(g);

  // ---------- 日志 ----------

  const gLog = group(
    "运行日志",
    "取词结果、实际发给模型的内容、服务端返回的状态都记在这里。翻译结果不对时先看它。",
  );
  addRow(
    gLog,
    actionRow(
      "日志文件",
      `${info.logPath}　当前 ${info.logSizeKb} KB`,
      button("打开", open(info.logPath, "日志文件")),
    ),
  );
  addRow(
    gLog,
    actionRow("轮转与隐私", "超过 1 MB 自动轮转，不会无限长大。日志里不记录 API key。"),
  );
  addRow(
    gLog,
    actionRow(
      "译文不对时看哪一行",
      "找「发起翻译」那一行的 user 字符数 和 user 预览 —— 那才是真正发给模型的内容，" +
        "跟你以为选中的往往不一样（多选了一段、少选了半句、复制到的是旧剪贴板）。" +
        "预览里会把肉眼看不见的字符标出来：<ZWSP> 零宽空格、<BOM> 字节序标记、<NBSP> 不换行空格。",
    ),
  );
  pane.append(gLog);

  // ---------- 依赖自检 ----------

  const gDeps = group("依赖自检", "取词、模拟按键这些事要靠系统上的外部程序，缺了会直接影响能不能用。");
  for (const d of info.deps) {
    addRow(gDeps, statusRow(d.name, d.ok, d.note));
  }
  pane.append(gDeps);

  const bad = info.deps.filter((d) => !d.ok);
  if (info.deps.length === 0) {
    pane.append(notice("这个平台还没有依赖自检项。", "info"));
  } else if (bad.length === 0) {
    pane.append(notice("依赖齐全，取词该走的路都通。", "ok"));
  } else {
    pane.append(
      notice(
        `有 ${bad.length} 项没通过：${bad.map((d) => d.name).join("、")}。` +
          "按上面每一行的说明装好或启动对应服务，取词才不会时灵时不灵。",
        "warn",
      ),
    );
  }
}
