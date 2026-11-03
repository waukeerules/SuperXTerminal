# Building SuperXTerminal on Windows

> **Early.** SuperXTerminal is built and released on the Mac first ([BUILD-macOS.md](BUILD-macOS.md)). The Windows
> build compiles and runs in development (`npm run tauri dev`, first tried on Windows 11 in November 2026), but a lot
> is still Mac-only: see [What works](#what-works-so-far) and [What still has to be done](#what-still-has-to-be-done-for-windows).
> No Windows release has been made.

On Windows, SuperXPuTTY remains the main application. Whether a Windows SuperXTerminal should ever replace it, or stay a
lighter option beside it, is an open question ([PLAN.md](PLAN.md), section 6).

## What you'll need

| Tool | Notes |
|---|---|
| Windows 10 (1809 or later) or 11 | 64-bit |
| Microsoft C++ Build Tools | "Desktop development with C++" in the Visual Studio Installer (Visual Studio 2022's MSVC 14.44 works) |
| WebView2 Runtime | Already on Windows 11 and up-to-date Windows 10 |
| Rust | stable, `x86_64-pc-windows-msvc`: `winget install Rustlang.Rustup`, then `rust-toolchain.toml` picks the toolchain on the first `cargo` run |
| Node.js | LTS (22 or later; 24 works) |
| OpenSSH client | Built into Windows 10 and 11 (Settings → Optional features → OpenSSH Client). SSH tabs, file transfer and Connect external use it |
| A code-signing certificate | For a release others install; otherwise SmartScreen warns |

rustup puts `cargo` in `%USERPROFILE%\.cargo\bin`; open a new terminal after installing it, or add that folder to the
PATH of the one you have.

## Running it

```powershell
git clone https://github.com/waukeerules/SuperXTerminal.git
cd SuperXTerminal
npm ci
npm run tauri dev
```

The first `npm run tauri dev` downloads and builds about 720 crates, which takes several minutes.

To inspect the pages with the browser's developer tools, start it with WebView2's debugging port and open
`http://127.0.0.1:9333` in Edge or Chrome (`edge://inspect`):

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9333"
npm run tauri dev
```

## Release build

Not tried yet. It should be:

```powershell
npm run tauri build
```

The release build should leave an `.msi` (WiX) and a setup `.exe` (NSIS) in `src-tauri\target\release\bundle\`. Signing
is set with `bundle.windows.certificateThumbprint` (or a signing command) in `src-tauri/tauri.conf.json`.

The manual GitHub workflow (`.github/workflows/build.yml`, run by hand from Actions) builds Windows unsigned, for an
occasional check that it still compiles.

## What works so far

Tried in `npm run tauri dev` on Windows 11:

- **The checks:** `npm run check` (no errors) and `cargo test` (all pass; the database tests are skipped, as on the Mac).
- **The window:** the quick-connect and command toolbars, the Sessions panel, tabs, the button bar and the status bar.
  The app menu (SuperXTerminal, File, Edit, View, Tools, Window) becomes a menu bar in the window.
- **Sessions:** a SuperXPuTTY settings folder (Sessions.xml, button bars, icons, highlighting) is read as it is: the
  tree, icons, environment colours on the tree and tabs, Recent and the button bar.
- **Local Shell and cmd sessions:** cmd.exe in a ConPTY through `portable-pty`, with typing and output.
- **SSH:** starts ssh in a tab and gets as far as the host key prompt (see the PATH problem below).
- **RDP:** the built-in client (IronRDP) opens its tab and asks for the login.

Not tried yet: logging in over SSH or RDP, Telnet and Raw, Database sessions, file transfer, layouts, scripts, and the
release build.

## What still has to be done for Windows

Found by running it:

- **ssh from the PATH:** SSH tabs, file transfer and Connect external run `ssh` (and `scp`) by name, so whichever comes
  first on the PATH runs: on a PC with MSYS2, Git for Windows or Cygwin that may not be Windows' own OpenSSH. Prefer
  `%SystemRoot%\System32\OpenSSH\ssh.exe` when it's there.
- **Serial:** the device code in `src-tauri/src/streams.rs` is termios, Unix only. On Windows it's stubbed out (serial
  tabs say they aren't supported yet); it needs COM port code (`CreateFile`, `SetCommState`, `SetCommTimeouts`,
  `SetCommBreak`). Serial names are already `COM1`… in shared sessions, so the "which `/dev/cu.*` is it" step isn't
  needed on Windows.
- **PowerShell sessions:** `launch.rs` only looks for pwsh where Homebrew puts it, and says to install it with brew.
  Use `pwsh.exe`, else `powershell.exe`.
- **WSL and cmd sessions:** both open the default shell (cmd.exe) with the notice "opened as a Local Shell on this Mac".
  WSL should run `wsl.exe` (with the session's distribution), and cmd needs no notice.
- **ssh's askpass:** the login helper (`src-tauri/src/askpass.rs`) uses a Unix socket, so on Windows it refuses ("Not
  on this system"): file transfer and Connect external can't be given a saved password. Windows needs a named pipe;
  Windows' OpenSSH supports `SSH_ASKPASS`.
- **Opening other apps:** `open` and `osascript` are run directly in `external.rs`, `lib.rs` (editing a file, showing
  the log) and `sftp.rs` (showing a file). On Windows: Windows Terminal or a console window for Connect external, RDP
  files in `mstsc.exe`, VNC in a Windows viewer, FileZilla's Windows path, `explorer /select,` to show a file and the
  default app (or Notepad) to edit one.
- **The app log:** `src-tauri/src/applog.rs` puts it in `~/Library/Logs/SuperXTerminal`, which on Windows makes a
  `Library` folder in the user's home. Use `%LOCALAPPDATA%\SuperXTerminal\logs`.
- **PuTTY profiles:** a session's profile (PuttySession) is looked up in SuperXTerminal's own profiles (Settings >
  Profiles), so on Windows a profile that only exists in PuTTY or KiTTY gives "Profile … isn't set up on this Mac".
  Reading PuTTY's saved sessions from the registry (and KiTTY's from its folder or `kitty.ini`) would bring them over.
- **"this Mac":** messages in the Rust core say "on this Mac"; they should name the system they're on.

Known from the code, not tried yet:

- **Saved passwords:** the `keyring` crate is already built with its Windows backend (Credential Manager).
- **RDP clipboard:** IronRDP's own Windows clipboard backend (`ironrdp-cliprdr-native`) in place of
  `src-tauri/src/rdp_clipboard.rs`'s Mac pasteboard.
- **RDP keys:** a low-level keyboard hook so Windows key combinations (Win+L, Win+R) reach the remote desktop instead of
  the local PC, following the session's "Windows key combinations" option. The ⌘-to-Ctrl and ⌘-as-Windows-key mapping
  in `src/lib/terminal/RdpHost.ts` is for the Mac only, and the ⌘ tap watcher (`src-tauri/src/command_tap.rs`) already
  builds to nothing elsewhere.
- **ssh:// and telnet:// links:** handed over by macOS only (`RunEvent::Opened`, `links_make_default`); Windows needs
  the schemes registered (Tauri's deep-link plugin) and the URL read from the command line.
- **The tray:** the menu bar icon becomes a notification area icon; check its menu and "keep running" behaviour.
- **Paths:** the user folder is `%APPDATA%\SuperXTerminal` (`sessions::user_folder`).
- **Third-party notices:** `scripts/notices.mjs` lists the crates for the system it runs on; run it on Windows for a
  Windows build, as some crates differ by system.

Fixed on the way:

- **Blank terminal tabs:** WebView2 puts its own `window.chrome.webview` in every frame, which replaced the stand-in
  from `public/web/host/webview.js`, so the pages' messages never reached the main window. The stand-in is now defined
  read-only, so it stays.

## Checks

The same as the Mac's ([BUILD-macOS.md](BUILD-macOS.md), section 5): `npm run check`, `cargo test`, `cargo deny check`
and `npm run licenses`.
