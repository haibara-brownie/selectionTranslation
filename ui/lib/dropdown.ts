/**
 * 把原生 `<select>` 换成自绘下拉。
 *
 * # 为什么非换不可
 *
 * 原生 `<select>` 的**弹层由操作系统画**，跟不上应用主题。在 Linux 的 WebKitGTK 上
 * GTK 画出来的样子跟 Catppuccin 还算接近，所以一直没人计较；到了 macOS 的 WKWebView，
 * 弹层是一个 NSMenu —— 系统蓝高亮、系统字体、系统圆角，跟窗口里的深色卡片完全两套东西，
 * 而且会从窗口顶部溢出去盖住正文。Windows 的 WebView2 又是第三种画法。
 *
 * 也就是说，只要还用原生弹层，「三个平台长得一样」就永远做不到。
 *
 * # 为什么是「接管」而不是「替换」
 *
 * `<select>` 留在 DOM 里当数据源，只是不再由它负责显示：调用方继续用 `select.value`、
 * `select.replaceChildren(...option)`、`change` 事件，一行都不用改。换成一个全新的组件
 * 就得把 `popup.ts` 和 `comboRow` 的每个调用点都改一遍，风险和收益不成比例。
 *
 * 选项由 `MutationObserver` 盯着 —— `popup.ts` 是 `replaceChildren` 整批换的，
 * 没有事件可听，只能观察。
 *
 * # 视觉语言是复用的
 *
 * 弹层用的 `.combo-list` / `.combo-item` 跟字体那个带搜索的下拉是同一套（见 `dom.ts`
 * 的 `searchComboRow`）。项目里本来就有这套自绘弹层，只是当初只给字体用了；
 * 现在推广到全部下拉，应用内和平台间就都统一了。
 */

/** 浮层离锚点的间距 */
const GAP = 5;
/** 浮层离窗口边缘至少留这么多 */
const EDGE = 8;
/** 浮层高度的上下限：太矮看不清几项，太高会盖住半个窗口 */
const MIN_H = 120;
const MAX_H = 280;
/** 连续按键当作「输入首字母跳转」的间隔上限 */
const TYPEAHEAD_MS = 700;

/**
 * 把浮层按锚点摆好。**浮层必须已经挂在 `document.body` 上**。
 *
 * # 为什么不能就地绝对定位
 *
 * 设置页的行卡片 `.rows` 为了圆角开了 `overflow: hidden`，外面的 `.pane` 又是
 * `overflow-y: auto` —— 绝对定位的浮层会被这两层**裁掉**。实测：「目标语言」有 21 个
 * 语种，展开后只露出 2 个，看着就像画在卡片里的一小块，而不是浮在上面的层。
 *
 * 挂到 `body` 上用 `fixed` 定位，才真正脱离所有祖先的裁剪和层叠上下文。代价是位置得
 * 自己算，而且锚点一动（窗口缩放、面板滚动）就要重算。
 */
export function placeFloating(anchor: HTMLElement, list: HTMLElement): void {
  const a = anchor.getBoundingClientRect();

  // 至少和锚点一样宽，看着才像它的下拉
  list.style.minWidth = `${a.width}px`;

  // 先放开高度限制量出「本来想多高」，再决定往哪边弹 —— 带着上一次的 maxHeight 量，
  // 量到的是被压过的高度，会一直判成「放得下」
  list.style.maxHeight = "";
  const need = list.offsetHeight;

  const below = window.innerHeight - a.bottom - GAP - EDGE;
  const above = a.top - GAP - EDGE;
  // 下面放不下、而上面更宽裕时才翻上去
  const up = need > below && above > below;

  list.classList.toggle("up", up);
  list.style.maxHeight = `${Math.max(MIN_H, Math.min(MAX_H, up ? above : below))}px`;

  // 压完高度再量，否则翻上去时的 top 会算错
  const w = list.offsetWidth;
  const h = list.offsetHeight;
  // 横向：贴着锚点左沿，右边放不下就整体左移，但不越过窗口左边缘
  list.style.left = `${Math.max(EDGE, Math.min(a.left, window.innerWidth - w - EDGE))}px`;
  list.style.top = `${up ? Math.max(EDGE, a.top - GAP - h) : a.bottom + GAP}px`;
}

/** 挂到 body 上显示出来并摆好位置 */
export function openFloating(anchor: HTMLElement, list: HTMLElement): void {
  document.body.append(list);
  list.hidden = false;
  placeFloating(anchor, list);
}

/**
 * 收起来，并把浮层挪回它原来的宿主。
 *
 * 挪回去是为了让浮层的生命周期跟着宿主走：设置页每次切标签都会 `replaceChildren` 重建
 * 整页，宿主没了浮层也跟着没。留在 body 上的话，切几次页就在 body 底下堆一堆孤儿。
 */
export function closeFloating(home: HTMLElement, list: HTMLElement): void {
  list.hidden = true;
  home.append(list);
}

/** 已经接管过的不再重复接管（`renderState` 会反复调） */
const DONE = new WeakSet<HTMLSelectElement>();

