//! Forgetting a host's changed key in known_hosts, from the tab that ssh refused it in (F); the work is in
//! `crate::known_hosts`.

/// Remove the tab's host's old key from the user's known_hosts with `ssh-keygen -R`: `host` as known_hosts names
/// it, `file` the one ssh said holds the old key. What ssh-keygen said, or why it wasn't done
// async: ssh-keygen is a program to wait for, which mustn't hold up the window
#[tauri::command(async)]
pub(crate) fn known_hosts_forget(host: String, file: String) -> Result<String, String> {
    crate::known_hosts::forget(&host, &file)
}
