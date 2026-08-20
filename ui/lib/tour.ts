/**
 * 新手引导引擎：一串带箭头的气泡，逐个指向界面上的真实控件。
 *
 * 三个要点：
 *
 * 1. **气泡挂 `document.body` + `position: fixed`**，按锚点的 `getBoundingClientRect()`
 *    算位置。就地绝对定位会被祖先的 `overflow: hidden` 裁掉 —— 自绘下拉那次已经
 *    踩过（提交 6b7d5a1），这里直接用同一套路子。
 *
 * 2. **不挡住功能。** 气泡是浮层，底下的按钮照常能点；用户点了真的控件就自动前进，
 *    不必非得点「下一步」。两条路都通 —— 强迫用户走我们规定的路径是最烦人的引导。
 *
 * 3. **步骤状态在后端。** 引导跨弹窗和设置两个窗口，而它们是各自独立的 webview，
 *    没有共享内存。每个窗口只渲染属于自己的那几步，交接靠后端那一份 `tour_step`。
 */

export type TourPlacement = "top" | "bottom" | "left" | "right";

export type TourStep = {
  /** 这一步归哪个窗口渲染 */
  window: "popup" | "settings";
  /** 锚点选择器。留空 = 屏幕正中的大卡片（开场和收尾用） */
  anchor?: string;
  title?: string;
  /** 允许 <kbd>，内容是本仓库的字面量，没有外来输入 */
  body: string;
  placement?: TourPlacement;
  /**
   * 锚点还没出现时最多等多久（毫秒）。
   * 对话框里的锚点是点开之后才有的，得等；等不到就跳过这一步，不能卡死。
   */
  waitMs?: number;
  /** 用户点了这个选择器命中的元素就自动前进（默认用 anchor 自己） */
  advanceOnClick?: string | false;
  /** 进入这一步时跑一次，用来做「顺手把设置窗口打开」这类事 */
  onEnter?: () => void | Promise<void>;
  /** 下一步按钮的文字 */
  nextLabel?: string;
};

export type TourHost = {
  /** 当前窗口是哪个 */
  window: "popup" | "settings";
  steps: TourStep[];
  /** 读后端的当前步；`u32::MAX` 表示已完成 */
  getStep: () => Promise<number>;
  setStep: (step: number) => Promise<void>;
  /** 走完或跳过 */
  finish: () => Promise<void>;
};

const DONE = 0xffffffff;

let host: TourHost | null = null;
let current = -1;
/** 当前这一步挂上去的清理函数（监听器、定时器、DOM） */
let cleanup: (() => void)[] = [];

function teardown() {
  for (const f of cleanup) f();
  cleanup = [];
  document.querySelector(".tour-bubble")?.remove();
  document.querySelector(".tour-spot")?.remove();
  for (const n of document.querySelectorAll(".tour-target")) {
    n.classList.remove("tour-target");
  }
}

/** 等一个元素出现。已经在就立刻返回。 */
function waitFor(selector: string, timeoutMs: number): Promise<Element | null> {
  const now = document.querySelector(selector);
  if (now) return Promise.resolve(now);
  if (timeoutMs <= 0) return Promise.resolve(null);

  return new Promise((resolve) => {
    let done = false;
    const finish = (el: Element | null) => {
      if (done) return;
      done = true;
      obs.disconnect();
      window.clearTimeout(timer);
      resolve(el);
    };
    // 用 MutationObserver 而不是轮询：对话框是一次性插进来的，观察者能立刻反应，
    // 轮询要么慢要么费电
    const obs = new MutationObserver(() => {
      const el = document.querySelector(selector);
      if (el) finish(el);
    });
    obs.observe(document.body, { childList: true, subtree: true });
    const timer = window.setTimeout(() => finish(null), timeoutMs);
    cleanup.push(() => finish(null));
  });
}

/** 把气泡摆到锚点旁边，并在放不下时翻到另一侧 */
function place(bubble: HTMLElement, rect: DOMRect, want: TourPlacement) {
  const gap = 12;
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const bw = bubble.offsetWidth;
  const bh = bubble.offsetHeight;

  // 放不下就翻到对面 —— 弹窗只有 560×480，锚点常常贴着边
  let placement = want;
  if (placement === "bottom" && rect.bottom + gap + bh > vh) placement = "top";
  else if (placement === "top" && rect.top - gap - bh < 0) placement = "bottom";
  else if (placement === "right" && rect.right + gap + bw > vw) placement = "left";
  else if (placement === "left" && rect.left - gap - bw < 0) placement = "right";

  let x: number;
  let y: number;
  if (placement === "top" || placement === "bottom") {
    x = rect.left + rect.width / 2 - bw / 2;
    y = placement === "bottom" ? rect.bottom + gap : rect.top - gap - bh;
  } else {
    y = rect.top + rect.height / 2 - bh / 2;
    x = placement === "right" ? rect.right + gap : rect.left - gap - bw;
  }

  // 夹回视口内，别让气泡有一半在屏幕外
  const m = 8;
  x = Math.max(m, Math.min(x, vw - bw - m));
  y = Math.max(m, Math.min(y, vh - bh - m));

  bubble.style.left = `${Math.round(x)}px`;
  bubble.style.top = `${Math.round(y)}px`;
  bubble.dataset.placement = placement;

  // 箭头指回锚点中心（气泡被夹过之后，箭头不一定在正中）
  const arrow = bubble.querySelector<HTMLElement>(".tour-arrow");
  if (arrow) {
    if (placement === "top" || placement === "bottom") {
      const cx = rect.left + rect.width / 2 - x;
      arrow.style.left = `${Math.max(14, Math.min(cx, bw - 14))}px`;
      arrow.style.top = "";
    } else {
      const cy = rect.top + rect.height / 2 - y;
      arrow.style.top = `${Math.max(14, Math.min(cy, bh - 14))}px`;
      arrow.style.left = "";
    }
  }
}

