//! 划词翻译：捕获其他应用中当前选中的文本
//! 原理：保存剪贴板 → 向前台应用发送 Ctrl+C → 读取剪贴板 → 还原剪贴板
//! 限制：不支持禁用复制功能的应用（部分PDF阅读器/终端等），此类场景无文本返回

use windows::Win32::Foundation::{HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL,
};

/// 标准剪贴板文本格式（CF_UNICODETEXT = 13）
const CF_UNICODETEXT: u32 = 13;
/// 字符 'C' 的虚拟键码
const VK_C: VIRTUAL_KEY = VIRTUAL_KEY(0x43);

/// 捕获前台应用当前选中的文本
/// 返回 None 表示没有新选中文本（未选中、目标应用不支持复制等）
pub fn capture_selected_text() -> Option<String> {
    unsafe {
        let prev_clip = read_clipboard_text();
        let seq_before = GetClipboardSequenceNumber();

        send_ctrl_c();

        // 轮询等待目标应用完成复制（剪贴板序列号变化=有新内容写入）
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

        // 还原用户剪贴板（尽力而为：仅能还原文本类内容）
        if let Some(prev) = &prev_clip {
            write_clipboard_text(prev);
        }

        // 序列号未变化 = 复制未发生 = 无选中文本
        new_text.filter(|t| !t.trim().is_empty())
    }
}

/// 向前台应用发送 Ctrl+C
unsafe fn send_ctrl_c() {
    let ctrl_down = key_input(VK_CONTROL, None);
    let c_down = key_input(VK_C, None);
    let c_up = key_input(VK_C, Some(KEYEVENTF_KEYUP));
    let ctrl_up = key_input(VK_CONTROL, Some(KEYEVENTF_KEYUP));

    // 分步发送并留出间隔，兼容对按键时序敏感的应用
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
