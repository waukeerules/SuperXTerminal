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
use crate::{secrets, variables};

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
    secrets::get(KEYCHAIN_ACCOUNT).or_else(|| {
        (!settings.password_placeholder.is_empty())
            .then(|| variables::substitute(&settings.password_placeholder, &variables::values(user_folder)))
    })
}

/// Keep the database password in the Keychain, or forget it when empty
pub fn set_password(password: &str) -> Result<(), String> {
    secrets::set(KEYCHAIN_ACCOUNT, password)
}

/// Whether a password is stored
pub fn has_password() -> bool {
    secrets::get(KEYCHAIN_ACCOUNT).is_some()
}

/// Connect as SuperXPuTTY does: application name SuperPuTTY, 8 s timeout, the chosen SSL mode, and the nickname.
/// `typed` is a password typed in the window but not saved yet (Test connection).
async fn connect(settings: &DbSettings, user_folder: &Path, typed: Option<&str>) -> Result<Client, String> {
    open(settings, user_folder, typed).await.map_err(|e| e.message)
}

/// Connect, saying whether a failure means the database can't be reached (so a change is queued) or refused us
async fn open(settings: &DbSettings, user_folder: &Path, typed: Option<&str>) -> Result<Client, WriteError> {
    if settings.host.trim().is_empty() || settings.user.trim().is_empty() {
        return Err(WriteError::refused("Set the host and username in Database sessions first."));
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
        let (client, connection) = config.connect(NoTls).await.map_err(write_error)?;
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
            .map_err(|e| WriteError::refused(&e.to_string()))?;
        let (client, connection) = config.connect(postgres_native_tls::MakeTlsConnector::new(tls)).await.map_err(write_error)?;
        tauri::async_runtime::spawn(connection);
        client
    };
    if !settings.nickname.trim().is_empty() {
        // read by the touch trigger for updated_by; a plain setting, so any login may set it
        client
            .execute("SELECT set_config('superputty.nickname', $1, false)", &[&settings.nickname.trim()])
            .await
            .map_err(write_error)?;
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

/// A row's values as a session (a change waiting in the outbox), as to_session does for a row read
pub fn session_of(row: &Row, id: &str, version: &str) -> StoredSession {
    let folder = row.folder.trim_matches('/');
    let session_id = if folder.is_empty() { format!("@db/{}", row.name) } else { format!("@db/{folder}/{}", row.name) };
    let mut s = StoredSession::new(&session_id);
    s.set("DatabaseId", id);
    s.set("DatabaseVersion", version);
    s.set("Color", &row.color);
    s.set("JumpHost", &row.jump_host);
    s.set("HighlightRules", &row.highlight_rules);
    s.set("SessionName", &row.name);
    s.set("ImageKey", &row.icon);
    s.set("Host", &row.host);
    s.set("Note", &row.notes);
    s.set("Port", &row.port.to_string());
    s.set("Proto", &row.protocol);
    s.set("PuttySession", &row.putty_profile);
    s.set("Username", &row.username);
    s.set("ExtraArgs", &row.extra_args);
    s.set("SPSLFileName", &row.spsl_script);
    s.set("RdpOptions", &row.rdp_options);
    s.set("LocalPath", &row.local_path);
    s.set("RemotePath", &row.remote_path);
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
    /// What Create / upgrade schema would add, as SuperXPuTTY's Probe: the table, columns, triggers, change counter,
    /// folders table; empty when it's up to date
    pub missing: Vec<String>,
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
    let (schema, name) = (settings.schema.as_str(), settings.table.as_str());
    let mut missing = Vec::new();
    if !table_exists {
        missing.push("the table".to_string());
    } else {
        let present: Vec<String> = client
            .query("SELECT column_name::text FROM information_schema.columns WHERE table_schema = $1 AND table_name = $2", &[&schema, &name])
            .await
            .map_err(describe)?
            .iter()
            .map(|r| r.get(0))
            .collect();
        let columns: Vec<&str> = COLUMNS.iter().map(|(c, _)| *c).filter(|c| !present.iter().any(|p| p == c)).collect();
        if !columns.is_empty() {
            missing.push(format!("columns {}", columns.join(", ")));
        }
        let triggers = vec![format!("{name}_changed"), format!("{name}_touch"), format!("{name}_guard")];
        let count: i64 = client
            .query_one("SELECT count(*) FROM pg_trigger WHERE tgrelid = to_regclass($1) AND tgname = ANY($2)", &[&qualified, &triggers])
            .await
            .map_err(describe)?
            .get(0);
        if count != 3 {
            missing.push("the change and lock triggers".into());
        }
        // the notify function must raise the counter too: an older SuperPuTTY's Create / upgrade puts back one that doesn't
        let counter: bool = client
            .query_one(
                "SELECT to_regclass($1) IS NOT NULL AND EXISTS (SELECT 1 FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
                 WHERE n.nspname = $2 AND p.proname = $3 AND strpos(p.prosrc, $4) > 0)",
                &[&versions, &schema, &format!("{name}_notify"), &format!("{name}_version")],
            )
            .await
            .map_err(describe)?
            .get(0);
        if !counter {
            missing.push("the change counter".into());
        }
        let folder_columns: Vec<&str> = FOLDER_COLUMNS.iter().map(|(c, _)| *c).collect();
        let count: i64 = client
            .query_one(
                "SELECT count(*) FROM information_schema.columns WHERE table_schema = $1 AND table_name = $2 AND column_name = ANY($3)",
                &[&schema, &format!("{name}_folders"), &folder_columns],
            )
            .await
            .map_err(describe)?
            .get(0);
        if count != FOLDER_COLUMNS.len() as i64 {
            missing.push("the folder settings table".into());
        }
    }
    Ok(TestResult { server_version, table_exists, rows, can_write, has_version_table, missing })
}

/// The cache file in the user's folder
pub fn cache_file(user_folder: &Path) -> PathBuf {
    user_folder.join("DatabaseSessions.cache.xml")
}

/// The last read, as kept in the cache
pub struct Cached {
    pub sessions: Vec<StoredSession>,
    pub folders: Vec<FolderSetting>,
    pub writable: bool,
    pub optional_columns: Vec<String>,
    /// When it was read, e.g. "2026-11-02T09:41:07.1234567-06:00"
    pub synced_at: String,
}

/// The cache, as SuperXPuTTY's LoadCache: only when it came from this connection (its Source), so another table's
/// or server's sessions are never shown in its place
pub fn read_cache(user_folder: &Path, settings: &DbSettings) -> Option<Cached> {
    let path = cache_file(user_folder);
    let text = std::fs::read_to_string(&path).ok()?;
    let element = |name: &str| -> Option<String> {
        let start = text.find(&format!("<{name}>"))? + name.len() + 2;
        let end = start + text[start..].find(&format!("</{name}>"))?;
        Some(text[start..end].to_string())
    };
    if element("Source")? != sessions::escape(&settings.source()) {
        return None;
    }
    let mut problems = Vec::new();
    let optional_columns = element("OptionalColumns")
        .map(|list| list.split("<string>").skip(1).filter_map(|s| s.split("</string>").next()).map(str::to_string).collect())
        .unwrap_or_default();
    Some(Cached {
        sessions: sessions::read_sessions_file(&path, &mut problems),
        folders: sessions::read_folder_settings(&path, &mut problems),
        writable: element("Writable").as_deref() == Some("true") && !settings.read_only,
        optional_columns,
        synced_at: element("SyncedAt").unwrap_or_default(),
    })
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
    /// How long a read of a big table takes (e.g. lots_of_sessions, 10k rows), and its change token. Run with
    /// SXT_DB_HOST, SXT_DB_USER, SXT_DB_PASSWORD and SXT_DB_TABLE set: `cargo test read_a_big_table -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn read_a_big_table() {
        let env = |k: &str| std::env::var(k).expect(k);
        let folder = std::env::temp_dir().join("sxt-db-test");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("EnvironmentVariables.json"),
            serde_json::json!({ "Variables": [{ "Key": "dbpw", "Value": env("SXT_DB_PASSWORD"), "Secret": false }] }).to_string(),
        )
        .unwrap();
        let settings = DbSettings {
            enabled: true,
            host: env("SXT_DB_HOST"),
            user: env("SXT_DB_USER"),
            table: env("SXT_DB_TABLE"),
            password_placeholder: "{dbpw}".into(),
            ..DbSettings::default()
        };
        let started = std::time::Instant::now();
        let fetched = tauri::async_runtime::block_on(fetch_all(&settings, &folder)).unwrap();
        println!("read {} sessions, {} folder settings in {:?}; token {}; optional columns {:?}; writable {}",
            fetched.sessions.len(), fetched.folders.len(), started.elapsed(), fetched.token, fetched.optional_columns, fetched.writable);
        let mut ids = std::collections::HashSet::new();
        let duplicates = fetched.sessions.iter().filter(|s| !ids.insert(s.id())).count();
        println!("duplicate ids: {duplicates}");
        let started = std::time::Instant::now();
        write_cache(&folder, &settings, &fetched).unwrap();
        println!("cache written in {:?}, {} bytes", started.elapsed(), std::fs::metadata(cache_file(&folder)).unwrap().len());
        let started = std::time::Instant::now();
        let cached = read_cache(&folder, &settings).unwrap();
        println!("cache read back: {} sessions in {:?}", cached.sessions.len(), started.elapsed());
    }

    /// Create / upgrade schema and the lock trigger on a table of its own, superputty.sxt_test_sessions, which it
    /// drops first. Run with SXT_DB_HOST, SXT_DB_USER and SXT_DB_PASSWORD set:
    /// `cargo test schema_and_locks -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn schema_and_locks_on_a_test_table() {
        tauri::async_runtime::block_on(schema_and_locks());
    }

    async fn schema_and_locks() {
        let env = |k: &str| std::env::var(k).expect(k);
        let folder = std::env::temp_dir().join("sxt-db-test");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("EnvironmentVariables.json"),
            serde_json::json!({ "Variables": [{ "Key": "dbpw", "Value": env("SXT_DB_PASSWORD"), "Secret": false }] }).to_string(),
        )
        .unwrap();
        let settings = DbSettings {
            enabled: true,
            host: env("SXT_DB_HOST"),
            user: env("SXT_DB_USER"),
            table: "sxt_test_sessions".into(),
            nickname: "sxt-test".into(),
            password_placeholder: "{dbpw}".into(),
            ..DbSettings::default()
        };
        let client = connect(&settings, &folder, None).await.unwrap();
        client
            .batch_execute(
                "DROP TABLE IF EXISTS superputty.sxt_test_sessions, superputty.sxt_test_sessions_version, superputty.sxt_test_sessions_folders; \
                 DROP FUNCTION IF EXISTS superputty.sxt_test_sessions_touch(), superputty.sxt_test_sessions_notify(), superputty.sxt_test_sessions_guard()",
            )
            .await
            .unwrap();

        let done = create_or_upgrade_schema(&settings, &folder).await.unwrap();
        println!("first run: {done}");
        assert!(done.starts_with("Done: created table superputty.sxt_test_sessions"), "{done}");
        assert!(test(&settings, &folder, None).await.unwrap().missing.is_empty());
        assert_eq!(create_or_upgrade_schema(&settings, &folder).await.unwrap(), "The schema is already up to date.");

        // a table made by an older version: no lock columns, no triggers, no folders table
        client
            .batch_execute(
                "ALTER TABLE superputty.sxt_test_sessions DROP COLUMN locked_fields, DROP COLUMN color; \
                 DROP TRIGGER sxt_test_sessions_guard ON superputty.sxt_test_sessions; DROP TABLE superputty.sxt_test_sessions_folders",
            )
            .await
            .unwrap();
        let missing = test(&settings, &folder, None).await.unwrap().missing;
        println!("older table is missing: {missing:?}");
        assert_eq!(missing.len(), 3, "{missing:?}");
        let done = create_or_upgrade_schema(&settings, &folder).await.unwrap();
        println!("upgrade: {done}");
        assert!(done.contains("added column color") && done.contains("added column locked_fields"), "{done}");
        assert!(test(&settings, &folder, None).await.unwrap().missing.is_empty());

        let v0: i64 = client.query_one("SELECT version FROM superputty.sxt_test_sessions_version", &[]).await.unwrap().get(0);
        client
            .batch_execute(
                "INSERT INTO superputty.sxt_test_sessions (folder, name, host) VALUES ('Test', 'open', '10.0.0.1'); \
                 INSERT INTO superputty.sxt_test_sessions (folder, name, host, locked) VALUES ('Test', 'locked', '10.0.0.2', true); \
                 INSERT INTO superputty.sxt_test_sessions (folder, name, host, locked_fields) VALUES ('Test', 'host-locked', '10.0.0.3', '{host,name}')",
            )
            .await
            .unwrap();
        let v1: i64 = client.query_one("SELECT version FROM superputty.sxt_test_sessions_version", &[]).await.unwrap().get(0);
        assert_eq!(v1, v0 + 3, "each statement raises the version");
        let by: String = client.query_one("SELECT updated_by FROM superputty.sxt_test_sessions WHERE name = 'open'", &[]).await.unwrap().get(0);
        assert_eq!(by, "sxt-test", "updated_by is the nickname");

        // as SuperPuTTY (this connection's application_name): what the guard refuses
        let refused = [
            ("UPDATE superputty.sxt_test_sessions SET notes = 'x' WHERE name = 'locked'", "\"locked\" is locked in the database"),
            ("DELETE FROM superputty.sxt_test_sessions WHERE name = 'locked'", "\"locked\" is locked in the database"),
            ("UPDATE superputty.sxt_test_sessions SET host = '10.9.9.9' WHERE name = 'host-locked'", "host of \"host-locked\" is locked"),
            ("UPDATE superputty.sxt_test_sessions SET locked = true WHERE name = 'open'", "the locks on \"open\" can only be changed"),
            ("UPDATE superputty.sxt_test_sessions SET external_id = 'x' WHERE name = 'open'", "who manages \"open\""),
        ];
        for (sql, expected) in refused {
            let error = describe(client.batch_execute(sql).await.unwrap_err());
            println!("refused: {error}");
            assert!(error.contains("locked: ") && error.contains(expected), "{sql}: {error}");
        }
        client.batch_execute("UPDATE superputty.sxt_test_sessions SET username = 'admin' WHERE name = 'host-locked'").await.unwrap();

        // the app's own write refuses a locked field before it reaches the trigger, and reads the locks
        let fetched = fetch_all(&settings, &folder).await.unwrap();
        assert_eq!(fetched.sessions.len(), 3);
        let locked = fetched.sessions.iter().find(|s| s.get("SessionName") == "locked").unwrap();
        assert_eq!(locked.get("DatabaseLocked"), "true");
        let host_locked = fetched.sessions.iter().find(|s| s.get("SessionName") == "host-locked").unwrap();
        assert_eq!(host_locked.get("DatabaseLockedFields"), "host,name");

        // other programs (the scripts that manage the table) aren't stopped
        let script = DbSettings { nickname: String::new(), ..settings.clone() };
        let other = connect(&script, &folder, None).await.unwrap();
        other.batch_execute("SET application_name = 'inventory-script'").await.unwrap();
        other.batch_execute("UPDATE superputty.sxt_test_sessions SET notes = 'by script' WHERE name = 'locked'").await.unwrap();
        println!("the rows stay in superputty.sxt_test_sessions for a look in the app");
    }

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
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
async fn connect_to_write(settings: &DbSettings, user_folder: &Path) -> Result<Client, WriteError> {
    if settings.read_only {
        return Err(WriteError::refused("The database connection is read-only, so nothing is created or changed."));
    }
    open(settings, user_folder, None).await
}

