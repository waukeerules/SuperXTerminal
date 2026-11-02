//! Database sessions: sessions shared through a PostgreSQL table, as SuperXPuTTY's DatabaseSessions.
//!
//! Both apps use the same table, so Mac and Windows users see each other's changes. This module connects (as
//! application `SuperPuTTY`, so the table's guard trigger enforces locks for the Mac too; docs/PLAN.md §3), sets the
//! user's nickname for `updated_by`, reads the table in pages of 500, reads the folders table, and checks the change
//! version every few seconds. The last read is cached in SuperXPuTTY's own `DatabaseSessions.cache.xml` format,
//! so the sessions still open while the database is down. Changes are written as SuperXPuTTY writes them: an
//! update only applies if the row is unchanged since it was read (its `updated_at`), and the lock trigger's
//! refusals come back as plain messages.
//!
//! Each row becomes a session like those in Sessions.xml, with id `@db/<folder>/<name>` and SuperXPuTTY's
//! `Database*` attributes (`DatabaseVersion` is `updated_at` as .NET ticks, as SuperXPuTTY stores it).

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio_postgres::{Client, NoTls};

use crate::sessions::{self, FolderSetting, StoredSession};
use crate::variables;

/// The application name every connection uses: the guard trigger enforces locks only for this name
const APPLICATION_NAME: &str = "SuperPuTTY";

/// Rows read per query, as SuperXPuTTY's PageSize
const PAGE_SIZE: i64 = 500;

/// The Keychain account the database password is kept under
const KEYCHAIN_ACCOUNT: &str = "database";

/// .NET ticks (100 ns since 0001-01-01) at the Unix epoch, for DatabaseVersion
const TICKS_AT_UNIX_EPOCH: i64 = 621_355_968_000_000_000;

/// The connection settings, as SuperXPuTTY's Database sessions window; the password is in the Keychain
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DbSettings {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub schema: String,
    pub table: String,
    pub user: String,
    /// Disable, Allow, Prefer, Require, VerifyCA or VerifyFull, as Npgsql's SslMode
    pub ssl_mode: String,
    pub nickname: String,
    pub read_only: bool,
    /// Seconds between change checks; 0 for never
    pub check_seconds: u32,
    /// A password kept as a {key} placeholder (from an import) rather than in the Keychain
    pub password_placeholder: String,
}

impl Default for DbSettings {
    fn default() -> DbSettings {
        DbSettings {
            enabled: false,
            host: String::new(),
            port: 5432,
            database: "superputty".into(),
            schema: "superputty".into(),
            table: "sessions".into(),
            user: String::new(),
            ssl_mode: "Prefer".into(),
            nickname: String::new(),
            read_only: false,
            check_seconds: 30,
            password_placeholder: String::new(),
        }
    }
}

impl DbSettings {
    /// `"schema"."table"`, quoted, for SQL
    fn table(&self) -> String {
        format!("{}.{}", quote(&self.schema), quote(&self.table))
    }

    /// The folders table, `<table>_folders`
    fn folders_table(&self) -> String {
        format!("{}.{}", quote(&self.schema), quote(&format!("{}_folders", self.table)))
    }

    /// The change counter, `<table>_version`
    fn version_table(&self) -> String {
        format!("{}.{}", quote(&self.schema), quote(&format!("{}_version", self.table)))
    }

    /// Where the sessions come from, as the cache's Source: "user@host:port/database/schema.table"
    pub fn source(&self) -> String {
        format!("{}@{}:{}/{}/{}.{}", self.user, self.host, self.port, self.database, self.schema, self.table)
    }
}

/// An identifier quoted for SQL
fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// The database password: the Keychain's, else the settings' {key} placeholder resolved from Environment Variables
fn password(settings: &DbSettings, user_folder: &Path) -> Option<String> {
    let stored = keyring::Entry::new("SuperXTerminal", KEYCHAIN_ACCOUNT).ok().and_then(|e| e.get_password().ok());
    stored.filter(|p| !p.is_empty()).or_else(|| {
        (!settings.password_placeholder.is_empty())
            .then(|| variables::substitute(&settings.password_placeholder, &variables::values(user_folder)))
    })
}

