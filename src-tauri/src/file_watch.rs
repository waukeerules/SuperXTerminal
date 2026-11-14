//! Whether the session files changed since the app last read them, without reading them to find out.
//!
//! The Sessions tree asks every few seconds (Settings > General's Seconds between checks), so that a teammate's save
//! to the shared folder, SuperXPuTTY or an editor shows by itself. The files are compared by their contents, hashed,
//! as SuperXPuTTY's SessionsFileWatcher does: a file written again with the same contents isn't a change. But a hash
//! is only had by reading the file, and the settings folder is usually a network drive: every check fetched every
//! file whole, megabytes with thousands of sessions.
//!
//! So each check first asks only for the files' sizes and when they were last changed ([`stamps`]), a small question
//! each. While those are as they were, nothing is read. When one differs the files are read and hashed, and only a
//! different hash is a change; the same hash (a file saved unchanged) is remembered with its new time, so it isn't
//! read again next check. And every [`FULL_CHECK`] the files are read whatever their times say: some shares and
//! synced folders keep times coarsely, and a change within the same second that leaves the size as it was would
//! otherwise never show.
//!
//! When the files are read for the tree, the hash is taken from those same bytes ([`Reading`]), not from a second
//! read of them.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// How long the files' sizes and times are trusted before the files are read again all the same
pub const FULL_CHECK: Duration = Duration::from_secs(60);

/// A file's size and when it was last changed; None when it isn't there, or can't be reached
pub type Stamp = Option<(u64, SystemTime)>;

/// The sizes and times of `files`, in their order: one small question each, nothing read
pub fn stamps(files: &[PathBuf]) -> Vec<Stamp> {
    files.iter().map(|file| std::fs::metadata(file).ok().and_then(|m| Some((m.len(), m.modified().ok()?)))).collect()
}

/// The hash of one file's contents; of nothing, for a file that isn't there or can't be read
pub fn file_hash(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// The files' contents as one number: their hashes ([`file_hash`]) in the files' order
pub fn fingerprint(hashes: impl Iterator<Item = u64>) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for hash in hashes {
        hash.hash(&mut hasher);
    }
    hasher.finish()
}

/// A file's contents, or nothing when it isn't there or can't be read (a settings folder that's away): how
/// [`Seen::changed`] reads the files
pub fn read_or_nothing(file: &Path) -> Vec<u8> {
    std::fs::read(file).unwrap_or_default()
}

/// The session files as the app last knew them
pub struct Seen {
    /// Their contents ([`fingerprint`]); 0 before they've been read
    hash: u64,
    /// Their sizes and times when that was taken; empty before they've been read
    stamps: Vec<Stamp>,
    /// When their contents were last hashed, for [`FULL_CHECK`]
    hashed: Instant,
}

impl Seen {
    /// Nothing read yet: the first check says they changed
    pub fn nothing() -> Seen {
        Seen { hash: 0, stamps: Vec::new(), hashed: Instant::now() }
    }

    /// The files as just read: their contents' `hash`, and the `stamps` taken before reading them (so a change made
    /// while they were read shows at the next check)
    pub fn read(hash: u64, stamps: Vec<Stamp>, now: Instant) -> Seen {
        Seen { hash, stamps, hashed: now }
    }

    /// Whether `files` changed since they were read. Nothing is read (`read` isn't called) while their sizes and
    /// times are as they were and it's less than [`FULL_CHECK`] since their contents were last looked at. Files
    /// that turn out the same are remembered with their new times; changed ones are left for the reading that
    /// follows to remember ([`Seen::read`]), so asking again says changed again until then.
    pub fn changed(&mut self, files: &[PathBuf], now: Instant, read: impl Fn(&Path) -> Vec<u8>) -> bool {
        let stamps = stamps(files);
        if stamps == self.stamps && now.saturating_duration_since(self.hashed) < FULL_CHECK {
            return false;
        }
        let hash = fingerprint(files.iter().map(|file| file_hash(&read(file))));
        if hash != self.hash {
            return true;
        }
        self.stamps = stamps;
        self.hashed = now;
        false
    }
}

/// Reads files for the loaders and keeps each one's hash, so the files read for the tree needn't be read again to
/// know their contents by ([`Reading::fingerprint`])
#[derive(Default)]
pub struct Reading {
    /// The hash of each file read
    hashes: RefCell<HashMap<PathBuf, u64>>,
}

