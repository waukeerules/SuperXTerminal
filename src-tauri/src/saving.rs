//! Saving a file whole: written beside it first, then put in its place, so a save cut short never leaves half a file
//! (Sessions.xml and PrivateSessions.xml, Folders.xml, ButtonBars.xml, layouts, saved scripts, the outbox).
//!
//! On Windows, putting the new file in place of the old fails with "Access is denied. (os error 5)" (or a sharing
//! violation) while another program has the old one open: OneDrive syncing a settings folder in Documents, antivirus
//! scanning it, Windows Search indexing it. Those let go within moments, so the replace is tried again for about a
//! second; if it still fails, the new contents are written into the old file directly, which they usually allow (they
//! hold it open for reading, not against being written). An error after that names the file and says what's likely.
//! A Mac doesn't lock files this way, so there it succeeds first time.
//!
//! A folder that won't take the new file at all (Program Files, where only administrators write) is another matter:
//! no wait mends it, so its error says the folder is read-only and to choose a settings folder the user can write to.
//!
//! A file the app keeps that it can no longer read (cut short, or mangled) is about to be saved over with whatever
//! the app starts from instead; [`set_aside`] keeps it beside the new one first, so what it held can be got back.

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How many times the replace is tried, and how long apart: about a second in all, enough for a sync or scan to let go
const TRIES: u32 = 10;
const PAUSE: Duration = Duration::from_millis(100);

/// Keep a copy of a file that couldn't be read before it's saved over: "settings.json" as
/// "settings.damaged-20261111_140312.json" beside it, the time being now. The settings file and the Database
/// sessions' waiting changes read as empty when they can't be parsed, so the app still starts; without the copy,
/// the next save would leave nothing of what they held. Returns the copy's path, or None when it couldn't be made
/// (the save goes ahead either way, as it did)
pub fn set_aside(path: &Path) -> Option<PathBuf> {
    let stem = path.file_stem()?.to_str()?;
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("txt");
    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let copy = path.with_file_name(format!("{stem}.damaged-{stamp}.{extension}"));
    std::fs::copy(path, &copy).ok()?;
    // the tests' files aren't the user's: nothing about them goes in the app's log
    if !cfg!(test) {
        crate::applog::warn(format!("{} couldn't be read, so it starts empty; what it held is kept as {}", path.display(), copy.display()));
    }
    Some(copy)
}

/// Write `contents` to `path` whole: to `path` with `temp_extension` first ("xml.saving"), then in its place, tried
/// again while another program holds the file, else written in place; an error names the file and the likely reason
pub fn save(path: &Path, temp_extension: &str, contents: impl AsRef<[u8]>) -> Result<(), String> {
    save_with(path, temp_extension, contents.as_ref(), |temp, contents| std::fs::write(temp, contents))
}

/// [`save`], with the new file written by `write`, so a test can stand in for a folder that takes no new files.
/// When the new file can't be made beside the old, the folder itself refuses (it's read-only to this account, such as
/// Program Files), which no wait mends: the error says so rather than blaming another program.
fn save_with(
    path: &Path,
    temp_extension: &str,
    contents: &[u8],
    write: impl FnOnce(&Path, &[u8]) -> io::Result<()>,
) -> Result<(), String> {
    let temp = path.with_extension(temp_extension);
    write(&temp, contents).map_err(|e| explain_folder(path, &e))?;
    replace(&temp, path, |from, to| std::fs::rename(from, to), std::thread::sleep).map_err(|e| explain(path, &e))
}

