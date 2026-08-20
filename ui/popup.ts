/**
 * 翻译弹窗。
 *
 * 只做三件事：把 Rust 给的状态渲染出来、把用户操作转成命令、把流式事件贴到译文区。
 * 任何"业务判断"（该用哪个供应商、提示词怎么渲染、什么算空输入）都在 Rust 侧，
 * 这里不做第二套。
 */

import { invoke, Channel } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";

import { enhanceAll } from "./lib/dropdown";

type PromptOption = { id: string; name: string; icon: string };
type ProviderOption = { id: string; name: string; model: string; models: string[] };

type UiState = {
  css: string;
  prompts: PromptOption[];
  activePrompt: string;
  providers: ProviderOption[];
  activeProvider: string;
  targetLang: string;
  configured: boolean;
};

/**
 * 一轮翻译请求。
 *
 * `text` 到这儿时**已经是取好词的结果** —— 取词归 Rust，而且必须赶在这个窗口拿到
 * 焦点之前做完，否则模拟出来的复制键会发给我们自己。取不到就带着 `error` 过来，
 * 界面把原因显示出来并让用户直接手输。
 */
type TranslateRequest = {
  text: string | null;
  inputMode: boolean;
  error: string | null;
};

type UiEvent =
  | { kind: "delta"; text: string }
  | { kind: "done" }
  | { kind: "error"; message: string };

const el = <T extends HTMLElement>(id: string): T => {
  const node = document.getElementById(id);
  if (!node) throw new Error(`页面里没有 #${id}`);
  return node as T;
};

const ui = {
  source: el<HTMLTextAreaElement>("source"),
  output: el<HTMLDivElement>("output"),
  srcCount: el<HTMLSpanElement>("src-count"),
  spinner: el<HTMLSpanElement>("spinner"),
  status: el<HTMLSpanElement>("status"),
  prompt: el<HTMLSelectElement>("prompt"),
  model: el<HTMLSelectElement>("model"),
  theme: el<HTMLStyleElement>("theme"),
  retranslate: el<HTMLButtonElement>("retranslate"),
  copy: el<HTMLButtonElement>("copy"),
  settings: el<HTMLButtonElement>("settings"),
  close: el<HTMLButtonElement>("close"),
};

let state: UiState | null = null;
/** 当前这轮翻译的序号。切提示词导致重译时，旧的那轮回来的分片要丢掉。 */
let run = 0;

const win = getCurrentWindow();

function setStatus(text: string, isError = false) {
  ui.status.textContent = text;
  ui.status.classList.toggle("error", isError);
}

function setBusy(busy: boolean) {
  ui.spinner.hidden = !busy;
  ui.output.classList.toggle("streaming", busy);
  // 收尾时把"已经在写"的标记摘掉，下一轮重新由第一个分片打上
  if (!busy) ui.output.classList.remove("typing");
}

/** 弹窗外壳。进场/退场动画都做在它身上（mac 上窗口不透明，动不了窗口本身）。 */
const shell = document.querySelector<HTMLElement>(".shell")!;

/** 退场动画的时长，和 style.css 里 shell-out 对齐 */
const LEAVE_MS = 110;
let leaving = false;

/**
 * 重播进场动画。
 *
 * 常驻模式下窗口是复用的，第二次按快捷键不会重建 webview —— 不显式重播的话，
 * 只有第一次弹出来有进场，之后都是硬出。先摘掉类再强制重排才能重播，
 * 同一帧内摘了又加浏览器会当成没变过。
 */
function playEnter() {
  document.documentElement.classList.remove("leaving");
  leaving = false;
  shell.classList.remove("enter");
  void shell.offsetWidth;
  shell.classList.add("enter");
}

/** 先把退场动画放完再真的关窗，否则用户根本看不见它 */
async function dismiss() {
  if (leaving) return;
  leaving = true;
  document.documentElement.classList.add("leaving");
  await new Promise((r) => setTimeout(r, LEAVE_MS));
  await win.close();
}

