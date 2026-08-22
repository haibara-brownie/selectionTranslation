//! Windows 取词。
//!
//! Windows 没有 Linux 那样的主选区，取词走**两级**：
//!
//! 1. **UI Automation**（首选）—— 零副作用，直接从控件的可访问性树读选中文本，
//!    不碰剪贴板也不模拟按键。见 [`via_uia`]。
//! 2. **模拟复制**（兜底）—— UIA 读不到时先发 `Ctrl+Insert` 再发 `Ctrl+C`，
//!    读走剪贴板再还原。见 [`copy_via_sendinput`]。
//!
//! 早先这里只有第 2 条，模块头当时写的理由是「UIA 的 TextPattern 只有部分控件实现，
//! 覆盖率还不如模拟复制」。**那个判断不准确**：漏掉的关键一步是「焦点元素自己没有选区时
//! 要沿祖先链往上找」—— Chromium / Edge / Electron 里拿到 UIA 焦点的是带 `tabindex` 的
//! 容器，选区由祖先的 Document 元素持有。补上这一步之后，现代应用基本都能走 UIA。
//! 这条经验来自 selection-hook（MIT，Cherry Studio 划词助手用的就是它）的
//! `docs/zh-CN/WINDOWS.md` 与 `src/windows/selection_hook.cc`。
//!
//! 兜底那条路上有三个必须处理的坑，按严重程度排：
//!
//! 1. **发 Ctrl+C 之前必须先把修饰键抬起来**。用户按下快捷键（比如 `Ctrl+Alt+T`）的那一刻
//!    手还按着，这时候直接发 Ctrl+C，目标应用收到的是 `Ctrl+Alt+Ctrl+C` —— 复制不到东西；
//!    更糟的是系统记录的修饰键状态会和物理按键脱节，表现就是键盘"卡住"、整台电脑没法
//!    操作（Linux 上真踩过这个事故）。所以这里用 RAII 守卫 [`ModifierGuard`]：进场时抬起
//!    左右 Ctrl/Shift/Alt/Win，析构时**再抬一次**，保证提前 return、出错、panic 都覆盖得到。
//!    别因为"Windows 好像没这个问题"就省掉 —— 修饰键状态是全局的，卡住的代价太大。
//! 2. **抬 Win 键会把开始菜单弹出来**。用户物理按下 Win 之后我们注入一个 Win 抬起，外壳
//!    看到的是一次完整的"单按 Win"，于是弹开始菜单挡住译文窗口。对策是在抬 Win 之前注入
//!    一个未分配的虚拟键（0xE8）把这个组合"弄脏"，外壳就不认它是单按了 —— AutoHotkey 的
//!    `A_MenuMaskKey` 用的是同一招。0xE8 在 Windows 上永久保留未分配，注入它没有副作用。
//! 3. **UIPI（用户界面特权隔离）挡提权窗口**。目标程序以管理员权限运行时，普通权限的
//!    进程发不进去输入：`SendInput` 会直接返回 0 且 `GetLastError() == ERROR_ACCESS_DENIED`。
//!    这是 Windows 的安全设计不是 bug，代码里绕不过去，只能在日志和依赖自检里如实说明
//!    （要么以管理员身份运行 seltrans，要么这个窗口就是取不了词）。
//!
//! 调用方还要注意一条时序约束：**取词必须发生在译文窗口抢到焦点之前**。一旦焦点跑到我们
//! 自己的窗口上，Ctrl+C 就发给我们自己了，永远取不到东西。
//!
//! 其余已知取舍（都写进了 [`deps_report`]）：
//! - 剪贴板只还原**纯文本**：原本是图片、富文本、文件列表的话还不回去；
//! - 我们临时写进剪贴板的内容会被"剪贴板历史"（Win+V）和第三方剪贴板管理器记一笔，避不开；
//! - 传统控制台（cmd / PowerShell 的 conhost）里 Ctrl+C 是中断信号不是复制 —— 所以兜底
//!   **先发 Ctrl+Insert**，那才是控制台的复制键，也不会打断正在跑的命令；
//! - 用 Raw Input / DirectInput 读键盘的程序（多数游戏）会无视注入的按键。

use std::time::{Duration, Instant};

use arboard::Clipboard;
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, GetLastError};
use windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_C, VK_INSERT, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
    VK_LWIN, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
};

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern, UIA_TextPatternId,
};

use crate::logging;

