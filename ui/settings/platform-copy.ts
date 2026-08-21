export type SelectionMode = "auto" | "primary" | "clipboard";

type SelectionModeCopy = {
  label: string;
  note: string;
};

export type SettingsPlatformCopy = {
  selectionIntro: string;
  selectionModes: Record<SelectionMode, SelectionModeCopy>;
  popupNotice: string | null;
};

const linux: SettingsPlatformCopy = {
  selectionIntro:
    "Wayland 没有统一的划词接口，只能在「主选区」和「模拟 Ctrl+C」之间权衡。",
  selectionModes: {
    auto: {
      label: "自动（推荐）",
      note: "先读主选区（划完词就有，零侵入）；读不到再模拟 Ctrl+C，读完把剪贴板原样还回去。",
    },
    primary: {
      label: "仅主选区",
      note: "只读主选区，绝不碰剪贴板。代价是少数应用（部分 Electron 应用、某些终端）压根不写主选区，这时取不到词。",
    },
    clipboard: {
      label: "仅模拟 Ctrl+C",
      note: "始终走模拟按键，兼容性最好。会临时改写剪贴板再还原，剪贴板管理器里可能多出一条历史。",
    },
  },
  popupNotice:
    "Wayland 下窗口位置由合成器决定，应用自己摆不了。想让弹窗固定出现在某处，改 ~/.config/niri/selectiontranslation.kdl 里的窗口规则。",
};

const macos: SettingsPlatformCopy = {
  selectionIntro:
    "macOS 优先通过辅助功能 API 读取选区；应用没有暴露选区时，再模拟 ⌘C。",
  selectionModes: {
    auto: {
      label: "自动（推荐）",
      note: "先用辅助功能 API 读取选区（不碰剪贴板）；读不到再模拟 ⌘C，之后还原纯文本剪贴板。",
    },
    primary: {
      label: "仅辅助功能",
      note: "只通过辅助功能 API 读取选区，绝不碰剪贴板。Electron 应用通常不暴露选区，这时会明确失败。",
    },
    clipboard: {
      label: "仅模拟 ⌘C",
      note: "始终模拟复制，覆盖面最广。会临时改写剪贴板，且只能还原原来的纯文本内容。",
    },
  },
  popupNotice: null,
};

const windows: SettingsPlatformCopy = {
  selectionIntro:
    "Windows 优先通过 UI Automation 读取选区；控件没有暴露 TextPattern 时，再模拟复制。",
  selectionModes: {
    auto: {
      label: "自动（推荐）",
      note: "先用 UI Automation 读取选区（不碰剪贴板）；读不到再尝试 Ctrl+Insert / Ctrl+C，之后还原纯文本剪贴板。",
    },
    primary: {
      label: "仅 UI Automation",
      note: "只通过 UI Automation 读取选区，绝不碰剪贴板。老式控件没有 TextPattern 时会明确失败。",
    },
    clipboard: {
      label: "仅模拟复制",
      note: "始终尝试 Ctrl+Insert / Ctrl+C，兼容老式控件和控制台。会临时改写剪贴板，且只能还原原来的纯文本内容。",
    },
  },
  popupNotice: null,
};

export function settingsPlatformCopy(os: string): SettingsPlatformCopy {
  if (os === "linux") return linux;
  if (os === "macos") return macos;
  return windows;
}
