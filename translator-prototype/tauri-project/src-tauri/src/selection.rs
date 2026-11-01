//! 划词翻译：捕获任意应用中当前选中的文本
//!
//! 三级策略（逐级兜底，任一级取到文本即返回）：
//!   1. **UI Automation**（主路径）：直接读目标控件的 TextPattern 选区。
//!      不碰剪贴板、不需要目标程序支持复制，因此**终端、PDF 阅读器、
//!      禁用复制的自绘界面**都能取到；也不会破坏用户剪贴板内容。
//!   2. **剪贴板**（兜底）：向目标发送 Ctrl+C 再读剪贴板（旧实现）。
//!      用于 UIA 未实现 TextPattern 的应用（部分游戏、老旧程序）。
//!   3. **WPF/浏览器** 等由第 1 级覆盖。
//!
//! 已知边界（诚实说明，不承诺 100%）：
//!   - 目标窗口以管理员身份运行而本程序不是 → Windows UIPI 阻断，
//!     此时返回明确错误而不是静默失败（调用方可提示用户提权重启）。
//!   - 纯自绘 UI（部分游戏、DirectX 渲染）既无 UIA 文本也不响应 Ctrl+C。

use windows::Win32::Foundation::{HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationTextPattern, UIA_TextPatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::core::Interface;

/// 标准剪贴板文本格式（CF_UNICODETEXT = 13）
const CF_UNICODETEXT: u32 = 13;
/// 字符 'C' 的虚拟键码
const VK_C: VIRTUAL_KEY = VIRTUAL_KEY(0x43);

/// 划词捕获结果：区分"没选中"与"权限/环境不支持"，便于给用户准确提示
#[derive(Debug, Clone, serde::Serialize)]
pub struct CaptureResult {
    pub text: Option<String>,
    /// 使用的捕获方式：uia / clipboard / none
    pub method: String,
    /// 失败原因（成功时为 None）
    pub reason: Option<String>,
}

impl CaptureResult {
    fn ok(text: String, method: &str) -> Self {
        Self { text: Some(text), method: method.to_string(), reason: None }
    }
    fn fail(method: &str, reason: Option<String>) -> Self {
        Self { text: None, method: method.to_string(), reason }
    }
}

/// 捕获前台应用当前选中的文本（UIA 优先，失败回落剪贴板）
pub fn capture_selection() -> CaptureResult {
    // 先试 UIA：不破坏剪贴板、不受"禁用复制"影响
    match capture_via_uia() {
        Ok(Some(t)) if !t.trim().is_empty() => return CaptureResult::ok(t, "uia"),
        Ok(_) => { /* UIA 没拿到（无选区或控件不支持 TextPattern）→ 走剪贴板 */ }
        Err(e) => {
            // UIA 本身失败（COM 初始化异常等）也继续尝试剪贴板
            eprintln!("[selection] UIA 失败，回落剪贴板: {e}");
        }
    }

    // 兜底：剪贴板
    match capture_via_clipboard() {
        Some(t) if !t.trim().is_empty() => CaptureResult::ok(t, "clipboard"),
        _ => {
            let reason = match foreground_is_elevated() {
                Some(true) => Some(
                    "目标窗口以管理员身份运行，当前权限无法读取其选区。\
                     请以管理员身份重新启动本程序，或改用截图翻译。"
                        .to_string(),
                ),
                _ => None,
            };
            CaptureResult::fail("none", reason)
        }
    }
}

/// 从候选 UIA 元素中取非空选区文本
fn selection_from_element(element: &windows::Win32::UI::Accessibility::IUIAutomationElement) -> Option<String> {
    use windows::Win32::UI::Accessibility::{IUIAutomationTextPattern, UIA_TextPatternId};
    use windows::core::Interface;
    unsafe {
    let pattern = element.GetCurrentPattern(UIA_TextPatternId).ok()?;
    let text_pattern: IUIAutomationTextPattern = pattern.cast().ok()?;
    let ranges = text_pattern.GetSelection().ok()?;
    let count = ranges.Length().unwrap_or(0);
    if count <= 0 {
        return None;
    }
    let mut out = String::new();
    for i in 0..count {
        let Ok(range) = ranges.GetElement(i) else { continue };
        let Ok(bstr) = range.GetText(-1) else { continue };
        let s = bstr.to_string();
        if s.trim().is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&s);
    }
    let t = out.trim().to_string();
    if t.is_empty() { None } else { Some(t) }
    }
}