/// Why a write failed, and whether it's because the database can't be reached (SuperXPuTTY's
/// DatabaseUnavailableException): then the change is queued rather than refused
#[derive(Debug)]
pub struct WriteError {
    pub message: String,
    pub unavailable: bool,
}

impl WriteError {
    pub fn refused(message: &str) -> WriteError {
        WriteError { message: message.to_string(), unavailable: false }
    }
}

impl From<WriteError> for String {
    fn from(e: WriteError) -> String {
        e.message
    }
}

/// A write's error as SuperXPuTTY words it: the lock trigger's refusal, a name clash, a refused name; no answer, a
/// dropped connection or a server shutting down (08..., 57P...) is "can't be reached"
fn write_error(error: tokio_postgres::Error) -> WriteError {
    let Some(db) = error.as_db_error() else {
        // no answer from the server (refused, timed out, dropped) is "can't be reached"; a mistake on our side isn't
        let unavailable = error.is_closed() || std::error::Error::source(&error).is_some_and(|e| e.is::<std::io::Error>());
        return WriteError { message: describe(error), unavailable };
    };
    let code = db.code().code();
    let message = if code == "P0001" && db.message().starts_with("locked: ") {
        let reason = &db.message()["locked: ".len()..];
        let mut chars = reason.chars();
        chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default() + "."
    } else if code == "23505" {
        "A session with that name is already in that folder in the database (perhaps hidden there by its deleted_at).".into()
    } else if code == "23514" {
        "The database refused the name or folder: names can't be empty or contain /.".into()
    } else {
        db.message().to_string()
    };
    WriteError { message, unavailable: code.starts_with("08") || code.starts_with("57P") }
}

