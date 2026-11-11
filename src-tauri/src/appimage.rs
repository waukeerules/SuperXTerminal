//! The environment of the programs the app starts, when it runs as an AppImage on Linux.
//!
//! An AppImage's launcher points `LD_LIBRARY_PATH`, `PATH`, `XDG_DATA_DIRS` and GTK's and GIO's variables at the
//! libraries and files packed inside it (its `APPDIR`, mounted at `/tmp/.mount_...`), so the app's own GTK and
//! WebKit load. Every program the app starts inherits them: a Local Shell and all that's run in it, ssh, scp, the
//! 1Password CLI, xdg-open. Those are the system's programs, and with the AppImage's older libsystemd and libcrypto
//! ahead of the system's they fail ("version `LIBSYSTEMD_254' not found (required by head)"). The app's own
//! environment can't simply be cleaned at start: WebKit's helper processes, which are the AppImage's, need it.
//!
//! So [`tidy`] gives each program the app starts the environment the system would: every entry inside `APPDIR` taken
//! out of each variable (a variable left empty removed), and the AppImage runtime's own variables (`APPDIR`,
//! `APPIMAGE`, `ARGV0`, `OWD`) and the GTK theme its launcher forces removed. Out of an AppImage (the .deb, the .rpm, the Mac, Windows) it changes nothing.

use std::ffi::{OsStr, OsString};

/// The variables the AppImage runtime sets to describe itself, which are nothing to the programs the app starts; and
/// GTK_THEME, which linuxdeploy's GTK hook (`apprun-hooks/linuxdeploy-plugin-gtk.sh`) sets to Adwaita, light or dark,
/// over whatever the user had: a GTK program started from a Local Shell would take it and lose its own theme
const RUNTIME_VARIABLES: [&str; 5] = ["APPDIR", "APPIMAGE", "ARGV0", "OWD", "GTK_THEME"];

/// One variable to change in a started program's environment: its new value, or None to remove it
pub type Change = (OsString, Option<OsString>);

/// What to change in the environment `vars` so nothing in it points inside `appdir`, where the AppImage is mounted:
/// each variable's entries (split at `:`, as path lists are) inside `appdir` taken out, a variable with nothing else
/// left removed, and the runtime's own variables removed. Variables with nothing inside `appdir` aren't listed.
pub fn changes(vars: impl IntoIterator<Item = (OsString, OsString)>, appdir: &str) -> Vec<Change> {
    let appdir = appdir.trim_end_matches('/');
    if appdir.is_empty() {
        return Vec::new();
    }
    let inside = |entry: &str| entry == appdir || entry.starts_with(&format!("{appdir}/"));
    let mut changes = Vec::new();
    for (name, value) in vars {
        if name.to_str().is_some_and(|n| RUNTIME_VARIABLES.contains(&n)) {
            changes.push((name, None));
            continue;
        }
        // a value that isn't UTF-8 isn't one the launcher wrote
        let Some(text) = value.to_str() else { continue };
        if !text.split(':').any(inside) {
            continue;
        }
        let kept: Vec<&str> = text.split(':').filter(|entry| !inside(entry)).collect();
        let value = if kept.iter().all(|entry| entry.is_empty()) { None } else { Some(OsString::from(kept.join(":"))) };
        changes.push((name, value));
    }
    changes
}

/// A program about to be started whose environment can be changed: std's, tokio's, and the pseudo terminal's
pub trait Environment {
    /// Set a variable for the program
    fn set_variable(&mut self, name: &OsStr, value: &OsStr);
    /// Leave a variable out of the program's environment
    fn remove_variable(&mut self, name: &OsStr);
}

impl Environment for std::process::Command {
    fn set_variable(&mut self, name: &OsStr, value: &OsStr) {
        self.env(name, value);
    }
    fn remove_variable(&mut self, name: &OsStr) {
        self.env_remove(name);
    }
}

impl Environment for tokio::process::Command {
    fn set_variable(&mut self, name: &OsStr, value: &OsStr) {
        self.env(name, value);
    }
    fn remove_variable(&mut self, name: &OsStr) {
        self.env_remove(name);
    }
}

impl Environment for portable_pty::CommandBuilder {
    fn set_variable(&mut self, name: &OsStr, value: &OsStr) {
        self.env(name, value);
    }
    fn remove_variable(&mut self, name: &OsStr) {
        self.env_remove(name);
    }
}

/// Make the [`changes`] to the environment of `command`, a program not yet started
fn apply<C: Environment + ?Sized>(command: &mut C, changes: &[Change]) {
    for (name, value) in changes {
        match value {
            Some(value) => command.set_variable(name, value),
            None => command.remove_variable(name),
        }
    }
}