// ---------------------------------------------------------------------------
// 路线 1：UI Automation
// ---------------------------------------------------------------------------

/// 沿祖先链往上找选区的层数上限。
///
/// 浏览器里持有选区的 Document 元素通常就在焦点元素上方几层，10 层够用；
/// 给个上限是因为 UIA 树在某些应用里可能很深，一路走到根既慢又没意义。
const MAX_WALK_UP: usize = 10;

/// UIA 用来占位「嵌入对象」（图片、图标）的字符。混在译文里没意义，读出来就滤掉。
const EMBEDDED_OBJECT: char = '\u{FFFC}';

/// 从某个 UIA 元素上读选中文本。拿不到、或者拿到的全是空白就当没有。
fn selection_of(el: &IUIAutomationElement) -> Option<String> {
    // SAFETY: 以下都是对活着的 COM 接口的常规调用，引用计数由 windows 的 RAII 包装管。
    let pattern: IUIAutomationTextPattern =
        unsafe { el.GetCurrentPatternAs(UIA_TextPatternId) }.ok()?;
    let ranges = unsafe { pattern.GetSelection() }.ok()?;
    let count = unsafe { ranges.Length() }.ok()?;

    let mut out = String::new();
    for i in 0..count {
        // 多段选区（表格里跨单元格选）会给多个 range，拼起来
        let Ok(range) = (unsafe { ranges.GetElement(i) }) else {
            continue;
        };
        // -1 表示不限长度
        let Ok(text) = (unsafe { range.GetText(-1) }) else {
            continue;
        };
        out.push_str(&text.to_string());
    }

    let cleaned: String = out.chars().filter(|c| *c != EMBEDDED_OBJECT).collect();
    if logging::is_blank(&cleaned) {
        return None;
    }
    Some(cleaned)
}

/// 走 UI Automation 读当前选中文本。**零副作用** —— 不碰剪贴板、不模拟任何按键。
///
/// 两步，顺序不能反：
///
/// 1. 问焦点元素自己要选区；
/// 2. 要不到就**沿祖先链往上找**。这一步不是可选的优化：Chromium / Edge / Electron 里
///    拿到 UIA 焦点的往往是一个带 `tabindex` 的容器，而**选区由祖先的 Document 元素
///    持有**，只问焦点元素会一路失败、把浏览器和 Electron 全推给剪贴板兜底。
///
/// 拿不到就返回 `None`，由调用方决定要不要退到模拟复制。
fn via_uia() -> Option<String> {
    // COM 初始化是按线程算的，所以每次都初始化一遍；这个线程已经初始化过（哪怕线程
    // 模型不同）会返回失败码，忽略即可 —— 后续调用照常。
    //
    // 这一行是 `grab` 把取词整体丢进一次性线程的原因：MTA 会把所在线程占成这个模型，
    // 在主线程上跑等于不让 tao 之后 OleInitialize（要 STA），建窗会 panic。
    // SAFETY: 参数无内存所有权含义，重复调用是被明确允许的。
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }

    // SAFETY: 标准的 COM 对象创建，失败会返回 Err 而不是给出无效指针。
    let uia: IUIAutomation =
        match unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) } {
            Ok(v) => v,
            Err(e) => {
                logging::info(&format!("UI Automation 不可用（{e}），转模拟复制"));
                return None;
            }
        };

    // SAFETY: 同上。
    let focused = match unsafe { uia.GetFocusedElement() } {
        Ok(v) => v,
        Err(e) => {
            logging::info(&format!("UIA 拿不到焦点元素（{e}），转模拟复制"));
            return None;
        }
    };

    if let Some(t) = selection_of(&focused) {
        logging::info("UIA 命中：焦点元素直接持有选区");
        return Some(t);
    }

    // SAFETY: 同上。
    let walker = match unsafe { uia.ControlViewWalker() } {
        Ok(w) => w,
        Err(e) => {
            logging::info(&format!("UIA 拿不到 ControlViewWalker（{e}）"));
            return None;
        }
    };

    let mut node = focused;
    for level in 1..=MAX_WALK_UP {
        // SAFETY: 同上；走到根之后返回 Err，循环就此结束。
        node = match unsafe { walker.GetParentElement(&node) } {
            Ok(p) => p,
            Err(_) => {
                logging::info(&format!("UIA 沿祖先链找到第 {level} 层已到顶，没有选区"));
                return None;
            }
        };
        if let Some(t) = selection_of(&node) {
            logging::info(&format!(
                "UIA 命中：选区在第 {level} 层祖先上（浏览器/Electron 常见）"
            ));
            return Some(t);
        }
    }
    logging::info(&format!(
        "UIA 往上找了 {MAX_WALK_UP} 层都没有选区，转模拟复制"
    ));
    None
}

