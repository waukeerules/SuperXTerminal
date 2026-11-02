//! Changes to Database sessions that haven't reached the table yet, as SuperXPuTTY's DatabaseOutbox: made while it
//! couldn't be reached (waiting, sent in order before the next read), or refused when sent (someone else's change,
//! a lock, a name clash; left for the user under Pending changes). They're shown in the tree on top of what was read.
//! Kept per connection (its Source) in DatabaseSessions.outbox.json in the user's folder, so they survive a restart.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::db::{self, Row};
use crate::sessions::StoredSession;

/// What a change does, as SuperXPuTTY's ChangeKind
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Insert,
    Update,
    Delete,
    RenameFolder,
    DeleteFolder,
}

/// One change, as SuperXPuTTY's PendingChange
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingChange {
    pub id: String,
    pub kind: Kind,
    /// Insert and Update: the session's values, folder included
    #[serde(default)]
    pub row: Option<Row>,
    /// Update and Delete: the row's id, or "pending:<change id>" for a session still waiting to be added
    #[serde(default)]
    pub row_id: String,
    /// Update: the row's version when it was read, so someone else's change in between is noticed
    #[serde(default)]
    pub version: String,
    /// The session's path (Delete) or folder (RenameFolder, DeleteFolder), for the list
    #[serde(default)]
    pub from: String,
    /// RenameFolder: the new path
    #[serde(default)]
    pub to: String,
    /// When it was made, "HH:MM"
    pub queued_at: String,
    /// Why the table refused it; None while it's waiting
    #[serde(default)]
    pub error: Option<String>,
    /// Update: save over someone else's change (Overwrite theirs)
    #[serde(default)]
    pub force: bool,
}

impl PendingChange {
    /// A new change of `kind`, with a fresh id
    pub fn new(kind: Kind) -> PendingChange {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        PendingChange {
            id: format!("{nanos:x}-{}", COUNTER.fetch_add(1, Ordering::Relaxed)),
            kind,
            row: None,
            row_id: String::new(),
            version: String::new(),
            from: String::new(),
            to: String::new(),
            queued_at: chrono::Local::now().format("%H:%M").to_string(),
            error: None,
            force: false,
        }
    }

    /// What it does, for the list, as SuperXPuTTY's ToString: "Add Lab/sw1", "Rename folder Lab to Labs"
    pub fn describe(&self) -> String {
        let path = |r: &Row| if r.folder.is_empty() { r.name.clone() } else { format!("{}/{}", r.folder, r.name) };
        match self.kind {
            Kind::Insert => format!("Add {}", self.row.as_ref().map(path).unwrap_or_default()),
            Kind::Update => format!("Change {}", self.row.as_ref().map(path).unwrap_or_default()),
            Kind::Delete => format!("Delete {}", self.from),
            Kind::RenameFolder => format!("Rename folder {} to {}", self.from, self.to),
            Kind::DeleteFolder => format!("Remove folder {}", self.from),
        }
    }
}

/// One connection's changes
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Outbox {
    source: String,
    changes: Vec<PendingChange>,
}

/// The file: every connection's changes
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OutboxFile {
    outboxes: Vec<Outbox>,
}

/// The file's path in the user's folder
fn file(user_folder: &Path) -> PathBuf {
    user_folder.join("DatabaseSessions.outbox.json")
}

