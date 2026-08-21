import assert from "node:assert/strict";
import test from "node:test";

import { settingsPlatformCopy } from "./platform-copy.ts";

test("Windows 设置页描述 UI Automation，不显示 Wayland 限制", () => {
  const copy = settingsPlatformCopy("windows");

  assert.match(copy.selectionIntro, /UI Automation/);
  assert.match(copy.selectionModes.primary.label, /UI Automation/);
  assert.doesNotMatch(copy.selectionModes.primary.label, /不支持/);
  assert.equal(copy.popupNotice, null);
});

test("Linux 设置页保留主选区和 niri 窗口说明", () => {
  const copy = settingsPlatformCopy("linux");

  assert.match(copy.selectionIntro, /Wayland/);
  assert.match(copy.selectionModes.primary.label, /主选区/);
  assert.match(copy.popupNotice ?? "", /niri/);
});

test("macOS 设置页描述辅助功能，不显示 Wayland 限制", () => {
  const copy = settingsPlatformCopy("macos");

  assert.match(copy.selectionIntro, /辅助功能/);
  assert.match(copy.selectionModes.primary.label, /辅助功能/);
  assert.equal(copy.popupNotice, null);
});