/// 未分配的虚拟键，注入它不会产生任何效果 —— 专门用来"弄脏" Win 键组合，免得抬 Win 时
/// 把开始菜单招出来。AutoHotkey 的 `A_MenuMaskKey` 默认值也是它。
const VK_MASK: VIRTUAL_KEY = 0xE8;

/// 必须抬起的修饰键。带名字是为了出问题时日志里能看出当时哪个键被按着。
///
/// Win 键排在最后：抬它之前要先插一次遮罩键（见模块头第 2 条），顺序不能乱。
const MODIFIERS: [(VIRTUAL_KEY, &str); 8] = [
    (VK_LCONTROL, "LCtrl"),
    (VK_RCONTROL, "RCtrl"),
    (VK_LSHIFT, "LShift"),
    (VK_RSHIFT, "RShift"),
    (VK_LMENU, "LAlt"),
    (VK_RMENU, "RAlt"),
    (VK_LWIN, "LWin"),
    (VK_RWIN, "RWin"),
];

/// 等剪贴板变化的上限。超过这个时间还没动静就当作"这个应用不响应复制"。
const CLIPBOARD_WAIT: Duration = Duration::from_millis(800);

fn key_event(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                // 留 0 让系统自己按虚拟键查扫描码。手工填扫描码只对读 Raw Input 的程序
                // （游戏）有意义，而那类程序本来就不是划词翻译的目标场景。
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn key_down(vk: VIRTUAL_KEY) -> INPUT {
    key_event(vk, 0)
}

fn key_up(vk: VIRTUAL_KEY) -> INPUT {
    key_event(vk, KEYEVENTF_KEYUP)
}

/// 一批事件一次性发出去。`SendInput` 保证同一批中间不会被别的输入插队，所以
/// "抬修饰键"和"按 Ctrl+C"各自都要凑成一批发。
fn send(inputs: &[INPUT]) -> Result<(), String> {
    // SAFETY: `inputs` 是一个活着的切片，长度和指针一起传给 SendInput，
    // cbSize 传的就是 INPUT 的实际大小 —— 这三者一致是这个调用的全部前提。
    // SendInput 只读不写这块内存，调用返回后我们才继续用 `inputs`。
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };
    if sent as usize == inputs.len() {
        return Ok(());
    }
    // SAFETY: GetLastError 没有任何前提，读的是本线程的错误码。
    // 必须紧跟在失败的调用之后 —— 中间插入任何 Win32 调用都可能把它冲掉。
    let code = unsafe { GetLastError() };
    let hint = if code == ERROR_ACCESS_DENIED {
        "：当前前台窗口以管理员权限运行，UIPI 不允许普通权限的程序向它注入按键。\
         这是 Windows 的安全设计，只能让 seltrans 也以管理员身份运行"
    } else {
        ""
    };
    Err(format!(
        "SendInput 只发出 {sent}/{} 个事件（GetLastError={code}）{hint}",
        inputs.len()
    ))
}

/// 当前物理上被按住的修饰键，只用于日志诊断。
fn held_modifiers() -> Vec<&'static str> {
    MODIFIERS
        .iter()
        .filter(|(vk, _)| {
            // 高位为 1 表示键当前处于按下状态。
            // SAFETY: 传的是 MODIFIERS 表里的常量虚拟键码，都在合法范围内；
            // 这个调用不碰任何我们的内存，只读系统的键盘状态。
            let state = unsafe { GetAsyncKeyState(*vk as i32) };
            (state as u16) & 0x8000 != 0
        })
        .map(|(_, name)| *name)
        .collect()
}

/// 把所有修饰键强制抬起。宁可多发一次，也不能让键卡住。
fn release_all_modifiers() {
    let mut inputs: Vec<INPUT> = Vec::with_capacity(MODIFIERS.len() + 2);
    for (vk, _) in MODIFIERS {
        // 抬 Win 之前先插一次遮罩键，否则外壳会把"物理按下 Win + 我们注入的抬起"
        // 当成一次单按 Win，弹出开始菜单
        if vk == VK_LWIN {
            inputs.push(key_down(VK_MASK));
            inputs.push(key_up(VK_MASK));
        }
        inputs.push(key_up(vk));
    }
    if let Err(e) = send(&inputs) {
        logging::warn(&format!("抬起修饰键失败：{e}"));
    }
}

