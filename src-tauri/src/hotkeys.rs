//! Global push-to-talk / hands-free shortcuts via a low-level keyboard hook.
//! Port of GlobalHotkeyService.swift / DictationShortcut.swift. Matched keys are swallowed
//! so the focused app never sees them.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const VK_SPACE: u32 = 0x20;
pub const VK_ESCAPE: u32 = 0x1B;
pub const VK_LSHIFT: u32 = 0xA0;
pub const VK_RSHIFT: u32 = 0xA1;
pub const VK_LCONTROL: u32 = 0xA2;
pub const VK_RCONTROL: u32 = 0xA3;
pub const VK_LMENU: u32 = 0xA4;
pub const VK_RMENU: u32 = 0xA5;
pub const VK_LWIN: u32 = 0x5B;
pub const VK_RWIN: u32 = 0x5C;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shortcut {
    pub vk: u32,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    /// A lone modifier key (e.g. Right Ctrl) held by itself.
    pub modifier_only: bool,
    pub label: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

impl Shortcut {
    pub fn default_push_to_talk() -> Self {
        Self::modifier(VK_RCONTROL)
    }

    pub fn default_hands_free() -> Self {
        Self::combo(VK_SPACE, Mods { ctrl: true, ..Default::default() }, Some(VK_RCONTROL))
    }

    pub fn modifier(vk: u32) -> Self {
        Self {
            vk,
            ctrl: false,
            alt: false,
            shift: false,
            win: false,
            modifier_only: true,
            label: key_name(vk),
        }
    }

    /// `ctrl_side` lets the label say "Right Ctrl" when that is the key that was held.
    pub fn combo(vk: u32, mods: Mods, ctrl_side: Option<u32>) -> Self {
        let mut parts = Vec::new();
        if mods.ctrl {
            parts.push(if ctrl_side == Some(VK_RCONTROL) { "Right Ctrl" } else { "Ctrl" }.to_string());
        }
        if mods.alt {
            parts.push("Alt".into());
        }
        if mods.shift {
            parts.push("Shift".into());
        }
        if mods.win {
            parts.push("Win".into());
        }
        parts.push(key_name(vk));
        Self {
            vk,
            ctrl: mods.ctrl,
            alt: mods.alt,
            shift: mods.shift,
            win: mods.win,
            modifier_only: false,
            label: parts.join(" + "),
        }
    }

    fn mods(&self) -> Mods {
        Mods { ctrl: self.ctrl, alt: self.alt, shift: self.shift, win: self.win }
    }

    pub fn matches(&self, vk: u32, mods: Mods) -> bool {
        self.vk == vk && (self.modifier_only || self.mods() == mods)
    }
}

pub fn is_modifier(vk: u32) -> bool {
    matches!(
        vk,
        0x10 | 0x11 | 0x12 | VK_LSHIFT | VK_RSHIFT | VK_LCONTROL | VK_RCONTROL | VK_LMENU | VK_RMENU | VK_LWIN | VK_RWIN
    )
}

pub fn key_name(vk: u32) -> String {
    let name = match vk {
        VK_LCONTROL => "Left Ctrl",
        VK_RCONTROL => "Right Ctrl",
        VK_LSHIFT => "Left Shift",
        VK_RSHIFT => "Right Shift",
        VK_LMENU => "Left Alt",
        VK_RMENU => "Right Alt",
        VK_LWIN => "Left Win",
        VK_RWIN => "Right Win",
        VK_SPACE => "Space",
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0D => "Enter",
        0x13 => "Pause",
        0x14 => "Caps Lock",
        0x21 => "Page Up",
        0x22 => "Page Down",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "Left",
        0x26 => "Up",
        0x27 => "Right",
        0x28 => "Down",
        0x2C => "Print Screen",
        0x2D => "Insert",
        0x2E => "Delete",
        0x5D => "Menu",
        0x91 => "Scroll Lock",
        0xC0 => "`",
        0xBD => "-",
        0xBB => "=",
        0xDB => "[",
        0xDD => "]",
        0xDC => "\\",
        0xBA => ";",
        0xDE => "'",
        0xBC => ",",
        0xBE => ".",
        0xBF => "/",
        0x30..=0x39 | 0x41..=0x5A => return char::from_u32(vk).unwrap().to_string(),
        0x60..=0x69 => return format!("Num {}", vk - 0x60),
        0x70..=0x87 => return format!("F{}", vk - 0x6F),
        _ => return format!("Key {vk:#04X}"),
    };
    name.to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Start,
    Stop,
    /// Push-to-talk was converted to hands-free (recording continues after release).
    HandsFree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Idle,
    PushToTalk,
    HandsFree,
    /// Hands-free was stopped by the push-to-talk key; swallow its release.
    IgnoreRelease,
}

/// Pure state machine; the hook feeds it every non-injected key event.
pub struct Engine {
    pub push_to_talk: Shortcut,
    pub hands_free: Shortcut,
    mode: Mode,
    down: HashSet<u32>,
    swallowed: HashSet<u32>,
}

pub struct Outcome {
    pub swallow: bool,
    pub action: Option<Action>,
}

impl Engine {
    pub fn new(push_to_talk: Shortcut, hands_free: Shortcut) -> Self {
        Self { push_to_talk, hands_free, mode: Mode::Idle, down: HashSet::new(), swallowed: HashSet::new() }
    }

    pub fn reset(&mut self) {
        self.mode = Mode::Idle;
    }

    fn held_mods(&self, except: u32) -> Mods {
        let held = |a: u32, b: u32| (a != except && self.down.contains(&a)) || (b != except && self.down.contains(&b));
        Mods {
            ctrl: held(VK_LCONTROL, VK_RCONTROL),
            alt: held(VK_LMENU, VK_RMENU),
            shift: held(VK_LSHIFT, VK_RSHIFT),
            win: held(VK_LWIN, VK_RWIN),
        }
    }

    pub fn handle(&mut self, vk: u32, is_down: bool) -> Outcome {
        if is_down {
            let repeat = !self.down.insert(vk);
            let mods = self.held_mods(vk);
            if self.push_to_talk.matches(vk, mods) {
                self.swallowed.insert(vk);
                let action = if repeat {
                    None
                } else {
                    match self.mode {
                        Mode::Idle => {
                            self.mode = Mode::PushToTalk;
                            Some(Action::Start)
                        }
                        Mode::HandsFree => {
                            self.mode = Mode::IgnoreRelease;
                            Some(Action::Stop)
                        }
                        _ => None,
                    }
                };
                return Outcome { swallow: true, action };
            }
            if self.hands_free.matches(vk, mods) {
                self.swallowed.insert(vk);
                let action = if repeat {
                    None
                } else {
                    match self.mode {
                        Mode::PushToTalk => {
                            self.mode = Mode::HandsFree;
                            Some(Action::HandsFree)
                        }
                        Mode::HandsFree => {
                            self.mode = Mode::Idle;
                            Some(Action::Stop)
                        }
                        Mode::Idle => {
                            self.mode = Mode::HandsFree;
                            Some(Action::Start)
                        }
                        Mode::IgnoreRelease => None,
                    }
                };
                return Outcome { swallow: true, action };
            }
            Outcome { swallow: repeat && self.swallowed.contains(&vk), action: None }
        } else {
            self.down.remove(&vk);
            let swallow = self.swallowed.remove(&vk);
            let mut action = None;
            if vk == self.push_to_talk.vk {
                match self.mode {
                    Mode::PushToTalk => {
                        self.mode = Mode::Idle;
                        action = Some(Action::Stop);
                    }
                    Mode::IgnoreRelease => self.mode = Mode::Idle,
                    _ => {}
                }
            }
            Outcome { swallow, action }
        }
    }
}

/// Captures the next key or key combination for the shortcut editor.
#[derive(Default)]
pub struct Recorder {
    mods_down: HashSet<u32>,
    pressed_other: bool,
}

pub enum RecordResult {
    Pending,
    Done(Shortcut),
    Cancelled,
}

impl Recorder {
    pub fn handle(&mut self, vk: u32, is_down: bool) -> RecordResult {
        if is_down {
            if vk == VK_ESCAPE && self.mods_down.is_empty() {
                return RecordResult::Cancelled;
            }
            if is_modifier(vk) {
                self.mods_down.insert(vk);
                return RecordResult::Pending;
            }
            self.pressed_other = true;
            let has = |a, b| self.mods_down.contains(&a) || self.mods_down.contains(&b);
            let mods = Mods {
                ctrl: has(VK_LCONTROL, VK_RCONTROL),
                alt: has(VK_LMENU, VK_RMENU),
                shift: has(VK_LSHIFT, VK_RSHIFT),
                win: has(VK_LWIN, VK_RWIN),
            };
            let side = self.mods_down.contains(&VK_RCONTROL).then_some(VK_RCONTROL);
            RecordResult::Done(Shortcut::combo(vk, mods, side))
        } else {
            let was_down = self.mods_down.remove(&vk);
            if was_down && !self.pressed_other && self.mods_down.is_empty() {
                return RecordResult::Done(Shortcut::modifier(vk));
            }
            RecordResult::Pending
        }
    }
}

pub enum HotkeyMsg {
    Action(Action),
    Recorded(Shortcut),
    RecordCancelled,
}

#[cfg(windows)]
mod hook {
    use super::*;
    use std::sync::mpsc::Sender;
    use std::sync::Mutex;
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, SetWindowsHookExW, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG,
        WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    struct HookState {
        engine: Engine,
        recorder: Option<Recorder>,
        tx: Sender<HotkeyMsg>,
    }

    static STATE: Mutex<Option<HookState>> = Mutex::new(None);

    /// Diagnostics only: KOTEV_ACCEPT_INJECTED=1 lets scripted (SendInput) keys drive the hook.
    fn accept_injected() -> bool {
        static ACCEPT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ACCEPT.get_or_init(|| std::env::var_os("KOTEV_ACCEPT_INJECTED").is_some())
    }

    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
            let message = wparam as u32;
            let is_down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
            let is_up = message == WM_KEYUP || message == WM_SYSKEYUP;
            let injected = kb.flags & LLKHF_INJECTED != 0;
            if (is_down || is_up) && (!injected || accept_injected()) && handle(kb.vkCode, is_down) {
                return 1;
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }

    fn handle(vk: u32, is_down: bool) -> bool {
        let Ok(mut guard) = STATE.lock() else { return false };
        let Some(state) = guard.as_mut() else { return false };
        if let Some(recorder) = state.recorder.as_mut() {
            match recorder.handle(vk, is_down) {
                RecordResult::Pending => {}
                RecordResult::Done(shortcut) => {
                    state.recorder = None;
                    let _ = state.tx.send(HotkeyMsg::Recorded(shortcut));
                }
                RecordResult::Cancelled => {
                    state.recorder = None;
                    let _ = state.tx.send(HotkeyMsg::RecordCancelled);
                }
            }
            return true;
        }
        let outcome = state.engine.handle(vk, is_down);
        if let Some(action) = outcome.action {
            crate::log::write(&format!("hotkey {action:?} (vk {vk:#04X})"));
            let _ = state.tx.send(HotkeyMsg::Action(action));
        }
        outcome.swallow
    }

    pub fn start(push_to_talk: Shortcut, hands_free: Shortcut, tx: Sender<HotkeyMsg>) {
        *STATE.lock().unwrap() = Some(HookState { engine: Engine::new(push_to_talk, hands_free), recorder: None, tx });
        std::thread::Builder::new()
            .name("kotev-keyboard-hook".into())
            .spawn(|| unsafe {
                let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), std::ptr::null_mut(), 0);
                if hook.is_null() {
                    crate::log::write(&format!("failed to install keyboard hook: {}", std::io::Error::last_os_error()));
                    return;
                }
                crate::log::write(&format!("keyboard hook installed (accept injected: {})", accept_injected()));
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {}
            })
            .expect("spawn keyboard hook thread");
    }

    fn with_state(f: impl FnOnce(&mut HookState)) {
        if let Ok(mut guard) = STATE.lock() {
            if let Some(state) = guard.as_mut() {
                f(state);
            }
        }
    }

    pub fn update(push_to_talk: Shortcut, hands_free: Shortcut) {
        with_state(|s| s.engine = Engine::new(push_to_talk, hands_free));
    }

    pub fn reset_mode() {
        with_state(|s| s.engine.reset());
    }

    pub fn set_recording(on: bool) {
        with_state(|s| s.recorder = on.then(Recorder::default));
    }
}

