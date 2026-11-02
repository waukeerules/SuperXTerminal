# SuperXTerminal: roadmap

Progress through the milestones in [PLAN.md §5](PLAN.md#5-milestones). Each line is a feature from the [feature matrix](PLAN.md#2-feature-matrix).

**Key:**
- ✅ **Done:** delivered and checked on a Mac.
- 🚧 **In progress.**
- ⬜ **Not started.**

**How lines change:** a line becomes ✅ in the same commit that delivers the feature, with a note of how it was checked when that isn't obvious. A milestone is done when all its lines are ✅ and its "done when" holds.

| Milestone | Status |
|---|---|
| [M0 Skeleton](#m0-skeleton) | 🚧 |
| [M1 MVP](#m1-mvp) | ⬜ |
| [M2 Database sessions](#m2-database-sessions) | ⬜ |
| [M3 Scripting and commands](#m3-scripting-and-commands) | ⬜ |
| [M4 Terminal extras](#m4-terminal-extras) | ⬜ |
| [M5 The rest](#m5-the-rest) | ⬜ |
| [M6 Distribution](#m6-distribution) | ⬜ |
| [Later](#later) | ⬜ |

---

## M0 Skeleton

**Done when:** the app opens on a Mac, with an empty tree and one local shell tab; CI builds all three platforms.

- ✅ Rust (`rustup`) and Node toolchain set up; versions pinned (`rust-toolchain.toml`, `.nvmrc`)
- ✅ Tauri 2 + Svelte 5 + TypeScript app that opens a window
- ✅ Main window layout: menu bar, toolbars, sessions sidebar, tab area, button bar, status bar (empty, with tooltips saying which milestone fills them)
- ✅ dockview tab area with one tab (and more, from New Local Shell)
- ✅ PTY (`portable-pty`) running the login shell in that tab, drawn by `web/terminal` (xterm.js) through the `chrome.webview` shim; the page starts the shell (checked), drawing and typing checked in the app (typed by Claude, read back by screenshot)
- ✅ `web/terminal`, `web/output`, `web/shared` and `web/lib` copied from SuperXPuTTY with their licences (default rules and themes still missing: not in SuperXPuTTY's repository)
- ✅ Licence checks (`cargo deny check` in `src-tauri/`, `npm run licenses`), including security advisories
- 🚧 CI: macOS, Windows and Linux builds, unsigned; workflow written, not run until the repository is on GitHub
- ⬜ Known issue: in the app (WKWebView) a small round macOS badge sits beside the terminal cursor and covers the end of the prompt. Not in the page's DOM, not a separate window. Not fixed by `writingsuggestions="false"`, a transparent caret, or `TSMLanguageIndicatorEnabled` off for the app. Next: find it with Web Inspector

## M1 MVP

**Done when:** a Mac user opens the team's Sessions.xml and connects to switches with the right icons and colours.

**Data**
- ⬜ Read and write `Sessions.xml`, keeping unknown attributes
- ⬜ Read and write `PrivateSessions.xml`
- ⬜ Read and write `Folders.xml` / `PrivateFolders.xml` (colours, defaults)
- ⬜ Settings folder choice (local, SMB share, synced cloud folder)
- ⬜ Per-user files in `~/Library/Application Support/SuperXTerminal/`

**Sessions tree**
- ⬜ Saved and Private roots, folders and sessions, filled on demand
- ⬜ Folder session counts, folders first, sort
- ⬜ Search box (filter while typing)
- ⬜ Open sessions marked; tooltips
- ⬜ New, rename, delete, drag to move (sessions and folders)
- ⬜ Sessions the Mac can't open shown dimmed with the reason

**Icons and colours**
- ⬜ Icon templates redrawn as SVG, with colourable keys
- ⬜ Brand logos
- ⬜ Custom icons from `<SettingsFolder>/icons`
- ⬜ Old PNG keys mapped onto templates
- ⬜ Environment colours: tree circle, tab, frame around the terminal
- ⬜ Colour resolution: session, else folder, else name rule

**Connecting**
- ⬜ SSH tab: `ssh -tt` in a PTY, host, port, username
- ⬜ Profiles: Settings → Profiles, "Default Settings", matched by name
- ⬜ Profile and session passwords in the Keychain, through the askpass helper
- ⬜ Key files and agent
- ⬜ Allow older algorithms (profile option, on by default): SHA-1 key exchange, `ssh-rsa`, CBC ciphers
- ⬜ Launch order: override, session, folder default, profile, defaults
- ⬜ `{key}` placeholders; Environment Variables with Keychain secrets
- ⬜ Quick connect toolbar
- ⬜ Edit Session (SSH fields, profile list, greyed fields explained)

**Settings**
- ⬜ Settings window basics (settings folder, theme, icon size, tree font)
- ⬜ Light and Dark themes matching SuperXPuTTY

## M2 Database sessions

**Done when:** Mac and Windows users edit the same table and see each other's changes within 30 s.

- ⬜ Connect as `SuperPuTTY`, with the nickname
- ⬜ Read in pages in the background; `@db/` ids
- ⬜ Cache (`DatabaseSessions.cache.xml`) and retry while offline
- ⬜ Writes (insert, update, delete, read back) and the outbox
- ⬜ Locks (`locked`, `locked_fields`) checked in the UI
- ⬜ Changes noticed every 30 s (`<table>_version`)
- ⬜ `deleted_at` rows hidden
- ⬜ Folders table (colours, defaults)
- ⬜ Move to / Copy to between Database and Saved/Private
- ⬜ Database settings dialog, Create / upgrade schema, read-only
- ⬜ Database settings export/import (JSON), Clear; password in the Keychain

## M3 Scripting and commands

**Done when:** the existing button bars and scripts run unchanged.

- ⬜ Button bar (`ButtonBars.xml`, bars per icon, JSON export/import)
- ⬜ Send Commands toolbar (active/selected tabs, mask, history)
- ⬜ `{prompt:...}` and `{secret:...}` snippet prompts
- ⬜ SPSL parser and runner: SENDLINE, SENDCHAR, SENDKEY, WAITFOR, SLEEP, PROMPT, PWDPROMPT, OPENSESSION
- ⬜ Script Editor (Run as, library, reference, Check script, Always on top)
- ⬜ Session's own script on open
- ⬜ Send Key (`TerminalKeys`)

## M4 Terminal extras

- ⬜ Highlighting rules (cisco, junos, generic, auto, user rules), per session and folder, per tab
- ⬜ Terminal preferences in Settings (theme, font, cursor, bell, copy on select, mark matches)
- ⬜ Highlighted Output (Monaco) beside the tab, from the menu's Open as Document
- ⬜ Terminal right-click menu complete, with Mac shortcuts (Cmd+C/V/F/K/W)
- ⬜ Search Output; Activity and Silence alerts
- ⬜ Paste check
- ⬜ Session logs (kept or temporary, time stamps)
- ⬜ Dropped sessions reconnect (badge, Reconnect, Stop Reconnecting)

## M5 The rest

**Done when:** the feature matrix is complete except the SFTP browser.

**Tree and data**
- ⬜ Favorites and Recent (`SessionShortcuts.json`)
- ⬜ Multi-select with Connect, Delete, Edit sessions...
- ⬜ Edit sessions... (bulk edit)
- ⬜ Local overrides (`SessionOverrides.json`)
- ⬜ Duplicate session, Copy host address, Save session as...
- ⬜ CSV export/import
- ⬜ Sessions.xml reloaded when changed elsewhere
- ⬜ Use Sessions.xml off
- ⬜ Import: MobaXterm, `.rdp` files, `~/.ssh/config`
- ⬜ Environment Colours dialog

**Connections**
- ⬜ Jump hosts (`-J`, chains, saved sessions as jump hosts)
- ⬜ Extra arguments translated from PuTTY syntax
- ⬜ Telnet (Rust client)
- ⬜ Raw (TCP)
- ⬜ Serial (`/dev/cu.*`, asked once per COM name)
- ⬜ Special Commands (Serial Break, SSH `~B`, Telnet IAC)
- ⬜ Local Shell for cmd/WSL sessions; `pwsh` for PowerShell
- ⬜ RDP in Windows App; VNC in Screen Sharing; FileZilla
- ⬜ Open in Terminal.app / iTerm2

**App**
- ⬜ Quick Selector
- ⬜ Splits, pop-out windows
- ⬜ Layouts (`layouts-mac/`)
- ⬜ Keyboard shortcuts editor, Cmd+Tab-style switching, global hotkeys
- ⬜ Single instance, command-line arguments, `ssh://` and `superxterminal://` URLs
- ⬜ Menu-bar extra
- ⬜ Log viewer
- ⬜ Update check

## M6 Distribution

**Done when:** a download opens on any Mac without warnings.

- ⬜ Developer ID signing and notarisation
- ⬜ `.dmg` (designed)
- ⬜ `THIRD-PARTY-NOTICES` generated per release
- ⬜ Auto update
- ⬜ Signed Windows MSI; Linux `.deb`, `.rpm`, AppImage

## Later

- ⬜ SFTP file browser
- ⬜ Import PuTTY profiles from a `reg export`