/// Whether this account can make files in `folder`: a small file is made there and taken away again. Settings >
/// General asks before using a settings folder that can't take one (Program Files, a read-only share), since nothing
/// changed in the app (sessions, button bars, layouts) could then be saved there.
pub fn writable(folder: &Path) -> bool {
    let probe = folder.join(format!(".superxterminal-write-check-{}", std::process::id()));
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(file) => {
            drop(file);
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Put `temp` in place of `path` with `rename`, trying again (waiting with `wait`) while the file is held, then
/// writing it in place; `temp` is gone afterwards either way it succeeds. Its parts are given, so tests can stand in
/// for a file OneDrive holds
fn replace(
    temp: &Path,
    path: &Path,
    mut rename: impl FnMut(&Path, &Path) -> io::Result<()>,
    mut wait: impl FnMut(Duration),
) -> io::Result<()> {
    let mut last = None;
    for attempt in 0..TRIES {
        if attempt > 0 {
            wait(PAUSE);
        }
        match rename(temp, path) {
            Ok(()) => return Ok(()),
            Err(e) if held(&e) => last = Some(e),
            Err(e) => return Err(e),
        }
    }
    // still held: write the contents into the file itself, which a program reading it usually still allows
    match std::fs::copy(temp, path) {
        Ok(_) => {
            let _ = std::fs::remove_file(temp);
            Ok(())
        }
        Err(_) => Err(last.unwrap_or_else(|| io::Error::from(io::ErrorKind::PermissionDenied))),
    }
}

/// Whether an error is another program holding the file (or a read-only one): Access is denied (5), a sharing
/// violation (32) or a lock violation (33) on Windows
fn held(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::PermissionDenied || (cfg!(windows) && matches!(error.raw_os_error(), Some(32 | 33)))
}

/// A save's error as the user sees it: which file, in which folder, the system's words, and what's likely
fn explain(path: &Path, error: &io::Error) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string());
    let folder = path.parent().map(|p| p.display().to_string()).unwrap_or_default();
    let likely = if held(error) {
        " It may be read-only, or open in another program (OneDrive or antivirus, while they sync or scan it): try again in a moment."
    } else {
        ""
    };
    format!("Couldn't save {name} in {folder}: {error}.{likely}")
}

/// A save's error when the new file couldn't be made in the folder at all: Access is denied there means the folder
/// is read-only to this account, not that another program holds the file, so trying again won't help. Under Program
/// Files (where a settings folder can end up beside the app, as SuperXPuTTY's often was) only an administrator can
/// write; the way out is a settings folder the user can write to, or someone who can change the file there.
fn explain_folder(path: &Path, error: &io::Error) -> String {
    if error.kind() != io::ErrorKind::PermissionDenied {
        return explain(path, error);
    }
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string());
    let folder = path.parent().unwrap_or(Path::new(""));
    let why = if under_program_files(folder) {
        "Only an administrator can change files in Program Files"
    } else {
        "This account can't make files in that folder"
    };
    format!(
        "Couldn't save {name} in {}: {error}. {why}: choose a settings folder you can write to in Settings > General, \
         or ask whoever looks after this one to change it.",
        folder.display()
    )
}

/// Whether `folder` is in Windows' Program Files (either of them), where only administrators may write; never on the
/// Mac or Linux
fn under_program_files(folder: &Path) -> bool {
    if !cfg!(windows) {
        return false;
    }
    let folder = folder.to_string_lossy().to_lowercase();
    ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|root| root.to_string_lossy().trim_end_matches('\\').to_lowercase())
        .any(|root| folder == root || folder.starts_with(&format!("{root}\\")))
}

#[cfg(test)]
mod tests {
    //! The replace with its rename stood in for: refused as Windows refuses it while OneDrive holds the file (Access is
    //! denied), a set number of times or always; the waits are recorded rather than slept. Real files in a folder
    //! of each test's own take the contents.

    use super::*;
    use std::cell::Cell;

    /// A folder of the test's own, emptied when it ends
    struct Folder(std::path::PathBuf);
    impl Drop for Folder {
        /// Remove the folder and what's in it
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A folder of its own (each test's named for it, as they run at once) with Sessions.xml (old) and
    /// Sessions.xml.saving (new) in it
    fn files(test: &str) -> (Folder, std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("sxt-saving-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Sessions.xml");
        let temp = dir.join("Sessions.xml.saving");
        std::fs::write(&path, "old").unwrap();
        std::fs::write(&temp, "new").unwrap();
        (Folder(dir), path, temp)
    }

    /// A rename refused with Access is denied the first `refusals` times, then done for real
    fn refused(refusals: u32, tried: &Cell<u32>) -> impl FnMut(&Path, &Path) -> io::Result<()> + '_ {
        move |from, to| {
            tried.set(tried.get() + 1);
            if tried.get() <= refusals {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            } else {
                std::fs::rename(from, to)
            }
        }
    }

