# SuperXTerminal: roadmap

Progress through the milestones in [PLAN.md §5](PLAN.md#5-milestones). Each line is a feature from the [feature matrix](PLAN.md#2-feature-matrix).

**Key:**
- ✅ **Done:** delivered and checked on a Mac.
- 🚧 **In progress.**
- ⬜ **Not started.**

**How lines change:** a line becomes ✅ in the same commit that delivers the feature, with a note of how it was checked when that isn't obvious. A milestone is done when all its lines are ✅ and its "done when" holds.

| Milestone | Status |
|---|---|
| [M0 Skeleton](#m0-skeleton) | ✅ |
| [M1 MVP](#m1-mvp) | ✅ |
| [M2 Database sessions](#m2-database-sessions) | ⬜ |
| [M3 Scripting and commands](#m3-scripting-and-commands) | ⬜ |
| [M4 Terminal extras](#m4-terminal-extras) | ⬜ |
| [M5 The rest](#m5-the-rest) | ⬜ |
| [M6 Distribution](#m6-distribution) | ⬜ |
| [Later](#later) | ⬜ |

---

## M0 Skeleton

**Done when:** the app opens on a Mac, with an empty tree and one local shell tab.

- ✅ Rust (`rustup`) and Node toolchain set up; versions pinned (`rust-toolchain.toml`, `.nvmrc`)
- ✅ Tauri 2 + Svelte 5 + TypeScript app that opens a window
- ✅ Main window layout: menu bar, toolbars, sessions sidebar, tab area, button bar, status bar (empty, with tooltips saying which milestone fills them)
- ✅ dockview tab area with one tab (and more, from New Local Shell)
- ✅ PTY (`portable-pty`) running the login shell in that tab, drawn by `web/terminal` (xterm.js) through the `chrome.webview` shim; the page starts the shell (checked), drawing and typing checked in the app (typed by Claude, read back by screenshot)
- ✅ `web/terminal`, `web/output`, `web/shared` and `web/lib` copied from SuperXPuTTY with their licences (default rules and themes still missing: not in SuperXPuTTY's repository)
- ✅ Licence checks (`cargo deny check` in `src-tauri/`, `npm run licenses`), including security advisories
- ✅ Builds made locally; the GitHub Actions workflow (macOS, Windows, Linux) runs only when started by hand, so pushes cost nothing
- ⬜ Known issue: development builds ask for the Keychain password again after each rebuild (a new unsigned app each time, to macOS); signed builds (M6) stop it
- ⬜ Known issue: a small round macOS badge sits beside the terminal cursor, over the end of the prompt (seen in the user's own screenshots too). Not in the page, not a separate window; not fixed by `writingsuggestions`, a transparent caret, or `TSMLanguageIndicatorEnabled`. Next: find it with Web Inspector
- ⬜ To check: a command typed once showed twice, with empty prompts between, on the lab switch (and a column of empty prompts in an early local shell). Not caused by clicks; not yet reproduced in a controlled test

## M1 MVP

**Done when:** a Mac user opens the team's Sessions.xml and connects to switches with the right icons and colours.

**Data**
- ✅ Read and write `Sessions.xml`, keeping unknown attributes: written as SuperXPuTTY's XmlSerializer does (the reference files write back byte for byte), re-read before each save so others' changes are kept, with SuperXPuTTY's 20 timestamped backups
- ✅ Read and write `PrivateSessions.xml`
- ✅ Read and write `Folders.xml` / `PrivateFolders.xml` (colours, defaults): Folder settings… and the folder menu's Colour; settings move with renamed folders
- ✅ Settings folder choice (local, SMB share, synced cloud folder): in Settings > General, and in the tree when none is set; the tree reloads when it changes
- ✅ Per-user files in `~/Library/Application Support/SuperXTerminal/` (`settings.json`, `PrivateSessions.xml`)

**Sessions tree**
- ✅ Saved and Private roots, folders and sessions; closed folders draw nothing
- ✅ Folder session counts, folders first, sort
- ✅ Search box (filter while typing: name, host, username, folder, note)
- ✅ Open sessions marked (green dot); tooltips (protocol, address, note, why it can't open)
- ✅ New, rename, delete, drag to move (sessions and folders): New session… / New folder… (tree-only until a session goes in, as in SuperXPuTTY), Rename…, Delete folder (with its sessions, after asking), dragging sessions and folders within Saved or Private; plus Connect all, Expand all, Collapse all
- ✅ Sessions the Mac can't open shown dimmed with the reason

**Icons and colours**
- ✅ Icon templates redrawn as SVG, with colourable keys (all 62 templates and glyphs; checked against SuperXPuTTY's Choose Icon window)
- ✅ Brand logos (26, from SuperXPuTTY's BrandLogos)
- ✅ Custom icons from `<SettingsFolder>/icons`: read and drawn as masks, in the PNG's own colour or any other (checked with a test PNG in Choose Icon). Adding and removing them from the app comes later
- ✅ Old PNG keys mapped onto templates
- ✅ Choose Icon window (Devices, General, Logos, Custom; palette colours or any other)
- ✅ Environment colours: tree circle, tab (selected filled, others tinted with a top stripe), frame around the terminal, cursor
- ✅ Colour resolution: session, else folder, else name rule (rules in `settings.json` until the Settings window)

**Connecting**
- ✅ SSH tab: `ssh -tt` in a PTY, host, port, username; checked against the lab switch (log in, `sh ip int bri`)
- ✅ Profiles: Settings → Profiles (Cmd+,), "Default Settings", matched by name; a missing profile falls back to Default Settings with a notice in the terminal. Checked: a `root` profile made Media Server log in as root
- ✅ Passwords: a session's `-pw`, Quick connect's password, or a profile's (kept in the Keychain) is typed at the first prompt; each checked on the lab switch
- ✅ Key files and agent: a profile's key file (`-i`, `IdentitiesOnly`), else ssh's own keys and agent (checked with the 1Password agent)
- ✅ Allow older algorithms (profile option, on in Default Settings): SHA-1 key exchange, `ssh-rsa`, CBC ciphers
- ✅ Launch order: session, folder default, profile, Default Settings (local overrides join it in M5); tested
- ✅ `{key}` placeholders; Environment Variables with Keychain secrets (Tools > Environment Variables; SuperXPuTTY's file, copied over as is: plain values come across, Windows-encrypted secrets ask to be typed again and stay in the file for Windows). Checked: Lab Switch - environment logs in
- ✅ Quick connect toolbar: Protocol, Host (`host`, `host:port`, `user@host`, `ssh://host:port`), Login, Password (used once, never saved) and Profile; Enter connects, Clear empties it. Checked on the lab switch. File > New Local Shell (Cmd+T) replaces M0's button
- ✅ Edit Session (all of SuperXPuTTY's fields, profile list, inherited folder values as hints, greyed fields explained, `-pw` shown masked and kept), from the session's right-click menu, which matches SuperXPuTTY's (Connect, Colour, Copy host address, Edit, Delete; the rest greyed with the milestone)

**Settings**
- ✅ Settings window basics (settings folder, theme, icon size, tree font): Settings (Cmd+,) > General and Profiles; the environment colour rules are edited there too until the Environment Colours window (M5)
- ✅ Light and Dark themes matching SuperXPuTTY, or following macOS (the default); the terminal keeps its own theme (PuTTY's black by default). Dark blue and Classic aren't carried over

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

- ✅ Highlighting rules (cisco, junos, generic, auto, user rules), per tab, and per session and folder (`HighlightRules`, set in Edit Session): SuperXPuTTY's defaults bundled, then the team's and the user's `highlighting` folders; checked against SuperXPuTTY's colours on the lab switch (`sh ip int bri`)
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