impl Reading {
    /// A file's text, as `std::fs::read_to_string` gives it, its hash kept
    pub fn text(&self, path: &Path) -> std::io::Result<String> {
        let bytes = std::fs::read(path)?;
        self.hashes.borrow_mut().insert(path.to_path_buf(), file_hash(&bytes));
        String::from_utf8(bytes).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// The contents of `files` as one number, as [`Seen::changed`] would find them: a file that wasn't read (it
    /// isn't there, the settings folder is away) counts as nothing, which is what reading it then would give
    pub fn fingerprint(&self, files: &[PathBuf]) -> u64 {
        let hashes = self.hashes.borrow();
        fingerprint(files.iter().map(|file| hashes.get(file).copied().unwrap_or_else(|| file_hash(&[]))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Two files in a folder of their own for one test, and the folder
    fn two_files(name: &str) -> (PathBuf, Vec<PathBuf>) {
        let folder = std::env::temp_dir().join(format!("sxt-file-watch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let files = vec![folder.join("Sessions.xml"), folder.join("Folders.xml")];
        std::fs::write(&files[0], "<ArrayOfSessionData />").unwrap();
        std::fs::write(&files[1], "<Folders />").unwrap();
        (folder, files)
    }

    /// The files as the app would know them after reading them now
    fn seen(files: &[PathBuf], now: Instant) -> Seen {
        Seen::read(fingerprint(files.iter().map(|f| file_hash(&read_or_nothing(f)))), stamps(files), now)
    }

    /// Say a file was last changed at `time`
    fn set_time(file: &Path, time: SystemTime) {
        std::fs::File::options().write(true).open(file).unwrap().set_modified(time).unwrap();
    }

    /// A time well before now, apart from any a file gets by being written: file systems keep times more or less
    /// finely
    fn earlier() -> SystemTime {
        SystemTime::now() - Duration::from_secs(100)
    }

    #[test]
    fn files_that_look_as_they_did_arent_read() {
        let (folder, files) = two_files("unread");
        let now = Instant::now();
        let mut seen = seen(&files, now);
        let reads = Cell::new(0);
        let counted = |file: &Path| {
            reads.set(reads.get() + 1);
            read_or_nothing(file)
        };
        for check in 1..=5 {
            assert!(!seen.changed(&files, now + Duration::from_secs(10 * check), counted));
        }
        assert_eq!(reads.get(), 0);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_file_saved_unchanged_is_read_once_and_isnt_a_change_and_a_changed_one_is() {
        let (folder, files) = two_files("saved");
        let now = Instant::now();
        let mut seen = seen(&files, now);
        let reads = Cell::new(0);
        let counted = |file: &Path| {
            reads.set(reads.get() + 1);
            read_or_nothing(file)
        };
        // written again with the same contents (an editor's Save): looked at, both files, and not a change
        std::fs::write(&files[0], "<ArrayOfSessionData />").unwrap();
        set_time(&files[0], earlier());
        assert!(!seen.changed(&files, now, counted));
        assert_eq!(reads.get(), 2);
        // and not looked at again
        assert!(!seen.changed(&files, now, counted));
        assert_eq!(reads.get(), 2);

        // a teammate's save: a change, and still one until the files are read for the tree
        std::fs::write(&files[0], "<ArrayOfSessionData><SessionData SessionId=\"Lab/sw1\" /></ArrayOfSessionData>").unwrap();
        assert!(seen.changed(&files, now, counted));
        assert!(seen.changed(&files, now, counted));
        // a file going (the settings folder away) is one too
        let mut seen = super::tests::seen(&files, now);
        std::fs::remove_file(&files[1]).unwrap();
        assert!(seen.changed(&files, now, counted));
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_change_the_size_and_time_dont_show_is_found_at_the_full_check() {
        let (folder, files) = two_files("coarse");
        let saved_at = earlier();
        set_time(&files[0], saved_at);
        let now = Instant::now();
        let mut seen = seen(&files, now);
        // other contents of the same length, the time put back: as a share that keeps times coarsely would have it
        std::fs::write(&files[0], "<ArrayOfSessionDatb />").unwrap();
        set_time(&files[0], saved_at);
        assert!(!seen.changed(&files, now + Duration::from_secs(50), read_or_nothing));
        assert!(seen.changed(&files, now + FULL_CHECK, read_or_nothing));
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn files_read_for_the_tree_are_known_by_those_bytes() {
        let (folder, files) = two_files("reading");
        let reading = Reading::default();
        assert_eq!(reading.text(&files[0]).unwrap(), "<ArrayOfSessionData />");
        // Folders.xml isn't there to read: nothing, as a check would find it
        std::fs::remove_file(&files[1]).unwrap();
        assert!(reading.text(&files[1]).is_err());
        let now = Instant::now();
        let mut seen = Seen::read(reading.fingerprint(&files), stamps(&files), now);
        // a check that does read them finds what the reading found
        assert!(!seen.changed(&files, now + FULL_CHECK, read_or_nothing));
        // nothing read yet: changed, so the tree reads them
        assert!(Seen::nothing().changed(&files, now, read_or_nothing));
        std::fs::remove_dir_all(folder).unwrap();
    }
}
