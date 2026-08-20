/**
 * 「供应商」页：模型供应商的增删改、模型列表拉取、连通性测试。
 *
 * 对照 GTK 版的 `src/settings_ui.rs::rebuild_providers / edit_provider / pick_model`。
 *
 * 两条贯穿全页的原则：
 * 1. **模型名不写死**。各家迭代太快（deepseek-chat 已停用、moonshot-v1 要退役），
 *    一律靠「拉取模型」实时从服务端取，拉到的顺手缓存进 `provider.models`。
 * 2. **API key 只出现在 password 输入框里**。状态栏、日志、错误提示一概不带它。
 */

import { api, newId, type Provider, type ProviderPreset } from "../lib/api";
import {
  actionRow,
  addRow,
  button,
  comboRow,
  entryRow,
  group,
  h,
  notice,
  textAreaRow,
} from "../lib/dom";
import { confirm, modal, type Ctx } from "../lib/shell";

/**
 * 给控件挂一个稳定的 id，供新手引导定位。
 *
 * 引导的锚点必须是**稳定的 id**，不能用 `.rows > .row:nth-child(2)` 这种位置
 * 选择器 —— 中间加一行就指错地方了。挂 id 是零成本，还能自说明"这里被引导指着"。
 */
function tourAnchor<T extends HTMLElement>(el: T, id: string): T {
  el.id = id;
  return el;
}

/** 接口协议。选错了不会报"协议不对"，只会 404，所以副标题里要点明 */
const KINDS: [string, string][] = [
  ["openai", "OpenAI 兼容 · /chat/completions"],
  ["anthropic", "Anthropic · /v1/messages"],
];

/** 一次最多渲染多少个模型 —— 聚合平台动辄上千个，全画出来会卡住界面 */
const MAX_ITEMS = 300;

// ---------------------------------------------------------------- 列表页

export async function render(pane: HTMLElement, ctx: Ctx): Promise<void> {
  let presets: ProviderPreset[] = [];
  try {
    presets = await api.providerPresets();
  } catch (e) {
    // 预设拉不到不该让整页白掉：已有供应商照样能改，只是新建时少了自动填充
    ctx.status(`读取供应商预设失败：${e}`, "error");
  }

  const g = group(
    "模型供应商",
    "标着「使用中」的那个才会被真正调用。模型列表不写死在程序里，进编辑框点「拉取模型」实时从服务端获取。",
  );

  addRow(
    g,
    actionRow(
      "添加供应商",
      "先挑一家预设，接口类型和 base_url 会自动填好，通常你只需要补一个 API key",
      tourAnchor(button("添加", () => void addProvider(ctx, presets), "accent"), "tour-add-provider"),
    ),
  );

  const providers = ctx.config.providers;
  // 配置里 activeProvider 可能是空串（首次运行）；此时按 GTK 版的规矩认第一个
  const active = ctx.config.activeProvider !== "" ? ctx.config.activeProvider : (providers[0]?.id ?? "");

  if (providers.length === 0) {
    addRow(
      g,
      h(
        "div",
        { class: "empty" },
        "还没有配置任何供应商。点上面的「添加」挑一家（DeepSeek、智谱、Kimi… 或自定义），填上 API key 就能用了。",
      ),
    );
  }

  for (const p of providers) {
    const current = p.id === active;
    const tail: (Node | string)[] = [];
    if (current) {
      tail.push(h("span", { class: "badge" }, "使用中"));
    } else {
      tail.push(
        button("启用", () => {
          ctx.config.activeProvider = p.id;
          ctx.save();
          ctx.status(`已切换到「${p.name}」`);
          ctx.rerender();
        }),
      );
    }
    tail.push(button("编辑", () => void editProvider(ctx, copyOf(p), false, presets)));
    tail.push(button("删除", () => void removeProvider(ctx, p.id, p.name), "danger"));

    const row = actionRow(p.name !== "" ? p.name : "未命名供应商", describe(p), ...tail);
    if (current) row.classList.add("current");
    addRow(g, row);
  }

  pane.append(g);
}

