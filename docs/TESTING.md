# Tests

Three kinds, each beside what it tests. All run on this computer; nothing runs on GitHub.

| Command | What | Where |
|---|---|---|
| `cd src-tauri && cargo test` | The Rust side: launching sessions (ssh's arguments, profiles, gateways), reading and writing the session files, imports, file transfer parsing | A `#[cfg(test)] mod tests` at the end of each file in `src-tauri/src/` |
| `npm test` | The front end's logic and its dialogs, with the Rust side stood in for | `*.test.ts` beside the file, in `src/` |
| `npm run test:browser` | The terminal page in real browsers: WebKit (the Mac app's engine) and Chromium (Windows') | `tests/browser/` |

## Front end (`npm test`)

[Vitest](https://vitest.dev), with the DOM from jsdom (`vitest.config.ts`, which takes the app's own Vite setup).

- **Plain logic**: import the function and check it, as `src/lib/settings/updates.test.ts` does. Logic inside a
  component is easier to test once it's a function of its own in a `.ts` file.
- **A dialog or tab**: render it with `@testing-library/svelte`, act on it with `@testing-library/user-event` (typing,
  clicks, right-clicks), and find things as a person would: by label, role and text. `ProfilesTab.test.ts` is the
  example.
- **The Rust side** is stood in for with Tauri's `mockIPC` (`@tauri-apps/api/mocks`): a function that answers each
  command (`profiles_list`, `profiles_save`, the dialog plugin's `plugin:dialog|ask`, …) and can record what it was
  sent. `src/test/setup.ts` clears it after each test, and gives jsdom the `matchMedia` it lacks.

## The terminal page (`npm run test:browser`)

[Playwright](https://playwright.dev) serves `public/web/terminal` with Vite on port 1421 (so `tauri dev` on 1420 can
run beside it) and drives it through the page's own test hooks, `window.terminalDev`: `message` sends it what a tab
would (config, prefs), `feedText` writes output as if a session sent it, and `term` is xterm.js itself. See
`tests/browser/terminal.spec.ts`.

- Wait for what's expected with `expect.poll` (or Playwright's own `expect(locator)`), not fixed pauses: WebKit draws
  later than Chromium, and a pause long enough for one is wasted on the other.
- The first time, install the browsers (about 900 MB, in `~/Library/Caches/ms-playwright`):

  ```bash
  npx playwright install webkit chromium
  ```

## When something breaks again

Every bug fix comes with a test that fails without the fix, and a bug that comes back (a regression) gets one before
it's fixed again, so it can't come back unnoticed a third time. `AGENTS.md` has the steps. `src/lib/icons/IconPicker.test.ts`
is an example: it checks every icon in Choose Icon is drawn in the colour picked.

## What isn't tested this way

The whole app, windows and all. Tauri's driver for that (`tauri-driver`, WebDriver) works on Windows and Linux only:
macOS's WKWebView has no WebDriver. So the app is tested in its parts (the Rust side, the front end with the Rust side
stood in for, the terminal page on its own), and checked by hand in the app for what joins them.

## Seeing what a tab is sent

When a tab draws something wrong (a line twice, a redraw in the wrong place), capture what it was actually sent
before guessing. Start the app with `SUPERXTERMINAL_CAPTURE` naming a folder:

```
$env:SUPERXTERMINAL_CAPTURE="C:\Dev\SuperXTerminal\captures"; npm run tauri dev
```

(`SUPERXTERMINAL_CAPTURE=~/captures npm run tauri dev` on the Mac.) Each tab writes a file there: its raw output
as it came from the pseudo terminal, its resizes, and its answer to the cursor position query, each with the
milliseconds since it opened (`pty.rs`, `capture_file`). Nothing typed is written, but what a device echoes back is,
so use it on lab devices and delete the folder after. It found that Windows' ConPTY repeats a program's first line
when resized before the program has printed anything (`PageHost.resizePty`).

## Demo mode

`SUPERXTERMINAL_DEMO=1 npm run tauri dev` starts the app on made-up sessions, each a stand-in device with canned
output (`src-tauri/src/demo.rs`, `demo_device.rs`), without touching your own sessions or settings. It's for
screenshots ([docs/screenshots/README.md](screenshots/README.md)), and for trying the tree, tabs, button bars and
highlighting without a lab. Its tests check the demo's files read as the app reads its own, and that every canned
answer is found by its command.
