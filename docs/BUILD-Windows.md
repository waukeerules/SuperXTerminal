# Building SuperXTerminal on Windows

> **Placeholder.** SuperXTerminal is built and released on the Mac first ([BUILD-macOS.md](BUILD-macOS.md)). The code
> is kept buildable on Windows, but a Windows release hasn't been made or tried yet. This page sets out what a Windows
> build needs and what still has to be done; fill it in as the Windows build is brought up.

On Windows, SuperXPuTTY remains the main application. Whether a Windows SuperXTerminal should ever replace it, or stay a
lighter option beside it, is an open question ([PLAN.md](PLAN.md), section 6).

## What you'll need

| Tool | Notes |
|---|---|
| Windows 10 (1809 or later) or 11 | 64-bit |
| Microsoft C++ Build Tools | "Desktop development with C++" in the Visual Studio Installer |
| WebView2 Runtime | Already on Windows 11 and up-to-date Windows 10 |
| Rust | stable, `x86_64-pc-windows-msvc` ([rustup.rs](https://rustup.rs)) |
| Node.js | LTS (22 or later) |
| OpenSSH client | Built into Windows 10 and 11 (Settings → Optional features → OpenSSH Client). SSH tabs, file transfer and Connect external use it |
| A code-signing certificate | For a release others install; otherwise SmartScreen warns |

## Expected steps

These match the Mac's; they haven't been run on Windows yet.

```powershell
git clone https://github.com/waukeerules/SuperXTerminal.git
cd SuperXTerminal
npm install
npm run tauri dev
```

```powershell
npm run tauri build
```

The release build should leave an `.msi` (WiX) and a setup `.exe` (NSIS) in `src-tauri\target\release\bundle\`. Signing
is set with `bundle.windows.certificateThumbprint` (or a signing command) in `src-tauri/tauri.conf.json`.

The manual GitHub workflow (`.github/workflows/build.yml`, run by hand from Actions) builds Windows unsigned, for an
occasional check that it still compiles.

## What still has to be done for Windows

Most of the app is shared, including the RDP engine (IronRDP), the SFTP code, the sessions tree, the database and the
pages. These parts are Mac-specific today:

- **Local shells and terminals:** ConPTY through `portable-pty` (it supports it), and PowerShell/cmd as the default
  shell instead of the user's login shell.
- **Saved passwords:** the `keyring` crate is already built with its Windows backend (Credential Manager); it needs
  trying.
- **RDP clipboard:** IronRDP's own Windows clipboard backend (`ironrdp-cliprdr-native`) in place of
  `src-tauri/src/rdp_clipboard.rs`'s Mac pasteboard.
- **RDP keys:** a low-level keyboard hook so Windows key combinations (Win+L, Win+R) reach the remote desktop instead of
  the local PC, following the session's "Windows key combinations" option. The ⌘-to-Ctrl and ⌘-as-Windows-key mapping
  in `src/lib/terminal/RdpHost.ts` is for the Mac only, and the ⌘ tap watcher (`src-tauri/src/command_tap.rs`) already
  builds to nothing elsewhere.
- **Connect external:** Windows Terminal or a console window instead of Terminal.app and AppleScript.
- **ssh's askpass:** the login helper (`src-tauri/src/askpass.rs`) uses a Unix socket; Windows needs a named pipe.
  Windows' OpenSSH supports `SSH_ASKPASS`.
- **Opening other apps:** RDP files in `mstsc.exe` (or the built-in client), VNC in a Windows viewer, FileZilla's Windows
  path, in place of `open`.
- **The menu bar and tray:** Windows has no app menu; its items move into the window's menus.
- **Paths:** serial ports are `COM1`… (already understood from shared sessions), and the user folder is under
  `%APPDATA%`.
- **Third-party notices:** `scripts/notices.mjs` lists the crates for the Mac it runs on; run it on Windows for a
  Windows build, as some crates differ by system.

## Checks

The same as the Mac's ([BUILD-macOS.md](BUILD-macOS.md), section 5): `npm run check`, `cargo test`, `cargo deny check`
and `npm run licenses`.
