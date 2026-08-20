/**
 * 引导的七步。**两个窗口共用这一份**，各自只渲染 `window` 对得上的那几步。
 *
 * 锚点一律选**稳定的、有 id 或语义类名的元素**。挑 `.rows > .row:nth-child(2)`
 * 这种位置选择器迟早会因为加了一行而指错地方。
 */

import type { TourStep } from "./tour";
import { formatShortcut } from "./keys";

export type TourContext = {
  /** [翻译, 设置] 两个快捷键，来自后端 */
  hotkeys: [string, string];
  os: string;
};

export function buildSteps(ctx: TourContext): TourStep[] {
  const isMac = ctx.os === "macos";
  const kTranslate = formatShortcut(ctx.hotkeys[0], isMac);
  const kSettings = formatShortcut(ctx.hotkeys[1], isMac);
  const trayWord = isMac ? "菜单栏图标" : "托盘图标";

  return [
    // ── 1. 开场 ──────────────────────────────────────────────────────────
    {
      window: "popup",
      title: "欢迎用划词翻译",
      body:
        "<p>选中任意界面里的文字，按一下快捷键，译文就浮出来。</p>" +
        "<p>还差一步才能用：得先告诉它用哪家大模型。" +
        "接下来带你配一遍，大概一分钟。</p>",
      nextLabel: "下一步",
    },

    // ── 2. 弹窗：指向设置按钮 ────────────────────────────────────────────
    {
      window: "popup",
      anchor: "#settings",
      placement: "bottom",
      title: "设置在这儿",
      body:
        `点这个齿轮打开设置（也可以按 <kbd>${kSettings}</kbd>）。` +
        "所有配置都在那里面。",
      // 点齿轮本身会开设置窗口；点「打开设置」按钮也走同一条路
      nextLabel: "打开设置",
    },

    // ── 3. 设置：指向「添加」 ────────────────────────────────────────────
    {
      window: "settings",
      anchor: "#tour-add-provider",
      placement: "left",
      waitMs: 4000,
      title: "加一家模型供应商",
      body:
        "<p>点「添加」，会列出十家预设 —— DeepSeek、智谱、Kimi、硅基流动……</p>" +
        "<p>挑一家你有账号的。接口类型和 base_url 会自动填好。</p>",
      nextLabel: "下一步",
    },

    // ── 4. 设置：对话框里的 API key ──────────────────────────────────────
    // 这个锚点在对话框里，用户点了「添加」→ 选完预设才存在，所以要等
    {
      window: "settings",
      anchor: "#tour-api-key",
      // 贴右侧而不是压在上方：这一步正在说「名称/接口类型/base_url 都填好了」，
      // 气泡盖住那三行就自相矛盾了。对话框 620px、窗口 900px，右边放得下。
      placement: "right",
      waitMs: 60000,
      title: "只要补一个 API key",
      body:
        "<p>名称、接口类型、base_url 预设都替你填好了，通常你只需要粘一个 key。</p>" +
        "<p>没有 key 的话，上一屏每家预设旁边都有「申请 Key」按钮。</p>",
      // key 是密码框，点它不该算「看完了」
      advanceOnClick: false,
      nextLabel: "下一步",
    },

    // ── 5. 设置：拉取模型 ────────────────────────────────────────────────
    {
      window: "settings",
      anchor: "#tour-fetch-models",
      placement: "right",
      waitMs: 60000,
      title: "挑一个模型",
      body:
        "<p>点「拉取模型」，从服务端实时取一份列表 —— 程序里不写死模型名，" +
        "各家迭代太快，写死很快就失效。</p>" +
        "<p>划词翻译重延迟，建议挑各家的<strong>快档</strong>而不是旗舰。</p>",
      nextLabel: "下一步",
    },

    // ── 6. 设置：测试连接 ────────────────────────────────────────────────
    {
      window: "settings",
      anchor: "#tour-test-conn",
      placement: "right",
      waitMs: 60000,
      title: "确认真的通了",
      body:
        "<p>点「测试连接」，会发一条最短的请求。通了会显示模型回了什么；" +
        "不通会把服务端返回的原文显示出来 —— 401 多半是 key 错了，" +
        "404 多半是 base_url 少了 <code>/v1</code>。</p>" +
        "<p>确认没问题就保存，然后关掉设置窗口。</p>",
      nextLabel: "下一步",
    },

    // ── 7. 收尾 ──────────────────────────────────────────────────────────
    {
      window: "popup",
      title: "配好了，开始用吧",
      body:
        `<p>选中任意界面里的文字，按 <kbd>${kTranslate}</kbd>。</p>` +
        `<p>不想选也行：直接在原文框里敲，<kbd>Ctrl</kbd>+<kbd>↩</kbd> 翻译。</p>` +
        `<p>顶部换风格、底部换模型，换完自动重译。<kbd>Esc</kbd> 收起。</p>` +
        `<p>${trayWord}左键开输入框、中键翻译选中的文字。</p>`,
      nextLabel: "开始用",
    },
  ];
}