/// Add a session; returns its id
pub async fn insert(settings: &DbSettings, user_folder: &Path, optional: &[String], row: &Row) -> Result<String, WriteError> {
    let client = connect_to_write(settings, user_folder).await?;
    let columns = write_columns(optional);
    let params: Vec<String> = (1..=columns.len()).map(|i| format!("${i}")).collect();
    let values = values(row, &columns);
    let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = values.iter().map(|v| v.as_ref() as _).collect();
    let sql = format!("INSERT INTO {} ({}) VALUES ({}) RETURNING id::text", settings.table(), columns.join(", "), params.join(", "));
    client.query_one(&sql, &refs).await.map(|r| r.get(0)).map_err(write_error)
}

/// Change a session, but only if nobody changed it since it was read (`version` is its updated_at as .NET ticks),
/// unless `force` (Overwrite theirs, or a row this sync already changed)
pub async fn update(
    settings: &DbSettings,
    user_folder: &Path,
    optional: &[String],
    id: &str,
    version: &str,
    row: &Row,
    force: bool,
) -> Result<(), WriteError> {
    let micros = match version.parse::<i64>() {
        Ok(ticks) => (ticks - TICKS_AT_UNIX_EPOCH) / 10,
        Err(_) if force => 0,
        Err(_) => return Err(WriteError::refused("This session was read by an older version; Refresh, then make your change again.")),
    };
    let client = connect_to_write(settings, user_folder).await?;
    let columns = write_columns(optional);
    let n = columns.len();
    let params: Vec<String> = (1..=n).map(|i| format!("${i}")).collect();
    let mut values = values(row, &columns);
    values.push(Box::new(id.to_string()));
    values.push(Box::new(micros));
    let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = values.iter().map(|v| v.as_ref() as _).collect();
    // Overwrite skips the version check; the parameter is still sent, so its type is given
    let version_check =
        if force { format!("${}::bigint IS NOT NULL", n + 2) } else { format!("(extract(epoch FROM updated_at) * 1000000)::bigint = ${}", n + 2) };
    let sql = format!(
        "UPDATE {} SET ({}) = ({}) WHERE id = ${}::text::uuid AND {version_check}",
        settings.table(),
        columns.join(", "),
        params.join(", "),
        n + 1,
    );
    let rows = client.execute(&sql, &refs).await.map_err(write_error)?;
    if rows == 0 {
        return Err(WriteError::refused("Someone else changed or deleted this session since it was read. Refresh, then make your change again."));
    }
    Ok(())
}

