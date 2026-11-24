//! A host's saved key in `~/.ssh/known_hosts`, and forgetting it when the host's key has changed. ssh refuses a
//! host whose key differs from the one saved ("REMOTE HOST IDENTIFICATION HAS CHANGED!"); when the user knows why
//! (the device was replaced or reinstalled), the tab offers F to remove the old key with OpenSSH's own
//! `ssh-keygen -R`, and the next connection meets the host as new (`src/lib/terminal/PageHost.ts`).
//!
//! Only the session's own host is forgotten, under the name launch.rs gave the tab ([`name`]), and only in the
//! user's own known_hosts: ssh's warning is text in the tab, and text a server prints could otherwise name another
//! host or another file.

use std::path::{Path, PathBuf};

/// The name ssh saves a host's key under in known_hosts: the host alone on port 22 (0 here is 22), else
/// "[host]:port", as OpenSSH writes it (and names it in its warning, "Host key for [host]:2222 has changed")
pub fn name(host: &str, port: u16) -> String {
    if port == 0 || port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

/// The user's own known_hosts, where ssh saves the keys it accepts and `ssh-keygen -R` looks by default
fn own_file() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh").join("known_hosts"))
}

/// Whether two paths are the same file: Windows' ssh writes "C:\Users\me/.ssh/known_hosts", with both slashes
fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Remove `host`'s keys from `file` with `ssh-keygen -f file -R host`, which keeps the file as it was in
/// `file.old`; what ssh-keygen said, or why it couldn't
fn forget_in(host: &str, file: &Path) -> Result<String, String> {
    // a name ssh-keygen would take for one of its own switches, or one with a space, isn't a host's
    if host.is_empty() || host.starts_with('-') || host.contains(char::is_whitespace) {
        return Err(format!("\"{host}\" isn't a host name."));
    }
    let mut command = std::process::Command::new(crate::launch::openssh("ssh-keygen"));
    command.arg("-f").arg(file).arg("-R").arg(host);
    crate::launch::no_window(&mut command);
    let out = command.output().map_err(|e| format!("Couldn't run ssh-keygen: {e}"))?;
    let said = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)).trim().to_string();
    if out.status.success() {
        Ok(said)
    } else {
        Err(if said.is_empty() { format!("ssh-keygen ended with {}", out.status) } else { said })
    }
}

/// Forget the changed key of the tab's host (F after ssh refused it). `host` is the tab's own (launch.rs's
/// `Look::known_host`); `file` is the one ssh named ("Offending key in ..."), which must be the user's own
/// known_hosts: any other is left for the user, with the command that would do it
pub fn forget(host: &str, file: &str) -> Result<String, String> {
    let own = own_file().ok_or("There's no home folder to find known_hosts in.")?;
    if !same_file(Path::new(file), &own) {
        return Err(format!("The old key is in {file}, not your own known_hosts: remove it with ssh-keygen -f \"{file}\" -R \"{host}\"."));
    }
    forget_in(host, &own)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_is_named_as_known_hosts_names_it() {
        assert_eq!(name("192.168.0.50", 22), "192.168.0.50");
        assert_eq!(name("192.168.0.50", 0), "192.168.0.50");
        assert_eq!(name("sw1.lab", 2222), "[sw1.lab]:2222");
    }

    /// A known_hosts of the test's own: ssh-keygen (OpenSSH's, on the Mac and Windows alike) takes out the one host
    /// and leaves the others
    #[test]
    fn forgetting_a_host_takes_out_its_key_and_leaves_the_others() {
        let folder = std::env::temp_dir().join(format!("sxt-known-hosts-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("known_hosts");
        let key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";
        std::fs::write(&file, format!("192.168.0.50 {key}\n[sw1.lab]:2222 {key}\n")).unwrap();
        let forgotten = forget_in("192.168.0.50", &file);
        let left = std::fs::read_to_string(&file).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&folder);
        assert!(forgotten.is_ok(), "{forgotten:?}");
        assert!(!left.contains("192.168.0.50"), "{left}");
        assert!(left.contains("[sw1.lab]:2222"), "{left}");
    }

    #[test]
    fn a_switch_or_a_space_isnt_taken_for_a_host() {
        let file = std::env::temp_dir().join("sxt-known-hosts-none");
        assert!(forget_in("-f", &file).is_err());
        assert!(forget_in("a b", &file).is_err());
        assert!(forget_in("", &file).is_err());
    }

    /// Text in the tab naming another file isn't acted on: only the user's own known_hosts is changed here
    #[test]
    fn only_the_users_own_known_hosts_is_changed() {
        let other = std::env::temp_dir().join(format!("sxt-known-hosts-other-{}", std::process::id()));
        std::fs::write(&other, "192.168.0.50 ssh-ed25519 AAAA\n").unwrap();
        let refused = forget("192.168.0.50", &other.to_string_lossy());
        let left = std::fs::read_to_string(&other).unwrap_or_default();
        let _ = std::fs::remove_file(&other);
        assert!(refused.is_err_and(|e| e.contains("ssh-keygen -f")));
        assert!(left.contains("192.168.0.50"));
    }
}