/// Keep the database password in the Keychain, or forget it when empty
pub fn set_password(password: &str) -> Result<(), String> {
    let entry = keyring::Entry::new("SuperXTerminal", KEYCHAIN_ACCOUNT).map_err(|e| e.to_string())?;
    if password.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    } else {
        entry.set_password(password).map_err(|e| e.to_string())
    }
}

/// Whether a password is stored
pub fn has_password() -> bool {
    keyring::Entry::new("SuperXTerminal", KEYCHAIN_ACCOUNT).ok().and_then(|e| e.get_password().ok()).is_some_and(|p| !p.is_empty())
}

/// Connect as SuperXPuTTY does: application name SuperPuTTY, 8 s timeout, the chosen SSL mode, and the nickname.
/// `typed` is a password typed in the window but not saved yet (Test connection).
async fn connect(settings: &DbSettings, user_folder: &Path, typed: Option<&str>) -> Result<Client, String> {
    if settings.host.trim().is_empty() || settings.user.trim().is_empty() {
        return Err("Set the host and username in Database sessions first.".into());
    }
    let mut config = tokio_postgres::Config::new();
    config
        .host(settings.host.trim())
        .port(settings.port)
        .dbname(&settings.database)
        .user(settings.user.trim())
        .application_name(APPLICATION_NAME)
        .connect_timeout(Duration::from_secs(8));
    if let Some(password) = typed.filter(|p| !p.is_empty()).map(str::to_string).or_else(|| password(settings, user_folder)) {
        config.password(password);
    }
    let mode = settings.ssl_mode.to_ascii_lowercase();
    let client = if mode == "disable" {
        let (client, connection) = config.connect(NoTls).await.map_err(describe)?;
        tauri::async_runtime::spawn(connection);
        client
    } else {
        // Prefer and Allow fall back to plain; Require encrypts without checking the certificate, as Npgsql's
        // Require; VerifyCA and VerifyFull check it
        config.ssl_mode(match mode.as_str() {
            "allow" | "prefer" => tokio_postgres::config::SslMode::Prefer,
            _ => tokio_postgres::config::SslMode::Require,
        });
        let verify = mode == "verifyca" || mode == "verifyfull";
        let tls = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(!verify)
            .danger_accept_invalid_hostnames(mode != "verifyfull")
            .build()
            .map_err(|e| e.to_string())?;
        let (client, connection) = config.connect(postgres_native_tls::MakeTlsConnector::new(tls)).await.map_err(describe)?;
        tauri::async_runtime::spawn(connection);
        client
    };
    if !settings.nickname.trim().is_empty() {
        // read by the touch trigger for updated_by; a plain setting, so any login may set it
        client
            .execute("SELECT set_config('superputty.nickname', $1, false)", &[&settings.nickname.trim()])
            .await
            .map_err(describe)?;
    }
    Ok(client)
}

/// A Postgres error as a readable message, with its cause ("error connecting to server" alone says too little)
fn describe(error: tokio_postgres::Error) -> String {
    if let Some(db) = error.as_db_error() {
        return db.message().to_string();
    }
    let mut text = error.to_string();
    let mut cause = std::error::Error::source(&error);
    while let Some(inner) = cause {
        text.push_str(": ");
        text.push_str(&inner.to_string());
        cause = inner.source();
    }
    text
}

/// What a read gives: the sessions, the folders' settings, whether the user may write, and the change token
pub struct Fetched {
    pub sessions: Vec<StoredSession>,
    pub folders: Vec<FolderSetting>,
    pub writable: bool,
    pub token: String,
    pub optional_columns: Vec<String>,
}

/// The change token, as SuperXPuTTY's ReadChangeToken: the version table's number, else the row count and newest
/// change for a table made before it existed
async fn change_token(client: &Client, settings: &DbSettings) -> Result<String, String> {
    match client.query_one(&format!("SELECT version::text FROM {} LIMIT 1", settings.version_table()), &[]).await {
        Ok(row) => return Ok(format!("v:{}", row.get::<_, String>(0))),
        Err(e) if e.code().is_some_and(|c| c.code() == "42P01" || c.code() == "42501") => {}
        Err(e) => return Err(describe(e)),
    }
    let row = client
        .query_one(&format!("SELECT count(*)::text || '/' || coalesce(max(updated_at)::text, '0') FROM {}", settings.table()), &[])
        .await
        .map_err(describe)?;
    Ok(format!("c:{}", row.get::<_, String>(0)))
}

