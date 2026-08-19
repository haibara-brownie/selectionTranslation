//! macOS 取词。
//!
//! mac 没有 X11/Wayland 那样的「主选区」，选中文字不会自动进任何缓冲区，所以两条路：
//!
//! 1. **辅助功能 API**（`AXUIElementCopyAttributeValue` 读 `kAXSelectedTextAttribute`）——
//!    零副作用，不碰剪贴板，Safari / 备忘录 / 邮件 / 终端这类原生（AppKit）应用直接就能读到；
//! 2. **模拟 ⌘C** —— 第 1 条拿不到时的兜底，读完之后会**还原原来的剪贴板内容**。
//!
//! 为什么必须有第 2 条：Electron（VS Code、Slack、Discord）和 Java Swing 应用的辅助功能树
//! 往往根本不暴露 `AXSelectedText`，AX 那边只会返回 `kAXErrorAttributeUnsupported`。
//! 这类应用在 mac 上占比不低，没有兜底等于一半场景不可用。
//!
//! 第 2 条路有个必须处理的坑：用户按快捷键的那一刻**手还按着修饰键**（比如 ⌘⇧T），
//! 这时候直接发 ⌘C，应用收到的可能是 `⌘⇧⌃C` —— 复制不到东西，还可能让系统记的修饰键
//! 状态和物理按键脱节（表现为键盘「卡住」、整台机器没法操作，Linux 上真踩过一次）。
//! 所以发 ⌘C 之前必须先把所有修饰键显式抬起，并且无论中途出什么岔子（提前 return、
//! panic）都要再抬一次 —— 见下面的 [`ModifierGuard`]。
//!
//! enigo 的 `independent_of_keyboard_state` 会用私有事件源，让我们发出的事件带的
//! flags 由 enigo 自己说了算，看着像是能免掉守卫；但事件最终是 post 到 HID tap 的，
//! 系统那份**物理**修饰键状态不归这个开关管。所以守卫照留不误。
//!
//! **权限**：两条路都要「辅助功能」授权（TCC）。这里绝不静默失败 —— 没授权就明说，
//! 并给出勾选路径。注意 TCC 是按「应用」授权的（bundle id + 代码签名），从终端直接跑
//! 裸二进制时授权算在**终端**头上，装成 .app 之后要重新勾一次。

use std::ptr::NonNull;
use std::time::{Duration, Instant};

use enigo::{Direction, Enigo, InputError, Key, Keyboard, NewConError, Settings};
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
use objc2_application_services::{AXError, AXIsProcessTrusted, AXUIElement};
use objc2_core_foundation::{CFRetained, CFString, CFType};
use objc2_foundation::NSString;

use crate::logging;

/// 没授权时给用户的原话。写死路径是有意的 —— 让用户在系统设置里现找「辅助功能」在哪
/// 比直接告诉他慢得多。
const GRANT_HINT: &str = "请到「系统设置 → 隐私与安全性 → 辅助功能」里勾选本应用；\
                          如果是从终端直接跑的裸二进制，被授权的其实是终端本身，\
                          装成 .app 之后需要重新勾一次";

/// `kAXFocusedUIElementAttribute` / `kAXSelectedTextAttribute` 的字面值。
///
/// 这两个常量在 C 头文件里是 `#define ... CFSTR("...")`，`objc2-application-services`
/// 没把 CFSTR 宏翻译出来，所以只能自己拼 CFString。字符串本身是 ABI 冻结的，不会变。
const ATTR_FOCUSED_UI_ELEMENT: &str = "AXFocusedUIElement";
const ATTR_SELECTED_TEXT: &str = "AXSelectedText";

/// kVK_ANSI_C（Carbon `Events.h` 里的物理键码）。
///
/// 故意**不用** `Key::Unicode('c')`：enigo 在 mac 上会去当前键盘布局里反查 'c' 落在哪个
/// 键上，而输入法切到中文/日文时这次反查可能拿不到 Unicode 布局数据；它查不到的返回值
/// 是 `0`，也就是 kVK_ANSI_A —— 于是我们的 ⌘C 会变成 ⌘A（全选）。用固定物理键码没这风险。
const KEY_ANSI_C: Key = Key::Other(8);

/// 左右两侧的 ⌘ / ⇧ / ⌃ / ⌥。宁可多发几次抬起，也不能让键卡住。
const MODIFIER_KEYS: [Key; 8] = [
    Key::Meta,
    Key::RCommand,
    Key::Shift,
    Key::RShift,
    Key::Control,
    Key::RControl,
    Key::Alt,
    Key::ROption,
];

