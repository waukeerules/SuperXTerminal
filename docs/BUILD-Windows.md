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

```powershell
npm run tauri build
```

It leaves both installers in `src-tauri\target\release\bundle\`:

| Package | File | Size |
|---|---|---|
| Windows Installer (WiX) | `msi\SuperXTerminal_0.1.0_x64_en-US.msi` | 7 MB |
| Setup program (NSIS) | `nsis\SuperXTerminal_0.1.0_x64-setup.exe` | 5 MB |

Both install `superxterminal.exe` (the pages are built into it) and `THIRD-PARTY-NOTICES.txt` in
`Program Files\SuperXTerminal`; WebView2 comes with Windows. Notes:

- **Time:** the first release build takes 15 to 20 minutes: every dependency is compiled again in release mode, and
  the last step, with `lto = true` and `codegen-units = 1` (`src-tauri/Cargo.toml`), runs on one core. Later builds
  redo only that step.
- **Tools:** the first build downloads WiX 3 and NSIS (about 30 MB, from Tauri's GitHub releases) into the user's
  Tauri cache.
- **Notices:** `npm run notices` runs first and writes `THIRD-PARTY-NOTICES.txt` for the system building, so the
  installer has Windows' crates. The copy in the repository is the Mac's: don't commit the Windows one over it
  (`git checkout THIRD-PARTY-NOTICES.txt` after a build).
- **Signing:** none yet, so SmartScreen warns on the first run. Signing is set with
  `bundle.windows.certificateThumbprint` (or a signing command) in `src-tauri/tauri.conf.json`.

The manual GitHub workflow (`.github/workflows/build.yml`, run by hand from Actions) builds Windows unsigned, for an
occasional check that it still compiles.

## What works so far

Tried in `npm run tauri dev` on Windows 11:

- **The checks:** `npm run check` (no errors) and `cargo test` (all pass; the database tests are skipped, as on the Mac).
- **The window:** the quick-connect and command toolbars, the Sessions panel, tabs, the button bar and the status bar.
  The app menu (SuperXTerminal, File, Edit, View, Tools, Window) becomes a menu bar in the window.
- **Sessions:** a SuperXPuTTY settings folder (Sessions.xml, button bars, icons, highlighting) is read as it is: the
  tree, icons, environment colours on the tree and tabs, Recent and the button bar.
- **Local Shell, cmd and PowerShell sessions:** cmd.exe in a ConPTY through `portable-pty`, with typing and output,
  and Ctrl+C stopping a running command (`ping -t`). As SuperXPuTTY: cmd sessions run cmd.exe, PowerShell sessions
  Windows PowerShell (powershell.exe) and WSL sessions wsl.exe, from System32, in the user's folder.
- **SSH:** Windows' own OpenSSH (`%SystemRoot%\System32\OpenSSH`), logging in with a password (a Cisco switch) or a
  key, a PuTTY .ppk key included (converted, below). An SSH agent (1Password's, Windows' ssh-agent) is used as ssh
  uses it: when it holds the session's key, ssh asks it to sign.
- **RDP:** the built-in client (IronRDP), logged in to a Windows 10 desktop.
- **Database sessions:** the PostgreSQL table, read and shown under Database sessions.
- **SSH algorithms:** per profile and per session (SSH algorithms…), listing what Windows' OpenSSH has.
- **File menu:** SuperXPuTTY's imports (SuperPuTTY XML, PuTTY/KiTTY from the registry, PuTTY Portable's folder,
  PuTTY CM, RDP's recent computers from the registry), Export Sessions, Edit Sessions Externally (in Notepad++ when
  it's installed) and Reload Sessions File. The registry is read through `reg.exe`.
- **Shortcuts:** Windows' own defaults (Ctrl+Shift+T, Ctrl+Shift+W, Ctrl+Shift+D...), caught by the page, with the
  terminal keeping its Ctrl keys (Ctrl+C, Ctrl+D, Ctrl+K...); menus and Settings > Shortcuts show "Ctrl+Shift+T".

- **File transfer:** over SCP to a Cisco switch (no SFTP there): logged in with the session's password through the
  login helper, listed `flash:`, and downloaded a file.

Not tried yet: file transfer over SFTP (a Linux server), Telnet and Raw, layouts, scripts, and the release build.

### Keys on Windows

The Mac's shortcuts are ⌘ keys, which a terminal never needs. On Windows the same keys with Ctrl are the terminal's
(Ctrl+D ends input, Ctrl+W deletes a word, Ctrl+K kills a line), so Windows has its own defaults, as Windows Terminal's
(`src-tauri/src/keys.rs`, `other`):

| Action | Mac | Windows |
|---|---|---|
| New Window / New Local Shell | ⌘N / ⌘T | Ctrl+Shift+N / Ctrl+Shift+T |
| Open Session / Switch Session | ⌘O / ⇧⌘O | Ctrl+Shift+O / Ctrl+Shift+P |
| Close Tab | ⌘W | Ctrl+Shift+W |
| Next / Previous Tab | ⇧⌘] / ⇧⌘[ | Ctrl+Tab / Ctrl+Shift+Tab |
| Split Right / Split Down | ⌘D / ⇧⌘D | Ctrl+Shift+D / Ctrl+Shift+E |
| Find / Clear Scrollback | ⌘F / ⌘K | Ctrl+Shift+F / Ctrl+Shift+K |
| Full Screen / Settings | ⌃⌘F / ⌘, | F11 / Ctrl+, |

Windows keeps Alt+F4, and the terminal page keeps Ctrl+Shift+C/V and Ctrl/Shift+Insert for copy and paste; Settings >
Shortcuts won't give them to an action. On Windows a menu's keys don't fire while the page has the keyboard, so
`src/lib/shortcuts.ts` catches them in the window and in each tab's page. In RDP tabs the Windows key goes to the
remote computer as itself, and Ctrl+Alt+End is Ctrl+Alt+Delete (Windows keeps that one), as in Remote Desktop
Connection.

## What still has to be done for Windows

Found by running it:

- **Serial:** the device code in `src-tauri/src/streams.rs` is termios, Unix only. On Windows it's stubbed out (serial
  tabs say they aren't supported yet); it needs COM port code (`CreateFile`, `SetCommState`, `SetCommTimeouts`,
  `SetCommBreak`). Serial names are already `COM1`… in shared sessions, so the "which `/dev/cu.*` is it" step isn't
  needed on Windows.
- **Opening other apps:** Connect external, RDP through Windows App and VNC still run the Mac's `open` and
  `osascript` (`external.rs`). On Windows: Windows Terminal or a console window for Connect external, RDP files in
  `mstsc.exe`, VNC in a Windows viewer, FileZilla's Windows path.
- **PuTTY profiles:** a session's profile (PuttySession) is looked up in SuperXTerminal's own profiles (Settings >
  Profiles), so on Windows a profile that only exists in PuTTY or KiTTY gives "Profile … isn't set up on this PC".
  Reading PuTTY's saved sessions from the registry (and KiTTY's from its folder or `kitty.ini`) would bring them over.
- **"this Mac":** the session launch messages say "this PC" on Windows; a few others (file transfer, Connect
  external) still say "this Mac", in code that's Mac-only today.

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

Fixed on the way:

- **The release build's scripts:** `scripts/notices.mjs` found its folder from the script URL's pathname
  (`/C:/Dev/...` on Windows), and both it and `check-licenses.mjs` ran `npm` directly, which is `npm.cmd` on Windows
  and needs a shell. They use `fileURLToPath` and run npm through the shell now.
- **The app log and showing files:** the log went to `~/Library/Logs` (a `Library` folder in a Windows user's home);
  it's in `%LOCALAPPDATA%\SuperXTerminal\logs` now. Show Log File and file transfer's show in folder use Explorer
  (`/select,`), and Third-Party Notices opens in the app for .txt files, instead of the Mac's `open`.
- **Blank terminal tabs:** WebView2 puts its own `window.chrome.webview` in every frame, which replaced the stand-in
  from `public/web/host/webview.js`, so the pages' messages never reached the main window. The stand-in is now defined
  read-only, so it stays.
- **ssh from the PATH:** `ssh`, `scp` and sftp were run by name, so on a PC with MSYS2, Git for Windows or Cygwin
  another ssh.exe could come first. Windows' own OpenSSH is used when it's installed (`launch::openssh`).
- **Ctrl+C didn't stop `ping -t`:** a process started with "ignore Ctrl+C" (some terminals and launchers start
  programs that way) passes it on to every program it starts, so the shells in its tabs ignored the Ctrl+C that the
  pseudo console raises for 0x03. The app clears the flag as it starts (`console.rs`), and never sets it.
- **Shortcuts:** the Mac's defaults took the terminal's Ctrl keys, the menu's keys never fired, and labels showed ⌘
  (see Keys on Windows).
- **File transfer on Windows:** it failed at once: the login helper that gives ssh the session's password
  (`askpass.rs`) used a Unix socket only. On Windows it listens on 127.0.0.1 (a port Windows picks, never the
  network), with the port in the token, and finds the asking ssh by its parent process. Then scp never started:
  Windows' pseudo console asks where the cursor is (`ESC [6n`) as it starts and waits for the answer, which a tab's
  page gives and file transfer's hidden one didn't; `run_scp` answers it now.
- **Downloads over SCP that copied nothing:** the local folder came as Windows' canonical path (`\\?\C:\...`), which
  scp read as a host, so it asked the switch to copy between two remote places, which IOS ended at once without an
  error, and the transfer said Done. Local paths lose that prefix now, and a download counts only when the file is
  there at the listed size.
- **PuTTY .ppk keys:** OpenSSH can't read them, so ssh gets a converted copy (`ppk.rs`), written in the user's folder
  (`keys\`) with the user's own access only: Windows' OpenSSH refuses a key with any other entry, even one inherited
  from the folder, and then quietly falls back to an agent or other keys. Keys with a passphrase still need
  converting with puttygen.

## Checks

The same as the Mac's ([BUILD-macOS.md](BUILD-macOS.md), section 5): `npm run check`, `cargo test`, `cargo deny check`
and `npm run licenses`.
