//! Windows only: puts right taskbar pins of SuperXTerminal whose icon has gone, so they show the app's icon again
//! rather than a blank file.
//!
//! Until 0.8.3 the MSI's Start Menu shortcut took its icon from `C:\Windows\Installer\<product code>\ProductIcon`, a
//! copy Windows Installer keeps for each version. A pin is a copy of the shortcut as it was when pinned, and every
//! upgrade gets a new product code and deletes the old one's folder, so after the next upgrade the pin pointed at an
//! icon that was gone. Windows then drew a blank file: in the pin's right-click menu at once, and on the taskbar
//! itself whenever it redrew the pin from its shortcut. The MSI no longer does that (`windows/main.wxs`), but pins
//! made before stay broken until unpinned and pinned again, which is what this does for the user.
//!
//! When the app starts, [`repair`] looks at the shortcuts in the taskbar's folder of pins. One that opens this
//! program and names an icon file that isn't there is given the program's own icon; anything else is left alone.
//! The shortcuts are read and written through Windows' shell (IShellLinkW, IPersistFile), as Explorer does.

use std::path::{Path, PathBuf};

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile,
    STGM_READ,
};
use windows::Win32::UI::Shell::{IShellLinkW, SHCNE_UPDATEITEM, SHCNF_PATHW, SHChangeNotify, ShellLink};
use windows::core::{HSTRING, Interface, PCWSTR};

/// Repairs this program's pins, on a thread of its own so the window doesn't wait on the shell. Each pin put right
/// goes in the app's log.
pub fn repair() {
    std::thread::spawn(|| {
        let (Ok(program), Some(folder)) = (std::env::current_exe(), pins_folder()) else { return };
        match repair_folder(&folder, &program) {
            Ok(fixed) => {
                for link in fixed {
                    crate::applog::info(format!("The taskbar pin {} had lost its icon; it now has the app's", link.display()));
                }
            }
            Err(e) => crate::applog::warn(format!("The taskbar pins couldn't be checked: {e}")),
        }
    });
}

/// The folder Windows keeps the user's taskbar pins in, as shortcuts:
/// `%APPDATA%\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar`
fn pins_folder() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join(r"Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar"))
}

/// Gives every shortcut in `folder` that opens `program`, and names an icon file that isn't there, `program`'s own
/// icon, and tells Explorer so it draws the pin again. Returns the shortcuts put right; a folder that isn't there
/// has none.
fn repair_folder(folder: &Path, program: &Path) -> Result<Vec<PathBuf>, String> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Ok(Vec::new()) };
    let Some(program_key) = same_file_key(program) else { return Ok(Vec::new()) };
    let _com = Com::init()?;
    let mut fixed = Vec::new();
    for entry in entries.flatten() {
        let link = entry.path();
        if !link.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk")) {
            continue;
        }
        match repair_link(&link, program, &program_key) {
            Ok(true) => fixed.push(link),
            Ok(false) => {}
            Err(e) => crate::applog::warn(format!("The taskbar pin {} couldn't be checked: {e}", link.display())),
        }
    }
    Ok(fixed)
}

/// Puts one shortcut right if it opens the program (`program_key`, from [`same_file_key`]) and its icon file is
/// gone, and returns whether it changed it. A shortcut with no icon of its own already shows its program's, so it's
/// left as it is.
fn repair_link(link: &Path, program: &Path, program_key: &str) -> Result<bool, String> {
    let e = |e: windows::core::Error| e.message();
    // SAFETY: COM is started on this thread (Com::init), and every buffer handed to the shell outlives the call
    unsafe {
        let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(e)?;
        let file: IPersistFile = shell_link.cast().map_err(e)?;
        let link_name = HSTRING::from(link.as_os_str());
        file.Load(&link_name, STGM_READ).map_err(e)?;

        let mut target = [0u16; 32768];
        shell_link.GetPath(&mut target, std::ptr::null_mut(), 0).map_err(e)?;
        let target = PathBuf::from(wide_to_string(&target));
        if same_file_key(&target).as_deref() != Some(program_key) {
            return Ok(false);
        }

        let mut icon = [0u16; 32768];
        let mut index = 0;
        shell_link.GetIconLocation(&mut icon, &mut index).map_err(e)?;
        let icon = wide_to_string(&icon);
        // environment variables (%SystemRoot%...) are left alone rather than guessed at: only plain paths are checked
        if icon.is_empty() || icon.contains('%') || Path::new(&icon).exists() {
            return Ok(false);
        }

        shell_link.SetIconLocation(&HSTRING::from(program.as_os_str()), 0).map_err(e)?;
        file.Save(PCWSTR::null(), true).map_err(e)?;
        SHChangeNotify(SHCNE_UPDATEITEM, SHCNF_PATHW, Some(link_name.as_ptr().cast()), None);
    }
    Ok(true)
}

/// A key that's the same for two paths to the same file, whatever their case or form (`C:\Program Files` or
/// `C:\PROGRA~1`), or None when there's no such file
fn same_file_key(path: &Path) -> Option<String> {
    Some(std::fs::canonicalize(path).ok()?.to_string_lossy().to_lowercase())
}