fn load(user_folder: &Path) -> OutboxFile {
    std::fs::read_to_string(file(user_folder)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Write it in one step (removed when nothing is left), so a crash never leaves half a file
fn save(user_folder: &Path, mut outbox: OutboxFile) -> Result<(), String> {
    let path = file(user_folder);
    outbox.outboxes.retain(|o| !o.changes.is_empty());
    if outbox.outboxes.is_empty() {
        return match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        };
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_string_pretty(&outbox).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, &path).map_err(|e| e.to_string())
}

/// Change one connection's list and save it
fn edit(user_folder: &Path, source: &str, change: impl FnOnce(&mut Vec<PendingChange>)) -> Result<(), String> {
    let mut outbox = load(user_folder);
    let index = match outbox.outboxes.iter().position(|o| o.source == source) {
        Some(i) => i,
        None => {
            outbox.outboxes.push(Outbox { source: source.to_string(), changes: Vec::new() });
            outbox.outboxes.len() - 1
        }
    };
    change(&mut outbox.outboxes[index].changes);
    save(user_folder, outbox)
}

/// A connection's changes, oldest first
pub fn changes(user_folder: &Path, source: &str) -> Vec<PendingChange> {
    load(user_folder).outboxes.into_iter().find(|o| o.source == source).map(|o| o.changes).unwrap_or_default()
}

/// How many are waiting, and how many were refused
pub fn count(changes: &[PendingChange]) -> (usize, usize) {
    let failed = changes.iter().filter(|c| c.error.is_some()).count();
    (changes.len() - failed, failed)
}

/// Queue a change, folding it into earlier ones as SuperXPuTTY's Add: a change to a session still waiting to be added
/// changes that addition (a delete drops it); a second edit replaces the first but keeps the version it was read at;
/// a delete drops the row's waiting edits
pub fn add(user_folder: &Path, source: &str, mut change: PendingChange) -> Result<(), String> {
    edit(user_folder, source, |changes| {
        if let Some(insert_id) = change.row_id.strip_prefix("pending:")
            && let Some(i) = changes.iter().position(|c| c.kind == Kind::Insert && c.id == insert_id) {
                if change.kind == Kind::Delete {
                    changes.remove(i);
                    return;
                }
                if change.kind == Kind::Update {
                    changes[i].row = change.row.take();
                    changes[i].error = None;
                    return;
                }
            }
        if change.kind == Kind::Update && !change.row_id.is_empty()
            && let Some(earlier) = changes.iter_mut().find(|c| c.kind == Kind::Update && c.row_id == change.row_id) {
                earlier.row = change.row.take();
                earlier.error = None;
                return;
            }
        if change.kind == Kind::Delete && !change.row_id.is_empty() {
            changes.retain(|c| !(c.kind == Kind::Update && c.row_id == change.row_id));
        }
        changes.push(change);
    })
}

/// Drop a change (sent, or discarded)
pub fn remove(user_folder: &Path, source: &str, id: &str) -> Result<(), String> {
    edit(user_folder, source, |changes| changes.retain(|c| c.id != id))
}

/// Mark why a change was refused, or clear it (None) to send it again; `force` saves an edit over someone else's
pub fn set_error(user_folder: &Path, source: &str, id: &str, error: Option<String>, force: bool) -> Result<(), String> {
    edit(user_folder, source, |changes| {
        if let Some(c) = changes.iter_mut().find(|c| c.id == id) {
            c.error = error;
            c.force |= force;
        }
    })
}

/// Whether a folder path is `root` or below it
fn in_folder(folder: &str, root: &str) -> bool {
    folder == root || folder.starts_with(&format!("{root}/"))
}

/// The sessions as read, with the changes on top, as SuperXPuTTY's Apply; changed sessions are marked
/// (DatabasePending, with a note for the tooltip)
pub fn apply(server: &[StoredSession], changes: &[PendingChange]) -> Vec<StoredSession> {
    let mut sessions = server.to_vec();
    for change in changes {
        let note = match &change.error {
            None => format!("Waiting to be saved to the database (queued {})", change.queued_at),
            Some(e) => format!("Not saved to the database: {e} (Database sessions > Pending changes)"),
        };
        let mark = |mut s: StoredSession| {
            s.set("DatabasePending", "true");
            s.set("DatabasePendingNote", &note);
            s
        };
        match change.kind {
            Kind::Insert => {
                if let Some(row) = &change.row {
                    let added = mark(db::session_of(row, &format!("pending:{}", change.id), ""));
                    sessions.retain(|s| s.id() != added.id());
                    sessions.push(added);
                }
            }
            Kind::Update => {
                if let (Some(row), Some(i)) = (&change.row, sessions.iter().position(|s| s.get("DatabaseId") == change.row_id)) {
                    let old = &sessions[i];
                    let mut updated = db::session_of(row, &change.row_id, &change.version);
                    for keep in ["DatabaseLocked", "DatabaseLockedFields", "DatabaseManagedBy", "DatabaseUpdatedBy"] {
                        updated.set(keep, old.get(keep));
                    }
                    sessions[i] = mark(updated);
                }
            }
            Kind::Delete => sessions.retain(|s| s.get("DatabaseId") != change.row_id),
            Kind::RenameFolder => {
                for s in sessions.iter_mut() {
                    let folder = s.folder().to_string();
                    if in_folder(&folder, &change.from) {
                        let moved = format!("{}{}", change.to, &folder[change.from.len()..]);
                        let name = s.get("SessionName").to_string();
                        let id = if moved.is_empty() { format!("@db/{name}") } else { format!("@db/{moved}/{name}") };
                        s.set("SessionId", &id);
                        *s = mark(std::mem::take(s));
                    }
                }
            }
            Kind::DeleteFolder => sessions.retain(|s| !in_folder(s.folder(), &change.from)),
        }
    }
    sessions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(folder: &str, name: &str, host: &str) -> Row {
        Row { folder: folder.into(), name: name.into(), host: host.into(), protocol: "SSH".into(), ..Row::default() }
    }

    #[test]
    fn folds_changes_as_superxputty() {
        let folder = std::env::temp_dir().join(format!("sxt-outbox-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let source = "u@h:5432/db/s.t";

        // an edit to a session still waiting to be added changes the addition; a delete drops it
        let mut insert = PendingChange::new(Kind::Insert);
        insert.row = Some(row("Lab", "sw1", "10.0.0.1"));
        let pending_id = format!("pending:{}", insert.id);
        add(&folder, source, insert).unwrap();
        let mut edit = PendingChange::new(Kind::Update);
        edit.row_id = pending_id.clone();
        edit.row = Some(row("Lab", "sw1", "10.0.0.2"));
        add(&folder, source, edit).unwrap();
        let queued = changes(&folder, source);
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].row.as_ref().unwrap().host, "10.0.0.2");
        let shown = apply(&[], &queued);
        assert_eq!(shown[0].id(), "@db/Lab/sw1");
        assert_eq!(shown[0].get("DatabasePending"), "true");
        let mut delete = PendingChange::new(Kind::Delete);
        delete.row_id = pending_id;
        add(&folder, source, delete).unwrap();
        assert!(changes(&folder, source).is_empty());
        assert!(!file(&folder).exists(), "the file goes when nothing is left");

        // a second edit keeps the first one's version; a delete drops the row's edits
        let mut first = PendingChange::new(Kind::Update);
        first.row_id = "r1".into();
        first.version = "100".into();
        first.row = Some(row("Lab", "sw2", "a"));
        add(&folder, source, first).unwrap();
        let mut second = PendingChange::new(Kind::Update);
        second.row_id = "r1".into();
        second.version = "200".into();
        second.row = Some(row("Lab", "sw2", "b"));
        add(&folder, source, second).unwrap();
        let queued = changes(&folder, source);
        assert_eq!((queued.len(), queued[0].version.as_str(), queued[0].row.as_ref().unwrap().host.as_str()), (1, "100", "b"));
        let mut gone = PendingChange::new(Kind::Delete);
        gone.row_id = "r1".into();
        add(&folder, source, gone).unwrap();
        let queued = changes(&folder, source);
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].kind, Kind::Delete);

        // a folder rename moves the sessions shown; another connection's list is separate
        let mut server = db::session_of(&row("Lab/Core", "sw3", "c"), "r3", "1");
        server.set("SessionName", "sw3");
        let mut rename = PendingChange::new(Kind::RenameFolder);
        rename.from = "Lab".into();
        rename.to = "Labs".into();
        let shown = apply(&[server], &[rename]);
        assert_eq!(shown[0].id(), "@db/Labs/Core/sw3");
        assert!(changes(&folder, "other").is_empty());
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
