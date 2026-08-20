/**
 * 一套很小的控件词汇，对应 libadwaita 的 PreferencesGroup / ActionRow / ComboRow…
 *
 * 为什么不上框架：设置页就是"一组分好组的行"，用 DOM 直接拼比引入响应式框架轻得多，
 * 也不用为了几个下拉背上一整套构建链。但**必须有共同的词汇** —— 四个页面各写各的 DOM
 * 会立刻长得不一样，这个文件就是那个共同词汇。
 *
 * 命名刻意跟 libadwaita 对齐，方便和 GTK 版的 settings_ui.rs 对照着看。
 */

import { enhanceSelect } from "./dropdown";

export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Record<string, string> = {},
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v;
    else node.setAttribute(k, v);
  }
  node.append(...children);
  return node;
}

export function el<T extends HTMLElement>(id: string): T {
  const node = document.getElementById(id);
  if (!node) throw new Error(`页面里没有 #${id}`);
  return node as T;
}

/** 一组设置项，带标题和可选说明 */
export function group(title: string, subtitle?: string): HTMLDivElement {
  const box = h("div", { class: "group" });
  const head = h("div", { class: "group-head" }, h("h2", {}, title));
  if (subtitle) head.append(h("p", { class: "group-sub" }, subtitle));
  box.append(head, h("div", { class: "rows" }));
  return box;
}

/** 往组里塞一行 */
export function addRow(g: HTMLElement, row: HTMLElement) {
  g.querySelector(".rows")!.append(row);
}

function rowShell(title: string, subtitle?: string): [HTMLDivElement, HTMLDivElement] {
  const text = h("div", { class: "row-text" }, h("div", { class: "row-title" }, title));
  if (subtitle) text.append(h("div", { class: "row-sub" }, subtitle));
  const tail = h("div", { class: "row-tail" });
  return [h("div", { class: "row" }, text, tail), tail];
}

/** 只有文字、右边可以放任意控件的一行 */
export function actionRow(
  title: string,
  subtitle?: string,
  ...tail: (Node | string)[]
): HTMLDivElement {
  const [row, tailBox] = rowShell(title, subtitle);
  tailBox.append(...tail);
  return row;
}

/** 下拉选择。options 是 [值, 显示文字][] */
export function comboRow(
  title: string,
  subtitle: string | undefined,
  options: [string, string][],
  current: string,
  onChange: (value: string) => void,
): HTMLDivElement {
  const select = h("select", {});
  for (const [value, label] of options) {
    const o = h("option", { value }, label);
    if (value === current) o.selected = true;
    select.append(o);
  }
  select.addEventListener("change", () => onChange(select.value));
  const [row, tail] = rowShell(title, subtitle);
  // 用 div 不用 label：接管后触发器是个 <button>，label 会把点击再转发给它，
  // toggle 触发两次等于没反应（踩过）
  tail.append(h("div", { class: "select-wrap" }, select));
  // 换成自绘下拉，跟字体那个带搜索的下拉、以及弹窗里的两个下拉共用一套长相。
  // 原生弹层由系统画，三个平台三种样子（见 lib/dropdown.ts）。
  enhanceSelect(select);
  return row;
}

/**
 * 带搜索的下拉。字体家族有好几百个，原生 select 根本没法选。
 *
 * 搜索是**子串匹配**不是前缀匹配 —— 搜 `maple` 要能搜到 `JetBrains Maple Mono`。
 * GTK 版当初默认前缀匹配，被明确要求改过，这里一开始就做对。
 */
