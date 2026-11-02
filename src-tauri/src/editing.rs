//! Edit Session: one session's fields for the window, and saving them back to its file (`sessions::update_file`).
//!
//! The fields are SuperXPuTTY's Edit Session's. A `-pw` password in the extra arguments is shown masked; saved with
//! the mask still in place, the stored password is kept, so the window never holds it.

use serde::{Deserialize, Serialize};

use crate::sessions::{self, StoredSession};

/// What stands in for a stored `-pw` password in the window
const MASK: &str = "••••••";

/// One session's editable fields, as Edit Session shows them
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SessionFields {
    pub name: String,
    pub folder: String,
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub profile: String,
    pub color: String,
    pub icon: String,
    pub highlight_rules: String,
    pub extra_args: String,
    pub jump_host: String,
    pub script: String,
    pub note: String,
    pub local_path: String,
    pub remote_path: String,
    /// Its own RDP options, as SuperXPuTTY's RdpOptions ("" for the defaults)
    pub rdp_options: String,
}

impl SessionFields {
    /// A stored session's fields, its password masked
    pub fn of(session: &StoredSession) -> SessionFields {
        let name = match session.get("SessionName") {
            "" => session.id().rsplit('/').next().unwrap_or("").to_string(),
            name => name.to_string(),
        };
        SessionFields {
            name,
            folder: session.folder().to_string(),
            protocol: session.get("Proto").to_string(),
            host: session.get("Host").to_string(),
            port: session.port(),
            username: session.get("Username").to_string(),
            profile: session.get("PuttySession").to_string(),
            color: session.get("Color").to_string(),
            icon: session.get("ImageKey").to_string(),
            highlight_rules: session.get("HighlightRules").to_string(),
            extra_args: mask_password(session.get("ExtraArgs")),
            jump_host: session.get("JumpHost").to_string(),
            script: session.get("SPSLFileName").to_string(),
            note: session.get("Note").to_string(),
            local_path: session.get("LocalPath").to_string(),
            remote_path: session.get("RemotePath").to_string(),
            rdp_options: session.get("RdpOptions").to_string(),
        }
    }

    /// The session's id: its folder and name
    pub fn id(&self) -> String {
        let folder = self.folder.trim_matches('/');
        if folder.is_empty() { self.name.trim().to_string() } else { format!("{folder}/{}", self.name.trim()) }
    }

    /// Write the fields into a session, putting the stored password back where the mask still stands
    pub fn apply(&self, session: &mut StoredSession, stored_extra_args: &str) {
        let id = self.id();
        session.set("SessionId", &id);
        session.set("SessionName", self.name.trim());
        session.set("Proto", if self.protocol.is_empty() { "SSH" } else { &self.protocol });
        session.set("Host", self.host.trim());
        session.set("Port", &self.port.to_string());
        session.set("Username", self.username.trim());
        session.set("PuttySession", if self.profile.trim().is_empty() { "Default Settings" } else { self.profile.trim() });
        session.set("Color", self.color.trim());
        session.set("ImageKey", self.icon.trim());
        session.set("HighlightRules", self.highlight_rules.trim());
        session.set("ExtraArgs", &unmask_password(&self.extra_args, stored_extra_args));
        session.set("JumpHost", self.jump_host.trim());
        session.set("SPSLFileName", self.script.trim());
        session.set("Note", &self.note);
        session.set("LocalPath", self.local_path.trim());
        session.set("RemotePath", self.remote_path.trim());
        session.set("RdpOptions", self.rdp_options.trim());
    }
}

/// The fields as a Database row, a masked `-pw` put back from the stored session (`stored`, None for a new one)
pub fn to_row(fields: &SessionFields, stored: Option<&StoredSession>) -> crate::db::Row {
    let stored_extra = stored.map(|s| s.get("ExtraArgs")).unwrap_or("");
    crate::db::Row {
        folder: fields.folder.trim_matches('/').to_string(),
        name: fields.name.trim().to_string(),
        protocol: if fields.protocol.is_empty() { "SSH".into() } else { fields.protocol.clone() },
        host: fields.host.trim().to_string(),
        port: fields.port as i32,
        username: fields.username.trim().to_string(),
        putty_profile: if fields.profile.trim().is_empty() { "Default Settings".into() } else { fields.profile.trim().to_string() },
        extra_args: unmask_password(&fields.extra_args, stored_extra),
        icon: fields.icon.trim().to_string(),
        notes: fields.note.clone(),
        spsl_script: fields.script.trim().to_string(),
        rdp_options: fields.rdp_options.trim().to_string(),
        local_path: fields.local_path.trim().to_string(),
        remote_path: fields.remote_path.trim().to_string(),
        color: fields.color.trim().to_string(),
        jump_host: fields.jump_host.trim().to_string(),
        highlight_rules: fields.highlight_rules.trim().to_string(),
    }
}