/// 只要这个守卫还活着，析构时一定会再抬一次修饰键 —— 提前 return / 出错 / panic 都覆盖得到。
struct ModifierGuard;

impl ModifierGuard {
    fn engage() -> Self {
        let held = held_modifiers();
        if !held.is_empty() {
            logging::info(&format!("取词时仍被按住的修饰键：{}", held.join("+")));
        }
        release_all_modifiers();
        // 给系统和目标应用一点时间消化抬起事件；顺便也给用户松开快捷键的时间
        std::thread::sleep(Duration::from_millis(120));
        ModifierGuard
    }
}

impl Drop for ModifierGuard {
    fn drop(&mut self) {
        release_all_modifiers();
    }
}

/// 剪贴板内容的版本号，任何一次写入都会让它 +1。
///
/// 比轮询内容可靠：用户复制的内容恰好和原内容相同、或者复制的是图片时，比内容看不出变化，
/// 看序号一目了然。返回 0 表示当前窗口站没有剪贴板访问权限（服务、隔离会话里会这样），
/// 这时只能退回比内容。
fn clipboard_seq() -> u32 {
    // SAFETY: 无参数、无内存操作，只读一个系统全局计数器。
    unsafe { GetClipboardSequenceNumber() }
}

/// 剪贴板同一时刻只允许一个进程打开，被别人（输入法、剪贴板管理器）占着是常态，
/// 所以每次操作都退避重试几轮再认输。
fn with_clipboard<T>(
    mut op: impl FnMut(&mut Clipboard) -> Result<T, arboard::Error>,
) -> Result<T, arboard::Error> {
    for attempt in 0..5u32 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(20));
        }
        match Clipboard::new().and_then(|mut c| op(&mut c)) {
            Ok(v) => return Ok(v),
            Err(arboard::Error::ClipboardOccupied) => continue,
            Err(e) => return Err(e),
        }
    }
    Err(arboard::Error::ClipboardOccupied)
}

/// 读剪贴板里的纯文本。剪贴板是空的、或者里面是图片，都返回 `None`。
fn read_clipboard() -> Option<String> {
    match with_clipboard(|c| c.get_text()) {
        Ok(s) if !s.is_empty() => Some(s),
        Ok(_) => None,
        // 空剪贴板和"里面不是文本"都走这个错误，不值得记日志
        Err(arboard::Error::ContentNotAvailable) => None,
        Err(e) => {
            logging::info(&format!("读剪贴板失败：{e}"));
            None
        }
    }
}

/// 还原剪贴板。`None` 表示原本就是空的（或者原本是图片 —— 这里还不回去，只能清掉）。
fn write_clipboard(text: Option<&str>) {
    let result = match text {
        Some(t) => with_clipboard(|c| c.set_text(t)),
        None => with_clipboard(|c| c.clear()),
    };
    if let Err(e) = result {
        logging::warn(&format!("还原剪贴板失败：{e}"));
    }
}

/// 用来复制的按键。两个都要试，覆盖面不一样。
#[derive(Clone, Copy)]
enum CopyKey {
    /// `Ctrl+Insert`。**先试它。**
    ///
    /// 传统控制台（cmd / PowerShell 的 conhost）里 `Ctrl+C` 是中断信号而不是复制 ——
    /// 那正是模块头里记的已知限制之一；而 `Ctrl+Insert` 在那里就是复制，也不会打断
    /// 正在跑的命令。它被应用自己占作他用的概率也更低。
    CtrlInsert,
    /// `Ctrl+C`。最通用，但在控制台里会变成中断，也有应用把它绑了别的功能。
    CtrlC,
}

impl CopyKey {
    fn vk(self) -> VIRTUAL_KEY {
        match self {
            CopyKey::CtrlInsert => VK_INSERT,
            CopyKey::CtrlC => VK_C,
        }
    }

    fn name(self) -> &'static str {
        match self {
            CopyKey::CtrlInsert => "Ctrl+Insert",
            CopyKey::CtrlC => "Ctrl+C",
        }
    }

    /// 等剪贴板变化的上限。
    ///
    /// 第一发给短一点：它只是"更安全的首选"，不响应就该赶紧换 Ctrl+C，别让用户为这次
    /// 试探等满 800ms。第二发是最后手段，给足时间。
    fn wait(self) -> Duration {
        match self {
            CopyKey::CtrlInsert => Duration::from_millis(250),
            CopyKey::CtrlC => CLIPBOARD_WAIT,
        }
    }
}