/// Read every visible row and the folders table, as SuperXPuTTY's FetchAll
pub async fn fetch_all(settings: &DbSettings, user_folder: &Path) -> Result<Fetched, String> {
    let client = connect(settings, user_folder, None).await?;
    // taken first: a change made while the pages are read then shows up at the next check
    let token = change_token(&client, settings).await?;
    let writable = !settings.read_only && {
        let table = format!("{}.{}", settings.schema, settings.table);
        client
            .query_one("SELECT has_table_privilege($1, 'INSERT') AND has_table_privilege($1, 'UPDATE')", &[&table])
            .await
            .map(|r| r.get::<_, bool>(0))
            .unwrap_or(false)
    };
    let present: Vec<String> = client
        .query(
            "SELECT column_name::text FROM information_schema.columns WHERE table_schema = $1 AND table_name = $2",
            &[&settings.schema, &settings.table],
        )
        .await
        .map_err(describe)?
        .iter()
        .map(|r| r.get(0))
        .collect();
    if present.is_empty() {
        return Err(format!("The table {} isn't there. Create it from Database sessions (Create / upgrade schema).", settings.table()));
    }
    let has = |c: &str| present.iter().any(|p| p == c);
    // columns added after the first schema read as their defaults until Create / upgrade schema adds them
    let or = |c: &str, fallback: &str| if has(c) { format!("{c}::text") } else { fallback.to_string() };
    let columns = [
        "id::text", "folder", "name", "protocol", "host", "port::text", "username", "putty_profile", "extra_args",
        "coalesce(icon, '')", "coalesce(notes, '')", "coalesce(spsl_script, '')", "coalesce(rdp_options, '')",
        "coalesce(local_path, '')", "coalesce(remote_path, '')",
        "(extract(epoch FROM updated_at) * 1000000)::bigint::text",
    ]
    .iter()
    .map(|c| c.to_string())
    .chain([
        or("locked", "'false'"),
        if has("locked_fields") { "array_to_string(locked_fields, ',')".into() } else { "''".into() },
        or("managed_by", "''"),
        or("updated_by", "''"),
        or("color", "''"),
        or("jump_host", "''"),
        or("highlight_rules", "''"),
    ])
    .map(|c| format!("coalesce({c}, '')"))
    .collect::<Vec<_>>()
    .join(", ");
    let visible = if has("deleted_at") { "deleted_at IS NULL" } else { "true" };

    let mut sessions = Vec::new();
    let mut last: Option<(String, String, String)> = None;
    loop {
        let rows = match &last {
            None => {
                client
                    .query(
                        &format!("SELECT {columns} FROM {} WHERE {visible} ORDER BY folder, name, id LIMIT $1", settings.table()),
                        &[&PAGE_SIZE],
                    )
                    .await
            }
            Some((folder, name, id)) => {
                client
                    .query(
                        &format!(
                            "SELECT {columns} FROM {} WHERE {visible} AND (folder, name, id) > ($1, $2, $3::text::uuid) \
                             ORDER BY folder, name, id LIMIT $4",
                            settings.table()
                        ),
                        &[folder, name, id, &PAGE_SIZE],
                    )
                    .await
            }
        }
        .map_err(describe)?;
        let read = rows.len() as i64;
        for row in &rows {
            let text = |i: usize| row.get::<_, String>(i);
            last = Some((text(1), text(2), text(0)));
            sessions.push(to_session(&(0..23).map(text).collect::<Vec<_>>()));
        }
        if read < PAGE_SIZE {
            break;
        }
    }

    let folders = fetch_folders(&client, settings).await;
    let optional_columns = ["color", "jump_host", "highlight_rules"].iter().filter(|c| has(c)).map(|c| c.to_string()).collect();
    Ok(Fetched { sessions, folders, writable, token, optional_columns })
}