/** 列表行的副标题：模型 · key 有没有填 · base_url 的主机名 */
function describe(p: Provider): string {
  const model = p.model.trim() !== "" ? p.model : "未选模型";
  const key = p.apiKey.trim() !== "" ? "key 已填" : "未填 key";
  return `${model} · ${key} · ${hostOf(p.baseUrl)}`;
}

function hostOf(baseUrl: string): string {
  try {
    return new URL(baseUrl).host;
  } catch {
    // 用户可能还没填完 / 填了个不合法的地址，原样显示比抛错强
    return baseUrl !== "" ? baseUrl : "未填 base_url";
  }
}

/** 编辑时改的必须是副本，取消了不能污染 ctx.config */
function copyOf(p: Provider): Provider {
  return { ...p, models: [...p.models] };
}

async function removeProvider(ctx: Ctx, id: string, name: string): Promise<void> {
  const yes = await confirm("删除供应商？", `「${name}」的配置和 API key 会一并删除，此操作不可撤销。`);
  if (!yes) return;

  ctx.config.providers = ctx.config.providers.filter((x) => x.id !== id);
  // 删掉的正好是当前启用项时得改指向，否则翻译会去调一个已经不存在的供应商
  if (ctx.config.activeProvider === id) {
    ctx.config.activeProvider = ctx.config.providers[0]?.id ?? "";
  }
  ctx.save();
  ctx.status(`已删除「${name}」`);
  ctx.rerender();
}

// ---------------------------------------------------------------- 添加流程

async function addProvider(ctx: Ctx, presets: ProviderPreset[]): Promise<void> {
  const list = withCustom(presets);
  const preset = await pickPreset(ctx, list);
  if (preset === null) return;

  await editProvider(
    ctx,
    {
      id: newId("p"),
      name: preset.name,
      preset: preset.id,
      kind: preset.kind !== "" ? preset.kind : "openai",
      baseUrl: preset.baseUrl,
      apiKey: "",
      model: "",
      models: [],
      extraBody: "",
    },
    true,
    list,
  );
}

/** 预设里本来就带一项 custom；万一后端没给（版本不匹配），补一个兜底，别让"自定义"这条路断掉 */
function withCustom(presets: ProviderPreset[]): ProviderPreset[] {
  if (presets.some((p) => p.id === "custom")) return presets;
  return [
    ...presets,
    {
      id: "custom",
      name: "自定义（OpenAI 兼容）",
      kind: "openai",
      baseUrl: "",
      keysUrl: "",
      hint: "任何提供 OpenAI 兼容 /chat/completions 接口的服务都可以填在这里。",
    },
  ];
}

function pickPreset(ctx: Ctx, presets: ProviderPreset[]): Promise<ProviderPreset | null> {
  return modal<ProviderPreset>("选一家供应商", (body, done) => {
    const g = group(
      "内置预设",
      "选中后自动填好接口类型和 base_url。下面的说明里写了各家哪个模型适合划词、哪个已经停用。",
    );
    for (const p of presets) {
      const tail: (Node | string)[] = [];
      if (p.keysUrl !== "") tail.push(button("申请 Key", () => void openUrl(ctx, p.keysUrl)));
      tail.push(button("选这个", () => done(p), "accent"));
      addRow(g, actionRow(p.name, p.hint, ...tail));
    }
    body.append(g);
  });
}

async function openUrl(ctx: Ctx, url: string): Promise<void> {
  try {
    await api.openPath(url);
  } catch (e) {
    ctx.status(`打不开 ${url}：${e}`, "error");
  }
}

// ---------------------------------------------------------------- 编辑对话框

