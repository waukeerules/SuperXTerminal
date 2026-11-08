# SuperXTerminal

A consistent Mac and Windows alternative to SuperPuTTY: a terminal and session manager for **teams** who look after a
lot of servers and network devices together. One shared set of sessions for the whole team, with each person's own
login, settings and exceptions kept on their own computer, where they don't get in a teammate's way. A tree of
sessions, tabs and splits, SSH, Telnet, serial and Remote Desktop in the same window, a button bar of commands, and
highlighting for Cisco and Junos output, built to look and work the same on both systems.

![The main window: Favorites, Recent, Saved, Private and Database sessions, and the button bar](docs/screenshots/main-window.png)

## Built for teams

Most SSH managers are a personal address book: your hosts, your passwords, perhaps synced between your own devices.
Sharing one with a team means sharing everyone's logins too, or everyone keeping their own copy that drifts.
SuperXTerminal is built around the other problem: **one list of devices that a whole team works from, where nobody's
login, key or habits are written into it.**

It does that by splitting a session in two, as PuTTY's saved sessions did:

| Shared with the team | Kept on your own computer |
|---|---|
| **What** a session is: its name, folder, host, protocol, icon, environment colour, and the *name* of the profile it uses | **How you** log in: the profile of that name, with your username, password (in the Keychain or Windows' Credential Manager), key file, port, jump host, SSH algorithms and terminal preferences |
| Button bars, scripts and highlighting rules | Environment Variables, your own values for `{placeholders}` in shared sessions, secrets included |
| Folder colours and folder defaults | Local overrides: your own host, port, username, profile or jump host for a session, without changing it for anyone else |
| | Private sessions, Favorites and Recent |

So a session named "core-sw1" using the profile `netops` logs one engineer in with their own account and a key, and the
next with a different account and a password, from the same shared row. Nobody's password is ever in the shared
files or the shared table.

### Profiles: standard settings, each person's own login

A profile is a named set of login and connection settings, like a PuTTY saved session. The team agrees on the names
(`netops`, `linux-root`, `cyberark`), and each person fills in their own profile of that name, in Settings > Profiles.
A shared session only says which profile it uses.

- **One change, every session.** Change your key file, username or SSH algorithms in the profile, and every session
  that uses it follows. Nothing in the shared list changes.
- **Older devices handled once.** A profile can carry the older key exchange and ciphers an old switch needs, so the
  sessions for those devices just name that profile.
- **A profile can name the host itself,** for a gateway such as CyberArk's SSH gateway: the session keeps naming the
  real device, and the profile says where ssh connects ([docs/CYBERARK.md](docs/CYBERARK.md)).
- **Passwords and keys can stay in 1Password.** A password can be a 1Password reference (`op://vault/item/field`),
  read with 1Password's approval as the session opens, and a profile's key can be one in 1Password, which ssh gets from
  1Password's SSH agent ([docs/ONEPASSWORD.md](docs/ONEPASSWORD.md)).
- **Default Settings** is the profile every other one falls back to, as in PuTTY.

When a session opens, each setting comes from the first place that has it:

1. your local override
2. the session itself
3. its folder's defaults
4. its profile
5. Default Settings

### Environment Variables: placeholders each person fills in

A shared session, profile or script can use `{placeholders}`, such as `{ad_user}@{host_address}` or a CyberArk login
built from `{email}`, `{cyberark_region}` and `{cyberark_connector}`. Each person sets their own values in
Tools > Environment Variables, and secrets go in the system's keychain, never in a file, or stay in 1Password as a
reference read when a session uses it.

- The window shows where each variable is used, and lists the placeholders used but not yet defined, each with Add.
  A session that needs one you haven't set says which, rather than connecting with the wrong login.
- **Export…** writes your set of variables to a file to hand to a new teammate, secrets by name only. **Import…** reads
  one, adds what's new, and lists the secrets left to type. Files from SuperXPuTTY on Windows work too.

### Local overrides: your exception, not the team's

Sometimes a session has to open differently just for you: from home through a jump host, to a lab copy at another
address, or as yourself rather than through the gateway. A local override changes the host, port, username, profile,
extra arguments or jump host on your computer only, and can carry a note of why.

- Set them for one session, a selection, or a whole folder and everything under it.
- Turn them off and on without losing them, per session, per folder, or all at once from Tools > Use Local
  Overrides. Off, the session opens exactly as the team's.
- Save a set as a preset ("From home", "Lab") and apply it to other sessions.
- The tree marks a session that has one, and its tooltip shows what's different.

### Ways to share the sessions

- **A shared settings folder** on a file share or a synced cloud folder, holding `Sessions.xml`, the button bars,
  scripts and highlighting rules. Changes a teammate makes show up within seconds.
- **Database sessions:** a PostgreSQL table for larger teams, with locked rows and fields so the standard entries stay
  standard, an offline cache, and changes queued and sent when the connection comes back.
- **Private sessions** for what's only yours, and Copy to between all three.
- **A mixed team on Mac and Windows.** The files and the table are the same as SuperXPuTTY's, so people using either
  app work from one set of sessions.
- **Environment colours** (production in red, say) on the tree, the tab, the terminal's frame and the cursor, the same
  for everyone, so nobody types into production thinking it's the lab.

### A team of one, on several computers

The same works for one person with more than one computer: a Mac at the desk and a Windows laptop on the road, say,
or a work machine and a home one. Point each computer at the same settings folder, in iCloud Drive, OneDrive, Dropbox
or a share, or at the same Database table, and they all have one set of sessions, button bars, scripts and
highlighting rules. A session added or changed on one shows up on the others, on the Mac and on Windows alike, rather
than one copy per computer that slowly drifts apart.

What stays on each computer is what belongs to that computer: its passwords in its own keychain, its key files, its
local overrides, and its Private sessions (so keep anything you want everywhere in Saved or Database sessions).
Profiles and Environment Variables are each computer's too. Set them up once on each, and use Environment Variables'
Export… and Import… to carry the variables across (secrets by name, to type again).

## Know a device at a glance

Every session can have an icon, so the tree reads like a network diagram: you look down it and see a core switch, a
firewall, an Ubuntu server, without reading a name.

- **Network symbols** drawn for the job: switch, core, distribution, stack, layer 3 and data centre switches, router,
  voice router, firewall, load balancer, VPN concentrator, access point and wireless controller, console server, IP
  phone, UPS, server, storage, database, workstation and more.
- **Any colour.** The same symbol in any colour, so a team can make its own code: green switches for layer 2, blue for
  layer 3, orange for the lab.
- **Brand logos:** Cisco, Fortinet, Palo Alto Networks, VMware, Proxmox, Docker, Kubernetes, Ubuntu, Red Hat, Debian,
  Rocky, Alma and other Linux and BSD systems.
- **Your own:** a PNG in the settings folder's `icons` folder shows up under Custom, drawn in its own colour or any
  other.
- **Set once, everywhere:** pick one with Choose Icon in Edit Session, or give a whole folder or a selection its icon
  at once with Edit sessions….

The icon follows the session wherever it appears: the Sessions tree, its tab, and the open tabs list. A button bar can
be set to show by itself for tabs with a given icon (Show for Tab Icons), so switching to a switch's tab brings up the
switch commands, and a Linux server's brings up the Linux ones. Icons are part of the shared session, so the whole team
sees the same picture. Your Private sessions take any icon you like.

## Where it comes from

SuperXTerminal is inspired by [SuperPuTTY](https://github.com/jimradford/superputty) by Jim Radford, the Windows
application that put PuTTY sessions into tabs with a sessions tree beside them, and by
[SilverGreen93's fork](https://github.com/SilverGreen93/superputty), which carried it on.

SuperPuTTY puts PuTTY's own windows inside its tabs, which only Windows can do. SuperXTerminal draws every tab itself
instead: SSH goes through the system's own OpenSSH in a terminal drawn by [xterm.js](https://xtermjs.org), so it looks
and works the same on the Mac and on Windows. It's built with [Tauri 2](https://tauri.app) and Svelte 5.

## Screenshots

| | |
|---|---|
| ![A Cisco switch and a Linux server side by side, the switch's output highlighted](docs/screenshots/split-panels.png) | ![The terminal's right-click menu](docs/screenshots/right-click-menu.png) |
| **Splits and highlighting.** Sessions side by side, each framed in its environment colour, with Cisco output coloured live (interfaces up in green, down in red). The buttons send to the split you're typing in. | **The terminal's menu.** Copy and paste, Find, clearing, Open as Document, highlighting, theme, font and cursor, Wrap Lines, and Send Key. |
| ![Edit Session for a Database session](docs/screenshots/edit-session.png) | ![File transfer from a Cisco switch over SCP](docs/screenshots/file-transfer.png) |
| **Edit Session.** Connection, profile, highlighting, jump host, script and file transfer paths. A Database session is shared with the team, so it warns against passwords and takes `{key}` placeholders that each user fills in. | **File transfer.** This computer's files beside the session's: here a switch's flash over SCP, with an IOS image downloading. |
| ![Settings > Terminal](docs/screenshots/settings-terminal.png) | |
| **Settings.** The terminal's theme, font, cursor, highlighting, wrapping, scrollback and paste check, for every tab. | |

## Features

**Sessions**

- A sessions tree with folders, Saved and Private sessions, Favorites and Recent, search as you type, drag to move,
  multi-select, and bulk editing.
- Sessions in SuperPuTTY's `Sessions.xml` format, with folders, button bars, highlighting rules and scripts beside it.
  Keep them in a shared folder and a team on Mac and Windows works from one set; changes made elsewhere show up within
  seconds.
- Database sessions: a PostgreSQL table shared by the team, with an offline cache, locked rows and fields, and changes
  written when the connection comes back.
- Icons (62 templates and 26 brand logos, or your own), and environment colours (production in red, say) on the tree,
  the tab, the terminal's frame and the cursor.
- Profiles for usernames, passwords (kept in the system's keychain), key files (PuTTY `.ppk` keys too), SSH algorithms
  and older algorithms for older devices.
- Jump hosts, PuTTY-style extra arguments, `{key}` placeholders from Environment Variables, and local overrides
  (see [Built for teams](#built-for-teams)).
- A session's tab and its place in the tree show yellow while it's connecting, green once it's connected, and a red
  cross when it drops.
- Import from SuperPuTTY XML, PuTTY and KiTTY, MobaXterm, `.rdp` files and `~/.ssh/config`; CSV export and import.

**Connections**

- SSH, Telnet, Raw, Serial (`/dev/cu.*` on the Mac, COM ports on Windows), local shells, and Remote Desktop in a tab.
- Serial line settings that change without reconnecting, Break, and Telnet's Special Commands.
- VNC through Screen Sharing, FileZilla for files, and Connect external to open a session in Terminal.app (or
  Windows App for RDP).
- Dropped sessions reconnect by themselves.

**Terminal**

- Tabs, splits (side by side or stacked), more windows, and layouts that reopen a set of sessions.
- Highlighting rules for Cisco, Junos and generic output, chosen per tab, session or folder.
- Wrap Lines on or off. With it off, long lines stay whole, a scroll bar pans along them, and the view follows the
  cursor as you type.
- Find, Search Output, Highlighted Output (the tab's output in Monaco), and Compare Output between two tabs.
- Server load: a Linux server's CPU, memory, disk and network traffic in the status bar, with a 15-minute graph (a
  profile setting, off by default).
- Session logs with time stamps, paste checking for multi-line pastes, and watching a tab for activity or silence.

**Commands and scripts**

- A button bar and a Send Commands bar that send to the tab you're in, or to every tab you choose.
- SPSL, SuperPuTTY's scripting language, extended: everything SuperPuTTY's scripts do (SENDLINE, WAITFOR, SENDKEY,
  SENDBREAK and more), plus the tools to cope with a device that doesn't always answer the same way:
  - EXPECT, for whichever prompt comes first (a login asked in any order)
  - regular expressions and any-case matching
  - timeouts that branch rather than stop, and `/idle:` for transfers that stall
  - variables (the session's own `{host_address}` and `{session_name}` among them) and the parts of what a wait matched
  - IF, FOREACH over a list of matches, and REPEAT
  - prerequisite CHECKs that a script REQUIREs before it changes anything
  - FAILON, to stop at the device's first refusal
  - PAGER, to answer `--More--` by itself
  - SAVE a command's output to a file, and CALL a shared script
- The Script Editor: Monaco with SPSL's colours and hover help, Check script, and Step to run a script a line at a
  time. A script can also run when a session opens, and stops if its session drops. The whole language, with patterns
  and examples, is in **[docs/SPSL.md](docs/SPSL.md)**.
- [Rhai](https://rhai.rs/book), a second script language beside SPSL, with variables, arithmetic, loops and functions
  for scripts that have to decide, count or remember. A script reads the tab's session and your Environment Variables,
  so one shared script logs in as whoever runs it. The guide and examples are in **[docs/RHAI.md](docs/RHAI.md)**.
- `{prompt:...}` and `{secret:...}` snippets that ask before sending.

**File transfer**

- SFTP with this computer's files beside the session's: drag, upload, download, rename and delete, with progress.
- SCP mode for network devices without SFTP (Cisco IOS with `ip scp server enable`).

Anything that can't work on the system it's running on is shown greyed out, with the reason, rather than hidden.

## How it works

SuperXTerminal is a [Tauri 2](https://tauri.app) app: a core written in Rust, and a window drawn by the system's own
web view (WebKit on the Mac, WebView2 on Windows), with the interface in TypeScript and Svelte 5.

- **SSH is OpenSSH.** Each SSH tab runs the system's own `ssh` (Windows has it built in too) in a pseudo terminal, so
  keys, ssh-agent, `~/.ssh/config`, jump hosts and known hosts work as they do in any other terminal. The app builds
  the command from the session, its folder's defaults and its profile. A saved password is kept in the Keychain (or
  Windows' Credential Manager) and typed at the first password prompt, never put on the command line. When ssh ends,
  the tab reads what ssh said last to tell you what went wrong, and reconnects only when that can help.
- **The core, in Rust,** runs the pseudo terminals (ConPTY on Windows) and what OpenSSH doesn't: Telnet, Raw TCP and
  serial ports are spoken by the core itself, and Remote Desktop by [IronRDP](https://github.com/Devolutions/IronRDP).
  File transfer goes over the session's own ssh: SFTP, or SCP for network devices that have no SFTP. It also reads
  and writes SuperXPuTTY's files (sessions, button bars, highlighting rules) in their own formats, so both apps can
  share a settings folder, and keeps Database sessions in PostgreSQL, the same table SuperXPuTTY uses.
- **The interface, in TypeScript and Svelte:** the sessions tree, tabs and splits ([dockview](https://dockview.dev)),
  Settings, the button bar, and SPSL scripts, which run here and send to the tabs.
- **Each terminal is a page of its own,** in plain JavaScript: [xterm.js](https://xtermjs.org) draws it, the
  highlighting rules colour the output as it arrives, and Open as Document shows it in
  [Monaco](https://microsoft.github.io/monaco-editor/), the editor from VS Code.

Its tests are Rust's own for the core, [Vitest](https://vitest.dev) for the interface and
[Playwright](https://playwright.dev) for the terminal page, in WebKit and Chromium (WebView2's engine).

## Download

Installers are on the [Releases page](https://github.com/waukeerules/SuperXTerminal/releases): a `.dmg` for Apple
silicon Macs, and an MSI or setup `.exe` for Windows. The Mac build isn't notarised yet, so on first open right-click
the app and choose Open; Windows SmartScreen warns about the unsigned installer the first time it runs.

## Building

- **[docs/BUILD-macOS.md](docs/BUILD-macOS.md):** what you need, the signing certificate, running it while developing,
  the checks, the release build and its `.dmg`, notarisation, and publishing a release.
- **[docs/BUILD-Windows.md](docs/BUILD-Windows.md):** building and running it on Windows, what works there and what's
  still Mac-only, and its MSI and setup `.exe`.
- **[docs/RELEASE.md](docs/RELEASE.md):** making a release: the version, building on both systems into `releases/`,
  and publishing the installers on GitHub.
- **[docs/PLAN.md](docs/PLAN.md):** the plan: the chosen stack (Tauri 2 and Svelte 5, with xterm.js and Monaco),
  licensing, the feature matrix, data compatibility and open questions.
- **[docs/ROADMAP.md](docs/ROADMAP.md):** one line per feature, ticked as each is delivered.
- **[docs/SPSL.md](docs/SPSL.md):** SPSL, the script language: how a script sees the output, every command, patterns
  that work, pitfalls and complete examples. Written so a person or an LLM can write a working script from it alone.

The short version, on a Mac with Rust and Node:

```bash
npm install
```

```bash
npm run tauri dev
```

## Contributing

Help is welcome, whether it's a bug found on a real network, an idea from a day of using it, or a fix of your own.

- **Something wrong, or missing?** [Open an issue](https://github.com/waukeerules/SuperXTerminal/issues): the steps
  that led to it, what should have happened, and what did. A screenshot often says it best; blur the hosts,
  usernames and passwords in it first.
- **A change of your own?** Fork the repository, work on a branch, and send it back as a pull request.

A pull request goes in faster when:

- **The tests pass.** `npm run check` and `npm test` for the app, `npm run test:browser` if you touched the terminal
  page, and `cargo test` in `src-tauri` for the Rust side; [docs/TESTING.md](docs/TESTING.md) has which covers what.
  A bug fix comes with a test that fails without it.
- **Nothing real is in it.** No real device names, addresses, logins, passwords or exported session files, in the
  code, the tests or a screenshot. Make some up, as the tests do: `core-sw1` at `10.0.0.5` in `/team/Sessions.xml`.
- **It reads like the code around it.** [AGENTS.md](AGENTS.md) sets the house style, for people and AI assistants
  alike: each file and function says what it's for, and each commit is one change.

## Licence

MIT, as SuperPuTTY is: you're free to use, copy, change and share it, as long as the copyright notice comes with it.
See [LICENSE](LICENSE). The components it uses keep their own licences, listed in
[THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt).

## Thanks

- [Jim Radford](https://github.com/jimradford) and SuperPuTTY's contributors, for the original.
- [SilverGreen93](https://github.com/SilverGreen93/superputty), for carrying SuperPuTTY on.
- [xterm.js](https://xtermjs.org), [Monaco](https://microsoft.github.io/monaco-editor/), [Tauri](https://tauri.app),
  [IronRDP](https://github.com/Devolutions/IronRDP) and the other components listed in
  [THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt).