export function enhanceSelect(select: HTMLSelectElement): void {
  if (DONE.has(select)) return;
  DONE.add(select);

  const trigger = document.createElement("button");
  trigger.type = "button";
  trigger.className = "dd-trigger";
  trigger.setAttribute("aria-haspopup", "listbox");
  trigger.setAttribute("aria-expanded", "false");

  const label = document.createElement("span");
  label.className = "dd-label";
  const caret = document.createElement("span");
  caret.className = "dd-caret";
  caret.textContent = "▾";
  caret.setAttribute("aria-hidden", "true");
  trigger.append(label, caret);

  const list = document.createElement("div");
  list.className = "combo-list dd-list";
  list.setAttribute("role", "listbox");
  list.hidden = true;

  const box = document.createElement("div");
  box.className = "dd";
  select.replaceWith(box);
  // select 留在 DOM 里当数据源，但不再显示，也不该被 Tab 走到 —— 焦点归 trigger
  select.classList.add("dd-native");
  select.tabIndex = -1;
  select.setAttribute("aria-hidden", "true");
  box.append(select, trigger, list);

  let active = -1;
  let typed = "";
  let typedAt = 0;

  const options = () => [...select.options];

  const syncTrigger = () => {
    const cur = select.selectedOptions[0];
    label.textContent = cur?.textContent ?? "";
    // 空列表时（比如一个模型都没拉取过）给个说明，别显示成一个空按钮
    trigger.classList.toggle("empty", select.options.length === 0);
    if (select.options.length === 0) label.textContent = "无可选项";
    trigger.disabled = select.disabled || select.options.length === 0;
    trigger.title = select.title || label.textContent || "";
  };

  const setActive = (i: number) => {
    const items = [...list.querySelectorAll<HTMLElement>(".combo-item")];
    if (items.length === 0) return;
    active = Math.max(0, Math.min(items.length - 1, i));
    items.forEach((n, idx) => n.classList.toggle("active", idx === active));
    items[active]?.scrollIntoView({ block: "nearest" });
  };

  const pick = (index: number) => {
    if (index < 0 || index >= select.options.length) return;
    if (select.selectedIndex !== index) {
      select.selectedIndex = index;
      // 调用方监听的是 change；程序性赋值不会自己发，得补一个
      select.dispatchEvent(new Event("change", { bubbles: true }));
    }
    syncTrigger();
    close();
  };

  const render = () => {
    list.replaceChildren(
      ...options().map((o, i) => {
        const item = document.createElement("div");
        item.className = "combo-item";
        item.setAttribute("role", "option");
        item.setAttribute("aria-selected", String(i === select.selectedIndex));
        item.textContent = o.textContent;
        if (i === select.selectedIndex) item.classList.add("current");
        // 用 mousedown 而不是 click：click 之前会先触发 blur，列表已经收起来了
        item.addEventListener("mousedown", (e) => {
          e.preventDefault();
          pick(i);
        });
        return item;
      }),
    );
  };

  const open = () => {
    if (trigger.disabled) return;
    render();
    openFloating(trigger, list);
    trigger.setAttribute("aria-expanded", "true");
    setActive(select.selectedIndex);
  };

  const close = () => {
    closeFloating(box, list);
    trigger.setAttribute("aria-expanded", "false");
    active = -1;
    typed = "";
  };

  const toggle = () => (list.hidden ? open() : close());

  trigger.addEventListener("click", toggle);

  trigger.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (list.hidden) return open();
      setActive(active + (e.key === "ArrowDown" ? 1 : -1));
    } else if (e.key === "Home" || e.key === "End") {
      if (list.hidden) return;
      e.preventDefault();
      setActive(e.key === "Home" ? 0 : select.options.length - 1);
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      if (list.hidden) open();
      else pick(active);
    } else if (e.key === "Escape") {
      if (!list.hidden) {
        // 下拉是开着的就只收下拉，别让 Esc 穿透到窗口把整个弹窗关掉
        e.stopPropagation();
        close();
      }
    } else if (e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) {
      // 首字母跳转。原生 select 有这个行为，自绘的不做会显得退化
      const now = Date.now();
      typed = now - typedAt < TYPEAHEAD_MS ? typed + e.key : e.key;
      typedAt = now;
      const q = typed.toLowerCase();
      const hit = options().findIndex((o) => (o.textContent ?? "").toLowerCase().startsWith(q));
      if (hit >= 0) {
        if (list.hidden) open();
        setActive(hit);
      }
    }
  });

  // 点到别处就收起来。用 pointerdown 而不是 click，跟 item 的 mousedown 顺序才对得上。
  // 浮层现在挂在 body 上，不再是 box 的后代，所以两边都要放过
  document.addEventListener("pointerdown", (e) => {
    const t = e.target as Node;
    if (!list.hidden && !box.contains(t) && !list.contains(t)) close();
  });

  window.addEventListener("resize", () => !list.hidden && placeFloating(trigger, list));
  // 浮层是 fixed 定位的，锚点跟着面板滚走了它不会自己动，会脱锚悬在半空。
  // 用捕获阶段才能听到 .pane 这种内层滚动容器的滚动。
  window.addEventListener("scroll", () => !list.hidden && placeFloating(trigger, list), true);

  // 调用方是整批 replaceChildren 换选项的，没有事件可听，只能观察
  new MutationObserver(() => {
    syncTrigger();
    if (!list.hidden) {
      render();
      placeFloating(trigger, list);
    }
  }).observe(select, {
    childList: true,
    attributes: true,
    attributeFilter: ["disabled", "title"],
  });

  // 别处直接改了 select.value 时（程序性切换供应商之类）也要跟上
  select.addEventListener("change", syncTrigger);

  syncTrigger();
}

/** 把一棵子树里所有还没接管的 `<select>` 一次性接管掉 */
export function enhanceAll(root: ParentNode = document): void {
  root.querySelectorAll<HTMLSelectElement>("select:not(.dd-native)").forEach(enhanceSelect);
}
