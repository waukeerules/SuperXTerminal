# Screenshots

The pictures in the main [README](../../README.md). Three are also what Linux software centres and the AppImage catalog
show of the app (`main-window.png`, `grid-view.png`, `settings-profiles.png`): the AppStream metainfo,
`src-tauri/linux/com.waukeerules.superxterminal.metainfo.xml`, links to them on GitHub, so keep their names, or
change the links with them (`scripts/metainfo.test.ts` fails on a link to a file that isn't there). Take them in **demo mode**, so no real session, host, address,
username or prompt is in them.

## Demo mode

Started with `--demo` (or `SUPERXTERMINAL_DEMO=1`), the app uses a fresh folder of made-up sessions instead of yours,
and every session runs a stand-in device instead of ssh: a Cisco IOS switch or router, a Juniper, a Linux server, or
for New Local Shell a made-up shell. They answer everyday commands (`show ip interface brief`, `show logging`,
`show interfaces terse`, `df -h`, `docker ps`, `ping`...) with canned output, highlighted as a real device's would be.

```bash
SUPERXTERMINAL_DEMO=1 npm run tauri dev
```

It runs beside a SuperXTerminal already open, and leaves yours alone: your sessions, settings, Keychain and window
position aren't read or changed. What it opens with:

- **Saved sessions:** Production (red, from the environment colour rules), Staging (orange), Branches and Lab (folder
  colours), with switch, router, firewall, server and brand icons.
- **Private sessions:** a home lab.
- **Database sessions:** 3,247 of them, a made-up company's sites by region, read from a cache (no database).
- **Favorites**, and **Recent** full (all 10, the Demo layout's tabs first).
- **Button bars** for Cisco, Junos and Linux tabs (text, SPSL and Rhai buttons).
- **Environment Variables:** twenty, used by sessions, folders and profiles. The Database sessions use them unevenly
  (usernames, regional jump hosts, DNS names, ssh options; the same "random" spread each start), so each shows its
  own count. One is a 1Password reference, and `{branch_user}` and `{field_user}` are used but deliberately unset.
- **The Demo layout:** core-sw-01, dist-sw-01 and edge-srx-01 beside web-01, k8s-ctl-01 and stg-db-01, some with a
  command already run; the terminal unwrapped, with line numbers.
- **Profiles:** twelve, from Network devices, Linux servers and Web servers (the one with a tunnel, so only web-01's
  tab asks for its port) to a jump host, CyberArk, short-lived keys and a light-coloured lab, used by the sessions.
- **Saved scripts:** the Rhai and SPSL examples in `docs/`. cisco-tour.rhai runs against the demo's switches.
- **File transfer:** a switch's flash over SCP (the device's own `dir`), beside the demo's own Downloads folder.
  Copies show their progress for about a minute and leave an empty file of the right size. Nothing on the device
  changes.
- The window at 1400 x 900 each time.

Everything is in `src-tauri/demo/` (sessions, button bars, the layout, and each device's answers in `devices/`) and
`src-tauri/src/demo.rs`; the folder is made again at every demo start, so changes made while demoing are gone the next
time.

Remote Desktop needs a real Windows machine. Name a test one and the demo's win-jump session connects to it; its
address goes only into the demo's own folder, and the tab asks for the password:

```bash
SUPERXTERMINAL_DEMO=1 SUPERXTERMINAL_DEMO_RDP=admin@<test machine> npm run tauri dev
```

## Taking them

Before each picture, make sure one terminal has the solid (focused) cursor, ideally the top left one: click into it.
Turning on Grid View, for one, can leave the focus in another tab. Clear anything left at a prompt too.

Capture a window (click it; no shadow):

```bash
screencapture -i -w -o docs/screenshots/<name>.png
```

| File | What it shows |
|---|---|
| `main-window.png` | Demo mode as it opens: the sessions tree's roots (Favorites, Recent with its 10, Saved, Private, 3,247 Database sessions), a switch and a server side by side with line numbers, and the button bar |
| `grid-view.png` | The same window after View > Grid View: the Demo layout's six tabs tiled three rows by two |
| `split-panels.png` | The tab area of `main-window.png`: core-sw-01's `show ip interface brief` highlighted beside web-01 |
| `right-click-menu.png` | The terminal's right-click menu: Wrap Lines off, Line Numbers on |
| `edit-session.png` | Edit Session for dc2-core-01, a Database session, with its `{noc_user}` placeholder |
| `file-transfer.png` | core-sw-01's file transfer over SCP, widened, an IOS image downloading |
| `settings-terminal.png` | Settings > Terminal |
| `settings-profiles.png` | Settings > Profiles, with Linux servers chosen |
| `remote-desktop.png` | win-jump's Remote Desktop tab (a test Windows 11 machine), widened |
| `environment-variables.png` | Tools > Environment Variables: where each is used, a 1Password reference, and one used but not set |
| `script-editor.png` | The Script Editor with cisco-tour.rhai open, wrapping off, scrolled to its code |
| `rhai-script.png` | cisco-tour.rhai run in core-sw-01 alone (Select sessions to send to), its result in the status bar |

Wanted later: Highlighted Output beside a terminal.
