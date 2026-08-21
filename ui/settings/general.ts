/**
 * 「通用」页：翻译 / 取词 / 外观 / 弹窗 / 后台常驻 / 快捷键。
 *
 * 对照基准是 GTK 版的 `src/settings_ui.rs::build_general`，行的顺序、文案口径都尽量对齐，
 * 方便两个界面层在迁移期并存时用户不至于找不到东西。
 */

import { api } from "../lib/api";
import {
  actionRow,
  addRow,
  comboRow,
  entryRow,
  group,
  hotkeyRow,
  notice,
  searchComboRow,
  switchRow,
} from "../lib/dom";
import type { Ctx } from "../lib/shell";
import {
  settingsPlatformCopy,
  type SelectionMode,
  type SettingsPlatformCopy,
} from "./platform-copy";

/** 目标语言下拉里「自定义…」那一项的哨兵值。真实语言名不会长这样，不怕撞。 */
const CUSTOM_LANG = "__custom__";

/** 取词方式三档。值必须和 Rust 侧 `selection_mode` 认的字符串一致。 */
const SELECTION_MODES: SelectionMode[] = ["auto", "primary", "clipboard"];

// 跟 GTK 版的 SpinRow 范围保持一致。收窄下限会让原本设得下的尺寸突然变非法，
// 用户升级后打开设置页就会看到一条报错，而他什么都没改。
const POPUP_MIN = 240;
const POPUP_MAX = 2000;

/**
 * 包一层 try/catch：后端任何一个命令返回 Err 都不该把整页搞白，
 * 拿兜底值继续渲染，错误进状态栏。
 */
async function attempt<T>(ctx: Ctx, what: string, run: () => Promise<T>, fallback: T): Promise<T> {
  try {
    return await run();
  } catch (e) {
    ctx.status(`${what}失败：${e}`, "error");
    return fallback;
  }
}

/** 改一行的副标题。只有建行时传过 subtitle 才有这个节点，所以要判空。 */
function setSub(row: HTMLElement, text: string, warn = false): void {
  const sub = row.querySelector<HTMLElement>(".row-sub");
  if (!sub) return;
  sub.textContent = text;
  sub.classList.toggle("warn", warn);
}

/**
 * 藏 / 显一行。
 *
 * 不能用 `row.hidden` —— `.row { display: flex }` 是作者样式，优先级高过 `[hidden]`
 * 那条 UA 规则，设了 hidden 行照样显示。只能直接压 display。
 */
function setRowVisible(row: HTMLElement, on: boolean): void {
  row.style.display = on ? "" : "none";
}

export async function render(pane: HTMLElement, ctx: Ctx): Promise<void> {
  // 一次性把要用的后端数据都拉齐，每个都各自兜底，一个挂了不影响别的分组
  const [langs, themes, fonts] = await Promise.all([
    attempt(ctx, "读取语言列表", () => api.targetLangs(), [] as [string, string][]),
    attempt(ctx, "读取配色列表", () => api.themeChoices(), [] as [string, string][]),
    attempt(ctx, "读取系统字体", () => api.listFonts(), [] as string[]),
  ]);
  // 平台在外壳启动时取过一次放进 ctx —— 不走 api.about()，那个会顺带做依赖自检
  // 和 stat 日志文件，为了一个字符串付这个成本不划算
  const isLinux = ctx.os === "linux";
  const platformCopy = settingsPlatformCopy(ctx.os);

  pane.append(
    buildTranslate(ctx, langs),
    buildSelection(ctx, platformCopy),
    ...buildAppearance(ctx, themes, fonts),
    ...buildPopup(ctx, platformCopy),
    await buildAutostart(ctx),
    ...(await buildShortcuts(isLinux, ctx)),
  );
}

// ---------------------------------------------------------------- 翻译