async function editProvider(
  ctx: Ctx,
  draft: Provider,
  isNew: boolean,
  presets: ProviderPreset[],
): Promise<void> {
  const preset = presets.find((x) => x.id === draft.preset);

  // 对话框内的结果条：测试结果、拉取失败的原文、保存时的校验提示都往这儿写。
  // 不用 ctx.status 是因为状态栏被模态遮罩压在底下，用户不一定看得见。
  const result = notice("", "info");
  result.hidden = true;
  const say = (text: string, kind: "info" | "warn") => {
    result.hidden = false;
    result.className = `notice ${kind}`;
    result.textContent = text;
  };

  // dom.ts 的 entryRow 只返回整行，拿不到里面的 input；拉取模型要往回写值，只能捞一下
  let modelInput: HTMLInputElement | null = null;

  const saved = await modal<boolean>(
    isNew ? "添加供应商" : "编辑供应商",
    (body) => {
      if (preset) {
        const hg = group("预设说明");
        const tail: (Node | string)[] = [];
        if (preset.keysUrl !== "") {
          tail.push(button("申请 API Key", () => void openUrl(ctx, preset.keysUrl)));
        }
        addRow(hg, actionRow(preset.name, preset.hint, ...tail));
        body.append(hg);
      }

      const g = group("连接");
      addRow(
        g,
        entryRow("名称", draft.name, (v) => { draft.name = v; }, {
          placeholder: "显示在列表里的名字",
        }),
      );
      addRow(
        g,
        comboRow("接口类型", "填错不会提示协议不对，只会 404", KINDS, draft.kind, (v) => {
          draft.kind = v;
        }),
      );
      addRow(
        g,
        entryRow("base_url", draft.baseUrl, (v) => { draft.baseUrl = v; }, {
          placeholder: "https://api.example.com/v1",
        }),
      );
      addRow(
        g,
        tourAnchor(
          entryRow("API Key", draft.apiKey, (v) => { draft.apiKey = v; }, {
            password: true,
            subtitle: "只写进本机配置文件，不会出现在日志或状态栏里",
          }),
          "tour-api-key",
        ),
      );

      const modelRow = entryRow("模型", draft.model, (v) => { draft.model = v.trim(); }, {
        placeholder: "不确定就点下面的「拉取模型」",
      });
      modelInput = modelRow.querySelector<HTMLInputElement>("input");
      addRow(g, modelRow);

      const fetchBtn = tourAnchor(button("拉取模型", () => void doFetch()), "tour-fetch-models");
      const testBtn = tourAnchor(button("测试连接", () => void doTest()), "tour-test-conn");
      addRow(
        g,
        actionRow(
          "模型列表 / 连通性",
          "两个都会真的发一次网络请求，慢的时候要等几秒",
          fetchBtn,
          testBtn,
        ),
      );
      body.append(g);

      const eg = group("高级");
      addRow(
        eg,
        textAreaRow(
          "附加请求体（JSON 对象，可留空）",
          draft.extraBody,
          (v) => { draft.extraBody = v; },
          '这里填的键会原样合并进请求体，用来塞各家特有的参数，例如 {"reasoning_effort": "none"}。' +
            '注意：程序默认不发 temperature（Claude 当前世代收到会直接 400），确实需要就在这里自己加 {"temperature": 0.3}。',
        ),
      );
      body.append(eg);
      body.append(result);

      /** 两个网络按钮期间一起禁用：并发点两下既浪费额度，回来的顺序也没法保证 */
      async function busy(btn: HTMLButtonElement, label: string, run: () => Promise<void>) {
        const original = btn.textContent ?? "";
        fetchBtn.disabled = true;
        testBtn.disabled = true;
        btn.textContent = label;
        try {
          await run();
        } finally {
          fetchBtn.disabled = false;
          testBtn.disabled = false;
          btn.textContent = original;
        }
      }

      async function doFetch() {
        await busy(fetchBtn, "拉取中…", async () => {
          let models: string[];
          try {
            models = await api.listModels(draft);
          } catch (e) {
            // 服务端原文照抄。401/404 的响应体是排查 key 填错、base_url 少写 /v1 的主要线索
            say(`拉取模型失败：${e}`, "warn");
            return;
          }
          if (models.length === 0) {
            say("服务端返回了空的模型列表，可能这个 key 还没开通任何模型。", "warn");
            return;
          }
          draft.models = models; // 顺手缓存，保存后下次不用再拉
          const picked = await pickModel(models, draft.model);
          if (picked === null) {
            say(`已拉到 ${models.length} 个模型，这次没有选。`, "info");
            return;
          }
          draft.model = picked;
          if (modelInput) modelInput.value = picked;
          say(`已选择模型：${picked}`, "info");
        });
      }

      async function doTest() {
        await busy(testBtn, "请求中…", async () => {
          say("正在请求，等一下…", "info");
          try {
            const reply = await api.testConnection(draft);
            say(`✅ 连通。模型回了：${reply}`, "info");
          } catch (e) {
            say(`❌ 连接失败：${e}`, "warn");
          }
        });
      }
    },
    {
      okLabel: isNew ? "添加" : "保存",
      // 校验不过就返回 null —— modal() 靠这个把对话框留在原地，不然用户填的东西全没了
      onOk: () => {
        if (draft.name.trim() === "") {
          say("名称不能为空。", "warn");
          return null;
        }
        if (draft.baseUrl.trim() === "") {
          say("base_url 不能为空。", "warn");
          return null;
        }
        const extra = draft.extraBody.trim();
        if (extra !== "") {
          let parsed: unknown;
          try {
            parsed = JSON.parse(extra);
          } catch (e) {
            say(`附加请求体不是合法 JSON：${e}`, "warn");
            return null;
          }
          // 必须是对象：数组和裸标量没法合并进请求体，存进去只会在调用时炸
          if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
            say('附加请求体必须是 JSON 对象，例如 {"reasoning_effort": "none"}。', "warn");
            return null;
          }
        }
        draft.name = draft.name.trim();
        draft.baseUrl = draft.baseUrl.trim();
        draft.extraBody = extra;
        return true;
      },
    },
  );

  if (saved !== true) return;

  const at = ctx.config.providers.findIndex((x) => x.id === draft.id);
  if (at >= 0) ctx.config.providers[at] = draft;
  else ctx.config.providers.push(draft);
  if (ctx.config.activeProvider === "") ctx.config.activeProvider = draft.id;

  ctx.save();
  ctx.status(`已保存「${draft.name}」`);
  ctx.rerender();
}

