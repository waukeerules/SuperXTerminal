# SuperXTerminal

A macOS counterpart of [SuperXPuTTY](https://github.com/waukeerules/superputty): the same sessions tree, PostgreSQL Database sessions, icons, environment colours and tabs, with SSH through the Mac's own OpenSSH instead of PuTTY.

- **Shared data:** it reads the same `Sessions.xml`, `Folders.xml`, button bars, highlighting rules and Database sessions table, so Windows and Mac users of one team see the same sessions.
- **Unsupported features:** anything that needs PuTTY or Windows is shown greyed out, with the reason, rather than hidden.

## Status

M0 (skeleton), M1 (sessions tree, SSH, profiles) and M2 (Database sessions) are done; M3 (scripting and commands) is next.

- **[docs/PLAN.md](docs/PLAN.md):** the plan. It covers the options and the chosen stack (Tauri 2 and Svelte 5, reusing SuperXPuTTY's xterm.js and Monaco pages), licensing, the feature matrix, data compatibility, the UI mapping, milestones and open questions.
- **[docs/ROADMAP.md](docs/ROADMAP.md):** progress, one line per feature, ticked ✅ as each is delivered.

Mac first; the code stays buildable on Windows and Linux. Builds and releases are made locally, as for SuperXPuTTY, and pushed to GitHub; nothing builds on GitHub automatically.

## Building

Needs Rust (`rustup`, stable) and Node (LTS). Then, in this folder:

```bash
npm install          # once, and after package.json changes
npm run tauri dev    # run the app; UI changes reload live, Rust changes rebuild and restart
npm run tauri build  # release build: the .app and .dmg in src-tauri/target/release/bundle/
```

Checks:

```bash
npm run check                          # Svelte and TypeScript
npm run licenses                       # licences of the shipped npm packages
(cd src-tauri && cargo deny check)     # Rust licences, advisories and sources (cargo install cargo-deny)
```

## Licence

To be decided. SuperXPuTTY is MIT.