#[cfg(windows)]
pub use hook::{reset_mode, set_recording, start, update};

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Engine {
        Engine::new(Shortcut::default_push_to_talk(), Shortcut::default_hands_free())
    }

    #[test]
    fn push_to_talk_starts_and_stops() {
        let mut e = engine();
        let down = e.handle(VK_RCONTROL, true);
        assert!(down.swallow);
        assert_eq!(down.action, Some(Action::Start));
        assert_eq!(e.handle(VK_RCONTROL, true).action, None, "auto-repeat is ignored");
        let up = e.handle(VK_RCONTROL, false);
        assert!(up.swallow);
        assert_eq!(up.action, Some(Action::Stop));
    }

    #[test]
    fn space_while_holding_locks_hands_free_then_ptt_stops() {
        let mut e = engine();
        assert_eq!(e.handle(VK_RCONTROL, true).action, Some(Action::Start));
        let space = e.handle(VK_SPACE, true);
        assert!(space.swallow);
        assert_eq!(space.action, Some(Action::HandsFree));
        assert!(e.handle(VK_SPACE, false).swallow);
        assert_eq!(e.handle(VK_RCONTROL, false).action, None, "release keeps recording");
        assert_eq!(e.handle(VK_RCONTROL, true).action, Some(Action::Stop));
        assert_eq!(e.handle(VK_RCONTROL, false).action, None);
        assert_eq!(e.handle(VK_RCONTROL, true).action, Some(Action::Start), "back to idle");
    }

    #[test]
    fn other_keys_pass_through() {
        let mut e = engine();
        let a = e.handle(0x41, true);
        assert!(!a.swallow);
        assert!(a.action.is_none());
        assert!(!e.handle(VK_SPACE, true).swallow, "plain Space is not the hands-free combo");
    }

    #[test]
    fn custom_combo_push_to_talk() {
        let ptt = Shortcut::combo(0x44, Mods { ctrl: true, alt: true, ..Default::default() }, None);
        let mut e = Engine::new(ptt, Shortcut::default_hands_free());
        e.handle(VK_LCONTROL, true);
        e.handle(VK_LMENU, true);
        assert_eq!(e.handle(0x44, true).action, Some(Action::Start));
        assert_eq!(e.handle(0x44, false).action, Some(Action::Stop));
    }

    #[test]
    fn recorder_captures_modifier_only_and_combos() {
        let mut r = Recorder::default();
        assert!(matches!(r.handle(VK_RCONTROL, true), RecordResult::Pending));
        match r.handle(VK_RCONTROL, false) {
            RecordResult::Done(s) => assert_eq!(s, Shortcut::default_push_to_talk()),
            _ => panic!("expected modifier-only shortcut"),
        }
        let mut r = Recorder::default();
        r.handle(VK_RCONTROL, true);
        match r.handle(VK_SPACE, true) {
            RecordResult::Done(s) => {
                assert_eq!(s, Shortcut::default_hands_free());
                assert_eq!(s.label, "Right Ctrl + Space");
            }
            _ => panic!("expected combo"),
        }
        let mut r = Recorder::default();
        assert!(matches!(r.handle(VK_ESCAPE, true), RecordResult::Cancelled));
    }
}
