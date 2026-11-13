//! Windows key combinations in a Remote Desktop tab, on Windows: Win+R, Win+E and the Windows key itself go to the
//! remote computer and not to this one, as Remote Desktop Connection's "Apply Windows key combinations: On the
//! remote computer" (the session's Windows key combinations option).
//!
//! Windows acts on the Windows key before any program's window hears of it (Win+R opens Run here, and the tab's page
//! only ever sees the Windows key go down and up, which opened Start on both computers). The one way in front of
//! that is a low-level keyboard hook (WH_KEYBOARD_LL), which Remote Desktop Connection uses too. The tab asks for it
//! while it has the keyboard and its option sends the keys there (`rdp_grab_keys`, from RdpHost.ts), and gives it up
//! when it hasn't. While it's held, the Windows key and every key pressed with it are kept from Windows and sent to
//! the session as scancodes; every other key is left alone and reaches the tab's page as before.
//!
//! The hook is only in place while a tab holds it, and only acts while one of SuperXTerminal's windows is in front,
//! so a tab that never said it lost the keyboard can't take keys from another program. Windows keeps Win+L (lock)
//! and Ctrl+Alt+Delete for itself whatever a hook does; the tab's menu sends those.
//!
//! The hook only hears keys because the app is built without the window toolkit's raw device events (`run` in
//! lib.rs, `device_event_filter`): Windows doesn't call the keyboard hook of a program registered for raw input, and
//! says nothing about it. No test covers that, as it takes the real app: a hook that goes in and never takes a key
//! (Start opening on both computers) is the sign of it.
//!
//! [`Taken`], which decides what happens to each key, is plain logic built and tested on every system; the hook
//! itself exists on Windows alone. On the Mac ⌘ is the Windows key, sorted out in the page and `command_tap.rs`.

use ironrdp_client::rdp::RdpInputEvent;
use tokio::sync::mpsc::UnboundedSender;

/// A key for the remote desktop: its PC scancode, whether it's an extended (E0) key, and pressed or let go
pub type Key = (u8, bool, bool);

/// The left Windows key's virtual key code (VK_LWIN); its scancode is the same number, extended
const LEFT_WINDOWS: u32 = 0x5b;
/// The right Windows key's (VK_RWIN)
const RIGHT_WINDOWS: u32 = 0x5c;

/// The keys kept from this computer and pressed in the remote desktop instead: the Windows keys held, and the keys
/// pressed while one was. A key taken going down is taken going up too, whenever that is, so none sticks there.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
#[derive(Debug, Default)]
pub struct Taken {
    /// The Windows keys held (their scancodes)
    windows: Vec<u8>,
    /// The other keys pressed while a Windows key was held: scancode, and whether extended
    others: Vec<(u8, bool)>,
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
impl Taken {
    /// A key went down or up on this computer's keyboard (`vk` its virtual key code, `scancode` and `extended` as the
    /// keyboard hook gives them). Some: it's kept from Windows and this goes to the remote desktop. None: it's left
    /// to Windows and the tab's page (a key with no Windows key held, or one with no scancode, as a media key).
    pub fn key(&mut self, vk: u32, scancode: u32, extended: bool, down: bool) -> Option<Key> {
        if vk == LEFT_WINDOWS || vk == RIGHT_WINDOWS {
            let code = if vk == LEFT_WINDOWS { 0x5b } else { 0x5c };
            let held = self.windows.contains(&code);
            if down {
                if !held {
                    self.windows.push(code);
                }
                return Some((code, true, true));
            }
            // let go without having gone down here (held as the tab got the keyboard): Windows has it
            self.windows.retain(|c| *c != code);
            return held.then_some((code, true, false));
        }
        let key = (u8::try_from(scancode).ok().filter(|code| *code != 0)?, extended);
        let held = self.others.contains(&key);
        if down {
            if self.windows.is_empty() {
                return None;
            }
            if !held {
                self.others.push(key);
            }
            return Some((key.0, key.1, true));
        }
        self.others.retain(|k| *k != key);
        held.then_some((key.0, key.1, false))
    }

    /// Let go of everything held (the tab lost the keyboard, or another program came in front): what to send so no
    /// key stays down in the remote desktop, the Windows keys last
    pub fn release(&mut self) -> Vec<Key> {
        let others = self.others.drain(..).map(|(code, extended)| (code, extended, false));
        let keys: Vec<Key> = others.chain(self.windows.iter().map(|code| (*code, true, false))).collect();
        self.windows.clear();
        keys
    }
}

/// Send the Windows key and what's pressed with it to this session from now on: its tab has the keyboard
#[cfg(windows)]
pub fn grab(session: u32, input: UnboundedSender<RdpInputEvent>) {
    hook::grab(session, input);
}

/// Stop, if it's this session that has them: its tab lost the keyboard, or it ended
#[cfg(windows)]
pub fn let_go(session: u32) {
    hook::let_go(session);
}

/// Elsewhere there's no Windows key to take: the Mac's ⌘ is sorted out in the tab's page
#[cfg(not(windows))]
pub fn grab(_session: u32, _input: UnboundedSender<RdpInputEvent>) {}

/// Elsewhere nothing was taken
#[cfg(not(windows))]
pub fn let_go(_session: u32) {}

/// The keyboard hook itself: a thread of its own with a message loop, which Windows calls the hook on
#[cfg(windows)]
mod hook {
    use std::sync::{Mutex, OnceLock};