// ---------------------------------------------------------------------------
// 权限
// ---------------------------------------------------------------------------

fn is_trusted() -> bool {
    // SAFETY: 无参数、无前置条件，任何线程都能调
    unsafe { AXIsProcessTrusted() }
}

// ---------------------------------------------------------------------------
// 路线 1：辅助功能 API
// ---------------------------------------------------------------------------

/// 把 AXError 翻译成人话。日志里只看到一串 -25205 对排查没什么帮助。
fn describe(err: AXError) -> String {
    let why = match err {
        AXError::APIDisabled => "辅助功能 API 被系统关掉了",
        AXError::AttributeUnsupported => "这个应用不提供该属性（Electron / Java 应用常见）",
        AXError::NoValue => "属性存在但没有值（多半是没选中文本）",
        AXError::CannotComplete => "跟目标应用通信失败，它可能正忙或没响应",
        AXError::NotImplemented => "这个应用没实现辅助功能 API",
        AXError::InvalidUIElement => "元素已经失效（窗口被关掉了？）",
        AXError::IllegalArgument => "参数非法",
        AXError::Failure => "系统内部错误",
        _ => "未知错误",
    };
    format!("{why}（AXError {}）", err.0)
}

/// 读某个元素的一个属性，返回值按 Copy 语义是 +1 引用，交给 `CFRetained` 接管。
fn copy_attribute(
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<CFRetained<CFType>, AXError> {
    let key = CFString::from_static_str(attribute);
    let mut raw: *const CFType = std::ptr::null();

    // SAFETY: `&mut raw` 是一个有效可写的出参槽位；成功时 AX 往里塞一个 +1 的对象，
    // 下面立刻用 CFRetained 接管所有权，不会泄漏也不会二次释放。
    let err = unsafe { element.copy_attribute_value(&key, NonNull::from(&mut raw)) };
    if err != AXError::Success {
        return Err(err);
    }

    // 有些应用返回 Success 但给个空指针，别信它
    let ptr = NonNull::new(raw.cast_mut()).ok_or(AXError::NoValue)?;
    // SAFETY: 指针来自 AX 的 Copy 系列函数，引用计数已经是 +1
    Ok(unsafe { CFRetained::from_raw(ptr) })
}

/// 走辅助功能 API 读选中文本。零副作用 —— 不碰剪贴板，不模拟任何按键。
fn read_via_accessibility() -> Result<String, String> {
    if !is_trusted() {
        return Err(format!("没有辅助功能授权，读不到选中文本。{GRANT_HINT}"));
    }

    // SAFETY: 拿全局的 system-wide 元素，没有额外前置条件
    let system = unsafe { AXUIElement::new_system_wide() };

    // 先问系统「现在焦点在谁身上」，再问那个元素要选中文本。
    // 不走 kAXFocusedApplicationAttribute 再往下钻 —— 那样要遍历子树，慢且容易走错分支。
    let focused = copy_attribute(&system, ATTR_FOCUSED_UI_ELEMENT)
        .map_err(|e| format!("拿不到焦点元素：{}", describe(e)))?
        .downcast::<AXUIElement>()
        .map_err(|_| "焦点元素返回的不是 AXUIElement".to_string())?;

    let value = copy_attribute(&focused, ATTR_SELECTED_TEXT)
        .map_err(|e| format!("焦点元素给不出 AXSelectedText：{}", describe(e)))?;

    let text = value
        .downcast::<CFString>()
        .map_err(|_| "AXSelectedText 返回的不是字符串".to_string())?
        .to_string();

    if logging::is_blank(&text) {
        // 网页上选到空行、图标字体时很容易拿到一串肉眼看不见的字符
        let msg = format!(
            "AXSelectedText 有 {} 字符但全是空白/零宽字符：{}",
            text.chars().count(),
            logging::preview(&text)
        );
        logging::warn(&msg);
        return Err(msg);
    }
    Ok(text)
}

// ---------------------------------------------------------------------------
// 路线 2：模拟 ⌘C
// ---------------------------------------------------------------------------

/// 只要这个守卫还活着，析构时一定会再抬一次修饰键 —— 提前 return / panic 都覆盖得到。
///
/// 它把 `Enigo` 一起拿走，是为了让「发按键」这件事只能通过守卫做：想发按键就必须先
/// 经过 `engage()`，绕不过去。
struct ModifierGuard {
    enigo: Enigo,
}

impl ModifierGuard {
    fn engage(enigo: Enigo) -> Self {
        let mut guard = ModifierGuard { enigo };
        guard.release_all();
        // 给系统时间把抬起事件派发到目标应用；顺便也给用户松开快捷键的时间
        std::thread::sleep(Duration::from_millis(120));
        guard
    }

    fn release_all(&mut self) {
        for key in MODIFIER_KEYS {
            if let Err(e) = self.enigo.key(key, Direction::Release) {
                logging::warn(&format!("抬起修饰键 {key:?} 失败：{e}"));
            }
        }
    }
}

impl Drop for ModifierGuard {
    fn drop(&mut self) {
        self.release_all();
    }
}

fn enigo_settings() -> Settings {
    Settings {
        // mac 专属：用私有事件源，我们发出去的事件带什么 flags 由 enigo 说了算，
        // 不跟物理键盘状态合并。但事件最终 post 到 HID tap，系统那份物理修饰键状态
        // 这个开关管不着 —— 所以 ModifierGuard 一样不能省。
        independent_of_keyboard_state: true,
        // 没授权时让系统弹出授权面板。一次性的事，弹一下比让用户自己猜强；
        // 我们同时还会返回一条写清路径的错误信息，两手都要。
        open_prompt_to_get_permissions: true,
        // Enigo 析构时兜底再放一次按住的键
        release_keys_when_dropped: true,
        ..Settings::default()
    }
}

fn pasteboard_text(pb: &NSPasteboard) -> Option<String> {
    // SAFETY: 框架导出的常量字符串，进程活着它就有效
    let ty = unsafe { NSPasteboardTypeString };
    pb.stringForType(ty).map(|s| s.to_string())
}

/// 还原剪贴板。只还原纯文本 —— 富文本、图片还不回去，这是已知取舍（见 mod.rs）。
fn restore_pasteboard(pb: &NSPasteboard, text: Option<&str>) {
    // NSPasteboard 写入必须先 clearContents 取得所有权，这一步同时会把我们刚
    // ⌘C 进去的内容清掉
    pb.clearContents();
    let Some(t) = text else {
        logging::info("原剪贴板里没有纯文本，只能清空（原来若是图片/富文本，找不回来了）");
        return;
    };
    // SAFETY: 同上
    let ty = unsafe { NSPasteboardTypeString };
    if !pb.setString_forType(&NSString::from_str(t), ty) {
        logging::warn("还原剪贴板失败：NSPasteboard 拒绝了写入");
    }
}

/// 发一次 ⌘C。拆成独立函数是为了让上面用 `?` 提前返回时，守卫的 Drop 一定跑得到。
fn send_command_c(guard: &mut ModifierGuard) -> Result<(), InputError> {
    guard.enigo.key(Key::Meta, Direction::Press)?;
    let clicked = guard.enigo.key(KEY_ANSI_C, Direction::Click);
    // C 键失败也要把 ⌘ 放开再报错，不能让它悬着
    let released = guard.enigo.key(Key::Meta, Direction::Release);
    clicked.and(released)
}

/// 模拟一次 ⌘C，读走剪贴板内容，然后把剪贴板还原成原样。
fn copy_via_command_c() -> Result<String, String> {
    let enigo = Enigo::new(&enigo_settings()).map_err(|e| {
        let msg = match e {
            NewConError::NoPermission => {
                format!("没有辅助功能授权，系统拒绝了模拟按键。{GRANT_HINT}")
            }
            other => format!("初始化按键模拟失败：{other}"),
        };
        logging::error(&msg);
        msg
    })?;

    let pb = NSPasteboard::generalPasteboard();
    let original = pasteboard_text(&pb);
    // changeCount 是「剪贴板被换过几次」的单调计数器。用它判断 ⌘C 有没有生效，比对比
    // 内容可靠得多 —— 用户选中的文本正好和剪贴板里已有的一样时（重复翻译同一个词，很常见），
    // 对比内容会误判成「没变化」。
    let before = pb.changeCount();
    logging::info(&format!(
        "走 ⌘C 兜底取词，先备份剪贴板（{} 字符，changeCount={before}）",
        original.as_ref().map(|s| s.chars().count()).unwrap_or(0)
    ));

    // 先抬修饰键再发 ⌘C；守卫析构时会再抬一次
    let mut guard = ModifierGuard::engage(enigo);

    if let Err(e) = send_command_c(&mut guard) {
        let msg = format!("模拟 ⌘C 失败：{e}");
        logging::error(&msg);
        // 这里提前 return，guard 的 Drop 负责把修饰键再抬一次。
        // 剪贴板没动过（changeCount 没变），不需要还原。
        return Err(msg);
    }

    // 等剪贴板真的被换掉，最多 800ms
    let deadline = Instant::now() + Duration::from_millis(800);
    let mut touched = false;
    let mut captured = None;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(40));
        if pb.changeCount() != before {
            touched = true;
            captured = pasteboard_text(&pb);
            break;
        }
    }

    let result = match captured {
        Some(t) if !logging::is_blank(&t) => {
            logging::info(&format!("⌘C 取到 {} 字符", t.chars().count()));
            Ok(t)
        }
        Some(t) => {
            let msg = format!(
                "⌘C 取到的内容全是空白/零宽字符（{} 字符）：{}",
                t.chars().count(),
                logging::preview(&t)
            );
            logging::warn(&msg);
            Err(msg)
        }
        None if touched => {
            let msg = "⌘C 换掉了剪贴板，但里面没有纯文本（复制到的可能是图片或附件）".to_string();
            logging::warn(&msg);
            Err(msg)
        }
        None => {
            let msg = "模拟 ⌘C 后剪贴板没有变化，可能是当前应用不响应复制，或者并没有选中文本"
                .to_string();
            logging::warn(&msg);
            Err(msg)
        }
    };

    // 只有真被换过才还原 —— 没换过就别去 clearContents，那会白白毁掉里面的富文本/图片
    if touched {
        restore_pasteboard(&pb, original.as_deref());
    }
    result
}

