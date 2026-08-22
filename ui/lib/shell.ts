/**
 * 设置页各页共用的东西：配置的读写与落盘、状态栏、对话框。
 *
 * 关键约定：**配置只有一份，在内存里**。各页直接改 `ctx.config`，改完调 `ctx.save()`。
 * 各页自己去 `loadConfig()` 的话，同时开着两页时后保存的那个会把前一个的改动冲掉。
 */

import { api, type Config } from "./api";
import { h } from "./dom";

export type Ctx = {
  /** 当前配置，各页直接改这个对象 */
  config: Config;
  /** "linux" | "macos" | "windows"。好几处界面要按平台分叉，启动时取一次给各页共用。 */
  os: string;
  /** 落盘。会防抖，连续改不会打爆磁盘 */
  save: () => void;
  /** 立刻落盘并等它完成（关窗口、切页之前用） */
  flush: () => Promise<void>;
  /** 状态栏提示 */
  status: (text: string, kind?: "info" | "error") => void;
  /** 重新拉主题 CSS 刷新界面（改了配色或字体之后） */
  refreshTheme: () => Promise<void>;
  /** 重新渲染当前页（列表增删之后） */
  rerender: () => void;
};

export function createCtx(
  config: Config,
  os: string,
  statusEl: HTMLElement,
  themeEl: HTMLStyleElement,
  rerender: () => void,
): Ctx {
  let timer: number | undefined;
  let pending: Promise<void> = Promise.resolve();

  const doSave = async () => {
    try {
      await api.saveConfig(ctx.config);
    } catch (e) {
      ctx.status(String(e), "error");
    }
  };

  const ctx: Ctx = {
    config,
    os,
    save() {
      // 防抖：拖滑块、连打字的时候别每次都写盘
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        pending = doSave();
      }, 250);
    },
    async flush() {
      window.clearTimeout(timer);
      await pending;
      await doSave();
    },
    status(text, kind = "info") {
      statusEl.textContent = text;
      statusEl.classList.toggle("error", kind === "error");
      if (kind === "info" && text) {
        // 普通提示过几秒自己消失，错误留着
        const mine = text;
        window.setTimeout(() => {
          if (statusEl.textContent === mine) statusEl.textContent = "";
        }, 3000);
      }
    },
    async refreshTheme() {
      // 主题 CSS 由 Rust 按**磁盘上的**配置生成，而 save() 是防抖的（250ms）——
      // 不先落盘，刷出来的就是上一轮的配置：切主题永远慢一拍，下拉框写着 Mocha、
      // 界面却是上一次选的 Latte，配色和字体下拉全中招。实测踩过（用户连切多次
      // 之后彻底对不上号）。所以刷新前先 flush，Rust 再读盘读到的才是刚改的值。
      await ctx.flush();
      const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
      themeEl.textContent = await api.themeCss(dark);
    },
    rerender,
  };
  return ctx;
}

// ---------- 对话框 ----------

export type ModalResult<T> = T | null;

/**
 * 打开着的模态框，最后一个是最上层。
 *
 * 为什么要维护这么个栈：Esc 的监听器如果各自挂在 document 上，嵌套模态时**一次 Esc
 * 会把两层一起关掉** —— 同一个节点上的多个监听器，`stopPropagation` 拦不住彼此，
 * 先注册的（外层）照样会跑。所以只让栈顶那个响应。
 */
const modalStack: ((value: null) => void)[] = [];

// 全局只挂一个 Esc 监听器，由它决定该关谁
document.addEventListener(
  "keydown",
  (e) => {
    if (e.key !== "Escape" || modalStack.length === 0) return;
    e.preventDefault();
    e.stopPropagation(); // 别让设置窗口的 Esc-关窗口也跟着触发
    modalStack[modalStack.length - 1]!(null);
  },
  true, // 捕获阶段：抢在窗口自己的处理之前
);

/**
 * 弹一个模态框。返回一个 Promise，点确定给值、点取消或按 Esc 给 null。
 *
 * `build` 拿到的 `done` 用来提前关闭（比如列表里点了某一项就直接选中）。
 */
export function modal<T>(
  title: string,
  build: (body: HTMLElement, done: (value: ModalResult<T>) => void) => void,
  opts: { okLabel?: string; onOk?: () => ModalResult<T>; wide?: boolean } = {},
): Promise<ModalResult<T>> {
  return new Promise((resolve) => {
    const body = h("div", { class: "modal-body" });
    const box = h(
      "div",
      { class: opts.wide ? "modal wide" : "modal" },
      h("div", { class: "modal-head" }, title),
      body,
    );
    const backdrop = h("div", { class: "modal-backdrop" }, box);

    let settled = false;
    const done = (value: ModalResult<T>) => {
      if (settled) return;
      settled = true;
      const i = modalStack.indexOf(cancel);
      if (i >= 0) modalStack.splice(i, 1);
      backdrop.remove();
      resolve(value);
    };
    const cancel = () => done(null);
    modalStack.push(cancel);

    backdrop.addEventListener("mousedown", (e) => {
      if (e.target === backdrop) done(null);
    });

    build(body, done);

    if (opts.onOk) {
      const cancelBtn = h("button", { class: "btn" }, "取消");
      cancelBtn.addEventListener("click", cancel);
      const ok = h("button", { class: "btn accent" }, opts.okLabel ?? "保存");
      ok.addEventListener("click", () => {
        // onOk 返回 null 表示校验没过：对话框留在原地，用户填的东西不丢
        const v = opts.onOk!();
        if (v !== null) done(v);
      });
      box.append(h("div", { class: "modal-foot" }, cancelBtn, ok));
    }

    document.getElementById("modal-root")!.append(backdrop);
    // 让第一个输入框拿到焦点
    body.querySelector<HTMLElement>("input, textarea, select")?.focus();
  });
}

/**
 * 确认框。
 *
 * `okLabel` 一定要写成**那个动作本身**（「删除」「恢复内置」），不要写「确定」——
 * 用户在按下之前应该从按钮上就看出会发生什么。
 */
export async function confirm(
  title: string,
  message: string,
  okLabel = "删除",
): Promise<boolean> {
  const r = await modal<boolean>(
    title,
    (body) => body.append(h("p", { class: "row-sub" }, message)),
    { okLabel, onOk: () => true },
  );
  return r === true;
}