/// 遍历元素子树里所有"支持 TextPattern"的后代，找第一个非空选区。
/// 用 FindAll + 属性条件一次拿到候选列表（比 RawViewWalker 逐节点快得多，
/// 且 Chrome/Edge 的正文在深层 Document 控件里，必须用 Descendants 作用域）。
fn selection_from_descendants(
    element: &windows::Win32::UI::Accessibility::IUIAutomationElement,
    max_nodes: usize,
) -> Option<String> {
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{
        TreeScope_Descendants, UIA_IsTextPatternAvailablePropertyId,
    };

    let automation = get_automation().ok()?;
    unsafe {
        // 只要"支持 TextPattern"的控件，Chrome 里通常有几百个，逐个试选区
        let cond = automation
            .CreatePropertyCondition(
                UIA_IsTextPatternAvailablePropertyId,
                &VARIANT::from(true),
            )
            .ok()?;
        let found = element.FindAll(TreeScope_Descendants, &cond).ok()?;
        let n = found.Length().unwrap_or(0).min(max_nodes as i32);
        for i in 0..n {
            if let Ok(e) = found.GetElement(i) {
                if let Some(t) = selection_from_element(&e) {
                    return Some(t);
                }
            }
        }
    }
    None
}

thread_local! {
    static AUTOMATION: std::cell::RefCell<Option<windows::Win32::UI::Accessibility::IUIAutomation>> =
        const { std::cell::RefCell::new(None) };
}

fn get_automation() -> Result<windows::Win32::UI::Accessibility::IUIAutomation, String> {
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
    AUTOMATION.with(|cell| {
        if let Some(a) = cell.borrow().as_ref() {
            return Ok(a.clone());
        }
        let a: IUIAutomation = unsafe {
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("创建 IUIAutomation 失败: {e}"))?
        };
        *cell.borrow_mut() = Some(a.clone());
        Ok(a)
    })
}

/// 通过 UI Automation 读取当前选中文本
/// 策略：焦点元素 → 焦点元素的后代（文档/文本控件）→ 前台窗口的后代
fn capture_via_uia() -> Result<Option<String>, String> {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let need_uninit = hr.is_ok();
        let result = (|| -> Result<Option<String>, String> {
            let automation = get_automation()?;

            // 1) 焦点元素本身
            if let Ok(focused) = automation.GetFocusedElement() {
                if let Some(t) = selection_from_element(&focused) {
                    return Ok(Some(t));
                }
                // 2) 焦点元素的后代（浏览器正文、IDE 编辑区等）
                if let Some(t) = selection_from_descendants(&focused, 400) {
                    return Ok(Some(t));
                }
            }

            // 3) 前台窗口的后代（焦点在工具栏/地址栏等位置时仍能找到正文选区）
            if let Ok(hwnd) = automation.ElementFromHandle(foreground_hwnd()) {
                if let Some(t) = selection_from_descendants(&hwnd, 800) {
                    return Ok(Some(t));
                }
            }
            Ok(None)
        })();
        if need_uninit {
            CoUninitialize();
        }
        result
    }
}

fn foreground_hwnd() -> windows::Win32::Foundation::HWND {
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() }
}

/// 前台窗口是否比本进程权限更高（UIPI 会不会阻断）
/// 返回 Some(true) = 高权限；None = 无法判断
fn foreground_is_elevated() -> Option<bool> {
    unsafe {
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        // 打不开进程句柄，通常就是权限不足（更高完整性级别）
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => {
                let _ = windows::Win32::Foundation::CloseHandle(h);
                Some(false)
            }
            Err(_) => Some(true),
        }
    }
}