/// The folders table's settings; a missing table is no settings. Read as JSON, so a table made before some
/// columns existed still reads, as SuperXPuTTY's FetchFolders
async fn fetch_folders(client: &Client, settings: &DbSettings) -> Vec<FolderSetting> {
    let Ok(rows) = client.query(&format!("SELECT to_jsonb(f)::text FROM {} f", settings.folders_table()), &[]).await else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|r| serde_json::from_str::<serde_json::Value>(&r.get::<_, String>(0)).ok())
        .map(|v| {
            let text = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            FolderSetting {
                path: text("folder").trim_matches('/').to_string(),
                color: text("color"),
                username: text("username"),
                putty_profile: text("putty_profile"),
                port: v.get("port").and_then(|x| x.as_u64()).unwrap_or(0) as u16,
                jump_host: text("jump_host"),
                highlight_rules: text("highlight_rules"),
            }
        })
        .filter(|f| !f.path.is_empty())
        .collect()
}

/// One row as a session, with SuperXPuTTY's attributes in its order (as SuperXPuTTY's ToSession)
fn to_session(c: &[String]) -> StoredSession {
    let folder = c[1].trim_matches('/');
    let name = &c[2];
    let id = if folder.is_empty() { format!("@db/{name}") } else { format!("@db/{folder}/{name}") };
    let micros: i64 = c[15].parse().unwrap_or(0);
    let version = (TICKS_AT_UNIX_EPOCH + micros * 10).to_string();
    let mut s = StoredSession::new(&id);
    s.set("DatabaseId", &c[0]);
    s.set("DatabaseVersion", &version);
    s.set("DatabaseLocked", if c[16] == "true" { "true" } else { "" });
    s.set("DatabaseLockedFields", &c[17]);
    s.set("DatabaseManagedBy", &c[18]);
    s.set("DatabaseUpdatedBy", &c[19]);
    s.set("Color", &c[20]);
    s.set("JumpHost", &c[21]);
    s.set("HighlightRules", &c[22]);
    s.set("SessionName", name);
    s.set("ImageKey", &c[9]);
    s.set("Host", &c[4]);
    s.set("Note", &c[10]);
    s.set("Port", &c[5]);
    s.set("Proto", &c[3]);
    s.set("PuttySession", &c[7]);
    s.set("Username", &c[6]);
    s.set("ExtraArgs", &c[8]);
    s.set("SPSLFileName", &c[11]);
    s.set("RdpOptions", &c[12]);
    s.set("LocalPath", &c[13]);
    s.set("RemotePath", &c[14]);
    s
}

/// Whether the table changed since `token`; the new token
pub async fn check(settings: &DbSettings, user_folder: &Path, token: &str) -> Result<(bool, String), String> {
    let client = connect(settings, user_folder, None).await?;
    let now = change_token(&client, settings).await?;
    Ok((now != token, now))
}

/// What Test connection reports
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub server_version: String,
    pub table_exists: bool,
    pub rows: i64,
    pub can_write: bool,
    pub has_version_table: bool,
}

/// Connect and describe the server and the table, as the window's Test connection
pub async fn test(settings: &DbSettings, user_folder: &Path, typed: Option<&str>) -> Result<TestResult, String> {
    let client = connect(settings, user_folder, typed).await?;
    let server_version: String = client.query_one("SHOW server_version", &[]).await.map_err(describe)?.get(0);
    let qualified = format!("{}.{}", settings.schema, settings.table);
    let table_exists: bool = client.query_one("SELECT to_regclass($1) IS NOT NULL", &[&qualified]).await.map_err(describe)?.get(0);
    let (rows, can_write) = if table_exists {
        let rows: i64 = client.query_one(&format!("SELECT count(*) FROM {}", settings.table()), &[]).await.map_err(describe)?.get(0);
        let can: bool = client
            .query_one("SELECT has_table_privilege($1, 'INSERT') AND has_table_privilege($1, 'UPDATE')", &[&qualified])
            .await
            .map_err(describe)?
            .get(0);
        (rows, can)
    } else {
        (0, false)
    };
    let versions = format!("{}.{}_version", settings.schema, settings.table);
    let has_version_table: bool = client.query_one("SELECT to_regclass($1) IS NOT NULL", &[&versions]).await.map_err(describe)?.get(0);
    Ok(TestResult { server_version, table_exists, rows, can_write, has_version_table })
}

