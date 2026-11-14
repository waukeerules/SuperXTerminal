//! Opening a file with the system's own tools: in the app for its type, in the default text editor, or shown in
//! its folder (Finder, File Explorer or Linux's file manager). Each system has its own command for it (`open`,
//! cmd's `start` and Explorer's `/select,`, `xdg-open`), chosen here so the commands that open a log, the
//! sessions file or the notices don't each choose. Not a command the page calls; `commands/files.rs` has those.

// only Windows looks for an editor of its own (Notepad++)
#[cfg(windows)]
use std::path::PathBuf;

/// How [`open_command`] is to open a file
#[cfg_attr(windows, allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Opening {
    /// In the app the system has for it
    InItsApp,
    /// In the default text editor, whatever app its type opens in (the sessions file, the notices)
    AsText,
    /// Its folder shown, with it selected where the system can
    InItsFolder,
}

/// The command that opens `path` as `how` asks, on the Mac and Linux (Windows has its own, in [`open_in_app`] and
/// [`reveal`]). The Mac's `open`, with `-t` for the default text editor and `-R` for Finder with the file selected.
/// Linux's `xdg-open`, as the Linux build had the Mac's `open` and nothing opened. It has no text-editor or select
/// option, so a text file opens in the app for its type (a text editor, for .txt and .xml), and for InItsFolder the
/// folder it's in is opened.
#[cfg(not(windows))]
pub(crate) fn open_command(path: &std::path::Path, how: Opening) -> std::process::Command {
    if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        match how {
            Opening::InItsApp => {}
            Opening::AsText => {
                command.arg("-t");
            }
            Opening::InItsFolder => {
                command.arg("-R");
            }
        }
        command.arg(path);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        // the system's environment, not an AppImage's, or xdg-open and the app it opens fail on its libraries
        crate::appimage::tidy(&mut command);
        match (how, path.parent()) {
            (Opening::InItsFolder, Some(folder)) if !folder.as_os_str().is_empty() => command.arg(folder),
            _ => command.arg(path),
        };
        command
    }
}

/// Open a file or folder in the app the system uses for it: Explorer's "start" on Windows, else [`open_command`]
pub(crate) fn open_in_app(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        /// Start cmd without a console window flashing up
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // start's first quoted argument is a window title, hence the empty one
        std::process::Command::new("cmd").args(["/c", "start", ""]).arg(path).creation_flags(CREATE_NO_WINDOW).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        open_command(path, Opening::InItsApp).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

/// Notepad++, where its installer puts it, for editing the sessions file on Windows
#[cfg(windows)]
pub(crate) fn notepad_plus_plus() -> Option<PathBuf> {
    ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
        .iter()
        .filter_map(|var| std::env::var_os(var))
        .map(|root| PathBuf::from(root).join("Notepad++").join("notepad++.exe"))
        .chain(std::env::var_os("LOCALAPPDATA").map(|root| PathBuf::from(root).join("Programs").join("Notepad++").join("notepad++.exe")))
        .find(|p| p.exists())
}

/// Show a file in its folder: selected in Finder on the Mac (`open -R`) and Explorer on Windows (`/select,`); on
/// Linux the folder alone, in the file manager ([`open_command`])
pub(crate) fn reveal(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let mut select = std::ffi::OsString::from("/select,");
        select.push(path.as_os_str());
        std::process::Command::new("explorer").arg(select).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        open_command(path, Opening::InItsFolder).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    /// What [`super::open_command`] would run: the program and its arguments
    #[cfg(not(windows))]
    fn opening(path: &str, how: super::Opening) -> Vec<String> {
        let command = super::open_command(std::path::Path::new(path), how);
        std::iter::once(command.get_program()).chain(command.get_args()).map(|a| a.to_string_lossy().into_owned()).collect()
    }

    /// The Linux build opened files with the Mac's `open`, which Linux hasn't got, so Help > Third-Party Notices,
    /// Edit Sessions Externally, Open log and showing a file in its folder did nothing there. Not on Windows, which
    /// opens files its own way (cmd's start, Explorer's /select,).
    #[cfg(not(windows))]
    #[test]
    fn files_open_with_the_systems_own_opener() {
        use super::Opening::*;
        let file = "/home/me/SuperXTerminal/Sessions.xml";
        if cfg!(target_os = "macos") {
            assert_eq!(opening(file, InItsApp), ["open", file]);
            assert_eq!(opening(file, AsText), ["open", "-t", file]);
            assert_eq!(opening(file, InItsFolder), ["open", "-R", file]);
        } else {
            assert_eq!(opening(file, InItsApp), ["xdg-open", file]);
            assert_eq!(opening(file, AsText), ["xdg-open", file]);
            // xdg-open can't select a file, so its folder is opened
            assert_eq!(opening(file, InItsFolder), ["xdg-open", "/home/me/SuperXTerminal"]);
            assert_eq!(opening("Sessions.xml", InItsFolder), ["xdg-open", "Sessions.xml"]);
        }
    }
}
