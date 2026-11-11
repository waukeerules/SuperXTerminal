//! The Mac: connecting the settings folder's network share again when it has gone and its server is back.
//!
//! Windows reaches a share by its address (`\\server\share\folder`) each time, so a settings folder there is back as
//! soon as the network is. The Mac reaches one only through a mount: Finder connects `smb://server/share` and it
//! appears under `/Volumes`. When the network drops macOS takes the mount away, and it doesn't put it back: the
//! settings folder stayed away, Saved sessions shown from the copy on this computer ([`crate::saved_cache`]), until
//! the user connected to the server again by hand (choosing the folder again did it, through the folder dialog).
//!
//! So while the settings folder is there, the share it's on is remembered ([`remember`]: its address, as `mount`
//! lists it, in `SettingsFolderShare.json` in the user's folder, so it's known at the next start too). And when the
//! folder is away, each check of the session files ([`reconnect`], from `sessions_files_changed`) looks whether the
//! share's server answers, and if it does asks macOS to mount the share again, as Finder's Connect to Server does.
//! Once only each time the server comes back: macOS takes the password from the Keychain, and asks for it when it
//! isn't kept there, which mustn't happen every few seconds.
//!
//! Only the two commands it runs are the Mac's (`mount`, `osascript`); the rest is the same anywhere, and is tested
//! on every system. Nothing here is called on Windows or Linux (`lib.rs` asks only on the Mac), hence the
//! allowance below for what they don't use.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Where the share the settings folder is on is remembered, in the user's folder
const FILE: &str = "SettingsFolderShare.json";
/// SMB's port, tried to see whether the server answers
const SMB_PORT: u16 = 445;
/// How long the server is given to answer
const ANSWER_WITHIN: Duration = Duration::from_secs(2);
/// How long macOS is given to mount the share: it may be asking for the password meanwhile
const MOUNT_WITHIN: Duration = Duration::from_secs(120);

/// A network share a folder is on
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Share {
    /// The folder it was found for (the settings folder): another folder's share isn't this one's
    folder: String,
    /// What to mount: "smb://user@server/share", as `mount` lists it after "smb:"
    url: String,
    /// Where it was mounted ("/Volumes/public"): where it has to be again for the folder to be there
    mount_point: String,
}

/// The SMB share `folder` is on, from what `mount` prints: the line whose mount point the folder is in (the longest,
/// for a mount inside another), when it's an `smbfs` one. Lines read
/// `//user@server/share on /Volumes/share (smbfs, nodev, nosuid, mounted by me)`: the address has no spaces (they're
/// %20), the mount point may.
fn share_of(mounts: &str, folder: &Path) -> Option<Share> {
    let folder_text = folder.to_str()?;
    mounts
        .lines()
        .filter_map(|line| {
            let (before, options) = line.rsplit_once(" (")?;
            let (source, mount_point) = before.split_once(" on ")?;
            let inside = folder_text == mount_point || folder_text.strip_prefix(mount_point).is_some_and(|rest| rest.starts_with('/'));
            (options.starts_with("smbfs") && source.starts_with("//") && mount_point.starts_with('/') && inside)
                .then(|| Share { folder: folder_text.to_string(), url: format!("smb:{source}"), mount_point: mount_point.to_string() })
        })
        .max_by_key(|share| share.mount_point.len())
}

/// The server of a share's address and the port it answers on: "smb://DOMAIN;user:pass@server:1445/share" gives
/// ("server", 1445), and [`SMB_PORT`] when none is said; None when there's no server in it
fn server_of(url: &str) -> Option<(String, u16)> {
    let after = url.split_once("//")?.1;
    let authority = after.split('/').next()?;
    let host = authority.rsplit('@').next()?;
    let (name, port) = match host.rsplit_once(':') {
        Some((name, port)) if port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() => (name, port.parse().ok()?),
        _ => (host, SMB_PORT),
    };
    (!name.is_empty()).then(|| (name.to_string(), port))
}

/// A share's address for the log and for the user: without the user's name or a password
fn shown(url: &str) -> String {
    match url.split_once("//") {
        Some((scheme, rest)) => format!("{scheme}//{}", rest.rsplit_once('@').map_or(rest, |(_, after)| after)),
        None => url.to_string(),
    }
}