/// The cache file in the user's folder
pub fn cache_file(user_folder: &Path) -> PathBuf {
    user_folder.join("DatabaseSessions.cache.xml")
}

/// The last read, from the cache: sessions, folders, whether it was writable
pub fn read_cache(user_folder: &Path) -> (Vec<StoredSession>, Vec<FolderSetting>, bool) {
    let path = cache_file(user_folder);
    let mut problems = Vec::new();
    let sessions = sessions::read_sessions_file(&path, &mut problems);
    let folders = sessions::read_folder_settings(&path, &mut problems);
    let writable = std::fs::read_to_string(&path).map(|t| t.contains("<Writable>true</Writable>")).unwrap_or(false);
    (sessions, folders, writable)
}

/// Write the cache in SuperXPuTTY's format (its CacheFile, as XmlSerializer writes it)
pub fn write_cache(user_folder: &Path, settings: &DbSettings, fetched: &Fetched) -> Result<(), String> {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n");
    out.push_str("<CacheFile xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\r\n");
    out.push_str(&format!("  <Source>{}</Source>\r\n", sessions::escape(&settings.source())));
    out.push_str(&format!("  <SyncedAt>{}</SyncedAt>\r\n", synced_at(chrono::Local::now())));
    if fetched.sessions.is_empty() {
        out.push_str("  <Sessions />\r\n");
    } else {
        out.push_str("  <Sessions>\r\n");
        for session in &fetched.sessions {
            out.push_str("    <SessionData");
            for (name, value) in &session.attributes {
                out.push_str(&format!(" {name}=\"{}\"", sessions::escape(value)));
            }
            out.push_str(" />\r\n");
        }
        out.push_str("  </Sessions>\r\n");
    }
    out.push_str(&format!("  <Writable>{}</Writable>\r\n", fetched.writable));
    if fetched.folders.is_empty() {
        out.push_str("  <Folders />\r\n");
    } else {
        out.push_str("  <Folders>\r\n");
        for f in &fetched.folders {
            out.push_str(&format!("    <FolderSetting Path=\"{}\"", sessions::escape(&f.path)));
            for (name, value) in [("Color", &f.color), ("Username", &f.username), ("PuttyProfile", &f.putty_profile)] {
                if !value.is_empty() {
                    out.push_str(&format!(" {name}=\"{}\"", sessions::escape(value)));
                }
            }
            if f.port > 0 {
                out.push_str(&format!(" Port=\"{}\"", f.port));
            }
            for (name, value) in [("JumpHost", &f.jump_host), ("HighlightRules", &f.highlight_rules)] {
                if !value.is_empty() {
                    out.push_str(&format!(" {name}=\"{}\"", sessions::escape(value)));
                }
            }
            out.push_str(" />\r\n");
        }
        out.push_str("  </Folders>\r\n");
    }
    if fetched.optional_columns.is_empty() {
        out.push_str("  <OptionalColumns />\r\n");
    } else {
        out.push_str("  <OptionalColumns>\r\n");
        for c in &fetched.optional_columns {
            out.push_str(&format!("    <string>{c}</string>\r\n"));
        }
        out.push_str("  </OptionalColumns>\r\n");
    }
    out.push_str("</CacheFile>");
    std::fs::create_dir_all(user_folder).map_err(|e| e.to_string())?;
    std::fs::write(cache_file(user_folder), out).map_err(|e| e.to_string())
}

/// A time as .NET's XmlSerializer writes a local DateTime: seven fraction digits and the offset,
/// "2026-11-01T21:09:40.4452974-06:00" (chrono has no seven-digit fraction of its own)
fn synced_at(now: chrono::DateTime<chrono::Local>) -> String {
    use chrono::Timelike;
    format!("{}.{:07}{}", now.format("%Y-%m-%dT%H:%M:%S"), now.nanosecond() % 1_000_000_000 / 100, now.format("%:z"))
}