/// The text in a buffer the shell filled, up to its terminating nul
fn wide_to_string(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// COM started on this thread for as long as it's held, as the shell's objects need; ended when dropped, unless it
/// was already started here (by Tauri, on the main thread), in which case it's left as it was.
struct Com {
    /// Whether this started COM, so must end it
    started: bool,
}

impl Com {
    /// Starts COM on this thread in a single-threaded apartment, the shell's own. A thread already in COM, either
    /// way, can use the shell as it is.
    fn init() -> Result<Com, String> {
        // SAFETY: paired with CoUninitialize in Drop, on the same thread (Com isn't Send)
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        // S_OK or S_FALSE (already started here): ours to end. RPC_E_CHANGED_MODE: started otherwise, not ours
        if result.is_ok() {
            Ok(Com { started: true })
        } else if result == windows::Win32::Foundation::RPC_E_CHANGED_MODE {
            Ok(Com { started: false })
        } else {
            Err(result.message())
        }
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.started {
            // SAFETY: CoInitializeEx succeeded on this thread
            unsafe { CoUninitialize() }
        }
    }
}

/// The repair, on shortcuts made in a folder of their own the way an old MSI's pin was: one to the program whose icon
/// file has gone (the bug), beside others that must be left alone. The program is a stand-in file, as the shell
/// only needs a path that's there.
#[cfg(test)]
mod tests {
    use super::*;

    /// A new, empty folder for one test, removed when the test ends
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let path = std::env::temp_dir().join(format!("superxterminal-pins-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Scratch(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Makes the shortcut `link` to `target`, with the icon `icon` (none when empty), as an installer or pinning does
    fn make_link(link: &Path, target: &Path, icon: &str) {
        let _com = Com::init().unwrap();
        unsafe {
            let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
            shell_link.SetPath(&HSTRING::from(target.as_os_str())).unwrap();
            if !icon.is_empty() {
                shell_link.SetIconLocation(&HSTRING::from(icon), 0).unwrap();
            }
            let file: IPersistFile = shell_link.cast().unwrap();
            file.Save(&HSTRING::from(link.as_os_str()), true).unwrap();
        }
    }

    /// The shortcut's icon file, as Windows reads it
    fn icon_of(link: &Path) -> String {
        let _com = Com::init().unwrap();
        unsafe {
            let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
            let file: IPersistFile = shell_link.cast().unwrap();
            file.Load(&HSTRING::from(link.as_os_str()), STGM_READ).unwrap();
            let mut icon = [0u16; 32768];
            let mut index = 0;
            shell_link.GetIconLocation(&mut icon, &mut index).unwrap();
            wide_to_string(&icon)
        }
    }

    #[test]
    fn a_pin_whose_installer_icon_has_gone_gets_the_programs_icon_again() {
        let dir = Scratch::new("gone");
        let program = dir.0.join("superxterminal.exe");
        std::fs::write(&program, b"").unwrap();
        let pin = dir.0.join("SuperXTerminal.lnk");
        // as an MSI before 0.8.3 left it, once the upgrade had deleted the old version's folder
        let gone = dir.0.join(r"Installer\{5C50076E-DC7A-4E6F-B65E-B10FF93534BB}\ProductIcon");
        make_link(&pin, &program, &gone.to_string_lossy());

        let fixed = repair_folder(&dir.0, &program).unwrap();

        assert_eq!(fixed, vec![pin.clone()]);
        assert_eq!(same_file_key(Path::new(&icon_of(&pin))), same_file_key(&program));
    }

    #[test]
    fn pins_of_other_programs_and_pins_whose_icon_is_there_are_left_alone() {
        let dir = Scratch::new("alone");
        let program = dir.0.join("superxterminal.exe");
        let other = dir.0.join("other.exe");
        let icon = dir.0.join("present.ico");
        for file in [&program, &other, &icon] {
            std::fs::write(file, b"").unwrap();
        }
        let gone = dir.0.join("gone.ico").to_string_lossy().into_owned();
        let present = icon.to_string_lossy().into_owned();
        // another program's broken pin is its own business
        make_link(&dir.0.join("Other.lnk"), &other, &gone);
        // this program's, with an icon that's there or none of its own, shows fine
        make_link(&dir.0.join("Present.lnk"), &program, &present);
        make_link(&dir.0.join("None.lnk"), &program, "");

        assert_eq!(repair_folder(&dir.0, &program).unwrap(), Vec::<PathBuf>::new());
        assert_eq!(icon_of(&dir.0.join("Other.lnk")), gone);
        assert_eq!(icon_of(&dir.0.join("Present.lnk")), present);
        assert_eq!(icon_of(&dir.0.join("None.lnk")), "");
    }

    #[test]
    fn a_missing_folder_of_pins_has_none_to_repair() {
        let dir = Scratch::new("missing");
        assert_eq!(repair_folder(&dir.0.join("nowhere"), &std::env::current_exe().unwrap()).unwrap(), Vec::<PathBuf>::new());
    }
}
