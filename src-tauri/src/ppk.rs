//! PuTTY's private keys (.ppk) for OpenSSH, which can't read them.
//!
//! SuperXPuTTY's sessions and profiles name .ppk key files, and teams share them. `launch.rs` hands ssh an OpenSSH
//! copy instead: [`openssh_copy`] converts the key once into the user's folder (`keys/`, readable by the user only)
//! and returns that file, for `ssh -i`. Keys without a passphrase (PuTTY's "Encryption: none", format 2 or 3) are
//! converted; a key with a passphrase is left for the user to convert with puttygen, as before.

use std::path::{Path, PathBuf};

use base64::Engine;

/// The OpenSSH copy of the .ppk key at `path`, converted now if it isn't there or the key has changed
///
/// Err says why it can't be used (a passphrase, a key type OpenSSH doesn't take, a damaged file), for the tab's notice.
pub fn openssh_copy(path: &Path) -> Result<PathBuf, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("Couldn't read the key file {}: {e}", path.display()))?;
    let key = to_openssh(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let folder = crate::sessions::user_folder().join("keys");
    std::fs::create_dir_all(&folder).map_err(|e| format!("Couldn't make {}: {e}", folder.display()))?;
    let copy = folder.join(copy_name(path));
    if std::fs::read_to_string(&copy).ok().as_deref() != Some(key.as_str()) {
        write_private(&copy, &key).map_err(|e| format!("Couldn't write {}: {e}", copy.display()))?;
    }
    // every time, so a copy written before (or whose folder's permissions changed) is put right too
    owner_only(&copy).map_err(|e| format!("Couldn't make {} private: {e}", copy.display()))?;
    Ok(copy)
}

/// The copy's file name: the key's own name and a short hash of its full path, so two keys called key.ppk in
/// different folders don't share a copy
fn copy_name(path: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy().to_lowercase().hash(&mut hasher);
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "key".into());
    format!("{stem}-{:08x}.key", hasher.finish() as u32)
}

/// Write a private key that only the user can read: ssh refuses one that others can
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        file.write_all(text.as_bytes())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, text)?;
        owner_only(path)
    }
}

/// Let only the user read a key file. Windows' OpenSSH refuses a key that anyone else can (an entry inherited from
/// the folder is enough: "UNPROTECTED PRIVATE KEY FILE"), and ssh then quietly tries other keys, so the file's
/// inherited entries are removed and the user alone is given access, with icacls.
#[cfg(windows)]
fn owner_only(path: &Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    let user = match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(domain), Ok(name)) if !domain.is_empty() => format!("{domain}\\{name}"),
        (_, Ok(name)) => name,
        _ => return Err(std::io::Error::other("no user name")),
    };
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = std::process::Command::new("icacls")
        .arg(path)
        .args(["/inheritance:r", "/grant:r", &format!("{user}:F")])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(String::from_utf8_lossy(&output.stdout).trim().to_string()))
    }
}

/// On the Mac the file is written with mode 600 (`write_private`); nothing more to do
#[cfg(not(windows))]
fn owner_only(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Convert a .ppk file's text (format 2 or 3, no passphrase) into an OpenSSH private key ("BEGIN OPENSSH PRIVATE KEY")
///
/// Err says why it can't: a passphrase, an unknown key type or a damaged file.
pub fn to_openssh(ppk: &str) -> Result<String, String> {
    let mut lines = ppk.lines().map(|l| l.trim_end_matches('\r'));
    let first = lines.next().unwrap_or_default();
    let key_type = ["PuTTY-User-Key-File-2: ", "PuTTY-User-Key-File-3: "]
        .iter()
        .find_map(|prefix| first.strip_prefix(prefix))
        .ok_or("not a PuTTY key file (format 2 or 3)")?
        .trim()
        .to_string();
    let (mut encryption, mut comment) = (String::new(), String::new());
    let (mut public, mut private) = (None, None);
    while let Some(line) = lines.next() {
        let Some((name, value)) = line.split_once(": ") else { continue };
        match name {
            "Encryption" => encryption = value.trim().to_string(),
            "Comment" => comment = value.to_string(),
            "Public-Lines" | "Private-Lines" => {
                let count: usize = value.trim().parse().map_err(|_| format!("a damaged key file ({name})"))?;
                let joined: String = lines.by_ref().take(count).collect();
                let bytes = base64::engine::general_purpose::STANDARD.decode(joined.trim()).map_err(|_| format!("a damaged key file ({name})"))?;
                if name == "Public-Lines" { public = Some(bytes) } else { private = Some(bytes) }
            }
            _ => {}
        }
    }
    if encryption != "none" {
        return Err("the key has a passphrase; convert it with: puttygen key.ppk -O private-openssh -o key".into());
    }
    let public = public.ok_or("a damaged key file (no public key)")?;
    let private = private.ok_or("a damaged key file (no private key)")?;
    let pub_fields = fields(&public);
    let priv_fields = fields(&private);

    // the private key as OpenSSH lists it: the type, the public parts, then the private ones
    let mut body = Vec::new();
    put_string(&mut body, key_type.as_bytes());
    if key_type == "ssh-ed25519" {
        let (a, k) = (field(&pub_fields, 1)?, field(&priv_fields, 0)?);
        put_string(&mut body, a);
        put_string(&mut body, &[k, a].concat());
    } else if key_type == "ssh-rsa" {
        // PuTTY: e, n public and d, p, q, iqmp private; OpenSSH: n, e, d, iqmp, p, q
        let (e, n) = (field(&pub_fields, 1)?, field(&pub_fields, 2)?);
        for part in [n, e, field(&priv_fields, 0)?, field(&priv_fields, 3)?, field(&priv_fields, 1)?, field(&priv_fields, 2)?] {
            put_string(&mut body, part);
        }
    } else if key_type.starts_with("ecdsa-sha2-") {
        for part in [field(&pub_fields, 1)?, field(&pub_fields, 2)?, field(&priv_fields, 0)?] {
            put_string(&mut body, part);
        }
    } else if key_type == "ssh-dss" {
        for part in [field(&pub_fields, 1)?, field(&pub_fields, 2)?, field(&pub_fields, 3)?, field(&pub_fields, 4)?, field(&priv_fields, 0)?] {
            put_string(&mut body, part);
        }
    } else {
        return Err(format!("{key_type} keys can't be converted for OpenSSH yet"));
    }
    put_string(&mut body, comment.as_bytes());

    // the private section: the same check number twice, the key, and padding to a whole block of 8
    let check = public.iter().fold(0u32, |sum, b| sum.rotate_left(5) ^ u32::from(*b));
    let mut section = Vec::new();
    section.extend(check.to_be_bytes());
    section.extend(check.to_be_bytes());
    section.extend(body);
    let mut pad = 1u8;
    while section.len() % 8 != 0 {
        section.push(pad);
        pad += 1;
    }

    // no cipher and no key derivation: the key has no passphrase, as the .ppk had none
    let mut blob = b"openssh-key-v1\0".to_vec();
    put_string(&mut blob, b"none");
    put_string(&mut blob, b"none");
    put_string(&mut blob, b"");
    blob.extend(1u32.to_be_bytes());
    put_string(&mut blob, &public);
    put_string(&mut blob, &section);

    let encoded = base64::engine::general_purpose::STANDARD.encode(&blob);
    let mut out = String::from("-----BEGIN OPENSSH PRIVATE KEY-----\n");
    for chunk in encoded.as_bytes().chunks(70) {
        out.push_str(std::str::from_utf8(chunk).unwrap_or_default());
        out.push('\n');
    }
    out.push_str("-----END OPENSSH PRIVATE KEY-----\n");
    Ok(out)
}

/// The length-prefixed parts of an SSH wire blob (strings and mpints alike); stops at the first one that doesn't fit
fn fields(blob: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut rest = blob;
    while rest.len() >= 4 {
        let length = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
        if rest.len() < 4 + length {
            break;
        }
        out.push(&rest[4..4 + length]);
        rest = &rest[4 + length..];
    }
    out
}

/// Part `i` of a blob's parts, or Err for a file that's missing it
fn field<'a>(list: &[&'a [u8]], i: usize) -> Result<&'a [u8], String> {
    list.get(i).copied().ok_or_else(|| "a damaged key file (a part is missing)".to_string())
}