/// The AppleScript that mounts a share as Finder's Connect to Server does, its address quoted for AppleScript
fn mount_script(url: &str) -> String {
    format!("mount volume \"{}\"", url.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Whether to ask for the share to be mounted now: only when its server answers, and only the first time since it
/// last didn't (`tried` says it's been asked since). A server that's away clears `tried`, so its return is tried.
fn worth_trying(server_answers: bool, tried: &AtomicBool) -> bool {
    if !server_answers {
        tried.store(false, Ordering::SeqCst);
        return false;
    }
    !tried.swap(true, Ordering::SeqCst)
}

/// The share remembered for `folder`; None when there's none, or it's another folder's
fn remembered(user_folder: &Path, folder: &Path) -> Option<Share> {
    let share: Share = serde_json::from_str(&std::fs::read_to_string(user_folder.join(FILE)).ok()?).ok()?;
    (Some(share.folder.as_str()) == folder.to_str()).then_some(share)
}

/// Remember the share the settings folder is on, while the folder is there: asked each time the session files are
/// read, and written only when it differs from what's kept. A folder that isn't on an SMB share (this computer's
/// own disk, a synced folder) leaves what's kept alone; it's another folder's by then, and never used.
pub fn remember(user_folder: &Path, folder: &Path) {
    // the Mac mounts shares under /Volumes: nothing else needs `mount` asked
    if !folder.starts_with("/Volumes") {
        return;
    }
    let Ok(output) = Command::new("/sbin/mount").stdin(Stdio::null()).output() else { return };
    let Some(share) = share_of(&String::from_utf8_lossy(&output.stdout), folder) else { return };
    if remembered(user_folder, folder).as_ref() == Some(&share) {
        return;
    }
    match serde_json::to_string(&share).map_err(|e| e.to_string()).and_then(|text| crate::saving::save(&user_folder.join(FILE), "saving", text)) {
        Ok(()) => crate::applog::info(format!("The settings folder is on {}, connected again by itself when it's been away", shown(&share.url))),
        Err(e) => crate::applog::warn(format!("Couldn't remember the settings folder's share: {e}")),
    }
}

/// Whether the share's server answers on its port within [`ANSWER_WITHIN`]
fn answers(server: &str, port: u16) -> bool {
    let Ok(addresses) = (server, port).to_socket_addrs() else { return false };
    addresses.into_iter().any(|address| TcpStream::connect_timeout(&address, ANSWER_WITHIN).is_ok())
}

/// The settings folder is away: if it's on a share that's remembered and the share's server answers, ask macOS to
/// mount the share again, once each time the server comes back ([`worth_trying`]). It waits for the mount (macOS may
/// be asking for the password), so it's for a thread that may wait, not the window's. The check of the session
/// files that follows finds the folder there, and the tree reads it.
pub fn reconnect(user_folder: &Path, folder: &Path) {
    /// One at a time: the next check comes while macOS may still be asking for the password
    static BUSY: AtomicBool = AtomicBool::new(false);
    /// Asked for since the server last didn't answer
    static TRIED: AtomicBool = AtomicBool::new(false);
    let Some(share) = remembered(user_folder, folder) else { return };
    let Some((server, port)) = server_of(&share.url) else { return };
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    if worth_trying(answers(&server, port), &TRIED) {
        let name = shown(&share.url);
        crate::applog::info(format!("The settings folder is away and {server} answers: connecting to {name} again"));
        match mount(&share.url) {
            Ok(()) if folder.is_dir() => crate::applog::info(format!("Connected to {name} again: the settings folder is back")),
            Ok(()) => crate::applog::warn(format!("Connected to {name} again, but not at {} as before: the settings folder is still away. Connect to it in Finder, or choose the folder again", share.mount_point)),
            Err(e) => crate::applog::warn(format!("Couldn't connect to {name} again: {e}. Connect to it in Finder (Go > Connect to Server)")),
        }
    }
    BUSY.store(false, Ordering::SeqCst);
}

/// Ask macOS to mount a share, as Finder's Connect to Server: the password comes from the Keychain, or macOS asks.
/// Given up after [`MOUNT_WITHIN`] (a password window left open).
fn mount(url: &str) -> Result<(), String> {
    let mut child = Command::new("/usr/bin/osascript")
        .args(["-e", &mount_script(url)])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) if status.success() => return Ok(()),
            Some(_) => {
                let mut said = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = std::io::Read::read_to_string(&mut stderr, &mut said);
                }
                let said = said.trim();
                return Err(if said.is_empty() { "macOS didn't say why".to_string() } else { said.to_string() });
            }
            None if started.elapsed() > MOUNT_WITHIN => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("it took too long".into());
            }
            None => std::thread::sleep(Duration::from_millis(200)),
        }
    }
}

