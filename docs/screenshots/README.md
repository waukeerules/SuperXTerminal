# Screenshots

The pictures in the main [README](../../README.md). Before adding one, mask anything private: your name and the
Mac's name in prompts, IP addresses and host names, usernames, and session names that say where things are.

Capture a window (click it; no shadow):

```bash
screencapture -i -w -o docs/screenshots/<name>.png
```

| File | What it shows |
|---|---|
| `main-window.png` | The sessions tree's roots (Favorites, Recent, Saved, Private, Database) and the button bar |
| `split-panels.png` | A Cisco switch and a Linux server side by side, `show ip interface brief` highlighted |
| `right-click-menu.png` | The terminal's right-click menu, with Wrap Lines |
| `edit-session.png` | Edit Session for a Database session, with `{key}` placeholders |
| `file-transfer.png` | A file transfer tab in SCP mode, an IOS image downloading |
| `settings-terminal.png` | Settings > Terminal |

Wanted later: a Remote Desktop tab, and Highlighted Output beside a terminal.