function buildTranslate(ctx: Ctx, langs: [string, string][]): HTMLElement {
  const g = group("翻译");

  // 存的是**语言名本身**不是 ISO 代码 —— 它会直接替换提示词里的 {target_lang}，
  // 对模型来说「简体中文」比「zh-Hans」稳得多
  const options: [string, string][] = [...langs, [CUSTOM_LANG, "自定义…"]];
  const known = langs.some(([v]) => v === ctx.config.targetLang);

  const customRow = entryRow(
    "自定义语言",
    known ? "" : ctx.config.targetLang,
    (v) => {
      const value = v.trim();
      if (!value) return; // 清空不代表想把目标语言设成空，忽略即可
      ctx.config.targetLang = value;
      ctx.save();
    },
    {
      subtitle: "直接写模型看得懂的语言名，比如「粤语」「文言文」「Bahasa Indonesia」",
      placeholder: "语言名",
    },
  );
  setRowVisible(customRow, !known);

  const langRow = comboRow("目标语言", "译文要输出成哪种语言", options, known ? ctx.config.targetLang : CUSTOM_LANG, (v) => {
    if (v === CUSTOM_LANG) {
      setRowVisible(customRow, true);
      const typed = customRow.querySelector<HTMLInputElement>("input.entry")?.value.trim() ?? "";
      if (typed) {
        ctx.config.targetLang = typed;
        ctx.save();
      }
      return;
    }
    setRowVisible(customRow, false);
    ctx.config.targetLang = v;
    ctx.save();
  });

  addRow(g, langRow);
  addRow(g, customRow);

  // 提示词列表归「提示词」页管，这里只选默认用哪个
  if (ctx.config.prompts.length === 0) {
    addRow(g, actionRow("默认提示词", "还没有提示词，去「提示词」页新建或恢复内置"));
  } else {
    const opts: [string, string][] = ctx.config.prompts.map((p) => [p.id, `${p.icon} ${p.name}`]);
    addRow(
      g,
      comboRow("默认提示词", "弹窗打开时默认用哪一套", opts, ctx.config.activePrompt, (id) => {
        ctx.config.activePrompt = id;
        ctx.save();
      }),
    );
  }
  return g;
}

// ---------------------------------------------------------------- 取词

function buildSelection(ctx: Ctx, copy: SettingsPlatformCopy): HTMLElement {
  const g = group("取词", copy.selectionIntro);

  const modeOf = (v: string) =>
    SELECTION_MODES.includes(v as SelectionMode) ? copy.selectionModes[v as SelectionMode] : null;
  const noteOf = (v: string) => modeOf(v)?.note ?? "";
  const opts: [string, string][] = SELECTION_MODES.map((value) => [
    value,
    copy.selectionModes[value].label,
  ]);

  const row = comboRow("取词方式", noteOf(ctx.config.selectionMode), opts, ctx.config.selectionMode, (v) => {
    ctx.config.selectionMode = v;
    ctx.save();
    // 三个选项差别不小，把当前这档的说明直接顶到副标题上——原生 select 塞不下逐项说明
    setSub(row, noteOf(v));
  });

  addRow(g, row);
  return g;
}

// ---------------------------------------------------------------- 外观