/// Delete a session
pub async fn delete(settings: &DbSettings, user_folder: &Path, id: &str) -> Result<(), WriteError> {
    let client = connect_to_write(settings, user_folder).await?;
    client.execute(&format!("DELETE FROM {} WHERE id = $1::text::uuid", settings.table()), &[&id]).await.map_err(write_error)?;
    Ok(())
}

/// The rows in a folder and below it, as SuperXPuTTY's InFolder: ignoring case, as the tree shows "test" and "Test" as
/// one folder, so renaming or removing it takes both
const IN_FOLDER: &str = "(lower(folder) = lower($1) OR lower(left(folder, length($1) + 1)) = lower($1) || '/')";

/// Rename or move a folder: every row below it, hidden ones too, and the folders' own settings; returns the rows' ids
pub async fn rename_folder(settings: &DbSettings, user_folder: &Path, from: &str, to: &str) -> Result<Vec<String>, WriteError> {
    let client = connect_to_write(settings, user_folder).await?;
    let (from, to) = (from.trim_matches('/'), to.trim_matches('/'));
    let moved = client
        .query(
            &format!("UPDATE {} SET folder = $2 || substr(folder, length($1) + 1) WHERE {IN_FOLDER} RETURNING id::text", settings.table()),
            &[&from, &to],
        )
        .await
        .map_err(write_error)?
        .iter()
        .map(|r| r.get(0))
        .collect();
    let _ = client
        .execute(&format!("UPDATE {} SET folder = $2 || substr(folder, length($1) + 1) WHERE {IN_FOLDER}", settings.folders_table()), &[&from, &to])
        .await;
    Ok(moved)
}