// ---------------------------------------------------------------------------
// 对外接口
// ---------------------------------------------------------------------------

/// 按配置的取词方式抓取选中文本。
///
/// mac 没有主选区的概念，所以 `"primary"` 在这里表示「只走辅助功能 API，不模拟按键」——
/// 语义上跟 Linux 的主选区对齐：都是零副作用、不碰剪贴板的那条路。
pub fn grab(mode: &str) -> Result<String, String> {
    logging::info(&format!("开始取词，方式={mode}"));

    let result = match mode {
        "primary" => read_via_accessibility().map_err(|e| {
            format!(
                "{e}（可在设置里把取词方式改成「自动」以启用 ⌘C 兜底，\
                 Electron / Java 应用基本都得靠它）"
            )
        }),
        "clipboard" => copy_via_command_c(),
        _ => match read_via_accessibility() {
            Ok(s) => {
                logging::info("辅助功能 API 命中，无需 ⌘C 兜底");
                Ok(s)
            }
            Err(e) => {
                logging::info(&format!("辅助功能 API 没拿到（{e}），转 ⌘C 兜底"));
                copy_via_command_c()
            }
        },
    };

    match &result {
        Ok(s) => logging::info(&format!(
            "取词成功：{} 字符 | {}",
            s.chars().count(),
            logging::preview(s)
        )),
        Err(e) => logging::warn(&format!("取词失败：{e}")),
    }
    result
}

