# SuperXTerminal: the plan

SuperXTerminal is a macOS counterpart of [SuperXPuTTY](https://github.com/waukeerules/superputty): the same sessions tree, Database sessions, icons, environment colours and tabs, with SSH through the Mac's own `ssh` instead of PuTTY.

This document is the starting point. It compares two ways to get there, lists every SuperXPuTTY feature with what happens to it on the Mac, and sets out the data the two apps share, the UI mapping and the milestones.

Sources: SuperXPuTTY's `FORK_CHANGES.md`, `AGENTS.md`, `docs/DatabaseSessions.md`, `docs/MonacoTerminal.md`, the code and the `web/` pages, as of commit `c1af592` (version 2.5.0.0), and a real Windows install's settings files in `reference/` (see [Reference material](#reference-material)).

## Contents

1. [Option A or B](#1-option-a-or-b)
2. [Feature matrix](#2-feature-matrix)
3. [Sharing data with SuperXPuTTY](#3-sharing-data-with-superxputty)
4. [UI mapping](#4-ui-mapping)
5. [Milestones](#5-milestones)
6. [Open questions](#6-open-questions)

---

## 1. Option A or B

### What can't move to the Mac, whichever option

- **PuTTY.** There's no PuTTY for macOS worth depending on (the Unix PuTTY needs X11/GTK). SSH goes through OpenSSH (`/usr/bin/ssh`), which every Mac has.
- **Embedding other programs' windows.** SuperXPuTTY's `ApplicationPanel` puts PuTTY, mstsc, VNC and console windows inside its tabs with `SetParent`. macOS has no equivalent: one app can't host another app's window. So every tab's content must be drawn by SuperXTerminal itself. That's how SuperXPuTTY's **built-in terminal** already works (xterm.js in a WebView, fed by a pseudo console), and it's the model to copy.
- **Windows-only programs and APIs:** cmd, PowerShell (Windows edition), WSL, Mintty/Cygterm, WinSCP, KiTTY, the RDP ActiveX control, DPAPI, the registry.

### Option A: port SuperXPuTTY itself

Move the existing C# code to a cross-platform UI.

- **The UI must be rewritten anyway.** WinForms is Windows-only (Mono's WinForms is unmaintained and X11-based). Upstream's `upgrade-to-NET10` branch moves to .NET 10, but WinForms on .NET 10 is still Windows-only.
- **Candidates:**
  - **Avalonia.** The most mature. Its `Dock` library could stand in for DockPanelSuite.
  - **.NET MAUI.** Mac Catalyst, desktop-weak; not recommended.
- **What carries over:**
  - **Data, with work:** `SessionData`, `FolderSettings`, `SessionShortcuts`, `SessionOverrides`, `SessionCsv`, `DatabaseSessions` (Npgsql runs on macOS), `SessionVariables`, the SPSL parser, `TerminalKeys` and `SnippetPrompts`. They'd move to a .NET 8+ class library.
  - **Hidden dependencies:** many of those classes reach into `SuperPuTTY` (the static core), WinForms types (`Keys`, `Image`, `TreeNode`) and DPAPI. Untangling that is a refactor of its own.
- **What doesn't:**
  - all 62 forms and dialogs, DockPanelSuite and its copied tab strips, DarkModeForms;
  - the GDI+ icon drawing (`IconTemplates`, `BrandLogos`; System.Drawing is Windows-only on .NET 6+);
  - `ApplicationPanel`, `RdpControl` and `ConPty`.
- **Terminal hosting:**
  - Avalonia has no mature terminal control and no built-in web view.
  - Hosting `web/terminal` would need a third-party WebView (some are paid), and a macOS pseudo-terminal through P/Invoke `forkpty`.
- **Effort:** high. Every screen is rewritten, plus the refactor.
- **Risk:** medium-high (the WebView, and the refactor touching a live Windows app).
- **Fidelity:** good if done well.
- **Pay-off:** one C# codebase for both systems, but only after Windows is moved to Avalonia too, which is a second rewrite of a working app.

### Option B: a new app that reproduces what it can

A new macOS app with SSH through `ssh`. It reads SuperXPuTTY's data files and Database sessions table, and reuses its web pages. The stack is the main choice.

| | **B1: Tauri 2** (Rust core, HTML/TypeScript UI) | **B2: Swift** (SwiftUI + AppKit) | **B3: Electron** (Node core, HTML UI) |
|---|---|---|---|
| Terminal | `web/terminal` (xterm.js) as is, in WKWebView | SwiftTerm (native), or `web/terminal` in a WKWebView | `web/terminal` as is, in Chromium |
| PTY + `ssh` | `portable-pty` crate (also ConPTY on Windows) | `forkpty` / SwiftTerm's `LocalProcess` | `node-pty` |
| Highlighting rules, themes, Highlighted Output (Monaco) | Reused: `web/shared/rules.js`, the JSON files, `web/output` | Rewritten, or the web pages in WKWebView (hybrid) | Reused |
| Postgres | `tokio-postgres` | PostgresNIO | `pg` |
| Secrets | `keyring` crate: Keychain on Mac, Credential Manager on Windows | Keychain (native) | `keytar` (archived) or `safeStorage` |
| Look | HTML/CSS: can copy SuperXPuTTY's VS2015 theme pixel for pixel; native menu bar and windows | Most Mac-like; copying SuperXPuTTY's look is harder | Same as Tauri |
| Size | ~10-20 MB | ~10 MB | ~150 MB |
| **Develop and test on Windows** | **Yes**: runs on Windows with WebView2 (as SuperXPuTTY's pages do) and Windows' own OpenSSH `ssh.exe` | **No**: needs a Mac to build and run | Yes |
| Effort / risk | Medium / low-medium | High / medium; Swift and a Mac are needed from day one | Medium / low, but heavy |

### Decision: B1, Tauri 2

- **Most reuse.** It keeps the parts of SuperXPuTTY that were hardest to get right:
  - the xterm.js terminal page with its decorations, WAITFOR text, paste check, logs and keys;
  - the highlighting rules engine and its JSON;
  - the Monaco Highlighted Output page.
- **Buildable now.** It's developed on a Mac (Apple silicon), and the same code builds on Windows and Linux.
- **The closest look.** HTML/CSS can match SuperXPuTTY's dark (VS2015) and light themes closely: tree rows, the environment-colour circles behind icons, tab stripes and the button bar.
- **Cross-platform.** Tauri builds for macOS (WKWebView), Windows (WebView2) and Linux (WebKitGTK). **The Mac comes first**; the code stays platform-neutral, with platform-specific parts behind small interfaces, and it stays buildable on Windows and Linux. They're polished later. On Windows it's a lighter SuperXPuTTY, without PuTTY windows or the in-tab RDP client; whether it ever replaces SuperXPuTTY is open question 2. Linux caveats: WebKitGTK is slower, so xterm.js may fall back from WebGL to its DOM renderer; secrets go to the Secret Service (GNOME Keyring, KWallet); serial ports are `/dev/ttyUSB*` and `/dev/ttyACM*`.

**The stack:**
- Tauri 2, Rust stable, TypeScript.
- Svelte 5 for the tree, dialogs and settings.
- [dockview](https://dockview.dev) for tabs, splits and floating groups.
- xterm.js 6.0.0 and Monaco 0.57.0, copied from SuperXPuTTY's `web/lib`.
- Crates: `portable-pty`, `tokio-postgres` (with `native-tls` or `rustls`), `quick-xml` + `serde` (Sessions.xml and friends), `keyring`, `notify` (the file watcher), `serialport` (serial sessions), `csv`.

### Licensing

**The aim:** a company can adopt SuperXTerminal without licence problems.

- **SuperXTerminal itself:** MIT, like SuperXPuTTY. Code ported from SuperXPuTTY keeps its MIT copyright notice.
- **What's shipped is permissive:** Tauri, Svelte, dockview, xterm.js, Monaco, `portable-pty`, `tokio-postgres`, `quick-xml`, `keyring` and `csv` are MIT and/or Apache-2.0. Their notices go in a generated `THIRD-PARTY-NOTICES` file in each release.
- **One weak copyleft:** `serialport` is MPL-2.0, which applies per file: only changes to its own files would have to be shared, and our code isn't affected. If zero copyleft is wanted, serial ports can be driven directly instead.
- **Not shipped, so no licence question:** OpenSSH, PuTTY, Windows App, Screen Sharing, iTerm2, FileZilla. The app launches what's already installed.
- **Linux:** WebKitGTK is LGPL, linked dynamically as a system library, which LGPL allows.
- **Fonts:** Segoe UI (Microsoft) and SF Pro (Apple) are never bundled; the system font is used.
- **Brand logos (Simple Icons):** the drawings are CC0, but the logos are their owners' trademarks. They label sessions (normal use), and never appear in SuperXTerminal's own icon, branding or marketing.
- **Kept that way:** `cargo deny check` and `npm run licenses`, run before each release, allow only MIT, Apache-2.0, BSD, ISC, Zlib, CC0, Unicode and MPL-2.0, so a new dependency with any other licence fails the build.

### How terminals are hosted

| Way | Fits the UI? | Notes |
|---|---|---|
| **Embedded: `ssh` in a pseudo-terminal, drawn by xterm.js inside the tab** | **Yes** | Tabs, splits, environment colours, WAITFOR, logs, highlighting, Send Key and the button bar all work, as in SuperXPuTTY's built-in terminal. **This is the main mode.** |
| Launch Terminal.app or iTerm2 (AppleScript, `open -a`) | No | A separate window, outside the tabs. SuperXTerminal can't see its output, so no WAITFOR, logs or highlighting. Offered only as a tab menu item, **Open in Terminal.app / iTerm2**, the way SuperXPuTTY offers Open in PuTTY. |

`ssh` runs as `ssh -tt <args> [user@]host` in the pseudo-terminal. Password, passphrase and host-key prompts appear in the terminal and are answered there, as with plink today.

---

## 2. Feature matrix

**How unsupported features look:** shown but disabled (greyed out), never hidden. A menu item, button or Options row that can't work is disabled, with a tooltip giving the reason, e.g. "Windows only: needs PuTTY". A session the Mac can't open is shown in the tree with a dimmed icon and the same tooltip.

**Key:**
- ✅ **Supported:** works as in SuperXPuTTY.
- 🔁 **Supported differently:** says how.
- ⬜ **Greyed out:** shown, disabled, with the reason.
- ❌ **Dropped:** not shown; it has no meaning on a Mac.

### Sessions tree

| Feature | Mac | How |
|---|---|---|
| Tree of folders and sessions, drag to move, rename, delete, new folder/session | ✅ | |
| Search box (all fields, filter while typing, Search in... fields) | ✅ | |
| Session counts on folders, folders first, sort | ✅ | |
| Filled on demand (placeholder children) | ✅ | Same idea, for large trees |
| Multi-select (Cmd/Shift-click) with Connect, Delete, Edit sessions... | 🔁 | Cmd instead of Ctrl |
| Open sessions marked in the tree | ✅ | |
| Tooltips (protocol, host, notes) | ✅ | |
| Expand tree on startup, show lines, root arrows | ✅ | |
| **Favorites and Recent** (`SessionShortcuts.json`, collapsed state remembered) | ✅ | Per user, in `~/Library/Application Support/SuperXTerminal/` |
| **Private sessions** (`PrivateSessions.xml`) | ✅ | Same file format, per user |
| Duplicate session, Copy host address, Save session as... | ✅ | |
| Edit Session dialog (all fields, hints, where it's kept in the title) | ✅ | PuTTY-only fields are greyed (see [SSH](#ssh-and-other-connections)) |
| **Edit sessions...** (bulk edit, locked fields skipped) | ✅ | |
| **Local overrides** (`SessionOverrides.json`, note, indicator) | ✅ | |
| **Folder defaults** (username, profile, port, jump host, rules) | ✅ | The profile is resolved against the user's own [Profiles](#profiles) |
| **Sessions.xml on a shared folder** | ✅ | The settings folder can be on a mounted SMB share, like on Windows, or a synced cloud folder (the reference install uses OneDrive; on a Mac that's under `~/Library/CloudStorage/`) |
| Sessions.xml reloaded when changed elsewhere (SHA-256, own saves ignored) | ✅ | `notify` crate |
| Use Sessions.xml off (Private and Database only) | ✅ | |
| **CSV export/import** (same columns as the Database table) | ✅ | Same columns, so a file moves between Mac and Windows |
| Import from PuTTY (registry), KiTTY | ✅ | The registry (PuTTY's and KiTTY's) on Windows; PuTTY's `~/.putty/sessions` on the Mac |
| Import from a SuperPuTTY XML file, a PuTTY CM export, PuTTY Portable's session folder | ✅ | Files, so the same everywhere |
| Import from RDP (registry): Remote Desktop's recent computers | ✅ Windows | Greyed on the Mac |
| Export sessions (one Sessions.xml), Edit sessions externally, Reload sessions file | ✅ | The editor is Notepad++ on Windows when it's installed, else the app for .xml files; the Mac's default text editor |
| Import from MobaXterm (`.mxtsessions`) | ✅ | It's a file; SSH entries come in, others greyed |
| Import `.rdp` files | 🔁 | Imported as RDP sessions, which open in Windows App (see RDP) |
| **Import from `~/.ssh/config`** | 🔁 New | Each `Host` becomes a session |

### Database sessions (PostgreSQL)

All supported, against **the same table** (see [Sharing data](#3-sharing-data-with-superxputty)).

| Feature | Mac | How |
|---|---|---|
| Read in pages in the background, cached (`DatabaseSessions.cache.xml`), retried while offline | ✅ | |
| `@db/` ids, never saved to Sessions.xml | ✅ | |
| Writes (Insert/Update/Delete, read back), the outbox while offline | ✅ | |
| Locks: `locked`, `locked_fields`, checked in the UI and enforced by the guard trigger | ✅ | Connects with `application_name = 'SuperPuTTY'` so the trigger treats it as SuperXPuTTY (see §3) |
| Changes noticed every 30 s (`<table>_version`) | ✅ | |
| `deleted_at` soft delete hidden | ✅ | |
| Nickname (`superputty.nickname`, `updated_by`) | ✅ | |
| Folders table (colours, defaults) | ✅ | |
| Move to / Copy to between Database and Saved/Private | ✅ | |
| Database settings dialog, Create / upgrade schema, read-only | ✅ | Schema SQL copied as is |
| Database settings export/import (JSON), Clear | ✅ | Same JSON, so settings move between Mac and Windows |
| Password | 🔁 | Keychain instead of DPAPI; `{env vars}` in it work |

### Icons and colours

| Feature | Mac | How |
|---|---|---|
| Vector icon templates, colourable keys (`switch#1E63B4`) | 🔁 | `IconTemplates` (GDI+ drawing code) is redrawn as SVG; same keys and names |
| Brand logos (Simple Icons, CC0) | ✅ | Already SVG paths in `BrandLogos`; copied |
| Custom icons (one-colour PNGs in `<SettingsFolder>/icons`) | ✅ | Same folder, shared |
| Icon picker, icon size option | ✅ | |
| Old PNG keys mapped onto templates | ✅ | Same mapping table |
| **Environment colours** on tabs, frame around the terminal, circle behind the tree icon | ✅ | |
| Colour from session, else nearest folder, else name rule | ✅ | Same resolution order |
| Environment Colours dialog (name rules) | ✅ | |

### Placeholders, variables, prompts

| Feature | Mac | How |
|---|---|---|
| `{key}` placeholders in host, username, args, scripts, buttons | ✅ | |
| Environment Variables dialog (`EnvironmentVariables.json`) | 🔁 | Same file format; **secret values in the Keychain** instead of DPAPI. Windows-encrypted secrets can't be read on a Mac, so they're asked for again |
| `{prompt:Name}`, `{prompt:Name=default}`, `{secret:Name}` | ✅ | Secrets never logged or remembered |

### SSH and other connections

| Feature | Mac | How |
|---|---|---|
| **SSH** | 🔁 | `ssh -tt` in a pseudo-terminal, inside the tab |
| Host, port, username | ✅ | `-p`, `user@` |
| **PuTTY profile** (`putty_profile`, `PuttySession`) | 🔁 | Resolved against SuperXTerminal's own **Profiles**, by name (see [Profiles](#profiles)). The shared value is never rewritten; each user's profile of that name decides the login |
| Keys and agent (Pageant) | 🔁 | A key file in the [profile](#profiles), or `ssh-agent` and the macOS Keychain (`UseKeychain`, `AddKeysToAgent`). `.ppk` keys without a passphrase are converted for ssh into the user's folder (`keys/`, readable by the user only); one with a passphrase needs converting (`puttygen key.ppk -O private-openssh`) |
| Password (`-pw`, with "allow -pw") | 🔁 | From the session's `-pw` (when allowed) or its [profile](#profiles). **Typed once by the app at the first password prompt**, never put on the command line, in the environment or in logs. (Chosen over an `SSH_ASKPASS` helper, which would also have been asked host-key questions; this way they stay in the terminal.) Only the first prompt is answered, so a later `Password:` such as Cisco's `enable` never gets it. With a password, ssh tries it before keys (`PreferredAuthentications=keyboard-interactive,password,publickey`): Cisco closes the connection after 3 failed attempts, and ssh's keys from `~/.ssh` and the agent would use them up. Stored passwords go in the Keychain |
| Extra arguments (PuTTY syntax) | 🔁 | Translated where `ssh` has an equivalent: `-L`, `-R`, `-D`, `-A`, `-X`, `-C`, `-4`/`-6`, `-v` as is; `-P n` → `-p n`; `-i file` as is (a `.ppk` converted, as a profile's key); `-load` ignored. Anything else is shown in Edit Session as "not used on Mac" |
| PuTTY default parameters (Options) | 🔁 | The "Default Settings" [profile](#profiles)'s extra `ssh` options |
| **Jump hosts** (session path or `user@host:port`, folder defaults) | 🔁 | `-J user@host:port` (OpenSSH ProxyJump); a saved session as the jump host is resolved to its address, as now. Chains work (`-J a,b`), which plink couldn't do |
| **Telnet** | 🔁 | macOS has no `telnet` since 10.13: a small Telnet client in the Rust core (option negotiation, NAWS), drawn in the same terminal |
| **Raw** | 🔁 | A TCP socket in the Rust core |
| **Serial** | 🔁 | The `serialport` crate, devices `/dev/cu.*`; speed, data bits, parity, stop bits and flow control from the session |
| **Rlogin** | ⬜ | "Not supported on Mac"; rare and insecure |
| **Special Commands** | 🔁 | **Serial Break:** `serialport`'s break, so it works on the Mac, unlike plink. **SSH Break:** OpenSSH's `~B` escape. **Telnet Break, Are You There...:** the Rust client sends the IAC codes. SSH signals: ⬜ ("ssh can't send them") |
| **Send Key** (Ctrl+C, Ctrl+Shift+6, Ctrl+]...) | ✅ | `TerminalKeys` ported (it's small and already xterm-style) |
| Dropped sessions reconnect (badge, Reconnect, Stop Reconnecting) | ✅ | `ssh` exiting with 255 starts the same logic as the built-in terminal |
| **Session logs** (kept or temporary, time stamps) | ✅ | Written from the screen by the page, as now |
| KiTTY, PuTTY path, pscp, plink/klink options | ⬜ | Options → Paths rows greyed: "Windows only" |
| Open in PuTTY | 🔁 | Becomes **Open in Terminal.app / iTerm2** |
| **RDP** | 🔁 | In-tab: ⬜. The session opens in Microsoft's **Windows App** (formerly Microsoft Remote Desktop) from a generated `.rdp` file, outside the tabs. Most RDP options map onto `.rdp` file settings |
| **VNC** | 🔁 | Opens in macOS Screen Sharing (`open vnc://host:port`), outside the tabs |
| **SCP file browser** (pscp) | ⬜ in the MVP | Later 🔁 with `sftp` |
| WinSCP | ⬜ | "Windows only" |
| FileZilla | 🔁 | FileZilla exists for macOS: opened with `sftp://user@host:port` |
| **cmd, PowerShell, WSL** | 🔁 / ⬜ | Opened as **Local Shell** (the user's login shell, zsh) for cmd and WSL sessions, which are marked "opened as Local Shell". PowerShell sessions use `pwsh` if installed, else ⬜. A new protocol, `LocalShell`, is added for Mac-made sessions |
| Mintty, Cygterm, Xorg (VcXsrv) | ⬜ | "Windows only" |

### Profiles

**The idea:** shared data (Sessions.xml, the Database table) says *what* a session is and how it looks in the tree: name, folder, host, protocol, icon, colour, and which profile it uses. *How each person logs in* is their own business. On Windows that's their PuTTY profile of that name in the registry; on the Mac it's their SuperXTerminal profile of that name. So "Media Server" with `PuttySession="root"` logs in as `root` for one person, with a key, and with a different account and a password for another, with no change to the shared row.

- **What a profile holds:**
  - **Login:** username, password, and an SSH key file or the agent.
  - **Connection:** port, jump host, keepalive, port forwards, and extra `ssh` options.
  - **Terminal:** cursor, bell and keys, read by `TerminalPreferences`' `"profile"` values.
  - **SSH config host:** optional, a `Host` from `~/.ssh/config` used for anything the profile doesn't set.
- **Where they're kept:** per user, in `Profiles.json` (see §3); passwords and key passphrases in the Keychain, never in a file. They're never written to the settings folder or the Database table.
- **Order at launch, first wins:**
  1. Local override
  2. The session's own field
  3. The folder's default
  4. The profile
  5. "Default Settings", then built-in defaults

  It's the same order as SuperXPuTTY, where a session's own username beats its PuTTY profile's.
- **"Default Settings"** is a built-in profile that always exists, like PuTTY's. Every other profile falls back to it.
- **A profile that isn't set up on this Mac:** the session still connects with "Default Settings". Its tree tooltip and Edit Session say "Profile 'root' isn't set up on this Mac", with a button that creates it.
- **Turned into `ssh` arguments** per launch: `-l`, `-p`, `-i … -o IdentitiesOnly=yes`, `-J`, `-o …`; the password is typed at the first prompt (see Password).
- **Filling them:** created by hand in Settings → Profiles (Cmd+,). A profile also holds keep-alive, a `~/.ssh/config` Host (the session's address is still used: `-o HostName=`), and the terminal's cursor and bell. Later, optionally, imported from a `reg export` of `HKCU\Software\SimonTatham\PuTTY\Sessions`, keeping the fields `ssh` can use.
- **Telnet, Raw and Serial** use a profile's terminal settings, and Serial its line settings, when they're set.
- **Older devices.** PuTTY still accepts old algorithms (with a warning); macOS's OpenSSH 10 refuses them. So Cisco gear that worked in SuperXPuTTY fails with "no matching key exchange method" or "no matching cipher". A profile's **Allow older algorithms** option (on in "Default Settings", so sessions behave as they did in PuTTY) adds them for that connection only: `-o KexAlgorithms=+diffie-hellman-group14-sha1,diffie-hellman-group-exchange-sha1 -o HostKeyAlgorithms=+ssh-rsa -o PubkeyAcceptedAlgorithms=+ssh-rsa -o Ciphers=+aes256-cbc,aes192-cbc,aes128-cbc`. Checked against the lab switch (IOS, 2026-11-01). `diffie-hellman-group1-sha1` and `3des-cbc` are left out: OpenSSH 10 no longer has them.

### Terminal

All of this comes with the reused `web/terminal` page.

| Feature | Mac | How |
|---|---|---|
| Highlighting as you type (rules: cisco, junos, generic, auto, user rules) | ✅ | Same JSON |
| Rules per session and folder, per-tab choice | ✅ | |
| Themes, font, cursor, bell, copy on select, mark matches | ✅ | `TerminalPreferences`, one JSON setting, as now. Its default for cursor, bell and keys is `"profile"`; on the Mac that means the session's [profile](#profiles) |
| Paste check (several lines, production sessions) | ✅ | |
| Find, clear scrollback/screen, links, unlimited scrollback | ✅ | Cmd shortcuts added beside the Ctrl ones |
| **Highlighted Output** (Monaco, beside the tab; the terminal menu's Open as Document) | ✅ | `web/output` reused: the screen rebuilt by a hidden xterm.js, then shown in Monaco with the same rules and themes |
| Search Output, Activity and Silence alerts | ✅ | |
| Classic look of PuTTY windows | ❌ | No PuTTY windows |

**Not through iTerm2.** Highlighting, the right-click menu and Monaco all need SuperXTerminal to see and draw the output, and it can't see or draw inside another app's window. So they live in SuperXTerminal's own terminal tab (xterm.js, the same page as SuperXPuTTY's built-in terminal). It's the main and default way sessions open. Terminal.app and iTerm2 are only an "Open in..." escape hatch, without these features.

### Terminal right-click menu

The menu is built by the page itself (`web/terminal/terminal.js`), so it comes across as it is. Item by item, from SuperXPuTTY 2.5:

| Item | Mac | How |
|---|---|---|
| Copy, Paste, Copy and Paste | ✅ | Shortcuts shown as Cmd+C / Cmd+V on the Mac; Ctrl+Insert and Shift+Insert still work. Every paste goes through paste check |
| Find..., Select All | ✅ | Cmd+F, Cmd+A |
| Clear Scrollback, Clear Screen, Clear Screen and Scrollback | ✅ | Cmd+K for Clear Screen and Scrollback, as in Terminal.app |
| **Open as Document** | ✅ | Highlighted Output: the output in Monaco, beside the tab |
| Highlighting, Theme, Bell, Font, Cursor (submenus) | ✅ | Same `TerminalPreferences`; Highlighting is for this tab only |
| Copy on Select, Mark Matches of Selection | ✅ | |
| Send Key | ✅ | `TerminalKeys` |
| Open in PuTTY (Special Commands) | 🔁 | Becomes two items: **Special Command** (a submenu sent by SuperXTerminal itself, see SSH and other connections) and **Open in Terminal.app / iTerm2** |
| Close Tab, Close Other Tabs, Close Tabs to the Right, Close All Tabs | ✅ | Cmd+W for Close Tab |

The menu is drawn by the page in the app's theme, as on Windows, rather than as a native macOS menu, so it looks the same on both.

### Commands and scripting

| Feature | Mac | How |
|---|---|---|
| **Button bar** (`ButtonBars.xml`, bars per icon, JSON export/import) | ✅ | Same file, shared |
| **Send Commands** toolbar (to active/selected tabs, mask button, history) | ✅ | |
| **SPSL**: SENDLINE, SENDCHAR, SENDKEY (modifiers, symbols, repeats), WAITFOR, SLEEP, PROMPT, PWDPROMPT, OPENSESSION | ✅ | Parser and runner ported to TypeScript; WAITFOR reads the page's output, as the built-in terminal does |
| SENDBREAK, CLOSESESSION | ⬜ | Listed "not available", as in SuperXPuTTY. SENDBREAK could become possible for serial and SSH (see Special Commands); open question |
| Script Editor (Run as, saved scripts library, reference, Check script, Always on top) | ✅ | Library in `<SettingsFolder>/scripts`, shared |
| Session's own script (`SPSLFileName`, run on open; URL too) | 🔁 | Windows paths (`C:\...`) can't be found: shown as a warning; URLs and paths in the settings folder work |

### Windows, tabs, app

| Feature | Mac | How |
|---|---|---|
| Tabs (wide, up to 30 characters, icons, colour stripe, close buttons, tab list) | ✅ | dockview |
| Splits and docking, floating windows, multiple monitors | 🔁 | dockview groups and splits; floating groups become separate windows |
| **Layouts** (save, restore, default layout) | 🔁 | Own JSON format; SuperXPuTTY's DockPanelSuite layout XML can't be read. Stored apart from Windows layouts (`layouts-mac/`) so they don't mix |
| Quick connect toolbar (Protocol, Host, Login, Password, Session) | ✅ | |
| **Quick Selector** | ✅ | |
| Keyboard shortcuts (editor), Ctrl+Tab switching, global hotkeys | 🔁 | Cmd-based defaults; global hotkeys through Tauri's global-shortcut plugin |
| Single instance, command-line arguments (`-session`, `ssh://`) | ✅ | Plus a `superxterminal://` / `ssh://` URL handler |
| Minimise to tray | 🔁 | A menu-bar extra (status item) with the session list |
| Themes (Dark, Dark Blue, Light, Classic) | 🔁 | Follows macOS Light/Dark by default, with SuperXPuTTY's themes to choose from |
| Options dialog | 🔁 | **Settings** window (Cmd+,), same tabs; rows for Windows-only paths greyed |
| Log viewer, log file | ✅ | `~/Library/Logs/SuperXTerminal/` |
| Update check (this fork's releases) | ✅ | GitHub releases of SuperXTerminal; Tauri's updater later |
| Installer (WiX MSI, portable zip) | 🔁 | `.dmg`, then signed and notarised (see Milestones) |

---

## 3. Sharing data with SuperXPuTTY

**The goal:** Windows and Mac users of one team share one settings folder and one Database table, and each app shows the other's sessions.

### Files in the settings folder (shared)

| File | Mac reads | Mac writes | Notes |
|---|---|---|---|
| `Sessions.xml` | ✅ | ✅ | Same XML (`ArrayOfSessionData`, attributes as now). The Mac app **keeps unknown attributes** on save, so a newer Windows field isn't lost. **The other direction doesn't hold:** SuperXPuTTY reads the file with `XmlSerializer` and has no `XmlAnyAttribute`, so a Mac-only attribute is dropped the next time a Windows user saves. **Unknown protocols:** an unknown `Proto` value (e.g. `LocalShell`) makes `XmlSerializer` throw, and SuperXPuTTY then loads **none** of the Saved sessions. (Database sessions are different: there an unknown protocol is read as SSH.) So the Mac app writes no new protocol or attribute to Sessions.xml until SuperXPuTTY can read it (open question 4) |
| `Folders.xml` | ✅ | ✅ | |
| `ButtonBars.xml` | ✅ | ✅ | |
| `icons/` | ✅ | ✅ | |
| `scripts/` (`.spsl`, `.txt`) | ✅ | ✅ | Line endings kept as found |
| `highlighting/` (rules, themes JSON) | ✅ | ✅ | |
| `layouts/` (DockPanelSuite XML) | ❌ | ❌ | Mac layouts go in `layouts-mac/` |

The file watcher's SHA-256 check works the same way, so each app reloads when the other saves.

### Per-user files (not shared; each machine has its own)

`SessionShortcuts.json`, `SessionOverrides.json`, `PrivateSessions.xml`, `EnvironmentVariables.json`, `DatabaseSessions.cache.xml`, `DatabaseSessions.outbox.xml`, and the Mac's own `Profiles.json` (the Windows counterpart is the user's PuTTY profiles in the registry). They're in `~/Library/Application Support/SuperXTerminal/`. Their formats are the same, so a user can copy theirs over, except secret values (DPAPI on Windows, Keychain on Mac), which are asked for again.

### The Postgres table

- **Same table and schema.** The Mac app runs the same Create / upgrade schema SQL (copied from SuperXPuTTY). It never changes the schema on its own, and never when read-only.
- **Same rules:** `@db/` ids, `deleted_at` hidden, `UNIQUE (folder, name)`, the folders table, the `<table>_version` table raised by the change trigger and checked every 30 s, and `updated_by` from `superputty.nickname`.
- **Locks.** The guard trigger enforces `locked` and `locked_fields` only for connections whose `application_name` is `SuperPuTTY`. The Mac app must connect with that same name, or the trigger would treat it as an outside program and let it change locked rows. A distinct name (e.g. `SuperXTerminal`) would need the trigger changed in both apps; that's an open question.
- **New columns:** one that only the Mac needs goes through the same optional-column path (`DatabaseSessions.OptionalWriteColumns`), so neither app breaks the other.

### Windows-specific values in shared data

| Value | Mac behaviour |
|---|---|
| Protocols `RDP`, `VNC` | Open in Windows App / Screen Sharing, outside the tabs |
| Protocols `WINCMD`, `WSL` | Open as Local Shell, marked so |
| Protocol `PS` | `pwsh` if installed, else greyed |
| Protocols `Mintty`, `Cygterm`, `Rlogin` | Greyed: dimmed icon, can't be opened, tooltip says why |
| `PuttySession` / `putty_profile` | The user's SuperXTerminal profile of that name, else "Default Settings" (Edit Session says so); never rewritten |
| `ExtraArgs` / `extra_args` | Translated per launch (§2); never rewritten |
| `-i C:\keys\x.ppk` | Not found: warning, connects without it; found (a shared drive), converted for ssh |
| `SPSLFileName = C:\...` | Not found: warning; URLs and relative paths work |
| `rdp_options` | Mapped to `.rdp` settings |
| `LocalPath` / `RemotePath` (SCP browser) | Ignored until the SFTP browser exists |
| Serial line (`COM6`) in host | Asked once per machine which `/dev/cu.*` device it is, kept per user |

**The rule:** the Mac app never rewrites a Windows-specific value just because it can't use it. It adapts it at launch time, as `SessionVariables.Resolve` does for placeholders.

---

## 4. UI mapping

Aim: someone who knows SuperXPuTTY recognises every screen. The main window keeps SuperXPuTTY's layout. Menus move to the macOS menu bar, and shortcuts use Cmd where macOS expects it.

| SuperXPuTTY | SuperXTerminal | Notes |
|---|---|---|
| Main window: menus File/View/Tools/Help | Main window + macOS menu bar (SuperXTerminal, File, Edit, View, Session, Tools, Window, Help) | Same items, in the same order inside each menu |
| Quick connect toolbar | Same toolbar, under the title bar | Protocol list shows only Mac-capable protocols enabled, the rest greyed |
| Send Commands toolbar (and mask button) | Same | |
| Sessions pane (DockPanelSuite tool window, left) | Left sidebar, resizable, collapsible (Cmd+Ctrl+S) | Same tree look: icons with colour circles, counts, bold Favorites/Recent/Private/Database roots |
| Document tabs (DockPanelSuite, wide tabs, colour stripe) | dockview tabs, styled to match | Tab strip CSS copied from the dark/light themes |
| Splits, floating windows | dockview splits; pop out to a window | |
| Tab context menu (Rename, Duplicate, Restart, Send Key, Special Command, Search Output, Close...) | Same menu | PuTTY items (Change settings, Event log, About PuTTY) greyed |
| Built-in terminal's own right-click menu | Same (it's the same page) | See [Terminal right-click menu](#terminal-right-click-menu) |
| Button bar above the status bar | Same | |
| Status bar (status text, script progress and Stop, version) | Same | |
| Options dialog (Paths, GUI, Shortcuts, Advanced, RDP, Terminal) | Settings window (Cmd+,), same tabs | Paths: PuTTY, pscp, plink, Mintty, WinSCP, VNC, Xorg rows greyed; adds `ssh` path, SSH config, Terminal.app/iTerm2 choice. RDP tab: options for the `.rdp` file. New **Profiles** tab: the user's profiles (see §2 Profiles) |
| Edit Session | Sheet or window, same fields | "PuTTY profile" becomes "Profile": a list of the user's profiles plus the stored name, with Edit profile... beside it; greyed fields explained |
| Edit sessions... (bulk), Local overrides, Environment Colours, Environment Variables, Database settings, Edit Button, Script Editor, Icon picker, Quick Selector, Paste check, Search Output, Rename | Windows or sheets with the same fields | |
| Messenger (themed message boxes) | Native alerts or themed modal | Native alerts follow the system theme on their own |
| Log viewer | Window | |
| System tray icon | Menu-bar extra | |

**Keeping the look close:**
- Carry over SuperXPuTTY's theme colours as CSS variables, taken from the DockPanelSuite VS2015 themes and DarkModeForms.
- Use SuperXPuTTY's fonts, or SF Pro where Segoe UI is used.
- Draw the icons from the same templates.
- Take screenshots of each SuperXPuTTY window as the reference for each screen.
- One concession: native macOS window chrome (traffic lights, title bar).

### Reference material

`reference/` holds a copy of a working Windows install's files: `Sessions.xml`, `PrivateSessions.xml`, `ButtonBars.xml`, `SessionShortcuts.json`, `SessionOverrides.json`, `EnvironmentVariables.json`, `DatabaseSessions.cache.xml`, the Database settings export, `SuperXPuTTY.settings`, `layouts/` and `scripts/`, plus screenshots. **It's git-ignored**, because it holds hosts, usernames and secrets. Use it as test data for the readers and writers, and never copy values from it into the repo.

What the reference install shows (main window screenshot, Light theme):
- **Settings:** `InterfaceTheme` 2 (Light), icon size 24, sessions tree in Segoe UI 12 pt, built-in terminal and Highlighted Output on, settings folder on OneDrive.
- **Toolbars:** the main window has two rows of toolbars under the menu (File, View, Tools, Help). The first is Quick connect: Protocol, Host, Login, Password, Session, Connect, Clear. The second is Send Commands: Command, Send, mask, script, editor, Stop.
- **Sessions tree:** search box with Search and Clear; a toolbar (New session, New folder, Expand, Collapse, Sort); roots Favorites, Recent, Saved, Private and Database sessions, each with a count. Session icons sit on a filled environment-colour circle (red, green, purple), with a small green dot when the session is open.
- **Tabs:** a coloured tab (red for production) with an icon and close button, a red frame around the terminal, and the tab list arrow at the right end.
- **Bottom:** the button bar (bar picker "Default", then coloured round buttons or icons with labels), and below it the status bar (status text on the left, version and layout on the right).

**Screenshots still wanted** (each in Light and Dark, at 100 % scaling if possible):
1. The main window in Dark, with several tabs open, including a split and a disconnected tab with its badge.
2. Session context menu, folder context menu, and each root's menu (Favorites, Private, Database).
3. Tab context menu, including Send Key and Special Command opened, and the terminal's own right-click menu.
4. Edit Session (each protocol that changes its fields: SSH, Serial, RDP), Folder settings, Local overrides, Edit sessions... (bulk).
5. Options: every tab (Paths, GUI, Shortcuts, Advanced, RDP, Terminal).
6. Database settings, Environment Colours, Environment Variables.
7. Script Editor with the library and reference panes; Edit Button; the icon picker.
8. Quick Selector, Paste check, Search Output, and Highlighted Output beside a tab.
9. File, View, Tools and Help menus opened.

**Files still wanted:** `Folders.xml` and `PrivateFolders.xml` if the install has them (folder colours and defaults), a user's `highlighting/` and `icons/` folders if any, and, optionally, a `reg export` of the PuTTY profiles (only if the profile import is built).

---

## 5. Milestones

Sizes are rough, for one developer working with an AI assistant. "w" means weeks.

| # | Milestone | Size | Done when |
|---|---|---|---|
| M0 | **Skeleton**: Tauri app, Svelte, dockview, licence checks, local builds | 1 w | The app opens on a Mac, with an empty tree and one local shell tab |
| M1 | **MVP**: SSH tabs (`ssh` in a PTY, `web/terminal`), sessions tree from Sessions.xml/PrivateSessions.xml/Folders.xml (read and write), icons (templates redrawn as SVG, logos, custom), environment colours, quick connect, Edit Session, Profiles (with Keychain passwords and keys), placeholders with Keychain secrets, Settings basics | 5-6 w | A Mac user opens the team's Sessions.xml and connects to switches with the right icons and colours |
| M2 | **Database sessions**: read, cache, outbox, locks, version polling, folders table, writes, Move/Copy, settings dialog and JSON | 3-4 w | Mac and Windows users edit the same table and see each other's changes within 30 s |
| M3 | **Scripting and commands**: button bar, Send Commands, SPSL (WAITFOR, SENDKEY...), Script Editor and library, snippet prompts, Send Key | 3 w | The existing button bars and scripts run unchanged |
| M4 | **Terminal extras**: highlighting rules and preferences in Settings, Highlighted Output, Search Output, alerts, paste check, logs, reconnect | 2 w | |
| M5 | **The rest**: Favorites/Recent, multi-select, bulk edit, local overrides, CSV, file watcher, Quick Selector, jump hosts (`-J`), extra-args translation, Telnet/Raw/Serial with Special Commands, RDP/VNC/FileZilla launchers, layouts, menu-bar extra, `~/.ssh/config` import | 4 w | Feature matrix complete except the SFTP browser |
| M6 | **Distribution**: Developer ID signing, notarisation, auto update, `.dmg` design, third-party notices | 1 w | A download opens on any Mac without warnings |
| Later | SFTP file browser | 2-3 w | |

**Total to M6:** about 19-21 weeks. Progress is tracked in [ROADMAP.md](ROADMAP.md).

### Developing on a Mac

- **Development** happens on a Mac (Apple silicon, macOS 27, Xcode installed). Rust stable via `rustup`, Node LTS for the UI build.
- **The Mac-only parts are checked all along**, not at the end: the Keychain, the menu bar and app menu, WKWebView (fonts, IME, clipboard), `/dev/cu.*` serial devices, Windows App and Screen Sharing launching.
- **Other platforms:** keep platform-specific code behind small interfaces. Windows (WebView2, ConPTY, Windows' own `ssh.exe`, Credential Manager) and Linux (WebKitGTK, Secret Service) are built and tried on a VM now and then; they're polished after the Mac.
- **Builds and releases are local**, as for SuperXPuTTY: `npm run tauri build` on the Mac, then the release is pushed to GitHub (`waukeerules/SuperXTerminal`, private). Pushes start no GitHub Actions run, since those cost money. The workflow in `.github/workflows/build.yml` (macOS, Windows, Linux) runs only when started by hand, for an occasional check of the other platforms.
- **The development signature.** A fresh debug build is a new app to macOS, so the Keychain asks for every saved password again after each rebuild. The Keychain lets an app in by its Apple team ID (each item's partition list); a build without one is let in by its exact hash, which every rebuild changes. So development builds are signed before they run (`scripts/dev-sign-run.sh`, Cargo's runner in `src-tauri/.cargo/config.toml`) with an **Apple Development** certificate, which has a team ID, and "Always Allow" then lasts. It's free with any Apple ID; once per Mac:
  1. Xcode → Settings → Accounts → + → Apple ID, and sign in.
  2. Select the account's team (a "Personal Team" for an Apple ID without a paid membership) → Manage Certificates… → + → Apple Development.
  3. The next time the app asks for a Keychain password, choose Always Allow (once per saved password).

  Without it, builds run unsigned, as before. (A self-signed certificate, tried first, isn't enough: it has no team ID.)
- **Running an unsigned Mac build.** Right-click → Open, or `xattr -dr com.apple.quarantine SuperXTerminal.app`. Local builds aren't quarantined.
- **Signing and notarisation (M6)** need an Apple Developer Program membership (USD 99/year) for a Developer ID certificate, or the company's own. `notarytool` runs locally with an app-specific password or API key kept in the Keychain. Windows signing needs a code-signing certificate.
- **Copying from SuperXPuTTY:**
  - `web/terminal`, `web/output`, `web/shared` and `web/lib` copy across with their licences (xterm.js and Monaco: MIT; Simple Icons: CC0).
  - SuperXPuTTY itself is MIT, so its code can be ported with the copyright notice kept.
  - The pages talk to the host through `chrome.webview.postMessage` (WebView2). A small shim maps that to Tauri's `invoke`/events, so the pages need no other change. They load their scripts by relative paths; SuperXPuTTY serves them from the made-up host `superputty-output.local`, and Tauri's bundled files or a custom protocol do the same.
  - **The host side is more than a shim.** The pages' messages (`config`, `data`, `ack`, `log`, `pasteRequest`, `pref`...) are answered by `Gui/TerminalControl.cs` (about 1,300 lines). It builds `config` from the rules and themes folders, the preferences and the PuTTY profile, writes the log, and runs paste check, WAITFOR marks and reconnect. That's ported to Rust and TypeScript in M1 and M4, and counted in their sizes.

---

## 6. Open questions

1. **Stack:** decided: Tauri 2, Svelte 5 and dockview, with no licence problems for companies that adopt it (§1 Licensing).
2. **Cross-platform:** the Mac comes first, and the code stays buildable on Windows and Linux. Still open: should the Windows build ever aim to replace SuperXPuTTY, or stay a lighter option beside it?
3. **Database application name:** connect as `SuperPuTTY` (no schema change, locks enforced as now), or as `SuperXTerminal` (needs the guard trigger to accept both, changed in SuperXPuTTY's schema SQL too)?
4. **New protocol `LocalShell` in shared Sessions.xml:** today an unknown protocol stops SuperXPuTTY loading Sessions.xml at all (§3). So SuperXPuTTY must change first: read an unknown protocol as greyed, and keep unknown attributes (`XmlAnyAttribute`). Do that, and ship it before the Mac app writes such sessions, or keep Mac-only sessions in Private sessions only?
5. **Profiles:** decided: SuperXTerminal's own per-user profiles, matched by name (§2 Profiles). Still open: is the `reg export` import worth building, and should the non-secret parts of profiles ever be shareable through the settings folder?
6. **RDP:** decided: an in-tab client built on IronRDP (MIT/Apache), with Windows App kept as an option (Settings > Remote Desktop) and for Connect external.
7. **SENDBREAK:** make it work on the Mac for serial and SSH sessions (it can), or keep it "not available" to match SuperXPuTTY?
8. **Distribution:** deferred (November 2026); the Mac release build, its .dmg, icon and notices are done, and the rest waits on two decisions:
   - **Developer ID and notarisation:** needs an Apple Developer Program membership (USD 99 a year). Only needed if people other than the developer install the app; without it, other Macs warn ("unidentified developer") and need right-click > Open. Builds are signed with an Apple Development certificate meanwhile.
   - **Where releases live:** the repository is private, so the app's update check can't see its releases (no token), and an auto updater couldn't either. Make the repository public, publish releases somewhere public (a public repository or a web server), or keep updating by hand. Tauri's updater also needs its own signing key (free, separate from Apple's); without notarisation, an update on another Mac still meets the warning.
   - **Also deferred:** a universal (Intel and Apple silicon) build, documented in BUILD-macOS.md but not tried.
9. **Mac hardware:** decided: development happens on a Mac.
10. **Repository:** name `SuperXTerminal` under `waukeerules`, public or private, and MIT licence like SuperXPuTTY?
