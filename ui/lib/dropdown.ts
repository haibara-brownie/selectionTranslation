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

/** 打开时列表离触发器的间距，和 CSS 里的 `top: calc(100% + 5px)` 对齐 */
const GAP = 5;
/** 连续按键当作「输入首字母跳转」的间隔上限 */
const TYPEAHEAD_MS = 700;

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

  /**
   * 决定往下弹还是往上弹。
   *
   * 弹窗窗口只有 480 高，底部状态栏那个「模型」下拉如果一律往下弹，会整个被
   * `.shell { overflow: hidden }` 裁掉 —— 用户看到的是「点了没反应」。
   */
  const place = () => {
    list.classList.remove("up", "right");
    const t = trigger.getBoundingClientRect();

    // 横向：默认贴触发器左边往右生长；右边放不下就改成贴右边往左长。
    // 不做这一步的话，设置页里那些靠右的下拉会把页面撑出横向滚动条，容器一滚，
    // 行标题就被推出可视区。
    const EDGE = 8;
    if (t.left + list.offsetWidth > window.innerWidth - EDGE) list.classList.add("right");

    const below = window.innerHeight - t.bottom - GAP;
    const above = t.top - GAP;
    const need = list.offsetHeight;
    // 下面放不下、而上面更宽裕时才翻上去
    if (need > below && above > below) list.classList.add("up");
    // 两边都放不下就取宽的那侧，并把自己压到能放下为止
    list.style.maxHeight = `${Math.max(120, Math.min(280, Math.max(below, above)))}px`;
  };

  const open = () => {
    if (trigger.disabled) return;
    render();
    list.hidden = false;
    trigger.setAttribute("aria-expanded", "true");
    place();
    setActive(select.selectedIndex);
  };

  const close = () => {
    list.hidden = true;
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

  // 点到别处就收起来。用 pointerdown 而不是 click，跟 item 的 mousedown 顺序才对得上
  document.addEventListener("pointerdown", (e) => {
    if (!list.hidden && !box.contains(e.target as Node)) close();
  });
  window.addEventListener("resize", () => !list.hidden && place());

  // 调用方是整批 replaceChildren 换选项的，没有事件可听，只能观察
  new MutationObserver(() => {
    syncTrigger();
    if (!list.hidden) {
      render();
      place();
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