/// Delete a folder: the sessions it showed (rows hidden by deleted_at stay) and the folders' own settings
pub async fn delete_folder(settings: &DbSettings, user_folder: &Path, folder: &str) -> Result<u64, WriteError> {
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
pub async fn set_folder(settings: &DbSettings, user_folder: &Path, f: &FolderSetting) -> Result<(), WriteError> {
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
            Err(WriteError::refused("The database has no folder settings table yet. Run Create / upgrade schema in Tools > Database sessions."))
        }
        Err(e) => Err(write_error(e)),
    }
}

/// The sessions table's columns, as SuperXPuTTY's Columns (Create / upgrade schema adds any that are missing)
const COLUMNS: [(&str, &str); 26] = [
    ("id", "uuid PRIMARY KEY DEFAULT gen_random_uuid()"),
    ("folder", "text NOT NULL DEFAULT ''"),
    ("name", "text NOT NULL"),
    ("protocol", "text NOT NULL DEFAULT 'SSH'"),
    ("host", "text NOT NULL DEFAULT ''"),
    ("port", "integer NOT NULL DEFAULT 0"),
    ("username", "text NOT NULL DEFAULT ''"),
    ("putty_profile", "text NOT NULL DEFAULT 'Default Settings'"),
    ("extra_args", "text NOT NULL DEFAULT ''"),
    ("icon", "text"),
    ("notes", "text"),
    ("spsl_script", "text"),
    ("rdp_options", "text"),
    ("local_path", "text"),
    ("remote_path", "text"),
    ("color", "text"),
    ("jump_host", "text"),
    ("highlight_rules", "text"),
    ("locked", "boolean NOT NULL DEFAULT false"),
    ("locked_fields", "text[] NOT NULL DEFAULT '{}'"),
    ("managed_by", "text"),
    ("external_id", "text"),
    ("created_at", "timestamptz NOT NULL DEFAULT now()"),
    ("updated_at", "timestamptz NOT NULL DEFAULT now()"),
    ("updated_by", "text NOT NULL DEFAULT current_user"),
    ("deleted_at", "timestamptz"),
];