/// Save a session: `original_id` is the one it's stored under, None for a new session. Returns its id.
pub fn save(path: &std::path::Path, original_id: Option<&str>, fields: &SessionFields, ids_elsewhere: &[String]) -> Result<String, String> {
    let name = fields.name.trim();
    if name.is_empty() {
        return Err("A session needs a name.".into());
    }
    if name.contains('/') {
        return Err("A session's name can't contain \"/\": that separates folders.".into());
    }
    let id = fields.id();
    sessions::update_file(path, |list| {
        let taken = |other: &str| other.eq_ignore_ascii_case(&id) && original_id.is_none_or(|o| !o.eq_ignore_ascii_case(other));
        if list.iter().any(|s| taken(s.id())) || ids_elsewhere.iter().any(|o| taken(o)) {
            return Err(format!("There's already a session called {id}."));
        }
        match original_id {
            Some(original) => {
                let index = list.iter().position(|s| s.id() == original).ok_or("The session is no longer in its file.")?;
                let mut session = list.remove(index);
                let stored = session.get("ExtraArgs").to_string();
                fields.apply(&mut session, &stored);
                // a renamed session moves to its new place in the sort; an unchanged id stays where it was
                if session.id() == original {
                    list.insert(index, session);
                } else {
                    sessions::insert_sorted(list, session);
                }
            }
            None => {
                let mut session = StoredSession::new(&id);
                fields.apply(&mut session, "");
                sessions::insert_sorted(list, session);
            }
        }
        Ok(())
    })?;
    Ok(id)
}

/// Remove a session from its file
pub fn delete(path: &std::path::Path, id: &str) -> Result<(), String> {
    sessions::update_file(path, |list| {
        let before = list.len();
        list.retain(|s| s.id() != id);
        if list.len() == before { Err("The session is no longer in its file.".into()) } else { Ok(()) }
    })
}

/// Extra arguments with a `-pw` value replaced by the mask
fn mask_password(extra: &str) -> String {
    replace_password(extra, |_| MASK.to_string())
}

/// Extra arguments with the mask replaced by the password from the stored arguments (a typed password is kept)
pub(crate) fn unmask_password(edited: &str, stored: &str) -> String {
    let mut stored_password = None;
    replace_password(stored, |p| {
        stored_password = Some(p.to_string());
        p.to_string()
    });
    replace_password(edited, |p| if p == MASK { stored_password.clone().unwrap_or_default() } else { p.to_string() })
}

/// Rewrite the word after `-pw`, keeping the rest of the text exactly as it was
pub(crate) fn replace_password(extra: &str, mut with: impl FnMut(&str) -> String) -> String {
    let mut out = String::new();
    let mut rest = extra;
    while let Some(at) = rest.find("-pw") {
        let before_ok = at == 0 || rest[..at].ends_with(char::is_whitespace);
        let after = &rest[at + 3..];
        if !before_ok || !after.starts_with(char::is_whitespace) {
            out.push_str(&rest[..at + 3]);
            rest = after;
            continue;
        }
        let spaces = after.len() - after.trim_start().len();
        let value_start = at + 3 + spaces;
        let value = &rest[value_start..];
        let quoted = value.starts_with('"');
        let len = if quoted {
            value[1..].find('"').map(|e| e + 2).unwrap_or(value.len())
        } else {
            value.find(char::is_whitespace).unwrap_or(value.len())
        };
        let password = if quoted { value[1..len.saturating_sub(1).max(1)].to_string() } else { value[..len].to_string() };
        out.push_str(&rest[..value_start]);
        let new = with(&password);
        if quoted || new.contains(' ') { out.push_str(&format!("\"{new}\"")) } else { out.push_str(&new) }
        rest = &value[len..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_never_sees_the_password_and_saving_keeps_it() {
        let stored = "-pw cisco -P 2222";
        let shown = mask_password(stored);
        assert_eq!(shown, "-pw •••••• -P 2222");
        assert_eq!(unmask_password(&shown, stored), stored);
        assert_eq!(unmask_password("-pw newone -P 2222", stored), "-pw newone -P 2222");
        assert_eq!(unmask_password("-P 2222", stored), "-P 2222");
    }

    #[test]
    fn saving_renames_and_refuses_duplicates() {
        let dir = std::env::temp_dir().join(format!("sxt-edit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Sessions.xml");
        let fields = SessionFields { name: "sw1".into(), folder: "Lab".into(), host: "10.0.0.1".into(), ..Default::default() };
        assert_eq!(save(&path, None, &fields, &[]).unwrap(), "Lab/sw1");
        assert!(save(&path, None, &fields, &[]).unwrap_err().contains("already"));
        let renamed = SessionFields { name: "sw2".into(), ..fields.clone() };
        assert_eq!(save(&path, Some("Lab/sw1"), &renamed, &[]).unwrap(), "Lab/sw2");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("SessionId=\"Lab/sw2\" SessionName=\"sw2\""), "{text}");
        delete(&path, "Lab/sw2").unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("sw2"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
