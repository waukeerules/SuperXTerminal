# SuperXTerminal

A macOS counterpart of [SuperXPuTTY](https://github.com/waukeerules/superputty): the same sessions tree, PostgreSQL Database sessions, icons, environment colours and tabs, with SSH through the Mac's own OpenSSH instead of PuTTY.

- **Shared data:** it reads the same `Sessions.xml`, `Folders.xml`, button bars, highlighting rules and Database sessions table, so Windows and Mac users of one team see the same sessions.
- **Unsupported features:** anything that needs PuTTY or Windows is shown greyed out, with the reason, rather than hidden.

## Status

Planning. No code yet.

- **[docs/PLAN.md](docs/PLAN.md):** the plan. It covers the options and the recommended stack (Tauri 2, reusing SuperXPuTTY's xterm.js and Monaco pages), the feature matrix, data compatibility, the UI mapping, milestones and open questions.

## Licence

To be decided. SuperXPuTTY is MIT.