/// The folders table's columns, as SuperXPuTTY's FolderColumns
const FOLDER_COLUMNS: [(&str, &str); 9] = [
    ("folder", "text PRIMARY KEY"),
    ("color", "text"),
    ("username", "text"),
    ("putty_profile", "text"),
    ("port", "integer"),
    ("jump_host", "text"),
    ("highlight_rules", "text"),
    ("updated_at", "timestamptz NOT NULL DEFAULT now()"),
    ("updated_by", "text NOT NULL DEFAULT current_user"),
];

/// Who changed a row: the user's nickname, else the login, as SuperXPuTTY's ChangedBy
const CHANGED_BY: &str = "coalesce(nullif(current_setting('superputty.nickname', true), ''), current_user)";

/// Create the database, schema, table, triggers and folders table, or add whatever is missing, exactly as
/// SuperXPuTTY's CreateOrUpgradeSchema, so either app's tables work with both. It never drops or rewrites data, and
/// runs only from the window's button, never when Read only is ticked. Returns what it did.
pub async fn create_or_upgrade_schema(settings: &DbSettings, user_folder: &Path) -> Result<String, String> {
    if settings.read_only {
        return Err("The database connection is read-only, so nothing is created or changed.".into());
    }
    let mut done: Vec<String> = Vec::new();
    let mut client = match connect(settings, user_folder, None).await {
        Ok(client) => client,
        Err(e) if e.contains("does not exist") && e.contains("database") => {
            let server = DbSettings { database: "postgres".into(), nickname: String::new(), ..settings.clone() };
            let admin = connect(&server, user_folder, None).await?;
            admin.batch_execute(&format!("CREATE DATABASE {}", quote(&settings.database))).await.map_err(describe)?;
            done.push(format!("created database {}", settings.database));
            connect(settings, user_folder, None).await?
        }
        Err(e) => return Err(e),
    };
    let version: i32 = client
        .query_one("SELECT current_setting('server_version_num')::int", &[])
        .await
        .map_err(describe)?
        .get(0);
    if version < 130000 {
        return Err("PostgreSQL 13 or later is needed.".into());
    }

    let (schema, name) = (settings.schema.as_str(), settings.table.as_str());
    let table = settings.table();
    let qualified = format!("{schema}.{name}");
    let table_exists: bool = client.query_one("SELECT to_regclass($1) IS NOT NULL", &[&qualified]).await.map_err(describe)?.get(0);
    let present: Vec<String> = client
        .query("SELECT column_name::text FROM information_schema.columns WHERE table_schema = $1 AND table_name = $2", &[&schema, &name])
        .await
        .map_err(describe)?
        .iter()
        .map(|r| r.get(0))
        .collect();
    // what's there before, as SuperXPuTTY's Probe, for what "Done:" lists
    let before = test(settings, user_folder, None).await?.missing;
    let had_counter = !before.iter().any(|m| m == "the change counter") && table_exists;
    let had_folders = !before.iter().any(|m| m == "the folder settings table") && table_exists;
    let had_triggers = !before.iter().any(|m| m == "the change and lock triggers") && table_exists;

    let grants: Vec<(String, String)> = client
        .query(
            "SELECT grantee::text, string_agg(privilege_type, ', ') FROM information_schema.role_table_grants \
             WHERE table_schema = $1 AND table_name = $2 AND privilege_type IN ('SELECT', 'INSERT', 'UPDATE', 'DELETE') \
             AND grantee <> current_user GROUP BY grantee",
            &[&schema, &name],
        )
        .await
        .map_err(describe)?
        .iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();

    let q = |s: &str| quote(s);
    let mut sql: Vec<String> = vec![format!("CREATE SCHEMA IF NOT EXISTS {}", q(schema))];
    if !table_exists {
        let columns: Vec<String> = COLUMNS.iter().map(|(c, t)| format!("{} {t}", q(c))).collect();
        sql.push(format!(
            "CREATE TABLE IF NOT EXISTS {table} ({}, UNIQUE (folder, name), CHECK (name <> '' AND strpos(name, '/') = 0), \
             CHECK (folder = '' OR (left(folder, 1) <> '/' AND right(folder, 1) <> '/')))",
            columns.join(", ")
        ));
        sql.push(format!("COMMENT ON TABLE {table} IS 'SuperPuTTY sessions: folder is the tree path (e.g. Lab/Core), '''' at the top'"));
        done.push(format!("created table {qualified}"));
    } else {
        for (c, t) in COLUMNS.iter().filter(|(c, _)| *c != "id" && !present.iter().any(|p| p == c)) {
            sql.push(format!("ALTER TABLE {table} ADD COLUMN IF NOT EXISTS {} {t}", q(c)));
            done.push(format!("added column {c}"));
        }
    }
    sql.push(format!(
        "CREATE UNIQUE INDEX IF NOT EXISTS {} ON {table} (managed_by, external_id) WHERE external_id IS NOT NULL",
        q(&format!("{name}_external_id"))
    ));
    let touch = format!("{}.{}", q(schema), q(&format!("{name}_touch")));
    sql.push(format!(
        "CREATE OR REPLACE FUNCTION {touch}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN \
         IF TG_OP = 'INSERT' OR TG_OP = 'UPDATE' THEN NEW.updated_at := now(); NEW.updated_by := {CHANGED_BY}; RETURN NEW; END IF; \
         RETURN OLD; END $$"
    ));
    let versions = settings.version_table();
    sql.push(format!("CREATE TABLE IF NOT EXISTS {versions} (version bigint NOT NULL, changed_at timestamptz NOT NULL DEFAULT now())"));
    sql.push(format!("INSERT INTO {versions} (version) SELECT 1 WHERE NOT EXISTS (SELECT 1 FROM {versions})"));
    sql.push(format!("GRANT SELECT ON {versions} TO PUBLIC"));
    sql.push(format!("COMMENT ON TABLE {versions} IS 'Raised on every change to {name}; SuperPuTTY reads it to notice changes'"));
    let notify = format!("{}.{}", q(schema), q(&format!("{name}_notify")));
    sql.push(format!(
        "CREATE OR REPLACE FUNCTION {notify}() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER \
         SET search_path = pg_catalog, pg_temp AS $$ BEGIN \
         UPDATE {versions} SET version = version + 1, changed_at = now(); \
         PERFORM pg_notify('superputty_sessions', TG_TABLE_SCHEMA || '.' || TG_TABLE_NAME); RETURN NULL; END $$"
    ));
    if !had_counter {
        done.push(format!("added the change counter {name}_version"));
    }
    let trigger = |t: &str| q(&format!("{name}_{t}"));
    sql.push(format!("DROP TRIGGER IF EXISTS {} ON {table}", trigger("touch")));
    sql.push(format!("CREATE TRIGGER {} BEFORE INSERT OR UPDATE ON {table} FOR EACH ROW EXECUTE FUNCTION {touch}()", trigger("touch")));
    let guard = format!("{}.{}", q(schema), q(&format!("{name}_guard")));
    sql.push(format!(
        "CREATE OR REPLACE FUNCTION {guard}() RETURNS trigger LANGUAGE plpgsql AS $$ DECLARE f text; BEGIN \
         IF coalesce(current_setting('application_name', true), '') <> '{APPLICATION_NAME}' THEN \
         IF TG_OP = 'DELETE' THEN RETURN OLD; END IF; RETURN NEW; END IF; \
         IF TG_OP = 'DELETE' THEN \
         IF OLD.locked THEN RAISE EXCEPTION 'locked: \"%\" is locked in the database', OLD.name; END IF; RETURN OLD; END IF; \
         IF NEW.locked IS DISTINCT FROM OLD.locked OR NEW.locked_fields IS DISTINCT FROM OLD.locked_fields THEN \
         RAISE EXCEPTION 'locked: the locks on \"%\" can only be changed in the database', OLD.name; END IF; \
         IF NEW.managed_by IS DISTINCT FROM OLD.managed_by OR NEW.external_id IS DISTINCT FROM OLD.external_id \
         OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN \
         RAISE EXCEPTION 'locked: who manages \"%\", and when it was created, can only be changed in the database', OLD.name; END IF; \
         IF OLD.locked AND (to_jsonb(NEW) - 'updated_at' - 'updated_by') IS DISTINCT FROM (to_jsonb(OLD) - 'updated_at' - 'updated_by') THEN \
         RAISE EXCEPTION 'locked: \"%\" is locked in the database', OLD.name; END IF; \
         FOREACH f IN ARRAY OLD.locked_fields LOOP \
         IF (to_jsonb(NEW) -> f) IS DISTINCT FROM (to_jsonb(OLD) -> f) THEN \
         RAISE EXCEPTION 'locked: % of \"%\" is locked in the database', f, OLD.name; END IF; END LOOP; \
         RETURN NEW; END $$"
    ));
    sql.push(format!("DROP TRIGGER IF EXISTS {} ON {table}", trigger("guard")));
    sql.push(format!("CREATE TRIGGER {} BEFORE UPDATE OR DELETE ON {table} FOR EACH ROW EXECUTE FUNCTION {guard}()", trigger("guard")));
    sql.push(format!("DROP TRIGGER IF EXISTS {} ON {table}", trigger("changed")));
    sql.push(format!(
        "CREATE TRIGGER {} AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON {table} FOR EACH STATEMENT EXECUTE FUNCTION {notify}()",
        trigger("changed")
    ));
    let folders = settings.folders_table();
    let folder_columns: Vec<String> = FOLDER_COLUMNS.iter().map(|(c, t)| format!("{} {t}", q(c))).collect();
    sql.push(format!(
        "CREATE TABLE IF NOT EXISTS {folders} ({}, CHECK (folder <> '' AND left(folder, 1) <> '/' AND right(folder, 1) <> '/'))",
        folder_columns.join(", ")
    ));
    sql.push(format!("COMMENT ON TABLE {folders} IS 'SuperPuTTY folder settings, inherited by the sessions under each folder of {name}'"));
    for (c, t) in FOLDER_COLUMNS.iter().filter(|(c, _)| *c != "folder") {
        sql.push(format!("ALTER TABLE {folders} ADD COLUMN IF NOT EXISTS {} {t}", q(c)));
    }
    sql.push(format!("DROP TRIGGER IF EXISTS {} ON {folders}", trigger("folders_touch")));
    sql.push(format!("CREATE TRIGGER {} BEFORE INSERT OR UPDATE ON {folders} FOR EACH ROW EXECUTE FUNCTION {touch}()", trigger("folders_touch")));
    sql.push(format!("DROP TRIGGER IF EXISTS {} ON {folders}", trigger("folders_changed")));
    sql.push(format!(
        "CREATE TRIGGER {} AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON {folders} FOR EACH STATEMENT EXECUTE FUNCTION {notify}()",
        trigger("folders_changed")
    ));
    // whoever may read or change the sessions may do the same with their folders
    for (grantee, privileges) in &grants {
        let who = if grantee == "PUBLIC" { "PUBLIC".to_string() } else { q(grantee) };
        sql.push(format!("GRANT {privileges} ON {folders} TO {who}"));
    }
    if !had_folders {
        done.push(format!("added or updated the folder settings table {name}_folders"));
    }
    if !had_triggers {
        done.push("added the change and lock triggers".into());
    }

    let transaction = client.transaction().await.map_err(describe)?;
    for statement in &sql {
        transaction.batch_execute(statement).await.map_err(describe)?;
    }
    transaction.commit().await.map_err(describe)?;
    Ok(if done.is_empty() { "The schema is already up to date.".into() } else { format!("Done: {}.", done.join(", ")) })
}
