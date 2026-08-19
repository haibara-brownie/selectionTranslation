/**
 * 「提示词」页：管理翻译风格（对照 GTK 版的 `settings_ui.rs::rebuild_prompts` / `edit_prompt`）。
 *
 * 提示词是这个程序里唯一"用户会反复手写"的东西，所以这一页比别的页更强调两件事：
 * 一是 {target_lang} 占位符必须让人看见，二是改坏了要能一键恢复出厂内容。
 */

import { api, newId, type Prompt } from "../lib/api";
import { actionRow, addRow, button, entryRow, group, h, textAreaRow } from "../lib/dom";
import { confirm, modal, type Ctx } from "../lib/shell";

/** 新建时给的骨架。空白模板不好用 —— 大多数人只想在通用译法上改一两句 */
const NEW_SYSTEM = `你是一个专业翻译引擎。把用户提供的文本翻译成{target_lang}。

要求：
- 只输出译文本身，不要任何解释、前言或引号。
- 忠实原意，同时符合{target_lang}的表达习惯。
- 保留原文的段落结构和换行。`;

const PLACEHOLDER = "{target_lang}";

/** 列表里的一行要显示的预览：正文第一段非空文字，截断 */
function preview(system: string): string {
  const line = system.split("\n").find((l) => l.trim() !== "")?.trim() ?? "（正文为空）";
  return line.length > 56 ? `${line.slice(0, 56)}…` : line;
}

function labelOf(p: Prompt): string {
  const name = p.name.trim() === "" ? "未命名" : p.name;
  return p.icon.trim() === "" ? name : `${p.icon} ${name}`;
}

export async function render(pane: HTMLElement, ctx: Ctx): Promise<void> {
  const prompts = ctx.config.prompts;

  const g = group(
    "提示词",
    "决定翻译的风格。弹窗顶部可以随时切换，切换后会立刻用新风格重译。" +
      `正文里的 ${PLACEHOLDER} 会在发请求前被替换成「通用」页设置的目标语言。`,
  );

  // 顶部操作条。dom.ts 的 group() 没有 header suffix 这种东西（libadwaita 有），
  // 所以把两个全局动作放在列表第一行，视觉上仍然是"这一组的顶部"
  addRow(
    g,
    actionRow(
      "管理",
      `当前共 ${prompts.length} 条`,
      button("恢复内置提示词", () => void restoreBuiltins(ctx)),
      button("添加提示词", () => void editPrompt(ctx, null), "accent"),
    ),
  );

  if (prompts.length === 0) {
    // 正常路径走不到这里（最后一条不给删），但配置文件被手改坏时得给条出路
    addRow(g, h("div", { class: "empty" }, "一条提示词都没有，点「恢复内置提示词」拿回出厂的七条"));
  }

  for (const p of prompts) {
    const isCurrent = p.id === ctx.config.activePrompt;

    const tail: HTMLElement[] = [];
    if (isCurrent) tail.push(h("span", { class: "badge" }, "默认"));
    else tail.push(button("设为默认", () => setActive(ctx, p.id)));
    tail.push(button("编辑", () => void editPrompt(ctx, p)));

    const del = button("删除", () => void removePrompt(ctx, p), "danger");
    // 最后一条不给删：activePrompt 必须指得到东西，弹窗的风格切换器也得有得选。
    // 另一个选择是"删空了自动恢复内置七条"，但那样按下删除反而冒出七条，
    // 比按钮变灰更让人意外 —— GTK 版同样是置灰（rebuild_prompts 里 sensitive(len > 1)）
    if (prompts.length <= 1) {
      del.disabled = true;
      del.title = "至少要保留一条提示词";
    }
    tail.push(del);

    const row = actionRow(labelOf(p), preview(p.system), ...tail);
    if (isCurrent) row.classList.add("current");
    addRow(g, row);
  }

  pane.append(g);
}

function setActive(ctx: Ctx, id: string) {
  ctx.config.activePrompt = id;
  ctx.save();
  ctx.rerender();
}

async function removePrompt(ctx: Ctx, p: Prompt) {
  if (ctx.config.prompts.length <= 1) return;
  const ok = await confirm(
    "删除提示词",
    `确定删除「${labelOf(p)}」吗？如果它是内置的七条之一，之后还能用「恢复内置提示词」找回来；自己写的删了就没了。`,
  );
  if (!ok) return;

  ctx.config.prompts = ctx.config.prompts.filter((x) => x.id !== p.id);
  // 删掉的正好是当前默认项时得改指向，否则弹窗会拿着一个不存在的 id 去找提示词
  if (ctx.config.activePrompt === p.id) {
    ctx.config.activePrompt = ctx.config.prompts[0]?.id ?? "";
  }
  ctx.save();
  ctx.rerender();
  ctx.status(`已删除「${labelOf(p)}」`);
}

/**
 * 恢复内置：按 id **逐条覆盖**，不是清空重来。
 *
 * 内置七条的 id 是固定值（general / github / paper / casual / explain / code / polish），
 * 用户自己新增的 id 是随机的，两边不会撞。所以覆盖内置 id、补回缺的、
 * 其余原样不动 —— 用户自己写的提示词一条都不会丢。
 */
