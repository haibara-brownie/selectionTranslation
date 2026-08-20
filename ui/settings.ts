/**
 * 设置窗口的外壳：标签页切换 + 各页挂载 + 配置的单一副本。
 *
 * 各页只导出一个 `render(pane, ctx)`，剩下的都归自己管。
 */

import { api } from "./lib/api";
import { el } from "./lib/dom";
import { createCtx, type Ctx } from "./lib/shell";
import { startTour, syncTour } from "./lib/tour";
import { buildSteps } from "./lib/tour-steps";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { render as renderGeneral } from "./settings/general";
import { render as renderProviders } from "./settings/providers";
import { render as renderPrompts } from "./settings/prompts";
import { render as renderAbout } from "./settings/about";

type PageId = "general" | "providers" | "prompts" | "about";

const PAGES: Record<PageId, (pane: HTMLElement, ctx: Ctx) => void | Promise<void>> = {
  general: renderGeneral,
  providers: renderProviders,
  prompts: renderPrompts,
  about: renderAbout,
};

const isPageId = (v: string): v is PageId => v in PAGES;

const win = getCurrentWindow();
const pane = el<HTMLDivElement>("pane");
const tabs = el<HTMLDivElement>("tabs");

let current: PageId = "general";
let ctx: Ctx;

async function show(page: PageId) {
  // 切页之前必须落盘：保存是防抖的（250ms），用户改完一个字段立刻点别的标签，
  // 那次改动还在定时器里。不 flush 就直接丢了。
  if (ctx) await ctx.flush();
  current = page;
  for (const t of tabs.querySelectorAll<HTMLElement>(".tab")) {
    t.classList.toggle("current", t.dataset.page === page);
  }
  pane.replaceChildren();
  await PAGES[page](pane, ctx);
  pane.scrollTop = 0;
}

async function boot() {
  const [config, css, os] = await Promise.all([
    api.loadConfig(),
    api.themeCss(window.matchMedia("(prefers-color-scheme: dark)").matches),
    // 取不到就当 linux —— 这只影响几句说明文案，不值得为它拦住整个界面
    api.platform().catch(() => "linux"),
  ]);
  el<HTMLStyleElement>("theme").textContent = css;

  ctx = createCtx(config, os, el("status"), el("theme"), () => void show(current));

  tabs.addEventListener("click", (e) => {
    const t = (e.target as HTMLElement).closest<HTMLElement>(".tab");
    const p = t?.dataset.page;
    if (p && isPageId(p)) void show(p);
  });

  el("close").addEventListener("click", async () => {
    await ctx.flush(); // 别把没落盘的改动带走
    await win.close();
  });

  document.addEventListener("keydown", async (e) => {
    if (e.key === "Escape") {
      await ctx.flush();
      await win.close();
    }
  });

  // 系统明暗变了要跟着换（主题设成"跟随系统"时才有效果，Rust 侧会判断）
  window
    .matchMedia("(prefers-color-scheme: dark)")
    .addEventListener("change", () => void ctx.refreshTheme());

  // 命令行 `settings 供应商` 可以直接跳到某一页
  const want = new URLSearchParams(location.search).get("page") ?? "";
  await show(isPageId(want) ? want : "general");

  // 新手引导的第 3～6 步在这个窗口里（加供应商、填 key、拉模型、测连接）。
  // 快捷键取自后端，和弹窗共用同一份步骤定义。
  const hotkeys = await invoke<[string, string]>("hotkeys").catch(
    () => ["", ""] as [string, string],
  );
  await startTour({
    window: "settings",
    steps: buildSteps({ hotkeys, os: ctx.os }),
    getStep: () => invoke<number>("tour_step"),
    setStep: (step) => invoke("set_tour_step", { step }).then(() => undefined),
    finish: () => invoke("dismiss_onboarding").then(() => undefined),
  });

  // 关掉本窗口回到弹窗之前，把最新的步数落下去 —— 弹窗那边靠它接上
  window.addEventListener("focus", () => void syncTour());
}

boot().catch((e) => {
  el("status").textContent = `启动失败：${e}`;
  el("status").classList.add("error");
});
