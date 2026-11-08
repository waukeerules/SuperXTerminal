# Working on SuperXTerminal

Rules for anyone changing this code, people and AI assistants alike. `CLAUDE.md` points here, so Claude Code reads it
too. How to build is in `docs/BUILD-macOS.md`, `docs/BUILD-Windows.md` and `docs/BUILD-Linux.md`; how to test, in `docs/TESTING.md`.

## Tests

### Every bug fix comes with a test

When you fix a bug, add a test that fails without the fix and passes with it. Then take the fix out for a moment and
check the test really fails. A test that passes either way protects nothing.

- **Name it for what should happen**, as the existing tests do: `the_end_of_a_program_is_told_though_its_output_stays_open`,
  `"draws every icon in the colour picked, and in its own again on Default"`.
- **Put it beside the code**: a `#[cfg(test)] mod tests` at the end of the Rust file, a `*.test.ts` next to the
  TypeScript or Svelte file, or `tests/browser/` for the terminal page (`docs/TESTING.md` has which is which).
- **Test what the user saw.** If the bug was "the icons in Choose Icon don't take the colour picked", render the
  dialog, pick a colour and check the icons, rather than testing a helper the dialog happens to call.

### A bug that comes back is fixed for good

If a problem was fixed before and has come back (a regression), the earlier fix had no test, or the test missed it.
Before fixing it again:

1. Look in `git log` for the earlier fix, and find out why it was lost (code rewritten, a merge, or never committed).
2. Write the test first, and watch it fail against the code as it is.
3. Fix it, and watch the test pass.
4. Say in the commit message that it's a regression, and what the test now guards.

### Tests run on both systems

The app ships on macOS and Windows, and each is tested on its own machine. A test that relies on one system's tools
must work on the other, or say why it can't:

- Don't assume `sh`, `sleep` or Unix paths. On Windows, `sh` may be Git's (MSYS), which behaves differently, and a
  `cmd` script on the PATH can come before Windows' `cmd.exe`. Choose by `cfg!(windows)` in Rust or `isMac` in
  TypeScript.
- A test driving a pseudo terminal on Windows must answer ConPTY's opening cursor position query (`ESC[6n`), as the
  tab's page does, or the program never starts (see `answer_the_terminal` in `src-tauri/src/pty.rs`).
- Mark a test `#[cfg(...)]` only when its subject exists on one system alone, and say so in a comment.

Before committing, run the suites for what you changed: `cd src-tauri && cargo test`, `npm test`,
and `npm run test:browser` for the terminal page.

## Code documentation

The comments are written for the next developer, or AI assistant, who has never seen the file. Most of the code
already follows this; keep it that way.

### Every file says what it is

Every source file and script starts with a comment saying what it's for, how it fits with the rest, and anything
surprising about it:

- Rust: `//!` lines (`src-tauri/src/pty.rs`).
- TypeScript and JavaScript: a `/** ... */` or `//` block before the imports (`src/lib/shortcuts.ts`,
  `scripts/check-licenses.mjs`).
- Svelte: an `<!-- ... -->` block before `<script>` (`src/lib/icons/IconPicker.svelte`).
- Tests: a comment saying what's tested and how the parts it needs are stood in for (`src/lib/sessions/Overrides.test.ts`).
- Shell, Swift and other scripts: a comment after the `#!` line, saying what it does and how to run it.

### Every function says what it does

Every function, method, type, constant and component prop gets a doc comment, including private ones:

- Rust: `///`, with `[`links`]` to the types it names.
- TypeScript: `/** ... */`, with `@param` and `@returns` when they say more than the names do.
- Say **why** as well as what: the reason for a delay, a limit or a workaround, and the system or device behaviour it
  answers (ConPTY, WebView2, Cisco's login attempts...). Where the code mirrors SuperXPuTTY, name what it mirrors
  (`TerminalControl.OnExited`, `IconLibrary.DrawOnCircle`).
- Keep them true. When you change what a function does, change its comment in the same commit.

### How to write

- Plain English, in the voice of the comments already here: short sentences, the user's words for things (the tab,
  the session, Settings > Profiles), no filler.
- Words that differ between the Mac and Windows (Cmd or Ctrl, Finder or File Explorer, `~/.ssh` or
  `C:\Users\<you>\.ssh`) come from `src/lib/platform.ts`, not written out in a component.
- Inside a function, comment what isn't obvious from the code, not what is.

## Commits

- One change per commit. The subject is the area and what changed, in the user's words, without a trailing full stop
  ("Choose Icon: the icons take the colour chosen, as the preview").
- The body says what was wrong, why, and what's different now, plus anything not yet checked on the other system.
