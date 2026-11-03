# SuperXTerminal

A consistent Mac and Windows alternative to SuperPuTTY: a terminal and session manager for people who look after a lot
of servers and network devices. A tree of saved sessions, tabs and splits, SSH, Telnet, serial and Remote Desktop in
the same window, a button bar of commands, and highlighting for Cisco and Junos output, built to look and work the same
on both systems.

![The main window: Favorites, Recent, Saved, Private and Database sessions, and the button bar](docs/screenshots/main-window.png)

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
- Jump hosts, PuTTY-style extra arguments, `{key}` placeholders from Environment Variables, and local overrides.
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
- Session logs with time stamps, paste checking for multi-line pastes, and watching a tab for activity or silence.

**Commands and scripts**

- A button bar and a Send Commands bar that send to the tab you're in, or to every tab you choose.
- SPSL, SuperPuTTY's scripting language (SENDLINE, WAITFOR, SENDKEY, SENDBREAK and more), with a Script Editor, and a
  script run when a session opens.
- `{prompt:...}` and `{secret:...}` snippets that ask before sending.

**File transfer**

- SFTP with this computer's files beside the session's: drag, upload, download, rename and delete, with progress.
- SCP mode for network devices without SFTP (Cisco IOS with `ip scp server enable`).

Anything that can't work on the system it's running on is shown greyed out, with the reason, rather than hidden.

## Download

Installers are on the [Releases page](https://github.com/waukeerules/SuperXTerminal/releases): a `.dmg` for Apple
silicon Macs, and an MSI or setup `.exe` for Windows. The Mac build isn't notarised yet, so on first open right-click
the app and choose Open; Windows SmartScreen warns about the unsigned installer the first time it runs.

## Status

M0 to M5 are done: the sessions tree and Database sessions, SSH, Telnet and serial, scripting and commands, terminal
extras, RDP in a tab (IronRDP), file transfer (SFTP, and SCP for network devices) and the rest. M6 (distribution)
is partly done: signed local release builds for the Mac and Windows, the installer window, the app icon and the
third-party notices. Developer ID signing, notarisation and auto update are deferred until it's decided who installs
it and where releases live (docs/PLAN.md §6, question 8).

- **[docs/PLAN.md](docs/PLAN.md):** the plan. It covers the options and the chosen stack (Tauri 2 and Svelte 5, with xterm.js and Monaco), licensing, the feature matrix, data compatibility, the UI mapping, milestones and open questions.
- **[docs/ROADMAP.md](docs/ROADMAP.md):** progress, one line per feature, ticked ✅ as each is delivered.

The Mac came first; Windows runs and has release builds, with some features still Mac-only
([BUILD-Windows.md](docs/BUILD-Windows.md)). Builds and releases are made locally and pushed to GitHub; nothing builds
on GitHub automatically.

## Building

- **[docs/BUILD-macOS.md](docs/BUILD-macOS.md):** what you need, the signing certificate, running it while developing,
  the checks, the release build and its `.dmg`, notarisation, and publishing a release.
- **[docs/BUILD-Windows.md](docs/BUILD-Windows.md):** building and running it on Windows, what works there and what's
  still Mac-only, and its MSI and setup `.exe`.
- **[docs/RELEASE.md](docs/RELEASE.md):** making a release: the version, building on both systems into `releases/`,
  and publishing the installers on GitHub.

The short version, on a Mac with Rust and Node:

```bash
npm install
```

```bash
npm run tauri dev
```

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