/** 锚点中途消失后重新等它出现的时限 */
const REVISIT_WAIT_MS = 10 * 60 * 1000;

async function render(i: number, revisit = false) {
  teardown();
  if (!host) return;
  const step = host.steps[i];
  if (!step) return;
  current = i;

  // 不归这个窗口管的步骤，本窗口什么都不画（另一个窗口会捡起来）
  if (step.window !== host.window) return;

  await step.onEnter?.();

  let target: Element | null = null;
  if (step.anchor) {
    // 首次进入按步骤自己声明的时限等；锚点中途丢了（切标签页、关对话框）再回来时
    // 给足时间 —— 用户可能去别处逛一圈，那不该被判成"这步做不了"
    target = await waitFor(step.anchor, revisit ? REVISIT_WAIT_MS : (step.waitMs ?? 0));
    if (!target) {
      // 等不到锚点就跳过这一步，不能把用户卡在这儿
      await next();
      return;
    }
  }

  const bubble = document.createElement("div");
  bubble.className = target ? "tour-bubble" : "tour-bubble tour-center";
  bubble.innerHTML = `
    ${target ? '<span class="tour-arrow"></span>' : ""}
    ${step.title ? `<h3>${step.title}</h3>` : ""}
    <div class="tour-body">${step.body}</div>
    <div class="tour-foot">
      <span class="tour-count">${i + 1} / ${host.steps.length}</span>
      <button class="tour-skip" type="button">跳过</button>
      <button class="tour-next" type="button">${step.nextLabel ?? "下一步"}</button>
    </div>`;
  document.body.append(bubble);

  if (target) {
    target.classList.add("tour-target");
    const reposition = () => place(bubble, target!.getBoundingClientRect(), step.placement ?? "bottom");
    reposition();
    // 窗口大小变了、页面滚了，气泡要跟着走
    window.addEventListener("resize", reposition);
    const pane = document.querySelector(".pane");
    pane?.addEventListener("scroll", reposition);
    cleanup.push(() => {
      window.removeEventListener("resize", reposition);
      pane?.removeEventListener("scroll", reposition);
    });

    // 锚点被移出 DOM 时（用户切了标签页、关了对话框），气泡不能留在原地指着空气。
    // 重跑这一步 —— render 会重新等锚点出现，等回来了自动接上。
    const detachObs = new MutationObserver(() => {
      if (!document.contains(target!)) {
        detachObs.disconnect();
        // 让本轮 DOM 改动落定再重跑，否则可能在替换的中途抓到空档
        window.setTimeout(() => void render(i, true), 0);
      }
    });
    detachObs.observe(document.body, { childList: true, subtree: true });
    cleanup.push(() => detachObs.disconnect());

    // 用户点了真的控件就自动前进 —— 不强迫他走「下一步」
    const advanceSel = step.advanceOnClick === false ? null : (step.advanceOnClick ?? step.anchor);
    if (advanceSel) {
      const onClick = (e: Event) => {
        if ((e.target as Element).closest(advanceSel)) {
          // 让控件自己的处理器先跑完（比如「添加」要先把对话框插进来）
          window.setTimeout(() => void next(), 60);
        }
      };
      document.addEventListener("click", onClick, true);
      cleanup.push(() => document.removeEventListener("click", onClick, true));
    }
  }

  bubble.querySelector(".tour-next")!.addEventListener("click", () => void next());
  bubble.querySelector(".tour-skip")!.addEventListener("click", () => void finish());
}

async function next() {
  if (!host) return;
  const i = current + 1;
  if (i >= host.steps.length) {
    await finish();
    return;
  }
  await host.setStep(i);
  await render(i);
}

async function finish() {
  teardown();
  current = -1;
  await host?.finish();
}

/**
 * 启动引导。每个窗口在自己起来之后调一次；
 * 后端说已经走完就什么都不做。
 */
export async function startTour(h: TourHost) {
  host = h;
  const step = await h.getStep();
  if (step === DONE) return;
  const i = Math.min(step, h.steps.length - 1);
  current = i - 1;
  await render(i);
}

/** 另一个窗口把状态推进了，本窗口跟一下（跨窗口交接用） */
export async function syncTour() {
  if (!host) return;
  const step = await host.getStep();
  if (step === DONE) {
    teardown();
    return;
  }
  if (step !== current) {
    current = step - 1;
    await render(step);
  }
}