/** 等首个 token 的那几百毫秒里的占位 */
function showSkeleton() {
  const box = document.createElement("div");
  box.className = "skeleton";
  for (let i = 0; i < 3; i++) {
    const line = document.createElement("div");
    line.className = "skeleton-line";
    box.append(line);
  }
  ui.output.replaceChildren(box);
}

function clearSkeleton() {
  ui.output.querySelector(".skeleton")?.remove();
}

/**
 * 把当前译文拓一份浮在卡片里淡出。
 *
 * 换风格/换供应商会立刻用新设置重译同一段文字，输出区必须当场腾空去接新分片，
 * 旧译文没法留在原地慢慢淡。拓一份浮层就没有这个矛盾，也不会和新分片抢位置。
 */
function crossFadeOut() {
  const old = ui.output.textContent ?? "";
  if (old.trim() === "") return;
  const ghost = document.createElement("div");
  ghost.className = "output-ghost";
  ghost.textContent = old;
  ui.output.parentElement?.append(ghost);
  ghost.addEventListener("animationend", () => ghost.remove());
}

function updateCount() {
  const n = [...ui.source.value].length;
  ui.srcCount.textContent = n > 0 ? `${n} 字` : "";
}

function renderState(s: UiState) {
  state = s;
  ui.theme.textContent = s.css;

  ui.prompt.replaceChildren(
    ...s.prompts.map((p) => {
      const o = document.createElement("option");
      o.value = p.id;
      o.textContent = p.icon ? `${p.icon} ${p.name}` : p.name;
      o.selected = p.id === s.activePrompt;
      return o;
    }),
  );

  const provider = s.providers.find((p) => p.id === s.activeProvider) ?? s.providers[0];
  // 当前模型未必在缓存的列表里（用户手填过），补进去才不会显示成别的
  const models = provider
    ? [...new Set([provider.model, ...provider.models].filter(Boolean))]
    : [];
  ui.model.replaceChildren(
    ...models.map((m) => {
      const o = document.createElement("option");
      o.value = m;
      o.textContent = m;
      o.selected = m === provider?.model;
      return o;
    }),
  );
  ui.model.disabled = models.length === 0;

  setStatus(provider ? `${provider.name} · ${s.targetLang}` : "还没有配置模型供应商");
}

async function translate() {
  const text = ui.source.value;
  if (text.trim() === "") {
    setStatus("没有可翻译的内容", true);
    return;
  }
  if (!state?.configured) {
    setStatus("还没有配置模型供应商，先去设置里加一个", true);
    return;
  }

  const mine = ++run;
  crossFadeOut(); // 旧译文拓一份淡出，输出区当场腾空
  ui.output.classList.remove("error");
  showSkeleton();
  setBusy(true);
  setStatus("翻译中…");

  const channel = new Channel<UiEvent>();
  channel.onmessage = (ev) => {
    if (mine !== run) return; // 已经被更新的一轮取代了

    switch (ev.kind) {
      case "delta": {
        clearSkeleton();
        // 有字了才让光标出来，免得和骨架屏叠着
        ui.output.classList.add("typing");
        // 每个分片单独包一层，才能逐段渐现；textContent 读出来仍是完整译文，
        // 复制那条路不受影响
        const chunk = document.createElement("span");
        chunk.className = "chunk";
        chunk.textContent = ev.text;
        ui.output.append(chunk);
        // 只在已经贴着底部时才跟着滚，否则会打断用户往回看
        if (isNearBottom(ui.output)) ui.output.scrollTop = ui.output.scrollHeight;
        break;
      }
      case "done":
        // 一个分片都没来过（空响应）时骨架屏还挂着，收尾时兜一下
        clearSkeleton();
        setBusy(false);
        setStatus(providerLabel());
        break;
      case "error":
        setBusy(false);
        ui.output.classList.add("error");
        ui.output.textContent = ev.message;
        setStatus("翻译失败", true);
        break;
    }
  };

  try {
    await invoke("translate", { text, promptId: ui.prompt.value, onEvent: channel });
  } catch (e) {
    if (mine !== run) return;
    setBusy(false);
    ui.output.classList.add("error");
    ui.output.textContent = String(e);
    setStatus("翻译失败", true);
  }
}