/// 发一次复制键，等剪贴板真的被换掉。换到了返回内容，超时返回 `None`。
///
/// 调用前必须已经抬起修饰键（[`ModifierGuard`]），这里不重复做。
fn copy_once(key: CopyKey, original: Option<&str>) -> Result<Option<String>, String> {
    // 读剪贴板不会改序号，所以每一发之前重新取基准值是安全的
    let seq_before = clipboard_seq();

    send(&[
        key_down(VK_LCONTROL),
        key_down(key.vk()),
        key_up(key.vk()),
        key_up(VK_LCONTROL),
    ])
    .map_err(|e| {
        let msg = format!("模拟 {} 失败：{e}", key.name());
        logging::error(&msg);
        msg
    })?;

    let deadline = Instant::now() + key.wait();
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
        if seq_before != 0 {
            if clipboard_seq() == seq_before {
                continue;
            }
            // 序号变了不代表内容写完了：复制方是先 EmptyClipboard（序号就已经跳了）
            // 再 SetClipboardData，中间读会读到空。等一下再读，读不到就继续等下一轮。
            //
            // 还有一类应用（Acrobat 之类）会分多次写：先写纯文本，再用富文本覆盖。
            // 这个等待同样帮到那种情况 —— 太早读会拿到中间态。
            std::thread::sleep(Duration::from_millis(30));
            if let Some(t) = read_clipboard() {
                return Ok(Some(t));
            }
        } else {
            let now = read_clipboard();
            if now.is_some() && now.as_deref() != original {
                return Ok(now);
            }
        }
    }
    Ok(None)
}

/// 模拟复制，读走剪贴板内容，然后把剪贴板还原成原样。
///
/// 先发 `Ctrl+Insert` 再发 `Ctrl+C`，理由见 [`CopyKey`]。
fn copy_via_sendinput() -> Result<String, String> {
    let original = read_clipboard();
    logging::info(&format!(
        "走模拟复制取词，先备份剪贴板（{} 字符）",
        original.as_ref().map(|s| s.chars().count()).unwrap_or(0)
    ));
    if clipboard_seq() == 0 {
        logging::info("拿不到剪贴板序号，退回比对内容判断复制是否发生");
    }

    // 先抬修饰键，两发共用一个守卫；析构时会再抬一次
    let _guard = ModifierGuard::engage();

    let mut captured = None;
    for key in [CopyKey::CtrlInsert, CopyKey::CtrlC] {
        match copy_once(key, original.as_deref()) {
            Ok(Some(t)) => {
                logging::info(&format!("{} 取到 {} 字符", key.name(), t.chars().count()));
                captured = Some(t);
                break;
            }
            Ok(None) => logging::info(&format!("{} 没让剪贴板发生变化", key.name())),
            Err(e) => {
                // 发不出按键（多半是 UIPI）就没必要再试第二发了
                write_clipboard(original.as_deref());
                return Err(e);
            }
        }
    }

    let result = match captured {
        Some(t) if !logging::is_blank(&t) => Ok(t),
        Some(t) => {
            let msg = format!(
                "复制到的内容全是空白/零宽字符（{} 字符）：{}",
                t.chars().count(),
                logging::preview(&t)
            );
            logging::warn(&msg);
            Err(msg)
        }
        None => {
            let msg = "Ctrl+Insert 和 Ctrl+C 都没让剪贴板发生变化。可能是没有选中文本，\
                       也可能是目标程序以管理员权限运行、UIPI 挡住了我们注入的按键"
                .to_string();
            logging::warn(&msg);
            Err(msg)
        }
    };

    // 不管有没有拿到，都把剪贴板还原回去
    write_clipboard(original.as_deref());
    result
}

