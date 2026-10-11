//! Inserts text into the focused application. Port of TextInsertionService.swift:
//! snapshot the clipboard, put the text on it, send the paste shortcut, and restore the
//! previous clipboard unless the user copied something else in the meantime.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteKeys {
    CtrlV,
    CtrlShiftV,
    ShiftInsert,
}

/// Terminals that don't paste with Ctrl+V (it is sent to the shell as a control character).
pub fn paste_keys_for(process_name: &str) -> PasteKeys {
    match process_name.to_ascii_lowercase().as_str() {
        "windowsterminal.exe" | "alacritty.exe" | "wezterm-gui.exe" | "tabby.exe" | "hyper.exe" | "warp.exe" => {
            PasteKeys::CtrlShiftV
        }
        "mintty.exe" | "putty.exe" | "kitty.exe" => PasteKeys::ShiftInsert,
        _ => PasteKeys::CtrlV,
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{CloseHandle, HWND};
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardSequenceNumber,
        OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
    use windows_sys::Win32::System::Ole::{
        CF_BITMAP, CF_DSPBITMAP, CF_DSPENHMETAFILE, CF_DSPMETAFILEPICT, CF_ENHMETAFILE, CF_METAFILEPICT,
        CF_OWNERDISPLAY, CF_PALETTE, CF_UNICODETEXT,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
        KEYEVENTF_UNICODE, VK_CONTROL, VK_INSERT, VK_LWIN, VK_MENU, VK_RETURN, VK_RWIN, VK_SHIFT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    const RESTORE_DELAY: Duration = Duration::from_millis(550);
    const VK_V: u16 = 0x56;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    struct Clipboard;

    impl Clipboard {
        fn open() -> Result<Self, String> {
            for _ in 0..10 {
                if unsafe { OpenClipboard(std::ptr::null_mut() as HWND) } != 0 {
                    return Ok(Clipboard);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err("the clipboard is in use by another application".into())
        }
    }

    impl Drop for Clipboard {
        fn drop(&mut self) {
            unsafe { CloseClipboard() };
        }
    }

    type Snapshot = Vec<(u32, Vec<u8>)>;

    fn is_handle_format(format: u32) -> bool {
        [
            CF_BITMAP, CF_METAFILEPICT, CF_PALETTE, CF_ENHMETAFILE, CF_OWNERDISPLAY, CF_DSPBITMAP,
            CF_DSPMETAFILEPICT, CF_DSPENHMETAFILE,
        ]
        .iter()
        .any(|f| *f as u32 == format)
    }

    fn snapshot(_cb: &Clipboard) -> Snapshot {
        let mut items = Vec::new();
        let mut format = 0;
        loop {
            format = unsafe { EnumClipboardFormats(format) };
            if format == 0 {
                break;
            }
            if is_handle_format(format) {
                continue;
            }
            unsafe {
                let handle = GetClipboardData(format);
                if handle.is_null() {
                    continue;
                }
                let ptr = GlobalLock(handle) as *const u8;
                if ptr.is_null() {
                    continue;
                }
                let size = GlobalSize(handle);
                items.push((format, std::slice::from_raw_parts(ptr, size).to_vec()));
                GlobalUnlock(handle);
            }
        }
        items
    }

    fn set_bytes(format: u32, bytes: &[u8]) {
        unsafe {
            let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
            if handle.is_null() {
                return;
            }
            let ptr = GlobalLock(handle) as *mut u8;
            if ptr.is_null() {
                return;
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
            GlobalUnlock(handle);
            SetClipboardData(format, handle);
        }
    }

    fn set_text(_cb: &Clipboard, text: &str) {
        unsafe { EmptyClipboard() };
        let utf16: Vec<u8> = wide(text).iter().flat_map(|u| u.to_le_bytes()).collect();
        set_bytes(CF_UNICODETEXT as u32, &utf16);
        // Keep dictations out of Win+V clipboard history and cloud clipboard.
        unsafe {
            let exclude = RegisterClipboardFormatW(wide("ExcludeClipboardContentFromMonitorProcessing").as_ptr());
            if exclude != 0 {
                set_bytes(exclude, &[0]);
            }
            let history = RegisterClipboardFormatW(wide("CanIncludeInClipboardHistory").as_ptr());
            if history != 0 {
                set_bytes(history, &0u32.to_le_bytes());
            }
        }
    }

    fn restore(_cb: &Clipboard, items: &Snapshot) {
        unsafe { EmptyClipboard() };
        for (format, bytes) in items {
            set_bytes(*format, bytes);
        }
    }

    fn key(vk: u16, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn unicode(unit: u16, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE | if up { KEYEVENTF_KEYUP } else { 0 },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn send(inputs: &[INPUT]) {
        unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
    }

    /// Release modifiers the user may still be holding so they don't combine with our keys.
    fn release_held_modifiers() {
        let held: Vec<INPUT> = [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
            .into_iter()
            .filter(|vk| unsafe { GetAsyncKeyState(*vk as i32) } as u16 & 0x8000 != 0)
            .map(|vk| key(vk, true))
            .collect();
        if !held.is_empty() {
            send(&held);
        }
    }

    pub fn foreground_process_name() -> String {
        unsafe {
            let hwnd = GetForegroundWindow();
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == 0 {
                return String::new();
            }
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return String::new();
            }
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
            CloseHandle(process);
            if ok == 0 {
                return String::new();
            }
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            path.rsplit(['\\', '/']).next().unwrap_or_default().to_string()
        }
    }

    fn send_paste(keys: PasteKeys) {
        let (modifiers, main): (&[u16], u16) = match keys {
            PasteKeys::CtrlV => (&[VK_CONTROL], VK_V),
            PasteKeys::CtrlShiftV => (&[VK_CONTROL, VK_SHIFT], VK_V),
            PasteKeys::ShiftInsert => (&[VK_SHIFT], VK_INSERT),
        };
        let mut inputs: Vec<INPUT> = modifiers.iter().map(|vk| key(*vk, false)).collect();
        inputs.push(key(main, false));
        inputs.push(key(main, true));
        inputs.extend(modifiers.iter().rev().map(|vk| key(*vk, true)));
        send(&inputs);
    }

    pub fn paste(text: &str) -> Result<(), String> {
        let process = foreground_process_name();
        let keys = paste_keys_for(&process);
        crate::log::write(&format!("inserting {} chars into {process} via {keys:?}", text.chars().count()));

        let saved = {
            let cb = Clipboard::open()?;
            let saved = snapshot(&cb);
            set_text(&cb, text);
            saved
        };
        let sequence = unsafe { GetClipboardSequenceNumber() };
        release_held_modifiers();
        send_paste(keys);

        std::thread::spawn(move || {
            std::thread::sleep(RESTORE_DELAY);
            // Don't overwrite something the user copied after the insertion.
            if unsafe { GetClipboardSequenceNumber() } != sequence || saved.is_empty() {
                return;
            }
            if let Ok(cb) = Clipboard::open() {
                restore(&cb, &saved);
            }
        });
        Ok(())
    }

    pub fn type_text(text: &str) -> Result<(), String> {
        release_held_modifiers();
        let mut inputs = Vec::new();
        for ch in text.chars() {
            if ch == '\n' {
                inputs.push(key(VK_RETURN, false));
                inputs.push(key(VK_RETURN, true));
                continue;
            }
            let mut units = [0u16; 2];
            for unit in ch.encode_utf16(&mut units) {
                inputs.push(unicode(*unit, false));
                inputs.push(unicode(*unit, true));
            }
        }
        send(&inputs);
        Ok(())
    }
}

#[cfg(windows)]
pub use win::{foreground_process_name, paste, type_text};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminals_get_their_paste_shortcut() {
        assert_eq!(paste_keys_for("WindowsTerminal.exe"), PasteKeys::CtrlShiftV);
        assert_eq!(paste_keys_for("mintty.exe"), PasteKeys::ShiftInsert);
        assert_eq!(paste_keys_for("Code.exe"), PasteKeys::CtrlV);
        assert_eq!(paste_keys_for("conhost.exe"), PasteKeys::CtrlV);
    }
}