/// 兜底：向目标发送 Ctrl+C 并读剪贴板
fn capture_via_clipboard() -> Option<String> {
    unsafe {
        let prev_clip = read_clipboard_text();
        let seq_before = GetClipboardSequenceNumber();

        send_ctrl_c();

        let mut new_text: Option<String> = None;
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(30));
            if GetClipboardSequenceNumber() != seq_before {
                if let Some(t) = read_clipboard_text() {
                    new_text = Some(t);
                    break;
                }
            }
        }

        if let Some(prev) = &prev_clip {
            write_clipboard_text(prev);
        }
        new_text.filter(|t| !t.trim().is_empty())
    }
}

/// 取前台窗口标题（诊断/提示用）
pub fn foreground_window_title() -> String {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return String::new();
        }
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf[..n as usize])
    }
}

/// 向前台应用发送 Ctrl+C
unsafe fn send_ctrl_c() {
    let ctrl_down = key_input(VK_CONTROL, None);
    let c_down = key_input(VK_C, None);
    let c_up = key_input(VK_C, Some(KEYEVENTF_KEYUP));
    let ctrl_up = key_input(VK_CONTROL, Some(KEYEVENTF_KEYUP));

    for input in [ctrl_down, c_down, c_up, ctrl_up] {
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
}

unsafe fn key_input(vk: VIRTUAL_KEY, flags: Option<KEYBD_EVENT_FLAGS>) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags.unwrap_or_default(),
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// 读取剪贴板文本（无文本/无法访问时返回 None）
unsafe fn read_clipboard_text() -> Option<String> {
    OpenClipboard(None).ok()?;

    let result = (|| {
        // 格式不可用（无文本内容）→ None
        if IsClipboardFormatAvailable(CF_UNICODETEXT).is_err() {
            return None;
        }
        let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
        let ptr = GlobalLock(HGLOBAL(handle.0));
        if ptr.is_null() {
            return None;
        }
        // 剪贴板文本为 null 结尾的 UTF-16
        let mut len = 0usize;
        while *(ptr as *const u16).add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr as *const u16, len);
        let text = String::from_utf16_lossy(slice);
        let _ = GlobalUnlock(HGLOBAL(handle.0));
        Some(text)
    })();

    let _ = CloseClipboard();
    result
}

/// 写入剪贴板文本（尽力而为，失败静默）
unsafe fn write_clipboard_text(s: &str) {
    if OpenClipboard(None).is_err() {
        return;
    }

    let result = (|| -> Result<(), String> {
        EmptyClipboard().map_err(|e| e.to_string())?;
        let mut utf16: Vec<u16> = s.encode_utf16().collect();
        utf16.push(0);
        let hmem = GlobalAlloc(GMEM_MOVEABLE, utf16.len() * 2)
            .map_err(|e| format!("GlobalAlloc失败: {e}"))?;
        let dst = GlobalLock(hmem);
        if dst.is_null() {
            return Err("GlobalLock失败".to_string());
        }
        std::ptr::copy_nonoverlapping(
            utf16.as_ptr() as *const u8,
            dst as *mut u8,
            utf16.len() * 2,
        );
        let _ = GlobalUnlock(hmem);
        SetClipboardData(CF_UNICODETEXT, Some(HANDLE(hmem.0)))
            .map_err(|e| format!("SetClipboardData失败: {e}"))?;
        Ok(())
    })();

    let _ = result;
    let _ = CloseClipboard();
}

#[cfg(test)]
mod tests {
    use super::*;


    /// 细粒度诊断：逐层打印 UIA 取文本的每一步，定位卡在哪
    #[ignore]
    #[test]
    fn diag_uia_steps() {
        // 见下方实现
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
        use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, IUIAutomationTextPattern, UIA_TextPatternId};
        use windows::core::Interface;