    use ironrdp_client::rdp::RdpInputEvent;
    use smallvec::smallvec;
    use tokio::sync::mpsc::UnboundedSender;
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
        UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, PM_NOREMOVE, WH_KEYBOARD_LL, WM_APP, WM_USER,
    };

    use super::{Key, Taken};

    /// The hook's "act on this" code (HC_ACTION); any other is passed on untouched
    const ACTION: i32 = 0;
    /// A key event's flags (KBDLLHOOKSTRUCT): an extended key, and a key let go
    const EXTENDED: u32 = 0x01;
    /// See [`EXTENDED`]
    const UP: u32 = 0x80;
    /// The message that tells the hook's thread to look again at whether the hook is wanted
    const LOOK_AGAIN: u32 = WM_APP;

    /// The session whose tab has the keyboard, where its keys go, and what's held there
    struct Grab {
        session: u32,
        input: UnboundedSender<RdpInputEvent>,
        taken: Taken,
    }

    /// The tab that has the Windows key now, if any; read by the hook for every key
    static GRAB: Mutex<Option<Grab>> = Mutex::new(None);
    /// The hook's thread (its id, to post to), started with the first grab; 0 when it couldn't be started
    static THREAD: OnceLock<u32> = OnceLock::new();
    /// For the test of the hook itself, which has no window to be in front: act whatever window is
    #[cfg(test)]
    pub static ANY_WINDOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    /// For the test of the hook itself: have a Windows key count as held, so the next key is taken without the test
    /// pressing the real Windows key on the desktop it runs on
    #[cfg(test)]
    pub fn hold_windows_key() {
        if let Some(grab) = state().as_mut() {
            grab.taken.key(super::LEFT_WINDOWS, 0x5b, true, true);
        }
    }

    /// The grab, whatever a panic elsewhere left of it: the hook must never stop answering Windows
    fn state() -> std::sync::MutexGuard<'static, Option<Grab>> {
        GRAB.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Send keys to a session; false when it has ended
    fn send(input: &UnboundedSender<RdpInputEvent>, keys: &[Key]) -> bool {
        keys.iter().all(|(scancode, extended, down)| {
            input.send(RdpInputEvent::FastPath(smallvec![crate::rdp::key_event(*scancode, *extended, *down)])).is_ok()
        })
    }

    /// See [`super::grab`]: keys held for a tab that had them before are let go there first
    pub fn grab(session: u32, input: UnboundedSender<RdpInputEvent>) {
        if let Some(mut before) = state().replace(Grab { session, input, taken: Taken::default() }) {
            send(&before.input, &before.taken.release());
        }
        look_again();
    }

    /// See [`super::let_go`]
    pub fn let_go(session: u32) {
        let mut grab = state();
        if let Some(mut gone) = grab.take_if(|g| g.session == session) {
            send(&gone.input, &gone.taken.release());
        }
        drop(grab);
        look_again();
    }

    /// Tell the hook's thread (started now, the first time) that what's wanted changed
    fn look_again() {
        let thread = *THREAD.get_or_init(|| {
            let (ready, started) = std::sync::mpsc::channel();
            let spawned = std::thread::Builder::new().name("rdp-keys".into()).spawn(move || run(ready));
            match spawned {
                Ok(_) => started.recv().unwrap_or(0),
                Err(error) => {
                    crate::applog::warn(format!("Remote Desktop's Windows keys: the keyboard thread couldn't start ({error})"));
                    0
                }
            }
        });
        if thread != 0 {
            // SAFETY: posts a message with no pointers in it to a thread's queue; a thread that's gone just fails it
            unsafe { PostThreadMessageW(thread, LOOK_AGAIN, 0, 0) };
        }
    }