/// Give `command`, a program the app is about to start, the system's environment, not the AppImage's: see the
/// module's comment. Nothing changes unless the app runs as an AppImage (the runtime sets both `APPIMAGE` and `APPDIR`).
pub fn tidy<C: Environment + ?Sized>(command: &mut C) {
    if std::env::var_os("APPIMAGE").is_none() {
        return;
    }
    let Some(appdir) = std::env::var("APPDIR").ok() else { return };
    apply(command, &changes(std::env::vars_os(), &appdir));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where an AppImage's runtime mounts it, as in the report
    const APPDIR: &str = "/tmp/.mount_SuperXbMBaaj";

    /// The environment as an AppImage's launcher leaves it: its own folders ahead of the system's, GTK pointed inside
    /// it, and the runtime's variables; plus the user's own, which it doesn't touch
    fn launched() -> Vec<(OsString, OsString)> {
        [
            ("LD_LIBRARY_PATH", format!("{APPDIR}/usr/lib:{APPDIR}/usr/lib/x86_64-linux-gnu:")),
            ("PATH", format!("{APPDIR}/usr/bin:/usr/local/bin:/usr/bin:/bin")),
            ("XDG_DATA_DIRS", format!("{APPDIR}/usr/share:/usr/local/share:/usr/share")),
            // as linuxdeploy's GTK hook writes them: "$APPDIR//usr", and the theme it forces
            ("GDK_PIXBUF_MODULE_FILE", format!("{APPDIR}//usr/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache")),
            ("GTK_THEME", "Adwaita:dark".to_string()),
            ("APPDIR", APPDIR.to_string()),
            ("APPIMAGE", "/home/me/SuperXTerminal-0.8.5-x86_64.AppImage".to_string()),
            ("ARGV0", "./SuperXTerminal-0.8.5-x86_64.AppImage".to_string()),
            ("HOME", "/home/me".to_string()),
            ("LANG", "en_US.UTF-8".to_string()),
        ]
        .into_iter()
        .map(|(name, value)| (OsString::from(name), OsString::from(value)))
        .collect()
    }

    #[test]
    fn a_started_program_gets_the_system_paths_without_the_appimages_own() {
        let changed: std::collections::HashMap<String, Option<String>> = changes(launched(), APPDIR)
            .into_iter()
            .map(|(name, value)| (name.into_string().unwrap(), value.map(|v| v.into_string().unwrap())))
            .collect();
        assert_eq!(changed["PATH"].as_deref(), Some("/usr/local/bin:/usr/bin:/bin"));
        assert_eq!(changed["XDG_DATA_DIRS"].as_deref(), Some("/usr/local/share:/usr/share"));
        // nothing of its own left: gone, so the system's own library search is used
        assert_eq!(changed["LD_LIBRARY_PATH"], None);
        assert_eq!(changed["GDK_PIXBUF_MODULE_FILE"], None);
        for runtime in ["APPDIR", "APPIMAGE", "ARGV0", "GTK_THEME"] {
            assert_eq!(changed[runtime], None, "{runtime}");
        }
        // the user's own are left as they are
        assert!(!changed.contains_key("HOME") && !changed.contains_key("LANG"));
    }

    #[test]
    fn out_of_an_appimage_nothing_changes() {
        let plain = [(OsString::from("PATH"), OsString::from("/usr/bin:/bin"))];
        assert!(changes(plain.clone(), "").is_empty());
        // a folder that merely starts the same isn't inside it
        let near = [(OsString::from("PATH"), OsString::from(format!("{APPDIR}-other/bin:/usr/bin")))];
        assert!(changes(near, APPDIR).is_empty());
    }

    /// The report: a Local Shell in the AppImage ran the system's programs with the AppImage's libraries. A shell
    /// started as the app starts it, from the launcher's environment, sees none of the AppImage's folders. Unix only:
    /// AppImages are Linux's, and this runs `/bin/sh`, which Windows hasn't got
    #[cfg(unix)]
    #[test]
    fn a_local_shell_started_from_an_appimage_sees_none_of_its_libraries() {
        let mut shell = std::process::Command::new("/bin/sh");
        shell.env_clear();
        for (name, value) in launched() {
            shell.env(name, value);
        }
        apply(&mut shell, &changes(launched(), APPDIR));
        let output = shell
            .args(["-c", r#"printf '%s|%s|%s' "${LD_LIBRARY_PATH-unset}" "$PATH" "${APPDIR-unset}""#])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout), "unset|/usr/local/bin:/usr/bin:/bin|unset");
    }
}