/// 「关于」页的依赖自检：(名称, 是否就绪, 说明)
pub fn deps_report() -> Vec<(String, bool, String)> {
    let trusted = is_trusted();
    let mut out = Vec::new();

    out.push((
        "辅助功能授权".into(),
        trusted,
        if trusted {
            "已授权，两条取词路线都可用".into()
        } else {
            format!("未授权，取词完全不可用。{GRANT_HINT}")
        },
    ));

    out.push((
        "辅助功能 API 取词".into(),
        trusted,
        if trusted {
            "可用。Safari / 备忘录 / 邮件 / 终端这类原生应用能直接读到选中文本，\
             不碰剪贴板"
                .into()
        } else {
            "需要辅助功能授权".into()
        },
    ));

    out.push((
        "⌘C 兜底取词".into(),
        trusted,
        if trusted {
            "可用。Electron（VS Code / Slack）和 Java 应用读不到 AXSelectedText，\
             会自动走这条；取完会还原剪贴板的纯文本内容"
                .into()
        } else {
            "需要辅助功能授权才能模拟按键".into()
        },
    ));

    let log = logging::log_path();
    out.push((
        "运行日志".into(),
        true,
        if log.exists() {
            format!(
                "{}（{} KB）",
                log.display(),
                std::fs::metadata(&log).map(|m| m.len() / 1024).unwrap_or(0)
            )
        } else {
            format!("{}（还没有内容）", log.display())
        },
    ));

    out
}