/// The settings export, as SuperXPuTTY's DatabaseSettingsFile: no password (unless a {key} placeholder), no nickname
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportFile {
    pub format: String,
    pub version: u32,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub schema: String,
    pub table: String,
    pub user: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    pub ssl_mode: String,
    pub read_only: bool,
    pub check_seconds: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn synced_at_is_written_as_dotnet_writes_it() {
        let time = chrono::Local.with_ymd_and_hms(2026, 11, 1, 21, 9, 40).unwrap() + chrono::Duration::nanoseconds(445_297_400);
        let text = synced_at(time);
        assert!(text.starts_with("2026-11-01T21:09:40.4452974"), "{text}");
        assert!(text.len() == "2026-11-01T21:09:40.4452974-06:00".len(), "{text}");
    }
}

/// One session's values for a write: the columns SuperXPuTTY writes (as its AddSessionParameters)
pub struct Row {
    pub folder: String,
    pub name: String,
    pub protocol: String,
    pub host: String,
    pub port: i32,
    pub username: String,
    pub putty_profile: String,
    pub extra_args: String,
    pub icon: String,
    pub notes: String,
    pub spsl_script: String,
    pub rdp_options: String,
    pub local_path: String,
    pub remote_path: String,
    pub color: String,
    pub jump_host: String,
    pub highlight_rules: String,
}

impl Row {
    /// A stored session's values, in `folder` (for moves)
    pub fn of(session: &StoredSession, folder: &str) -> Row {
        let name = match session.get("SessionName") {
            "" => session.id().rsplit('/').next().unwrap_or("").to_string(),
            n => n.to_string(),
        };
        Row {
            folder: folder.trim_matches('/').to_string(),
            name,
            protocol: session.get("Proto").to_string(),
            host: session.get("Host").to_string(),
            port: session.port() as i32,
            username: session.get("Username").to_string(),
            putty_profile: session.get("PuttySession").to_string(),
            extra_args: session.get("ExtraArgs").to_string(),
            icon: session.get("ImageKey").to_string(),
            notes: session.get("Note").to_string(),
            spsl_script: session.get("SPSLFileName").to_string(),
            rdp_options: session.get("RdpOptions").to_string(),
            local_path: session.get("LocalPath").to_string(),
            remote_path: session.get("RemotePath").to_string(),
            color: session.get("Color").to_string(),
            jump_host: session.get("JumpHost").to_string(),
            highlight_rules: session.get("HighlightRules").to_string(),
        }
    }
}

/// A row's value for a lockable column name (`locked_fields` uses the table's column names), for lock checks
pub fn column_of(row: &Row, column: &str) -> String {
    match column {
        "folder" => row.folder.clone(),
        "name" => row.name.clone(),
        "protocol" => row.protocol.to_ascii_uppercase(),
        "host" => row.host.clone(),
        "port" => row.port.to_string(),
        "username" => row.username.clone(),
        "putty_profile" => row.putty_profile.clone(),
        "extra_args" => row.extra_args.clone(),
        "icon" => row.icon.clone(),
        "notes" => row.notes.clone(),
        "spsl_script" => row.spsl_script.clone(),
        "rdp_options" => row.rdp_options.clone(),
        "local_path" => row.local_path.clone(),
        "remote_path" => row.remote_path.clone(),
        "color" => row.color.clone(),
        "jump_host" => row.jump_host.clone(),
        "highlight_rules" => row.highlight_rules.clone(),
        _ => String::new(),
    }
}

/// The columns written, and the optional ones the table has
fn write_columns(optional: &[String]) -> Vec<&'static str> {
    let mut columns = vec![
        "folder", "name", "protocol", "host", "port", "username", "putty_profile", "extra_args", "icon", "notes",
        "spsl_script", "rdp_options", "local_path", "remote_path",
    ];
    for c in ["color", "jump_host", "highlight_rules"] {
        if optional.iter().any(|o| o == c) {
            columns.push(c);
        }
    }
    columns
}