export function searchComboRow(
  title: string,
  subtitle: string | undefined,
  options: [string, string][],
  current: string,
  onChange: (value: string) => void,
): HTMLDivElement {
  const input = h("input", {
    class: "combo-input",
    type: "text",
    role: "combobox",
    autocomplete: "off",
    spellcheck: "false",
  });
  const list = h("div", { class: "combo-list", hidden: "" });
  const wrap = h("div", { class: "combo" }, input, list);

  const labelOf = (v: string) => options.find(([val]) => val === v)?.[1] ?? v;
  let value = current;
  input.value = labelOf(value);

  let active = -1;
  const render = (needle: string) => {
    const q = needle.trim().toLowerCase();
    const hits = q === "" ? options : options.filter(([, l]) => l.toLowerCase().includes(q));
    active = -1;
    list.replaceChildren(
      ...hits.slice(0, 300).map(([v, l]) => {
        const item = h("div", { class: "combo-item", "data-value": v }, l);
        if (v === value) item.classList.add("current");
        item.addEventListener("mousedown", (e) => {
          e.preventDefault(); // 别让输入框先失焦，否则 blur 会把列表收掉
          pick(v);
        });
        return item;
      }),
    );
    if (hits.length === 0) list.append(h("div", { class: "combo-empty" }, "没有匹配的字体"));
    if (hits.length > 300) {
      list.append(h("div", { class: "combo-empty" }, `还有 ${hits.length - 300} 项，再输几个字缩小范围`));
    }
  };

  const pick = (v: string) => {
    value = v;
    input.value = labelOf(v);
    close();
    onChange(v);
  };
  const open = () => {
    render(""); // 打开时先显示全部，而不是拿当前值当搜索词
    list.hidden = false;
  };
  const close = () => {
    list.hidden = true;
    input.value = labelOf(value); // 没选中就把输入框恢复成当前值，别留下半截搜索词
  };

  input.addEventListener("focus", open);
  input.addEventListener("input", () => {
    list.hidden = false;
    render(input.value);
  });
  input.addEventListener("blur", () => setTimeout(close, 120));
  input.addEventListener("keydown", (e) => {
    const items = [...list.querySelectorAll<HTMLElement>(".combo-item")];
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (list.hidden) open();
      active = Math.max(0, Math.min(items.length - 1, active + (e.key === "ArrowDown" ? 1 : -1)));
      items.forEach((n, i) => n.classList.toggle("active", i === active));
      items[active]?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter") {
      e.preventDefault();
      const v = items[active]?.dataset.value;
      if (v !== undefined) pick(v);
    } else if (e.key === "Escape") {
      close();
      input.blur();
    }
  });

  const [row, tail] = rowShell(title, subtitle);
  tail.append(wrap);
  return row;
}

/** 单行文本输入 */
export function entryRow(
  title: string,
  value: string,
  onChange: (value: string) => void,
  opts: { password?: boolean; placeholder?: string; subtitle?: string } = {},
): HTMLDivElement {
  const input = h("input", {
    class: "entry",
    type: opts.password ? "password" : "text",
    spellcheck: "false",
    ...(opts.placeholder ? { placeholder: opts.placeholder } : {}),
  });
  input.value = value;
  input.addEventListener("change", () => onChange(input.value));
  const [row, tail] = rowShell(title, opts.subtitle);
  tail.append(input);
  return row;
}

/** 多行文本输入，单独占一行（提示词正文那种） */
export function textAreaRow(
  title: string,
  value: string,
  onChange: (value: string) => void,
  subtitle?: string,
  rows = 10,
): HTMLDivElement {
  const area = h("textarea", { class: "textarea", spellcheck: "false", rows: String(rows) });
  area.value = value;
  area.addEventListener("change", () => onChange(area.value));
  const box = h("div", { class: "row row-block" });
  const text = h("div", { class: "row-text" }, h("div", { class: "row-title" }, title));
  if (subtitle) text.append(h("div", { class: "row-sub" }, subtitle));
  box.append(text, area);
  return box;
}

/** 开关 */
export function switchRow(
  title: string,
  subtitle: string | undefined,
  on: boolean,
  onChange: (on: boolean) => void,
): HTMLDivElement {
  const input = h("input", { type: "checkbox", class: "switch" });
  input.checked = on;
  input.addEventListener("change", () => onChange(input.checked));
  const [row, tail] = rowShell(title, subtitle);
  tail.append(h("label", { class: "switch-wrap" }, input, h("span", { class: "switch-track" })));
  return row;
}

export function button(
  label: string,
  onClick: () => void,
  variant: "normal" | "accent" | "danger" = "normal",
  opts: { disabled?: boolean; title?: string } = {},
): HTMLButtonElement {
  const b = h("button", { class: `btn ${variant}` }, label);
  b.addEventListener("click", onClick);
  if (opts.disabled) b.disabled = true;
  // 按钮置灰时必须说明为什么，光变灰是在让用户猜
  if (opts.title) b.title = opts.title;
  return b;
}

/** 状态指示：✓ / ✗ + 说明 */
export function statusRow(title: string, ok: boolean, note: string): HTMLDivElement {
  const [row, tail] = rowShell(title, note);
  tail.append(h("span", { class: `status-dot ${ok ? "ok" : "bad"}` }, ok ? "✓" : "✗"));
  return row;
}

/** 一句话提示条，用来说明"这个平台还没做"之类 */
export function notice(text: string, kind: "info" | "warn" | "ok" = "info"): HTMLDivElement {
  return h("div", { class: `notice ${kind}` }, text);
}
