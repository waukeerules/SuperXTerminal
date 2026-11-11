# Dependencies and vulnerabilities

How the app's npm packages and Rust crates are kept up to date, and how a known vulnerability in one is caught.
Updates are taken on a computer, not by bots: GitHub only watches and tells. Nothing runs on GitHub, as for builds
and releases.

## What watches for vulnerabilities

| Where | What it does | When |
|---|---|---|
| GitHub, **Dependabot alerts** | Checks `package-lock.json` and `src-tauri/Cargo.lock` against GitHub's advisory database; an alert arrives by email and in the repository's Security tab | All the time, on GitHub's side (free; no Actions run) |
| GitHub, **secret scanning** and **push protection** | Finds keys and tokens in the repository, and refuses a push that would add one | All the time, and at each push (free for a public repository) |
| `npm run audit` | `npm audit --omit=dev`: known vulnerabilities in the npm packages the app ships (the build tools, which don't ship, are left out) | In `npm run release:checks`, before every release |
| `cargo deny check` (in `src-tauri/`) | Known vulnerabilities in the Rust crates ([RustSec](https://rustsec.org)), yanked crates, licences and sources, by `src-tauri/deny.toml` | In `npm run release:checks`, before every release |

Dependabot's own pull requests (security updates and version updates) are off: they'd run on GitHub, and updates
are taken here instead, below.

A release can't go out with a known vulnerability in something shipped: `npm run release:checks` fails on it
([RELEASE.md](RELEASE.md)).

## Updating, step by step

At the start of each release, or once a month if none is due.

1. **See what's behind.**

   ```bash
   npm outdated
   ```

   ```bash
   cd src-tauri && cargo update --dry-run
   ```

2. **Take patch and minor updates together.**

   ```bash
   npm update
   ```

   ```bash
   cd src-tauri && cargo update
   ```

   When Playwright was updated, fetch its browsers too, on each computer, or every browser test fails to start
   ("Executable doesn't exist"):

   ```bash
   npx playwright install webkit chromium
   ```

   One commit, "Dependencies: patch and minor updates", the notable ones named in the body.

3. **A major version on its own** (TypeScript 6 to 7, Vite, Svelte, Tauri, xterm.js, dockview, Monaco): read its
   release notes first, then `npm install <package>@latest` (or the new version in `src-tauri/Cargo.toml`), in a
   commit of its own saying what had to change. The terminal page (xterm.js), the tab area (dockview) and the Script
   Editor (Monaco) are where a change of behaviour shows.

4. **Check it all,** then run the app on the Mac and on Windows:

   ```bash
   npm run release:checks
   ```

5. **Look again at what's set aside** (below), and take any fix that has come for one.

6. **Restart `tauri dev`** if it's running, so it uses the new Vite and crates.

## An alert between releases

- **In something shipped** (an npm dependency, not a dev dependency, or a Rust crate): update that package
  (`npm update <package>`, or `cargo update -p <crate>` in `src-tauri/`), check it (step 4), and make a release.
- **In a build tool only** (`npm audit` without `--omit=dev` lists these): take it with the next updates.
- **No fix yet:** decide whether the app can reach the weak spot. If it can't, set it aside (below) with the reason;
  if it can, find another way round it and say so in the release notes.

## What's set aside

Advisories let through on purpose, each with why and when to look again. Keep this list and the files' own the same.

| Advisory | In | Why it's let through | Look again |
|---|---|---|---|
| [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071) (the Marvin attack: timing that can leak an RSA private key while it decrypts) | `rsa`, through IronRDP (`picky`, `sspi`, `winscard`) | No fixed version exists. The Remote Desktop client only encrypts to the server's public key (NLA's CredSSP); it never decrypts or signs with an RSA private key of its own, as smart card logins, the one path that would, aren't used | When rsa 0.10 is released, or IronRDP moves on; `src-tauri/deny.toml`, `[advisories] ignore` |
| [GHSA-gcqx-w48m-w2h4](https://github.com/advisories/GHSA-gcqx-w48m-w2h4) (prototype poisoning in `deepmerge`), low | `deepmerge`, through `@sveltejs/vite-plugin-svelte` | A build tool, not shipped, so `npm run audit` leaves it out; no fixed version exists | When `deepmerge` or vite-plugin-svelte has a fix; shows in `npm audit` |
