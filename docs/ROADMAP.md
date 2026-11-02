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
| [M2 Database sessions](#m2-database-sessions) | ✅ |
| [M3 Scripting and commands](#m3-scripting-and-commands) | ✅ |
| [M4 Terminal extras](#m4-terminal-extras) | ✅ |
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
- ✅ Known issue fixed: development builds asked for the Keychain password again after each rebuild. They're now signed with an Apple Development certificate (free, from Xcode) before they run (scripts/dev-sign-run.sh; steps in docs/PLAN.md): the Keychain admits apps by team ID, so Always Allow lasts. Checked: a rebuild read the saved passwords without asking
- ✅ Known issue fixed: macOS 27's "Write with Siri" badge (the ⊖ by the terminal's cursor) no longer shows: AppKit's lightweight affordance is turned off in this app, and Writing Tools are off in its web view
- ⬜ To check: after renaming a Database folder, it showed closed once (it had been open)
- ⬜ To check: a click on a menu item sometimes does nothing the first time (seen twice while testing with simulated clicks)
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
- ✅ Folder session counts, folders first, sort; the toolbar under the search box (New session and New folder in the selected folder, Expand all, Collapse all, A to Z / Z to A)
- ✅ Search box (filter while typing: name, host, username, folder, note), with Search and Clear buttons and Escape; a search opens every folder holding a match, and clearing it puts the tree back as it was
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
- ✅ Quick connect toolbar (with its monitor icon, and the terminal icon on the Command bar, as SuperXPuTTY): Protocol, Host (`host`, `host:port`, `user@host`, `ssh://host:port`), Login, Password (used once, never saved) and Profile; Enter connects, Clear empties it. Checked on the lab switch. File > New Local Shell (Cmd+T) replaces M0's button
- ✅ Edit Session (all of SuperXPuTTY's fields, profile list, inherited folder values as hints, greyed fields explained, `-pw` shown masked and kept), from the session's right-click menu, which matches SuperXPuTTY's (Connect, Colour, Copy host address, Edit, Delete; the rest greyed with the milestone)

**Settings**
- ✅ Settings window basics (settings folder, theme, icon size, tree font): Settings (Cmd+,) > General and Profiles; the environment colour rules are edited there too until the Environment Colours window (M5)
- ✅ Light and Dark themes matching SuperXPuTTY, or following macOS (the default); the terminal keeps its own theme (PuTTY's black by default). Dark blue and Classic aren't carried over

## M2 Database sessions

**Done when:** Mac and Windows users edit the same table and see each other's changes within 30 s.

- ✅ Connect as `SuperPuTTY`, with the nickname (checked against the team's PostgreSQL 17.5)
- ✅ Read in pages in the background; `@db/` ids (the team's 4 sessions, folders, icons and colours as in SuperXPuTTY)
- ✅ Cache (`DatabaseSessions.cache.xml`) and retry while offline, as SuperXPuTTY: the cache is shown at once at start, then the table is read; a cache from another connection (its Source) isn't used; while offline the change check stops and a full read is tried every 2 minutes; the root shows "(loading...)", "(offline, cached 09:41)", "(1 pending)" or "(1 not saved)". Checked offline in the app (through a local forwarder that was stopped and started)
- ✅ Writes and the offline outbox, as SuperXPuTTY's Submit and DatabaseOutbox: online with nothing queued a change is written at once; offline (or behind queued changes) it's queued in `DatabaseSessions.outbox.json`, shown in italics with a note, and sent in order before the next read; refused ones are kept under Database sessions > Pending changes (Send again, Overwrite theirs, Discard). Checked in the app on a test table: a colour change and a delete made offline; the delete sent, the colour refused because someone else changed the row, then saved with Overwrite theirs
- ✅ Locks (`locked`, `locked_fields`): locked rows can't be edited, deleted or moved (Copy to still works), and locked fields are greyed in Edit Session; the table's guard trigger refuses the same (locked row, locked fields, the locks themselves, managed_by) while scripts under another application name aren't stopped. Checked on a test table, in the app and by an ignored test (`cargo test schema_and_locks -- --ignored`)
- ✅ Changes noticed every 30 s (`<table>_version`): a session added in SuperXPuTTY on Windows appeared on the Mac within the check
- ✅ `deleted_at` rows hidden (in the query; columns a table lacks read as defaults)
- ✅ Folders table (colours, defaults): read, applied and written (Colour, Folder settings…; moved and removed with renamed and deleted folders)
- ✅ Move to / Copy to between Database and Saved/Private (and between Saved and Private): to the same folder path, without database fields; a move out of the table asks first and leaves locked rows; clashing names are refused and stay put. Checked with a test session through all three
- ✅ Database settings dialog, Create / upgrade schema, read-only: Tools > Database Sessions… with Test connection and Read only. Create / upgrade schema ported from SuperXPuTTY (same SQL, so either app's run leaves the same table), enabled when Test finds something missing; checked in the app (a new table) and by the ignored test (a new table, and upgrading an older one)
- ✅ 10,000 sessions (`lots_of_sessions`): read in 0.84 s (20 pages of 500), cache written in 55 ms and read in 30 ms; in the app about 2 s from OK to the tree, search as you type (3,600 matches for "acc-sw0") and nested folders open at once
- ✅ Database settings export/import (SuperXPuTTY's JSON), Clear; password in the Keychain

## M3 Scripting and commands

**Done when:** the existing button bars and scripts run unchanged.

- ✅ Button bar (`ButtonBars.xml` in the settings folder, written as XmlSerializer writes it; the Windows file reads and writes back byte for byte): bar picker, bars per tab icon (Show for Tab Icons), buttons with a colour circle or session icon; click sends to the active tab, Shift+click to the Send Commands selection; Send string escapes (\r \n \t \b \e \p \v \\) and SPSL scripts; New, Edit, Delete, Move Left/Right; New, Rename, Delete bar; JSON export/import; Reload. Checked: the Windows buttons in a Local Shell, and a test button added and removed (file back to identical)
- ✅ Send Commands toolbar: Command box (Enter sends with Enter; Up/Down history; Ctrl+key sent at once), Send, mask, Script Editor, Select sessions to send to (Accept commands per tab, Send to visible tabs only), icon group. History lasts until the app closes (SuperXPuTTY's default; saving it comes with Options)
- ✅ `{prompt:...}` and `{secret:...}` snippet prompts (one window, each name once, last value offered); `{key}` placeholders filled first for buttons and scripts, offering Environment Variables when one has no value
- ✅ SPSL parser and runner: SENDLINE, SENDCHAR, SENDKEY (modifiers, names, {KEY n}; xterm sequences as TerminalKeys, US layout for characters), WAITFOR (/t:, reads the page's output as TerminalControl), SLEEP, PROMPT, PWDPROMPT, OPENSESSION; stops on a WAITFOR timeout, an error or a cancelled password; status-bar progress with Stop. Checked on the lab switch (enable, password, show clock, Ctrl+Z, disable)
- ✅ Script Editor: saved scripts (`<SettingsFolder>/scripts`, .spsl and .txt, CRLF), Insert, Save as, Delete, Open file, Save to file, Run as (Send lines / SPSL script), the SPSL panel (commands, saved scripts, Check script). It floats over the window without blocking it, so Always on top isn't needed
- ✅ Session's own script on open: a file (absolute, file://, or relative to the settings folder) or a URL, run when it starts #!/bin/spsl, its first WAITFOR seeing everything since the session started; a Windows path is reported as not on this Mac. Checked on the lab switch (WAITFOR the prompt, show clock, SLEEP, show version); the tab shows a spinner while a script runs
- ✅ Send Key (`TerminalKeys`): the tab menu's keys and Other Key… (any key as SENDKEY writes it)

## M4 Terminal extras

- ✅ Highlighting rules (cisco, junos, generic, auto, user rules), per tab, and per session and folder (`HighlightRules`, set in Edit Session): SuperXPuTTY's defaults bundled, then the team's and the user's `highlighting` folders; checked against SuperXPuTTY's colours on the lab switch (`sh ip int bri`)
- ✅ Terminal preferences in Settings > Terminal, as SuperXPuTTY's TerminalPreferences (theme, font, size, cursor, blink, bell, drawing, highlighting, mark matches, copy on select, scrollback, paste check lines, log time stamps, keys): shared by every tab and applied at once, also when changed from a tab's own menu. Checked: a font size reached the open tab
- ✅ Highlighted Output (SuperXPuTTY's web/output page, Monaco) beside the tab, from the tab menu or the page's Open as Document: follows the tab's log every 250 ms, with Rules, Theme, Follow, Wrap, Find, Save As, Open File, Rules Folder and Reload Rules; stays, marked closed, when its tab closes. Checked on the lab switch (Cisco rules detected, new output followed)
- ✅ Mac shortcuts: Cmd+C and Cmd+V (through paste check), Edit > Find… (Cmd+F, the terminal's find bar), Edit > Clear Scrollback (Cmd+K), File > Close Tab (Cmd+W, no longer the window). The page's bell (sound), Ctrl+click links (http/https, default browser) answered as TerminalControl does
- ✅ Search Output (the tab's log; Match case, Regex; up to 5,000 lines; double-click copies; Open log) and Watch for Activity / Silence (10 s to 5 min) with SuperXPuTTY's orange and grey tab badges, status text and a Dock bounce when the app is in the background. Checked: a search, and a pinging tab watched from another
- ✅ Paste check, as SuperXPuTTY's: a paste of 2 lines or more (Settings > Terminal; 0 never) is shown in the Paste window, editable, typed on Paste (Cmd+Return); Cmd+V, Ctrl+Shift+V, the menu and middle-click all go through it. Checked with a three-line paste
- ✅ Session logs, as SuperXPuTTY's SessionLog: every tab logs what it shows (finished lines, CRLF, a time stamp line every 30 s of output); kept in a chosen folder (Settings > General, default Documents/SuperXTerminal/logs) or temporary (deleted when the tab closes, swept after a day). Checked: the log matched the tab
- ✅ Tab right-click menu, as SuperXPuTTY's: New session (every session by folder), Duplicate session, Copy host address, Save session as, Rename tab, Refresh tab, Accept commands, Send Key, Special Command (not in the built-in terminal), Copy all, Restart session / Reconnect, Clear scrollback, Reset terminal, Close, Close others, Close others to the right, Close all. The output items come with the rest of M4. Checked on the lab switch
- ✅ A session that ends cleanly (a logout) closes its tab, as SuperXPuTTY's; any other end keeps the tab and its output, and Enter reconnects. Checked: exit on the switch closed the tab; a refused connection stayed, and Enter tried again
- ✅ Dropped sessions reconnect automatically, as SuperXPuTTY's (on by default: 6 attempts, 10 s apart, counted again once a session has stayed up a minute; Settings > General), with the disconnected badge on the tab, Enter to reconnect at once, and Stop Reconnecting in the tab menu. Checked with a refused connection

## M5 The rest

**Done when:** the feature matrix is complete except the SFTP browser.

**Tree and data**
- ✅ Favorites and Recent (`SessionShortcuts.json`, in SuperXPuTTY's format, in the user's own folder): groups at the top of the tree, shown only when they hold something, opened as last left; Add to / Remove from Favorites, Show in Sessions tree, Remove from Recent, Clear Recent; the last 10 sessions opened from the tree; they follow renames and moves and forget deleted sessions. Checked in the app
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
