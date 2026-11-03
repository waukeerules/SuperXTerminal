# SuperXTerminal

A macOS counterpart of [SuperXPuTTY](https://github.com/waukeerules/superputty): the same sessions tree, PostgreSQL Database sessions, icons, environment colours and tabs, with SSH through the Mac's own OpenSSH instead of PuTTY.

- **Shared data:** it reads the same `Sessions.xml`, `Folders.xml`, button bars, highlighting rules and Database sessions table, so Windows and Mac users of one team see the same sessions.
- **Unsupported features:** anything that needs PuTTY or Windows is shown greyed out, with the reason, rather than hidden.

## Status

M0 to M5 are done: the sessions tree and Database sessions, SSH, Telnet and serial, scripting and commands, terminal
extras, RDP in a tab (IronRDP), file transfer (SFTP, and SCP for network devices) and the rest. M6 (distribution)
is partly done: a signed local release build, its installer window, the app icon and the third-party notices.
Developer ID signing, notarisation and auto update are deferred until it's decided who installs it and where releases
live (docs/PLAN.md §6, question 8).

- **[docs/PLAN.md](docs/PLAN.md):** the plan. It covers the options and the chosen stack (Tauri 2 and Svelte 5, reusing SuperXPuTTY's xterm.js and Monaco pages), licensing, the feature matrix, data compatibility, the UI mapping, milestones and open questions.
- **[docs/ROADMAP.md](docs/ROADMAP.md):** progress, one line per feature, ticked ✅ as each is delivered.

Mac first; the code stays buildable on Windows and Linux. Builds and releases are made locally, as for SuperXPuTTY, and pushed to GitHub; nothing builds on GitHub automatically.

## Building

- **[docs/BUILD-macOS.md](docs/BUILD-macOS.md):** what you need, the signing certificate, running it while developing,
  the checks, the release build and its `.dmg`, notarisation, and publishing a release.
- **[docs/BUILD-Windows.md](docs/BUILD-Windows.md):** a placeholder: what a Windows build will need, and what's still
  Mac-only.

The short version, on a Mac with Rust and Node:

```bash
npm install
```

```bash
npm run tauri dev
```

## Licence

To be decided. SuperXPuTTY is MIT.