/// A row's values for the given columns; empty optional text is NULL, as SuperXPuTTY's NullIfEmpty
fn values(row: &Row, columns: &[&str]) -> Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> {
    let null_if_empty = |t: &str| -> Option<String> { (!t.is_empty()).then(|| t.to_string()) };
    columns
        .iter()
        .map(|c| -> Box<dyn tokio_postgres::types::ToSql + Sync + Send> {
            match *c {
                "folder" => Box::new(row.folder.clone()),
                "name" => Box::new(row.name.clone()),
                "protocol" => Box::new(if row.protocol.is_empty() { "SSH".to_string() } else { row.protocol.clone() }),
                "host" => Box::new(row.host.clone()),
                "port" => Box::new(row.port),
                "username" => Box::new(row.username.clone()),
                "putty_profile" => Box::new(row.putty_profile.clone()),
                "extra_args" => Box::new(row.extra_args.clone()),
                "icon" => Box::new(null_if_empty(&row.icon)),
                "notes" => Box::new(null_if_empty(&row.notes)),
                "spsl_script" => Box::new(null_if_empty(&row.spsl_script)),
                "rdp_options" => Box::new(null_if_empty(&row.rdp_options)),
                "local_path" => Box::new(null_if_empty(&row.local_path)),
                "remote_path" => Box::new(null_if_empty(&row.remote_path)),
                "color" => Box::new(null_if_empty(&row.color)),
                "jump_host" => Box::new(null_if_empty(&row.jump_host)),
                _ => Box::new(null_if_empty(&row.highlight_rules)),
            }
        })
        .collect()
}

/// Connect for a write, refusing when the settings are read-only
async fn connect_to_write(settings: &DbSettings, user_folder: &Path) -> Result<Client, String> {
    if settings.read_only {
        return Err("The database connection is read-only, so nothing is created or changed.".into());
    }
    connect(settings, user_folder, None).await
}

/// A write's error as SuperXPuTTY words it: the lock trigger's refusal, a name clash, a refused name
fn write_error(error: tokio_postgres::Error) -> String {
    if let Some(db) = error.as_db_error() {
        let code = db.code().code();
        if code == "P0001" {
            if let Some(reason) = db.message().strip_prefix("locked: ") {
                let mut chars = reason.chars();
                return chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default() + ".";
            }
        }
        if code == "23505" {
            return "A session with that name is already in that folder in the database (perhaps hidden there by its deleted_at).".into();
        }
        if code == "23514" {
            return "The database refused the name or folder: names can't be empty or contain /.".into();
        }
    }
    describe(error)
}

/// Add a session; returns its id
pub async fn insert(settings: &DbSettings, user_folder: &Path, optional: &[String], row: &Row) -> Result<String, String> {
    let client = connect_to_write(settings, user_folder).await?;
    let columns = write_columns(optional);
    let params: Vec<String> = (1..=columns.len()).map(|i| format!("${i}")).collect();
    let values = values(row, &columns);
    let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = values.iter().map(|v| v.as_ref() as _).collect();
    let sql = format!("INSERT INTO {} ({}) VALUES ({}) RETURNING id::text", settings.table(), columns.join(", "), params.join(", "));
    client.query_one(&sql, &refs).await.map(|r| r.get(0)).map_err(write_error)
}

/// Change a session, but only if nobody changed it since it was read (`version` is its updated_at as .NET ticks)
pub async fn update(settings: &DbSettings, user_folder: &Path, optional: &[String], id: &str, version: &str, row: &Row) -> Result<(), String> {
    let ticks: i64 = version.parse().map_err(|_| "This session was read by an older version; Refresh, then make your change again.")?;
    let micros = (ticks - TICKS_AT_UNIX_EPOCH) / 10;
    let client = connect_to_write(settings, user_folder).await?;
    let columns = write_columns(optional);
    let n = columns.len();
    let params: Vec<String> = (1..=n).map(|i| format!("${i}")).collect();
    let mut values = values(row, &columns);
    values.push(Box::new(id.to_string()));
    values.push(Box::new(micros));
    let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = values.iter().map(|v| v.as_ref() as _).collect();
    let sql = format!(
        "UPDATE {} SET ({}) = ({}) WHERE id = ${}::text::uuid AND (extract(epoch FROM updated_at) * 1000000)::bigint = ${}",
        settings.table(),
        columns.join(", "),
        params.join(", "),
        n + 1,
        n + 2
    );
    let rows = client.execute(&sql, &refs).await.map_err(write_error)?;
    if rows == 0 {
        return Err("Someone else changed or deleted this session since it was read. Refresh, then make your change again.".into());
    }
    Ok(())
}