/// Append an SSH string: its length (4 bytes, big-endian), then its bytes
fn put_string(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend((bytes.len() as u32).to_be_bytes());
    out.extend(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up Ed25519 key in .ppk form (public bytes 1 to 32, private 33 to 64; no passphrase, no real MAC)
    const ED25519: &str = "PuTTY-User-Key-File-3: ssh-ed25519\nEncryption: none\nComment: test\nPublic-Lines: 2\nAAAAC3NzaC1lZDI1NTE5AAAAIAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwd\nHh8g\nPrivate-Lines: 1\nAAAAICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj9A\nPrivate-MAC: 00\n";

    #[test]
    fn converts_an_ed25519_key() {
        let pem = to_openssh(ED25519).unwrap();
        assert!(pem.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----\n"));
        let body: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
        let blob = base64::engine::general_purpose::STANDARD.decode(body).unwrap();
        assert!(blob.starts_with(b"openssh-key-v1\0"));
        // after the magic: the cipher and key derivation ("none", "none"), their options (""), then one key
        let head = fields(&blob[15..35]);
        assert_eq!(head, [b"none".as_slice(), b"none", b""]);
        assert_eq!(blob[35..39], 1u32.to_be_bytes());
        let outer = fields(&blob[39..]);
        // the private section: two equal check numbers, the type, the public key, private ++ public, the comment
        let section = outer[1];
        assert_eq!(section[0..4], section[4..8]);
        assert_eq!(section.len() % 8, 0);
        let parts = fields(&section[8..]);
        let public: Vec<u8> = (1..=32).collect();
        let private: Vec<u8> = (33..=64).collect();
        assert_eq!(parts[0], b"ssh-ed25519");
        assert_eq!(parts[1], public.as_slice());
        assert_eq!(parts[2], [private, public].concat().as_slice());
        assert_eq!(parts[3], b"test");
    }

    #[test]
    fn refuses_a_key_with_a_passphrase() {
        let locked = ED25519.replace("Encryption: none", "Encryption: aes256-cbc");
        assert!(to_openssh(&locked).unwrap_err().contains("passphrase"));
    }

    #[test]
    fn refuses_other_files() {
        assert!(to_openssh("-----BEGIN OPENSSH PRIVATE KEY-----\n").is_err());
    }

    /// Real keys: each .ppk in SXT_PPK_DIR (made with puttygen, no passphrase) is written beside it as .key, for
    /// `ssh-keygen -y -f` to read back and compare with puttygen's own public key:
    /// `cargo test real_keys -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_keys() {
        let folder = std::env::var("SXT_PPK_DIR").expect("SXT_PPK_DIR");
        for entry in std::fs::read_dir(folder).unwrap().flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "ppk") {
                let key = to_openssh(&std::fs::read_to_string(&path).unwrap()).unwrap();
                write_private(&path.with_extension("key"), &key).unwrap();
                println!("converted {}", path.display());
            }
        }
    }
}