// ---------------------------------------------------------------- 模型选择框

/**
 * 带搜索的模型选择框。聚合平台一次能返回几百上千个模型，没有搜索根本没法用。
 *
 * 搜索是**子串匹配**：搜 `flash` 要能命中 `glm-4.7-flash`、搜 `haiku` 要能命中
 * `anthropic/claude-haiku-4-5`，前缀匹配在带厂商前缀的模型名上完全没用。
 */
function pickModel(models: string[], current: string): Promise<string | null> {
  // 这是套在编辑对话框之上的第二层模态。嵌套时 Esc 只关最上层那个 ——
  // 由 shell.ts 的模态栈保证，这里不用自己处理。
  return modal<string>("选择模型", (body, done) => {
    const search = h("input", {
      class: "entry",
      type: "text",
      placeholder: "搜索模型…",
      spellcheck: "false",
      autocomplete: "off",
    });

    const g = group("选择模型", `服务端返回了 ${models.length} 个。点一项即选中，按 Esc 放弃。`);
    addRow(g, actionRow("搜索", "子串匹配，输 flash 能搜到 glm-4.7-flash", search));
    body.append(g);

    const list = h("div", { class: "rows" });
    body.append(list);

    const draw = (needle: string) => {
      const q = needle.trim().toLowerCase();
      const hits = q === "" ? models : models.filter((m) => m.toLowerCase().includes(q));
      list.replaceChildren(
        ...hits.slice(0, MAX_ITEMS).map((m) => {
          const item = h("div", { class: "combo-item" }, m);
          if (m === current) item.classList.add("current");
          item.addEventListener("click", () => done(m));
          return item;
        }),
      );
      if (hits.length === 0) {
        list.append(h("div", { class: "combo-empty" }, "没有匹配的模型"));
      } else if (hits.length > MAX_ITEMS) {
        list.append(
          h("div", { class: "combo-empty" }, `还有 ${hits.length - MAX_ITEMS} 项，再输几个字缩小范围`),
        );
      }
    };
    draw("");
    search.addEventListener("input", () => draw(search.value));
  });
}