        println!("[steps] 5 秒内请保持一段文字被选中，并把光标留在该窗口...");
        for i in (1..=5).rev() { println!("  {i}"); std::thread::sleep(std::time::Duration::from_secs(1)); }
        println!("[steps] 前台: {:?}", foreground_window_title());

        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            println!("[steps] CoInitializeEx ok={} hr={:?}", hr.is_ok(), hr);

            let automation: IUIAutomation = match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
                Ok(a) => { println!("[steps] IUIAutomation 创建成功"); a }
                Err(e) => { println!("[steps] 创建失败: {e}"); CoUninitialize(); return; }
            };

            let element = match automation.GetFocusedElement() {
                Ok(e) => { println!("[steps] GetFocusedElement 成功"); e }
                Err(e) => { println!("[steps] GetFocusedElement 失败: {e}"); CoUninitialize(); return; }
            };
            println!("[steps] 控件名称: {:?}", element.CurrentName().map(|b| b.to_string()).ok());
            println!("[steps] 控件类型: {:?}", element.CurrentControlType().map(|c| c.0).ok());

            match element.GetCurrentPattern(UIA_TextPatternId) {
                Ok(_) => println!("[steps] 有 TextPattern"),
                Err(e) => println!("[steps] 无 TextPattern: {e}"),
            }
            let pat = match element.GetCurrentPattern(UIA_TextPatternId).and_then(|p| p.cast::<IUIAutomationTextPattern>()) {
                Ok(p) => { println!("[steps] 转 ITextPattern 成功"); p }
                Err(e) => { println!("[steps] 转 ITextPattern 失败: {e}"); CoUninitialize(); return; }
            };
            match pat.GetSelection() {
                Ok(ranges) => {
                    let n = ranges.Length().unwrap_or(0);
                    println!("[steps] GetSelection 成功，选区数 = {n}");
                    for i in 0..n {
                        if let Ok(r) = ranges.GetElement(i) {
                            match r.GetText(-1) {
                                Ok(b) => println!("[steps]   选区{i} 文本长度={} 内容={:?}", b.to_string().chars().count(), b.to_string().chars().take(60).collect::<String>()),
                                Err(e) => println!("[steps]   选区{i} GetText 失败: {e}"),
                            }
                        }
                    }
                }
                Err(e) => println!("[steps] GetSelection 失败: {e}"),
            }
            CoUninitialize();
        }
    }







    /// 真正的自验证：用 RichEdit 控件（支持 TextPattern）建窗口，全选后经 UIA 读回。
    /// 不依赖前台焦点切换，可在后台进程里确定性地验证整条 UIA 链路。
    #[ignore]
    #[test]
    fn diag_uia_selfcheck_richedit() {
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
        use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, SendMessageW, WS_POPUP, WS_VISIBLE, WM_SETTEXT,
        };
        use windows::core::PCWSTR;

        const EM_SETSEL: u32 = 0x00B1; // 编辑控件"设置选区"消息
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

            // RichEdit 20（msftedit.dll 需先加载）
            let _ = windows::Win32::System::LibraryLoader::LoadLibraryW(windows::core::w!("msftedit.dll"));
            let class: Vec<u16> = "RICHEDIT50W\0".encode_utf16().collect();
            let title: Vec<u16> = "uia-selfcheck\0".encode_utf16().collect();
            let hwnd: HWND = match CreateWindowExW(
                Default::default(),
                PCWSTR(class.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_POPUP | WS_VISIBLE,
                0, 0, 500, 200,
                None, None, None, None,
            ) {
                Ok(h) if !h.is_invalid() => h,
                Ok(_) => { println!("[re] CreateWindowExW 句柄无效"); return; }
                Err(e) => { println!("[re] CreateWindowExW 失败: {e}"); return; }
            };
            println!("[re] RichEdit 窗口 = {hwnd:?}");

            let text: Vec<u16> = "Hello world this is a UI Automation test.\0".encode_utf16().collect();
            SendMessageW(hwnd, WM_SETTEXT, Some(WPARAM(0)), Some(LPARAM(text.as_ptr() as isize)));
            // 全选：EM_SETSEL(0, -1)
            SendMessageW(hwnd, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
            std::thread::sleep(std::time::Duration::from_millis(400));

            let automation: IUIAutomation = match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
                Ok(a) => a, Err(e) => { println!("[re] UIA 创建失败: {e}"); let _ = DestroyWindow(hwnd); return; }
            };
            match automation.ElementFromHandle(hwnd) {
                Ok(el) => {
                    println!("[re] ElementFromHandle OK");
                    match selection_from_element(&el) {
                        Some(t) => println!("[re] ✅ 直接取到: {t:?}"),
                        None => {
                            println!("[re] 焦点元素无选区，试后代...");
                            match selection_from_descendants(&el, 200) {
                                Some(t) => println!("[re] ✅ 后代取到: {t:?}"),
                                None => println!("[re] ❌ 后代也没取到"),
                            }
                        }
                    }
                }
                Err(e) => println!("[re] ElementFromHandle 失败: {e}"),
            }
            let _ = DestroyWindow(hwnd);
            CoUninitialize();
        }
    }

    /// 端到端验证：扫描所有窗口，用完整 capture 逻辑（含后代遍历）找选区
    #[ignore]
    #[test]
    fn diag_capture_all_windows() {
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
        use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, TreeScope_Children};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let automation: IUIAutomation = match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
                Ok(a) => a, Err(e) => { println!("[all] 创建失败: {e}"); return; }
            };
            let root = match automation.GetRootElement() { Ok(r) => r, Err(_) => return };
            let tops = match root.FindAll(TreeScope_Children, &automation.CreateTrueCondition().unwrap()) {
                Ok(t) => t, Err(_) => return
            };
            let n = tops.Length().unwrap_or(0);
            let mut hit = 0;
            for i in 0..n {
                let Ok(win) = tops.GetElement(i) else { continue };
                let name = win.CurrentName().map(|b| b.to_string()).unwrap_or_default();
                if let Some(t) = selection_from_descendants(&win, 800) {
                    hit += 1;
                    println!("[all] ✅ 窗口「{}」选区: {:?}", name.chars().take(30).collect::<String>(), t.chars().take(60).collect::<String>());
                }
            }
            println!("[all] 扫描 {n} 个窗口，{hit} 个有非空选区");
            CoUninitialize();
        }
    }

    /// 验证后代遍历：对浏览器窗口统计 TextPattern 可用性
    #[ignore]
    #[test]
    fn diag_uia_children() {
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
        use windows::Win32::UI::Accessibility::{
            CUIAutomation, IUIAutomation, TreeScope_Children, TreeScope_Descendants,
            UIA_IsTextPatternAvailablePropertyId, UIA_TextPatternId,
        };
        use windows::Win32::System::Variant::VARIANT;
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let automation: IUIAutomation = match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
                Ok(a) => a, Err(e) => { println!("[ch] 创建失败: {e}"); return; }
            };
            let root = match automation.GetRootElement() { Ok(r) => r, Err(_) => return };
            let tops = match root.FindAll(TreeScope_Children, &automation.CreateTrueCondition().unwrap()) {
                Ok(t) => t, Err(_) => return
            };
            let n = tops.Length().unwrap_or(0);
            for i in 0..n {
                let Ok(win) = tops.GetElement(i) else { continue };
                let name = win.CurrentName().map(|b| b.to_string()).unwrap_or_default();
                if !name.contains("DeepSeek") && !name.contains("Microsoft") { continue; }
                println!("[ch] 窗口: {:?}", name.chars().take(30).collect::<String>());
                let all = win.FindAll(TreeScope_Descendants, &automation.CreateTrueCondition().unwrap()).ok();
                println!("[ch]   全部后代数 = {:?}", all.as_ref().map(|a| a.Length().unwrap_or(0)));
                let cond = automation.CreatePropertyCondition(UIA_IsTextPatternAvailablePropertyId, &VARIANT::from(true)).ok();
                if let Some(cond) = cond {
                    match win.FindAll(TreeScope_Descendants, &cond) {
                        Ok(f) => {
                            let m = f.Length().unwrap_or(0);
                            println!("[ch]   TextPattern 可用后代数 = {m}");
                            for k in 0..m.min(5) {
                                if let Ok(e) = f.GetElement(k) {
                                    let nm = e.CurrentName().map(|b| b.to_string()).unwrap_or_default();
                                    let ct = e.CurrentControlType().map(|t| t.0).unwrap_or(-1);
                                    let has = e.GetCurrentPattern(UIA_TextPatternId).is_ok();
                                    println!("[ch]     #{k} {:?} 类型={ct} 有Pattern={has}", nm.chars().take(25).collect::<String>());
                                }
                            }
                        }
                        Err(e) => println!("[ch]   FindAll 失败: {e}"),
                    }
                }
            }
            CoUninitialize();
        }
    }

    /// 枚举当前所有顶层窗口，统计哪些控件支持 TextPattern、其中哪些有非空选区。
    /// 用于确认 UIA 链路本身可用（不依赖前台焦点切换）。
    #[ignore]
    #[test]
    fn diag_uia_scan_windows() {
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
        use windows::Win32::UI::Accessibility::{
            CUIAutomation, IUIAutomation, TreeScope_Children, TreeScope_Descendants, UIA_TextPatternId,
        };
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let automation: IUIAutomation = match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
                Ok(a) => a, Err(e) => { println!("[scan] 创建失败: {e}"); return; }
            };
            let root = match automation.GetRootElement() {
                Ok(r) => r, Err(e) => { println!("[scan] 根元素失败: {e}"); return; }
            };
            let children = match root.FindAll(TreeScope_Children, &automation.CreateTrueCondition().unwrap()) {
                Ok(c) => c, Err(e) => { println!("[scan] 枚举窗口失败: {e}"); return; }
            };
            let n = children.Length().unwrap_or(0);
            println!("[scan] 顶层元素数 = {n}");
            let mut has_text_pattern = 0;
            let mut non_empty = 0;
            for i in 0..n {
                let Ok(win) = children.GetElement(i) else { continue };
                let name = win.CurrentName().map(|b| b.to_string()).unwrap_or_default();
                let ctype = win.CurrentControlType().map(|t| t.0).unwrap_or(-1);
                let cls = win.CurrentClassName().map(|b| b.to_string()).unwrap_or_default();
                println!("[scan] #{i} 名称={:?} 类型={ctype} 类名={:?}", name.chars().take(40).collect::<String>(), cls);
                if win.GetCurrentPattern(UIA_TextPatternId).is_ok() {
                    has_text_pattern += 1;
                    if let Some(t) = selection_from_element(&win) {
                        non_empty += 1;
                        println!("[scan] ✅ 窗口「{name}」有选区: {:?}", t.chars().take(40).collect::<String>());
                    }
                }
                // 后代里找（数量限制，避免过慢）
                if let Some(t) = selection_from_descendants(&win, 300) {
                    non_empty += 1;
                    println!("[scan] ✅ 窗口「{name}」后代有选区: {:?}", t.chars().take(40).collect::<String>());
                }
            }
            println!("[scan] 支持 TextPattern 的顶层元素 = {has_text_pattern}");
            println!("[scan] 检测到非空选区的窗口数 = {non_empty}");
            CoUninitialize();
        }
    }

    /// 自验证：不依赖前台窗口，直接验证 UIA 读取链路本身是否可用。
    /// 做法：用 UIA 从"本进程创建的隐藏窗口"里读，绕开"后台进程无法取得前台焦点"的限制。
    #[ignore]
    #[test]
    fn diag_uia_selfcheck() {
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
        use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, ShowWindow, SW_SHOW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
        };
        use windows::Win32::Foundation::{HWND, WPARAM, LPARAM};
        use windows::core::PCWSTR;

        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let automation: IUIAutomation = match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
                Ok(a) => a,
                Err(e) => { println!("[self] 创建 UIA 失败: {e}"); return; }
            };
            println!("[self] IUIAutomation 创建成功");

            // 用系统 "EDIT" 类创建一个编辑框（EDIT 支持 TextPattern）
            let class: Vec<u16> = "EDIT\0".encode_utf16().collect();
            let title: Vec<u16> = "uia-selfcheck\0".encode_utf16().collect();
            let hwnd: HWND = match CreateWindowExW(
                Default::default(),
                PCWSTR(class.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                100, 100, 400, 120,
                None, None, None, None,
            ) {
                Ok(h) if !h.is_invalid() => h,
                Ok(_) => { println!("[self] CreateWindowExW 返回无效句柄"); return; }
                Err(e) => { println!("[self] CreateWindowExW 失败: {e}"); return; }
            };
            println!("[self] EDIT 窗口句柄 = {hwnd:?}");

            // 写入文本并全选（WM_SETTEXT / EM_SETSEL）
            let text: Vec<u16> = "UI Automation self check text\0".encode_utf16().collect();
            windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::WM_SETTEXT,
                Some(WPARAM(0)),
                Some(LPARAM(text.as_ptr() as isize)),
            );
            windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                hwnd,
                0x00B1, // EM_SETSEL
                Some(WPARAM(0)),
                Some(LPARAM(22)),
            );
            let _ = ShowWindow(hwnd, SW_SHOW);
            std::thread::sleep(std::time::Duration::from_millis(300));

            // 通过 UIA 从该窗口取元素并读选区
            match automation.ElementFromHandle(hwnd) {
                Ok(el) => {
                    println!("[self] ElementFromHandle 成功");
                    match selection_from_element(&el) {
                        Some(t) => println!("[self] ✅ 焦点元素直接取到选区: {t:?}"),
                        None => match selection_from_descendants(&el, 200) {
                            Some(t) => println!("[self] ✅ 后代遍历取到选区: {t:?}"),
                            None => println!("[self] ❌ 两级都没取到选区"),
                        },
                    }
                }
                Err(e) => println!("[self] ElementFromHandle 失败: {e}"),
            }
            let _ = DestroyWindow(hwnd);
            CoUninitialize();
        }
    }

    /// 多级 UIA 诊断：验证"焦点元素 → 后代 → 前台窗口后代"三级策略
    #[ignore]
    #[test]
    fn diag_uia_multi() {
        println!("[multi] 10 秒内请：在任意窗口选中一段文字，然后点一下该窗口标题栏保证它在前台...");
        for i in (1..=10).rev() {
            println!("  倒计时 {i}");
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        println!("[multi] 前台窗口: {:?}", foreground_window_title());
        let r = capture_selection();
        println!("[multi] method = {}", r.method);
        println!("[multi] reason = {:?}", r.reason);
        match &r.text {
            Some(t) => println!("[multi] OK 长度={} 前80字={:?}", t.chars().count(), t.chars().take(80).collect::<String>()),
            None => println!("[multi] 未取到"),
        }
    }

    /// 实链路诊断（需人工配合）：
    ///   1. 打开记事本 / 浏览器 / 终端，随便选中一段文字
    ///   2. 运行：cargo test --release diag_capture_selection -- --ignored --nocapture
    ///   3. 在 5 秒内切回刚才那个窗口（保持选中）
    #[ignore]
    #[test]
    fn diag_capture_selection() {
        println!("[diag] 5 秒内请把焦点切到已选中文字的窗口...");
        for i in (1..=5).rev() {
            println!("  倒计时 {i}");
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        let title = foreground_window_title();
        println!("[diag] 前台窗口: {title:?}");
        let r = capture_selection();
        println!("[diag] method = {}", r.method);
        println!("[diag] reason = {:?}", r.reason);
        match &r.text {
            Some(t) => {
                println!("[diag] 文本长度 = {}", t.chars().count());
                println!("[diag] 文本前 80 字 = {:?}", t.chars().take(80).collect::<String>());
            }
            None => println!("[diag] 未取到文本"),
        }
    }
}