function buildAppearance(ctx: Ctx, themes: [string, string][], fonts: string[]): HTMLElement[] {
  const g = group(
    "外观",
    "配色取自 Catppuccin 官方调色板。「跟随系统」会跟着桌面的深浅色设置在 Latte 与 Mocha 之间自动切换。",
  );

  const refresh = () => {
    // 改完立刻重刷主题 CSS，不用重开窗口
    void ctx.refreshTheme().catch((e: unknown) => ctx.status(`刷新主题失败：${e}`, "error"));
  };

  if (themes.length === 0) {
    addRow(g, actionRow("配色", "配色列表没取到，稍后重开设置窗口再试"));
  } else {
    addRow(
      g,
      comboRow("配色", undefined, themes, ctx.config.theme, (v) => {
        ctx.config.theme = v;
        ctx.save();
        refresh();
      }),
    );
  }

  // 第一项「系统默认」对应空字符串 —— 三档都空就完全不干预字体
  const families: [string, string][] = [["", "系统默认"], ...fonts.map((f): [string, string] => [f, f])];

  const LATIN_BASE = "英文、数字、代码";
  const CJK_BASE = "汉字、中文标点、假名、谚文";

  const latinRow = searchComboRow("拉丁字体", LATIN_BASE, families, ctx.config.fontLatin, (v) => {
    ctx.config.fontLatin = v;
    ctx.save();
    refresh();
    void syncLatin(v);
  });

  const cjkRow = searchComboRow("中文字体", CJK_BASE, families, ctx.config.fontCjk, (v) => {
    ctx.config.fontCjk = v;
    ctx.save();
    refresh();
    void syncCjk(v);
  });

  addRow(g, latinRow);
  addRow(g, cjkRow);
  addRow(
    g,
    searchComboRow("后备字体", "前两档都没有的字形，比如 emoji、西里尔字母", families, ctx.config.fontFallback, (v) => {
      ctx.config.fontFallback = v;
      ctx.save();
      refresh();
    }),
  );

  /**
   * 拉丁档自带汉字不是错误，只是要说清「汉字不归它管」——
   * 见 docs/adr/0001：两档各自钉死字符区间，不靠回退顺序。
   */
  async function syncLatin(family: string): Promise<void> {
    if (!family) return setSub(latinRow, LATIN_BASE);
    const covers = await attempt(ctx, "检查字体字形", () => api.fontCoversCjk(family), false);
    setSub(
      latinRow,
      covers ? `${LATIN_BASE} · ${family} 自带汉字，已经帮你把它挡在汉字之外了，汉字仍由「中文字体」那一档决定` : LATIN_BASE,
    );
  }

  /**
   * 中文档选到纯拉丁族是真实踩过的坑：用户填了 `HarmonyOS Sans`，
   * 而带汉字的是 `HarmonyOS Sans SC`，结果汉字静默掉回系统默认字体。
   */
  async function syncCjk(family: string): Promise<void> {
    if (!family) return setSub(cjkRow, CJK_BASE);
    const covers = await attempt(ctx, "检查字体字形", () => api.fontCoversCjk(family), true);
    if (covers) setSub(cjkRow, CJK_BASE);
    else
      setSub(
        cjkRow,
        `${family} 没有汉字字形，汉字会落到系统默认字体。带汉字的通常是名字后面加 SC / TC / JP 的那一个，比如 HarmonyOS Sans → HarmonyOS Sans SC。`,
        true,
      );
  }

  // 进页面就先按当前配置校验一遍，别等用户动了才提示
  void syncLatin(ctx.config.fontLatin);
  void syncCjk(ctx.config.fontCjk);

  return [g];
}

// ---------------------------------------------------------------- 弹窗

function buildPopup(ctx: Ctx, copy: SettingsPlatformCopy): HTMLElement[] {
  const g = group("弹窗", "译文窗口的默认大小。");

  const sizeRow = (
    title: string,
    get: () => number,
    set: (v: number) => void,
  ): HTMLDivElement => {
    const row = entryRow(
      title,
      String(get()),
      (raw) => {
        const input = row.querySelector<HTMLInputElement>("input.entry");
        const n = Number(raw.trim());
        if (!Number.isFinite(n) || !Number.isInteger(n) || n < POPUP_MIN || n > POPUP_MAX) {
          ctx.status(`${title}要是 ${POPUP_MIN}–${POPUP_MAX} 之间的整数`, "error");
          if (input) input.value = String(get()); // 打回原值，别让界面显示一个没生效的数
          return;
        }
        set(n);
        ctx.save();
      },
      { subtitle: `${POPUP_MIN}–${POPUP_MAX} 像素` },
    );
    return row;
  };

  addRow(
    g,
    sizeRow(
      "宽度",
      () => ctx.config.popupWidth,
      (v) => (ctx.config.popupWidth = v),
    ),
  );
  addRow(
    g,
    sizeRow(
      "高度",
      () => ctx.config.popupHeight,
      (v) => (ctx.config.popupHeight = v),
    ),
  );

  return copy.popupNotice ? [g, notice(copy.popupNotice)] : [g];
}

