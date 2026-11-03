//! Ctrl+C for local shells on Windows.
//!
//! A Windows pseudo console turns a typed 0x03 into the console's Ctrl+C signal, which stops a running command in
//! cmd or PowerShell (`ping -t`), unless the program ignores Ctrl+C. That "ignore" flag is inherited: a process
//! started with it (by some terminals and launchers, or in a new process group) passes it to every program it starts,
//! so the shells in its tabs would never stop on Ctrl+C. [`allow_ctrl_c`] clears it once, as the app starts, before
//! any shell. On the Mac 0x03 is the signal already (the pty's line discipline), so it does nothing there.

/// Clear the inherited "ignore Ctrl+C" flag, so local shells started from now on stop on Ctrl+C. Never sets it.
#[cfg(windows)]
pub fn allow_ctrl_c() {
    use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

    if unsafe { SetConsoleCtrlHandler(None, 0) } == 0 {
        crate::applog::warn(format!("Couldn't clear the inherited Ctrl+C flag: {}", std::io::Error::last_os_error()));
    }
}

/// Nothing to do on the Mac: 0x03 typed into a pty is the signal
#[cfg(not(windows))]
pub fn allow_ctrl_c() {}