    /// The hook's thread: puts the hook in place while a tab has the Windows key and takes it out when none has, so
    /// SuperXTerminal isn't in the way of every key press while no Remote Desktop is being typed into. Windows calls
    /// the hook on this thread, from its message loop.
    fn run(ready: std::sync::mpsc::Sender<u32>) {
        // SAFETY: a zeroed MSG is a valid one for the calls below to fill in
        let mut message: MSG = unsafe { std::mem::zeroed() };
        // SAFETY: looking at the queue (which makes Windows create it, so messages can be posted to this thread);
        // `message` is valid to write to
        unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), WM_USER, WM_USER, PM_NOREMOVE) };
        // SAFETY: takes nothing and returns this thread's id
        let _ = ready.send(unsafe { GetCurrentThreadId() });
        let mut hook: HHOOK = std::ptr::null_mut();
        // SAFETY: `message` is valid to write to; the loop ends if the queue fails (-1) or is told to quit (0)
        while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
            if message.message != LOOK_AGAIN {
                continue;
            }
            let wanted = state().is_some();
            if wanted && hook.is_null() {
                // SAFETY: `on_key` is a hook procedure that lives as long as the program, in this module (the
                // program's own, which a low-level hook may name); thread 0 is every thread, as these hooks must be
                hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(on_key), GetModuleHandleW(std::ptr::null()), 0) };
                if hook.is_null() {
                    crate::applog::warn("Remote Desktop's Windows keys: Windows didn't let the keyboard hook in, so they stay on this computer");
                }
            } else if !wanted && !hook.is_null() {
                // SAFETY: `hook` came from SetWindowsHookExW on this thread and hasn't been taken out yet
                unsafe { UnhookWindowsHookEx(hook) };
                hook = std::ptr::null_mut();
            }
        }
    }

    /// Whether one of SuperXTerminal's own windows is the one in front
    fn in_front() -> bool {
        #[cfg(test)]
        if ANY_WINDOW.load(std::sync::atomic::Ordering::Relaxed) {
            return true;
        }
        let mut process = 0u32;
        // SAFETY: the window in front (or null, which the next call answers with 0), and a number valid to write to
        unsafe { GetWindowThreadProcessId(GetForegroundWindow(), &mut process) };
        process == std::process::id()
    }

    /// A key on this computer's keyboard: whether it's kept from Windows (it went to the remote desktop instead)
    fn take(key: &KBDLLHOOKSTRUCT) -> bool {
        let mut grab = state();
        let Some(held) = grab.as_mut() else { return false };
        // another program came in front without the tab hearing of it: its keys are its own
        if !in_front() {
            send(&held.input, &held.taken.release());
            return false;
        }
        let Some(taken) = held.taken.key(key.vkCode, key.scanCode, key.flags & EXTENDED != 0, key.flags & UP == 0) else { return false };
        if !send(&held.input, &[taken]) {
            // the session ended without letting go
            *grab = None;
            return false;
        }
        true
    }

    /// The hook: Windows calls it for every key before acting on it. 1 keeps the key from Windows and every other
    /// program; anything else is passed along.
    unsafe extern "system" fn on_key(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        // SAFETY: with HC_ACTION, `lparam` points to the key's KBDLLHOOKSTRUCT for the length of this call
        if code == ACTION && take(unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) }) {
            return 1;
        }
        // SAFETY: passes on exactly what Windows gave; the first argument is ignored
        unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
    }
}

// What happens to each key while a Remote Desktop tab has the Windows key, decided by `Taken` as the keyboard hook
// asks it: the keys are given as the hook gets them (virtual key, scancode, extended, down), and what comes back is
// what goes to the remote desktop, or None for a key left to this computer. The hook itself needs Windows and a real
// keyboard, so it isn't driven here; this logic is the same on every system.
#[cfg(test)]
mod tests {
    use super::*;

    /// R's virtual key code and scancode, and E's
    const R: (u32, u32) = (0x52, 0x13);
    /// See [`R`]
    const E: (u32, u32) = (0x45, 0x12);
    /// Left Shift's
    const SHIFT: (u32, u32) = (0xa0, 0x2a);

    #[test]
    fn windows_and_r_go_to_the_remote_desktop_and_not_to_this_computer() {
        let mut taken = Taken::default();
        // the hook gives the Windows key's scancode too, but it's the key code that says which it is
        assert_eq!(taken.key(LEFT_WINDOWS, 0x5b, true, true), Some((0x5b, true, true)));
        assert_eq!(taken.key(R.0, R.1, false, true), Some((0x13, false, true)));
        assert_eq!(taken.key(R.0, R.1, false, false), Some((0x13, false, false)));
        assert_eq!(taken.key(LEFT_WINDOWS, 0x5b, true, false), Some((0x5b, true, false)));
        // and Windows+E after it, with the right Windows key
        assert_eq!(taken.key(RIGHT_WINDOWS, 0x5c, true, true), Some((0x5c, true, true)));
        assert_eq!(taken.key(E.0, E.1, false, true), Some((0x12, false, true)));
    }