// ---------------------------------------------------------------- 后台常驻

async function buildAutostart(ctx: Ctx): Promise<HTMLElement> {
  const g = group(
    "后台常驻",
    "常驻后托盘会有图标，一眼看得出程序在不在跑；快捷键触发时复用这个进程，省掉冷启动，弹窗几乎瞬间出来。",
  );

  // 平台不支持自启时后端直接返回 Err，这里要能降级成一行说明，不能把整页拖崩
  let enabled: boolean;
  try {
    enabled = await api.autostartEnabled();
  } catch (e) {
    ctx.status(`读取开机自启状态失败：${e}`, "error");
    addRow(g, actionRow("开机自启动", `当前环境读不到自启状态：${e}`));
    return g;
  }

  const row = switchRow("开机自启动", "登录后自动常驻托盘", enabled, (on) => {
    void (async () => {
      try {
        await api.setAutostart(on);
        ctx.status(on ? "已开启开机自启动" : "已关闭开机自启动");
      } catch (e) {
        ctx.status(`设置开机自启失败：${e}`, "error");
        // 设置没成功，开关得弹回去，否则界面在撒谎
        const box = row.querySelector<HTMLInputElement>("input.switch");
        if (box) box.checked = !on;
      }
    })();
  });
  addRow(g, row);
  return g;
}

// ---------------------------------------------------------------- 快捷键

async function buildShortcuts(isLinux: boolean, ctx: Ctx): Promise<HTMLElement[]> {
  const g = group("快捷键");

  if (!isLinux) {
    const isMac = ctx.os === "macos";
    let [translate, settings] = ["", ""];
    try {
      [translate, settings] = await api.hotkeys();
    } catch (e) {
      addRow(g, actionRow("全局快捷键", `读不到当前快捷键：${e}`));
      return [g];
    }

    // 两个键任何一个改动都要一起提交：后端是同步校验+注册的，分两次提交会出现
    // "第一个已生效、第二个失败"的中间态，用户看到的界面和实际注册的对不上
    const apply = async (next: { t?: string; s?: string }) => {
      const t = next.t ?? translate;
      const st = next.s ?? settings;
      try {
        await api.setHotkeys(t, st);
        translate = t;
        settings = st;
        ctx.status("快捷键已更新");
        ctx.rerender(); // 重画一遍，把"恢复默认"之后的实际生效值显示出来
      } catch (e) {
        ctx.status(String(e), "error");
        throw e; // 让控件把自己退回原值
      }
    };

    addRow(
      g,
      hotkeyRow("翻译选中文本", "在任何应用里按下它取词翻译", translate, isMac, (v) =>
        apply({ t: v }),
      ),
    );
    addRow(
      g,
      hotkeyRow("打开设置", undefined, settings, isMac, (v) => apply({ s: v })),
    );

    return [
      g,
      notice(
        "全局快捷键是系统级独占的：注册之后所有应用里的这个组合都归 seltrans，别的程序收不到。" +
          "所以默认值刻意避开了浏览器的「重新打开关闭的标签页」。设不上多半是被别的程序占了，换一组即可。",
      ),
    ];
  }

  for (const [key, what] of [
    ["Mod+Shift+T", "划词翻译"],
    ["Mod+Alt+T", "打开本配置界面"],
    ["Esc", "关闭翻译弹窗"],
    ["F5", "在弹窗里重新翻译"],
    ["Ctrl+Shift+C", "在弹窗里复制译文"],
  ] as [string, string][]) {
    addRow(g, actionRow(what, key));
  }

  return [
    g,
    notice("前两个由 niri 配置提供：Wayland 下应用自己注册不了全局快捷键，这是协议层的限制。改键请编辑 ~/.config/niri/selectiontranslation.kdl。"),
  ];
}