async function restoreBuiltins(ctx: Ctx) {
  let builtins: Prompt[];
  try {
    builtins = await api.promptPresets();
  } catch (e) {
    ctx.status(`读取内置提示词失败：${e}`, "error");
    return;
  }

  const existing = new Set(ctx.config.prompts.map((p) => p.id));
  const overwritten = builtins.filter((b) => existing.has(b.id)).length;
  const added = builtins.length - overwritten;
  const mine = ctx.config.prompts.filter((p) => !builtins.some((b) => b.id === p.id)).length;

  // confirm() 的确定按钮写死是「删除」，这里不是删除动作，用不了 —— 直接用 modal 并自己给按钮文案
  const ok = await modal<boolean>(
    "恢复内置提示词",
    (body) =>
      body.append(
        h(
          "p",
          { class: "row-sub" },
          `内置的 ${builtins.length} 条提示词会恢复成出厂内容` +
            (overwritten > 0 ? `（其中 ${overwritten} 条会覆盖掉你对它们的改动）` : "") +
            (added > 0 ? `，缺失的 ${added} 条会补回来` : "") +
            "。" +
            (mine > 0 ? `你自己新增的 ${mine} 条不受影响。` : "你没有自己新增的提示词。"),
        ),
      ),
    { okLabel: "恢复内置", onOk: () => true },
  );
  if (ok !== true) return;

  for (const b of builtins) {
    const i = ctx.config.prompts.findIndex((p) => p.id === b.id);
    if (i >= 0) ctx.config.prompts[i] = b;
    else ctx.config.prompts.push(b);
  }
  if (!ctx.config.prompts.some((p) => p.id === ctx.config.activePrompt)) {
    ctx.config.activePrompt = ctx.config.prompts[0]?.id ?? "";
  }
  ctx.save();
  ctx.rerender();
  ctx.status("内置提示词已恢复");
}

/** 新建和编辑共用同一个对话框；`p` 为 null 表示新建 */
async function editPrompt(ctx: Ctx, p: Prompt | null) {
  const draft: Prompt = p
    ? { ...p }
    : { id: newId("prompt"), name: "", icon: "📝", system: NEW_SYSTEM };

  // 对话框里的控件是 change 事件（失焦才触发）回写 draft 的。点「保存」时浏览器
  // 会先 blur 再 click，理论上不会丢最后一次输入，但这条时序不值得赌 —— 存一份
  // 元素引用，保存时直接读 DOM
  let nameEl: HTMLInputElement | null = null;
  let iconEl: HTMLInputElement | null = null;
  let systemEl: HTMLTextAreaElement | null = null;

  const saved = await modal<Prompt>(
    p ? "编辑提示词" : "新增提示词",
    (body) => {
      const g = group("基本信息");
      const nameRow = entryRow("名称", draft.name, (v) => (draft.name = v), {
        placeholder: "例如：技术文档",
        subtitle: "显示在弹窗顶部的风格切换器里",
      });
      const iconRow = entryRow("图标", draft.icon, (v) => (draft.icon = v), {
        placeholder: "📝",
        subtitle: "一个 emoji，跟在名称前面",
      });
      nameEl = nameRow.querySelector<HTMLInputElement>("input");
      iconEl = iconRow.querySelector<HTMLInputElement>("input");
      addRow(g, nameRow);
      addRow(g, iconRow);

      const g2 = group(
        "System Prompt",
        `${PLACEHOLDER} 会在发请求前被替换成「通用」页设置的目标语言 —— ` +
          "写它才能跟着目标语言走；不写就等于把语言固定死在这条提示词里。" +
          "另外建议明确要求模型「只输出译文」，否则很多模型会加一段解释。",
      );
      const systemRow = textAreaRow(
        "正文",
        draft.system,
        (v) => (draft.system = v),
        "发给模型的 system 消息，选中的文字会作为 user 消息跟在后面。",
      );
      systemEl = systemRow.querySelector<HTMLTextAreaElement>("textarea");
      // dom.ts 的 textAreaRow 固定 10 行，提示词正文一般十几行，写起来太挤
      if (systemEl) systemEl.rows = 14;
      addRow(g2, systemRow);

      body.append(g, g2);
    },
    {
      okLabel: "保存",
      wide: true,
      onOk: () => {
        const name = (nameEl?.value ?? draft.name).trim();
        const system = systemEl?.value ?? draft.system;
        if (name === "") {
          ctx.status("名称不能为空", "error");
          return null; // 返回 null 对话框不关，用户不用重打一遍正文
        }
        if (system.trim() === "") {
          ctx.status("System Prompt 不能为空", "error");
          return null;
        }
        return { id: draft.id, name, icon: (iconEl?.value ?? draft.icon).trim(), system };
      },
    },
  );
  if (saved === null) return;

  const i = ctx.config.prompts.findIndex((x) => x.id === saved.id);
  if (i >= 0) ctx.config.prompts[i] = saved;
  else ctx.config.prompts.push(saved);
  // 之前一条都没有（配置被改坏）时，新写的这条顺手当默认
  if (!ctx.config.prompts.some((x) => x.id === ctx.config.activePrompt)) {
    ctx.config.activePrompt = saved.id;
  }
  ctx.save();
  ctx.rerender();

  // 没写占位符只提醒不拦 —— 确实有人就是想让某条提示词固定译成某种语言
  if (saved.system.includes(PLACEHOLDER)) {
    ctx.status(`已保存「${labelOf(saved)}」`);
  } else {
    ctx.status(`已保存「${labelOf(saved)}」。正文里没有 ${PLACEHOLDER}，这条提示词不会跟随目标语言设置。`);
  }
}