    #[test]
    fn the_windows_key_alone_is_start_in_the_remote_desktop() {
        let mut taken = Taken::default();
        assert_eq!(taken.key(LEFT_WINDOWS, 0x5b, true, true), Some((0x5b, true, true)));
        assert_eq!(taken.key(LEFT_WINDOWS, 0x5b, true, false), Some((0x5b, true, false)));
        assert!(taken.release().is_empty(), "nothing left held");
    }

    #[test]
    fn a_key_without_the_windows_key_is_left_to_the_tab() {
        let mut taken = Taken::default();
        assert_eq!(taken.key(R.0, R.1, false, true), None);
        assert_eq!(taken.key(R.0, R.1, false, false), None);
        // after a Windows key combination too
        taken.key(LEFT_WINDOWS, 0x5b, true, true);
        taken.key(LEFT_WINDOWS, 0x5b, true, false);
        assert_eq!(taken.key(E.0, E.1, false, true), None);
        // a Windows key let go that never went down here (held as the tab got the keyboard) is Windows' own
        assert_eq!(taken.key(RIGHT_WINDOWS, 0x5c, true, false), None);
        // and a key with no scancode (a media key) can't be sent, so it isn't taken
        taken.key(LEFT_WINDOWS, 0x5b, true, true);
        assert_eq!(taken.key(0xb3, 0, false, true), None);
    }

    #[test]
    fn a_key_let_go_after_the_windows_key_is_still_let_go_in_the_remote_desktop() {
        let mut taken = Taken::default();
        // Windows+Shift+S, Windows let go first
        taken.key(LEFT_WINDOWS, 0x5b, true, true);
        assert_eq!(taken.key(SHIFT.0, SHIFT.1, false, true), Some((0x2a, false, true)));
        assert_eq!(taken.key(LEFT_WINDOWS, 0x5b, true, false), Some((0x5b, true, false)));
        assert_eq!(taken.key(SHIFT.0, SHIFT.1, false, false), Some((0x2a, false, false)));
        // once up, Shift is the tab's again
        assert_eq!(taken.key(SHIFT.0, SHIFT.1, false, true), None);
    }

    /// The hook itself, on Windows alone (there's no such hook elsewhere), against the real keyboard queue: F24, a
    /// key no keyboard has and no program uses, is pressed by the test as a keyboard would, with a Windows key
    /// counted as held; it must come out of the session's input and not reach Windows. Not run with the others: it
    /// needs a desktop to hook (a build machine has none), and types a key into the one it runs on.
    #[cfg(windows)]
    #[test]
    #[ignore = "presses a key on the desktop it runs on; run by hand: cargo test -- --ignored the_keyboard_hook"]
    fn the_keyboard_hook_takes_a_key_from_windows_and_sends_it_to_the_session() {
        use ironrdp_pdu::input::fast_path::{FastPathInputEvent, KeyboardFlags};
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP};
        /// F24's virtual key code and scancode
        const F24: (u8, u8) = (0x87, 0x76);
        let (input, mut sent) = tokio::sync::mpsc::unbounded_channel();
        hook::ANY_WINDOW.store(true, std::sync::atomic::Ordering::Relaxed);
        grab(7, input);
        hook::hold_windows_key();
        // the hook goes in on its own thread
        std::thread::sleep(std::time::Duration::from_millis(300));
        // SAFETY: presses and lets go of a key as a keyboard would; nothing is pointed to
        unsafe {
            keybd_event(F24.0, F24.1, 0, 0);
            keybd_event(F24.0, F24.1, KEYEVENTF_KEYUP, 0);
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
        let_go(7);
        let mut keys = Vec::new();
        while let Ok(RdpInputEvent::FastPath(events)) = sent.try_recv() {
            for event in events {
                if let FastPathInputEvent::KeyboardEvent(flags, scancode) = event {
                    keys.push((scancode, !flags.contains(KeyboardFlags::RELEASE)));
                }
            }
        }
        // F24 down and up, then the Windows key let go as the tab gives the keyboard back
        assert_eq!(keys, [(0x76, true), (0x76, false), (0x5b, false)]);
    }

    #[test]
    fn keys_held_as_the_tab_loses_the_keyboard_are_let_go_in_the_remote_desktop() {
        let mut taken = Taken::default();
        taken.key(LEFT_WINDOWS, 0x5b, true, true);
        taken.key(R.0, R.1, false, true);
        // a key held down repeats: still one key to let go
        taken.key(R.0, R.1, false, true);
        assert_eq!(taken.release(), [(0x13, false, false), (0x5b, true, false)]);
        assert!(taken.release().is_empty());
        // their real key-ups, coming later, are no longer the remote desktop's
        assert_eq!(taken.key(R.0, R.1, false, false), None);
        assert_eq!(taken.key(LEFT_WINDOWS, 0x5b, true, false), None);
    }
}