function providerLabel(): string {
  if (!state) return "";
  const p = state.providers.find((x) => x.id === state!.activeProvider) ?? state.providers[0];
  return p ? `${p.name} · ${p.model}` : "";
}

function isNearBottom(node: HTMLElement): boolean {
  return node.scrollHeight - node.scrollTop - node.clientHeight < 40;
}

async function copyOutput() {
  const text = ui.output.textContent ?? "";
  if (!text) return;
  await writeText(text);
  setStatus("译文已复制");
}

function wire() {
  ui.source.addEventListener("input", updateCount);

  ui.retranslate.addEventListener("click", translate);
  ui.copy.addEventListener("click", copyOutput);
  ui.close.addEventListener("click", dismiss);
  ui.settings.addEventListener("click", async () => {
    try {
      await invoke("open_settings", { page: null });
    } catch (e) {
      setStatus(String(e), true);
    }
  });

  ui.prompt.addEventListener("change", async () => {
    await invoke("set_active_prompt", { id: ui.prompt.value });
    if (ui.source.value.trim()) translate();
  });

  ui.model.addEventListener("change", async () => {
    if (!state) return;
    const providerId = state.activeProvider || state.providers[0]?.id;
    if (!providerId) return;
    await invoke("set_active_model", { providerId, model: ui.model.value });
    const p = state.providers.find((x) => x.id === providerId);
    if (p) p.model = ui.model.value;
    setStatus(providerLabel());
  });

  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      dismiss();
      return;
    }
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      translate();
      return;
    }
    // Ctrl+C：有选区时让浏览器自己复制，没选区才复制整段译文
    if (e.key === "c" && (e.ctrlKey || e.metaKey) && !window.getSelection()?.toString()) {
      e.preventDefault();
      copyOutput();
    }
  });
}

/** 走一轮：把请求里的文本放进原文框，能翻就翻 */
async function apply(req: TranslateRequest) {
  if (req.inputMode) {
    ui.source.value = "";
    ui.output.replaceChildren();
    updateCount();
    ui.source.focus();
    return;
  }

  if (req.error !== null) {
    // 取词失败不是死路：把原因说清楚，让用户直接在输入框里敲
    setStatus(req.error, true);
    ui.source.focus();
    updateCount();
    return;
  }

  ui.source.value = req.text ?? "";
  updateCount();
  await translate();
}

async function boot() {
  wire();
  // 顶栏的「翻译风格」和底栏的「模型」换成自绘下拉：原生弹层由系统画，
  // mac 上是个 NSMenu，跟窗口里的深色卡片完全两套东西（见 lib/dropdown.ts）
  enhanceAll();

  const systemDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const [s, launch] = await Promise.all([
    invoke<UiState>("load_state", { systemDark }),
    invoke<TranslateRequest>("launch_args"),
  ]);
  renderState(s);

  // 系统明暗变了要跟着换（主题设成"跟随系统"时才有效果，Rust 侧会判断）
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", async (e) => {
    const next = await invoke<UiState>("load_state", { systemDark: e.matches });
    ui.theme.textContent = next.css;
  });

  // 常驻模式下窗口是复用的：第二次按快捷键不会重建 webview（那要几百毫秒，
  // 划词翻译最忌讳这个），Rust 取完词直接发事件过来换一批内容。
  await listen<TranslateRequest>("seltrans://translate", async (ev) => {
    // 配置可能在设置页里改过了，顺手刷一遍供应商/提示词/配色
    try {
      renderState(await invoke<UiState>("load_state", { systemDark: dark() }));
    } catch {
      // 刷不动就用旧的接着跑，别把这一轮翻译卡死
    }
    playEnter();
    await apply(ev.payload);
  });

  playEnter();
  await apply(launch);
}

const dark = () => window.matchMedia("(prefers-color-scheme: dark)").matches;

boot().catch((e) => {
  setStatus(`启动失败：${e}`, true);
});