/// 按配置的取词方式抓取选中文本。
///
/// 两条路，优先级固定：
///
/// 1. **UI Automation** —— 零副作用，不碰剪贴板也不模拟按键；
/// 2. **模拟复制** —— UIA 读不到时的兜底，读完把剪贴板还原。
///
/// `mode` 的语义跟另两个平台对齐：
/// - `"primary"`：**只走零副作用那条路**（Linux 是主选区，mac 是辅助功能 API，
///   这里是 UIA）。读不到就明确报错，不偷偷去动用户的剪贴板。
/// - `"clipboard"`：直接上模拟复制。
/// - 其他（含 `"auto"`）：先 UIA，不行再模拟复制。
pub fn grab(mode: &str) -> Result<String, String> {
    // 取词必须丢进**一次性线程**，不能占用调用方的线程 —— via_uia 会把所在线程的
    // COM 公寓初始化成 MTA（CoInitializeEx 按线程生效，先到先得）。冷启动
    // `seltrans popup` 那条路上，取词跑在主线程、还赶在建窗之前；随后 tao 建窗口要
    // OleInitialize（要求 STA），主线程已经是 MTA 就得到 RPC_E_CHANGED_MODE，tao 对此
    // 直接 panic —— GUI 子系统下无声无息，表现为「双击 exe 毫无反应」。实测踩过。
    //
    // 常驻托盘模式一直没出事纯属侥幸：快捷键回调跑在插件自己的线程上，弄脏的是那个
    // 线程。线程用完即弃，COM 公寓随线程一起销毁，调用方的线程永远保持干净。
    let mode = mode.to_string();
    std::thread::Builder::new()
        .name("seltrans-grab".into())
        .spawn(move || grab_on_own_thread(&mode))
        .map_err(|e| format!("起不了取词线程：{e}"))?
        .join()
        .unwrap_or_else(|_| Err("取词线程 panic 了，详情见日志".to_string()))
}

fn grab_on_own_thread(mode: &str) -> Result<String, String> {
    logging::info(&format!("开始取词，方式={mode}"));

    let result = match mode {
        "clipboard" => copy_via_sendinput(),
        "primary" => via_uia().ok_or_else(|| {
            "UI Automation 没读到选中文本。可在设置里把取词方式改成「自动」以启用模拟复制兜底 \
             —— 老式 Win32 控件、部分终端和 PDF 阅读器只能靠那条路"
                .to_string()
        }),
        _ => match via_uia() {
            Some(t) => Ok(t),
            None => copy_via_sendinput(),
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
    let mut out = Vec::new();

    out.push((
        "UI Automation 取词".into(),
        true,
        "系统自带，首选路线：直接从控件的可访问性树读选中文本，不碰剪贴板、不模拟按键。\
         现代应用（Chrome / Edge / VS Code / Office）基本都实现了；浏览器和 Electron 里选区\
         常挂在祖先的 Document 元素上，我们会沿祖先链往上找"
            .into(),
    ));

    out.push((
        "输入模拟（SendInput）".into(),
        true,
        "Windows 自带，无需安装 ydotool 之类的外部依赖。UIA 读不到时的兜底：\
         先发 Ctrl+Insert（传统控制台里它才是复制，Ctrl+C 是中断信号），不行再发 Ctrl+C"
            .into(),
    ));

    // 序号为 0 或者连剪贴板都打不开，说明当前会话根本没有剪贴板访问权限（例如跑在服务里）
    let seq_ok = clipboard_seq() != 0;
    let open_ok = Clipboard::new().is_ok();
    out.push((
        "剪贴板访问".into(),
        seq_ok && open_ok,
        match (seq_ok, open_ok) {
            (true, true) => "可读写，且能用剪贴板序号精确判断复制是否发生".into(),
            (false, true) => "能打开但拿不到剪贴板序号，只能退回比对内容，判断会不准".into(),
            _ => "打不开剪贴板。请确认 seltrans 跑在正常的桌面会话里，而不是服务/隔离会话中".into(),
        },
    ));

    // 这条不是"缺了什么要去装"，而是一条永远存在的系统限制，所以标成就绪 + 文字说明，
    // 免得「关于」页挂一个永远消不掉的红叉。
    out.push((
        "已知限制：UIPI".into(),
        true,
        "目标程序以管理员权限运行时，普通权限的 seltrans 无法向它注入按键，因而取不到词。\
         这是 Windows 的用户界面特权隔离机制，不是 bug；确实需要时可让 seltrans 也以\
         管理员身份运行"
            .into(),
    ));

    out.push((
        "已知限制：剪贴板还原".into(),
        true,
        "取词会临时占用剪贴板，事后只还原纯文本 —— 原本是图片、富文本、文件列表的话还不回去；\
         临时内容也会被剪贴板历史（Win+V）和第三方剪贴板管理器记一笔"
            .into(),
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