    #[test]
    fn a_file_another_program_holds_for_a_moment_is_saved_once_it_lets_go() {
        let (_dir, path, temp) = files("moment");
        let (tried, waited) = (Cell::new(0), Cell::new(Duration::ZERO));
        replace(&temp, &path, refused(3, &tried), |d| waited.set(waited.get() + d)).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert!(!temp.exists());
        assert_eq!((tried.get(), waited.get()), (4, PAUSE * 3));
    }

    #[test]
    fn a_file_held_all_along_is_written_in_place() {
        let (_dir, path, temp) = files("held");
        let tried = Cell::new(0);
        replace(&temp, &path, refused(u32::MAX, &tried), |_| {}).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert!(!temp.exists());
        assert_eq!(tried.get(), TRIES);
    }

    #[test]
    fn another_failure_is_not_tried_again() {
        let (_dir, path, temp) = files("other");
        let tried = Cell::new(0);
        let error = replace(&temp, &path, |_, _| { tried.set(tried.get() + 1); Err(io::Error::from(io::ErrorKind::NotFound)) }, |_| {}).unwrap_err();
        assert_eq!((error.kind(), tried.get()), (io::ErrorKind::NotFound, 1));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old");
    }

    #[test]
    fn the_error_names_the_file_its_folder_and_what_is_likely() {
        let path = Path::new("team").join("Sessions.xml");
        let message = explain(&path, &io::Error::from(io::ErrorKind::PermissionDenied));
        assert!(message.starts_with("Couldn't save Sessions.xml in team: "), "{message}");
        assert!(message.contains("open in another program (OneDrive or antivirus"), "{message}");
        assert!(!explain(&path, &io::Error::from(io::ErrorKind::NotFound)).contains("OneDrive"));
    }

    #[test]
    fn a_folder_that_takes_no_new_files_says_so_rather_than_to_try_again() {
        // the settings folder where the app is installed, which Windows lets only administrators write to; the Mac has
        // no Program Files, so there it's any folder this account can't write to
        let folder = match std::env::var_os("ProgramFiles") {
            Some(root) if cfg!(windows) => Path::new(&root).join("SuperXTerminal"),
            _ => Path::new("/Library/SuperXTerminal").to_path_buf(),
        };
        let refused = |_: &Path, _: &[u8]| Err(io::Error::from(io::ErrorKind::PermissionDenied));
        let message = save_with(&folder.join("ButtonBars.xml"), "xml.tmp", b"<ButtonBars />", refused).unwrap_err();
        assert!(message.starts_with(&format!("Couldn't save ButtonBars.xml in {}: ", folder.display())), "{message}");
        assert!(!message.contains("OneDrive") && !message.contains("try again"), "{message}");
        assert!(message.contains("choose a settings folder you can write to in Settings > General"), "{message}");
        let why = if cfg!(windows) { "Only an administrator can change files in Program Files" } else { "This account can't make files in that folder" };
        assert!(message.contains(why), "{message}");
    }

    #[test]
    fn a_folder_is_writable_when_a_file_can_be_made_there_and_nothing_is_left_behind() {
        let (dir, _path, _temp) = files("writable");
        assert!(writable(&dir.0));
        let mut left: Vec<_> = std::fs::read_dir(&dir.0).unwrap().map(|e| e.unwrap().file_name()).collect();
        left.sort();
        assert_eq!(left, ["Sessions.xml", "Sessions.xml.saving"]);
        assert!(!writable(&dir.0.join("not there")));
    }

    #[test]
    fn a_save_puts_the_whole_file_in_place() {
        let (_dir, path, _temp) = files("save");
        save(&path, "xml.saving", "<ArrayOfSessionData />").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "<ArrayOfSessionData />");
        assert!(!path.with_extension("xml.saving").exists());
    }
}