/// The remembered share's file in `user_folder`, for a test to look at
#[cfg(test)]
fn file(user_folder: &Path) -> std::path::PathBuf {
    user_folder.join(FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `mount` prints on a Mac with two shares connected, one with a space in its name
    const MOUNTS: &str = "/dev/disk3s1s1 on / (apfs, sealed, local, read-only, journaled)\n\
        devfs on /dev (devfs, local, nobrowse)\n\
        /dev/disk3s5 on /System/Volumes/Data (apfs, local, journaled, nobrowse, protect, root data)\n\
        //tj@openmediavault.localdomain/public on /Volumes/public (smbfs, nodev, nosuid, mounted by tjbuehrer)\n\
        //guest:@nas.example.net/Team%20Files on /Volumes/Team Files (smbfs, nodev, nosuid, mounted by tjbuehrer)\n\
        nas.example.net:/export/media on /Volumes/media (nfs, nodev, nosuid, mounted by tjbuehrer)\n";

    #[test]
    fn the_share_a_folder_is_on_is_found_among_the_macs_mounts() {
        let share = share_of(MOUNTS, Path::new("/Volumes/public/SuperXTerminal")).unwrap();
        assert_eq!(
            (share.url.as_str(), share.mount_point.as_str(), share.folder.as_str()),
            ("smb://tj@openmediavault.localdomain/public", "/Volumes/public", "/Volumes/public/SuperXTerminal")
        );
        // the share itself as the folder, and one with a space in its name
        assert_eq!(share_of(MOUNTS, Path::new("/Volumes/public")).unwrap().mount_point, "/Volumes/public");
        let spaced = share_of(MOUNTS, Path::new("/Volumes/Team Files/Settings")).unwrap();
        assert_eq!((spaced.url.as_str(), spaced.mount_point.as_str()), ("smb://guest:@nas.example.net/Team%20Files", "/Volumes/Team Files"));
        // not a folder that only starts like a share's, this Mac's own disk, or a share that isn't SMB
        for folder in ["/Volumes/public-old/SuperXTerminal", "/Users/tj/SuperXTerminal", "/Volumes/media/SuperXTerminal"] {
            assert_eq!(share_of(MOUNTS, Path::new(folder)), None, "{folder}");
        }
    }

    #[test]
    fn a_shares_server_and_port_are_read_from_its_address_and_shown_without_the_login() {
        assert_eq!(server_of("smb://tj@openmediavault.localdomain/public"), Some(("openmediavault.localdomain".into(), 445)));
        assert_eq!(server_of("smb://WORKGROUP;tj:secret@10.0.0.5:1445/Team%20Files"), Some(("10.0.0.5".into(), 1445)));
        assert_eq!(server_of("smb://nas/public"), Some(("nas".into(), 445)));
        assert_eq!(server_of("smb:///public"), None);
        assert_eq!(shown("smb://WORKGROUP;tj:secret@10.0.0.5:1445/Team%20Files"), "smb://10.0.0.5:1445/Team%20Files");
        assert_eq!(shown("smb://nas/public"), "smb://nas/public");
        assert_eq!(mount_script("smb://tj@nas/pub\"lic"), "mount volume \"smb://tj@nas/pub\\\"lic\"");
    }

    #[test]
    fn the_share_is_asked_for_once_each_time_its_server_comes_back() {
        let tried = AtomicBool::new(false);
        // the network away: nothing asked
        assert!(!worth_trying(false, &tried));
        // back: asked once, and not at the checks after (macOS may have asked for a password, and been told no)
        assert!(worth_trying(true, &tried));
        assert!(!worth_trying(true, &tried));
        assert!(!worth_trying(true, &tried));
        // away and back again: asked again
        assert!(!worth_trying(false, &tried));
        assert!(worth_trying(true, &tried));
    }

    #[test]
    fn the_share_remembered_is_its_own_folders() {
        let user = std::env::temp_dir().join(format!("sxt-remount-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&user);
        std::fs::create_dir_all(&user).unwrap();
        let folder = Path::new("/Volumes/public/SuperXTerminal");
        assert_eq!(remembered(&user, folder), None);
        let share = share_of(MOUNTS, folder).unwrap();
        std::fs::write(file(&user), serde_json::to_string(&share).unwrap()).unwrap();
        assert_eq!(remembered(&user, folder), Some(share));
        // another settings folder chosen since: not connected to the first one's share
        assert_eq!(remembered(&user, Path::new("/Volumes/other/SuperXTerminal")), None);
        std::fs::remove_dir_all(&user).unwrap();
    }
}
