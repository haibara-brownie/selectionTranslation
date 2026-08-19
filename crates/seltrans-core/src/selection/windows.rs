//! Windows 取词。
//!
//! Windows 既没有 Linux 那样的主选区，也没有一个"读取任意应用当前选中文本"的通用接口
//! —— UI Automation 的 TextPattern 只有部分控件实现，浏览器、Electron、终端各有各的窟窿，
//! 覆盖率还不如模拟复制。所以这里只有一条路：**模拟 Ctrl+C，读走剪贴板，再把剪贴板还原**。
//!
//! 这条路上有三个必须处理的坑，按严重程度排：
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
//! - 传统控制台（cmd / PowerShell 的 conhost）里 Ctrl+C 是中断信号不是复制，这类窗口取不到词
//!   （Windows Terminal 有选中时 Ctrl+C 是复制，没问题）；
//! - 用 Raw Input / DirectInput 读键盘的程序（多数游戏）会无视注入的按键。

use std::time::{Duration, Instant};

use arboard::Clipboard;
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, GetLastError};
use windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_C, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN,
    VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
};

use crate::logging;

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

/// 模拟一次 Ctrl+C，读走剪贴板内容，然后把剪贴板还原成原样。
fn copy_via_sendinput() -> Result<String, String> {
    let original = read_clipboard();
    logging::info(&format!(
        "走 Ctrl+C 取词，先备份剪贴板（{} 字符）",
        original.as_ref().map(|s| s.chars().count()).unwrap_or(0)
    ));
    // 读剪贴板不会改序号，所以备份之后再取基准值是安全的
    let seq_before = clipboard_seq();
    if seq_before == 0 {
        logging::info("拿不到剪贴板序号，退回比对内容判断复制是否发生");
    }

    // 先抬修饰键再发 Ctrl+C；守卫析构时会再抬一次
    let _guard = ModifierGuard::engage();

    send(&[
        key_down(VK_LCONTROL),
        key_down(VK_C),
        key_up(VK_C),
        key_up(VK_LCONTROL),
    ])
    .map_err(|e| {
        let msg = format!("模拟 Ctrl+C 失败：{e}");
        logging::error(&msg);
        msg
    })?;

    // 等剪贴板发生变化，最多 800ms
    let deadline = Instant::now() + CLIPBOARD_WAIT;
    let mut captured = None;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
        if seq_before != 0 {
            if clipboard_seq() == seq_before {
                continue;
            }
            // 序号变了不代表内容写完了：复制方是先 EmptyClipboard（序号就已经跳了）
            // 再 SetClipboardData，中间读会读到空。等一下再读，读不到就继续等下一轮。
            std::thread::sleep(Duration::from_millis(30));
            captured = read_clipboard();
            if captured.is_some() {
                break;
            }
        } else {
            let now = read_clipboard();
            if now.is_some() && now != original {
                captured = now;
                break;
            }
        }
    }

    let result = match captured {
        Some(t) if !logging::is_blank(&t) => {
            logging::info(&format!("Ctrl+C 取到 {} 字符", t.chars().count()));
            Ok(t)
        }
        Some(t) => {
            let msg = format!(
                "Ctrl+C 取到的内容全是空白/零宽字符（{} 字符）：{}",
                t.chars().count(),
                logging::preview(&t)
            );
            logging::warn(&msg);
            Err(msg)
        }
        None => {
            let msg = "模拟 Ctrl+C 后剪贴板没有变化。可能是没有选中文本，或者当前窗口不响应 Ctrl+C\
                       （传统控制台里 Ctrl+C 是中断信号），也可能是它以管理员权限运行、\
                       UIPI 挡住了我们注入的按键"
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
/// `mode` 三个取值在 Windows 上**都走模拟 Ctrl+C**。"primary"（主选区）这里不报错而是
/// 静默降级：配置可能是从 Linux 那边同步过来的，也可能是默认值，对 Windows 用户来说
/// 弹一句"本平台没有主选区"既看不懂又没法处理 —— 不如照常取词，把降级记进日志。
pub fn grab(mode: &str) -> Result<String, String> {
    logging::info(&format!("开始取词，方式={mode}"));
    if mode == "primary" {
        logging::info("Windows 没有主选区概念，取词方式「主选区」按「模拟复制」处理");
    }

    let result = copy_via_sendinput();

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
        "输入模拟（SendInput）".into(),
        true,
        "Windows 自带，无需安装 ydotool 之类的外部依赖".into(),
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
