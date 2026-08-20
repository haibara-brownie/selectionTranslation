/**
 * 快捷键的两件小事：把浏览器的 `KeyboardEvent.code` 翻成 Tauri 的键名，
 * 以及把 Tauri 的快捷键串排成给人看的样子。
 *
 * 单独一个模块是因为**两个窗口都要用**：设置页要录制（`dom.ts` 的 `hotkeyRow`），
 * 弹窗的首次使用提示要显示（`popup.ts`）。放在 `dom.ts` 里的话，弹窗为了一个
 * 格式化函数得把整套设置页控件都拖进包里。
 */

/** 键盘事件的 `code` → Tauri 快捷键语法里的键名。认不出来的返回 `null`。 */
export function codeToKey(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  const named: Record<string, string> = {
    Comma: "Comma", Period: "Period", Slash: "Slash", Semicolon: "Semicolon",
    Quote: "Quote", BracketLeft: "BracketLeft", BracketRight: "BracketRight",
    Backslash: "Backslash", Minus: "Minus", Equal: "Equal", Backquote: "Backquote",
    Space: "Space", Enter: "Enter", Tab: "Tab", Backspace: "Backspace",
    ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right",
    Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown",
    Insert: "Insert", Delete: "Delete",
  };
  return named[code] ?? null;
}

/** mac 上把修饰键写成符号，跟系统里到处都是的写法一致 */
const MAC_GLYPH: Record<string, string> = {
  Control: "⌃", Alt: "⌥", Shift: "⇧", Super: "⌘", Command: "⌘", CommandOrControl: "⌘",
};

/** 键名 → 给人看的写法。没列到的原样显示。 */
const KEY_LABEL: Record<string, string> = {
  Comma: ",", Period: ".", Slash: "/", Semicolon: ";", Quote: "'",
  BracketLeft: "[", BracketRight: "]", Backslash: "\\", Minus: "-",
  Equal: "=", Backquote: "`", Space: "空格", Enter: "↩", Tab: "⇥",
  Up: "↑", Down: "↓", Left: "←", Right: "→",
};

/** 把 Tauri 的快捷键串排成给人看的样子 */
export function formatShortcut(s: string, isMac: boolean): string {
  const parts = s.split("+").map((p) => p.trim()).filter(Boolean);
  if (parts.length === 0) return "";
  const out = parts.map((p) => (isMac && MAC_GLYPH[p]) || KEY_LABEL[p] || p);
  // mac 上修饰键符号是连着写的（⌥⇧T），另两家用加号
  return isMac ? out.join("") : out.join("+");
}