/// Delete a session
pub async fn delete(settings: &DbSettings, user_folder: &Path, id: &str) -> Result<(), String> {
    let client = connect_to_write(settings, user_folder).await?;
    client.execute(&format!("DELETE FROM {} WHERE id = $1::text::uuid", settings.table()), &[&id]).await.map_err(write_error)?;
    Ok(())
}

/// The rows in a folder and below it, as SuperXPuTTY's InFolder
const IN_FOLDER: &str = "(folder = $1 OR left(folder, length($1) + 1) = $1 || '/')";

/// Rename or move a folder: every row below it, hidden ones too, and the folders' own settings
pub async fn rename_folder(settings: &DbSettings, user_folder: &Path, from: &str, to: &str) -> Result<(), String> {
    let client = connect_to_write(settings, user_folder).await?;
    let (from, to) = (from.trim_matches('/'), to.trim_matches('/'));
    client
        .execute(&format!("UPDATE {} SET folder = $2 || substr(folder, length($1) + 1) WHERE {IN_FOLDER}", settings.table()), &[&from, &to])
        .await
        .map_err(write_error)?;
    let _ = client
        .execute(&format!("UPDATE {} SET folder = $2 || substr(folder, length($1) + 1) WHERE {IN_FOLDER}", settings.folders_table()), &[&from, &to])
        .await;
    Ok(())
}

/// Delete a folder: the sessions it showed (rows hidden by deleted_at stay) and the folders' own settings
pub async fn delete_folder(settings: &DbSettings, user_folder: &Path, folder: &str) -> Result<u64, String> {
    let client = connect_to_write(settings, user_folder).await?;
    let folder = folder.trim_matches('/');
    let removed = client
        .execute(&format!("DELETE FROM {} t WHERE {IN_FOLDER} AND (to_jsonb(t) ->> 'deleted_at') IS NULL", settings.table()), &[&folder])
        .await
        .map_err(write_error)?;
    let _ = client.execute(&format!("DELETE FROM {} WHERE {IN_FOLDER}", settings.folders_table()), &[&folder]).await;
    Ok(removed)
}

/// Save a folder's settings in the folders table (all empty removes them), as SuperXPuTTY's SetFolder
pub async fn set_folder(settings: &DbSettings, user_folder: &Path, f: &FolderSetting) -> Result<(), String> {
    let client = connect_to_write(settings, user_folder).await?;
    let path = f.path.trim_matches('/').to_string();
    let empty = f.color.is_empty() && f.username.is_empty() && f.putty_profile.is_empty() && f.port == 0
        && f.jump_host.is_empty() && f.highlight_rules.is_empty();
    let none = |t: &str| -> Option<String> { (!t.is_empty()).then(|| t.to_string()) };
    let result = if empty {
        client.execute(&format!("DELETE FROM {} WHERE folder = $1", settings.folders_table()), &[&path]).await
    } else {
        let port: Option<i32> = (f.port > 0).then_some(f.port as i32);
        client
            .execute(
                &format!(
                    "INSERT INTO {} (folder, color, username, putty_profile, port, jump_host, highlight_rules) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (folder) DO UPDATE SET color = EXCLUDED.color, \
                     username = EXCLUDED.username, putty_profile = EXCLUDED.putty_profile, port = EXCLUDED.port, \
                     jump_host = EXCLUDED.jump_host, highlight_rules = EXCLUDED.highlight_rules",
                    settings.folders_table()
                ),
                &[&path, &none(&f.color), &none(&f.username), &none(&f.putty_profile), &port, &none(&f.jump_host), &none(&f.highlight_rules)],
            )
            .await
    };
    match result {
        Ok(_) => Ok(()),
        Err(e) if e.code().is_some_and(|c| c.code() == "42P01") => {
            Err("The database has no folder settings table yet. Run Create / upgrade schema in Tools > Database sessions.".into())
        }
        Err(e) => Err(write_error(e)),
    }
}
