# SuperXTerminal

A macOS counterpart of [SuperXPuTTY](https://github.com/waukeerules/superputty): the same sessions tree, PostgreSQL Database sessions, icons, environment colours and tabs, with SSH through the Mac's own OpenSSH instead of PuTTY.

- **Shared data:** it reads the same `Sessions.xml`, `Folders.xml`, button bars, highlighting rules and Database sessions table, so Windows and Mac users of one team see the same sessions.
- **Unsupported features:** anything that needs PuTTY or Windows is shown greyed out, with the reason, rather than hidden.

## Status

Planning done; starting M0. No code yet.

- **[docs/PLAN.md](docs/PLAN.md):** the plan. It covers the options and the chosen stack (Tauri 2 and Svelte 5, reusing SuperXPuTTY's xterm.js and Monaco pages), licensing, the feature matrix, data compatibility, the UI mapping, milestones and open questions.
- **[docs/ROADMAP.md](docs/ROADMAP.md):** progress, one line per feature, ticked ✅ as each is delivered.

Mac first; CI also builds Windows and Linux.

## Licence

To be decided. SuperXPuTTY is MIT.
