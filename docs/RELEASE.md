# Making a release

Releases are built locally, on a Mac and on a Windows PC, and published to GitHub (`waukeerules/SuperXTerminal`).
Nothing builds on GitHub automatically: the manual workflow in `.github/workflows/build.yml` is only for an occasional
check, as its runs cost money.

## One command on each computer

The Mac goes first, then the Windows PC, which also builds the Linux packages in Docker:

```bash
./release.sh --release 0.8.0
```

```bat
release.bat --release 0.8.0
```

Each pulls, sets the version if it isn't already the one given (committed and pushed, so the second computer's pull
gets it), runs the checks, builds this computer's installers, and puts them in the GitHub release as a draft. The
first to run tags the commit and makes the draft; the second adds to it. Both just run `scripts/release-all.mjs`,
which stops at the first step that fails: fix it and run the same command again. Other options: `--skip-checks`,
`--final` (make the release public at the end, once everything is in), and `--no-linux` on Windows (or `--linux` on
the Mac) for where the Linux packages are built. Docker Desktop must be running on the PC.

The notes are still written by hand (step 3 below); `npm run release:publish` after writing them puts them in.

## The commands it runs

Four npm commands do it, the same on both computers:

| Command | What it does |
|---|---|
| `npm run release:version -- 0.3.0` | Sets the version in all five places it's written, and commits just those files (`scripts/set-version.mjs`) |
| `npm run release:checks` | The type check, the front-end and browser tests, the licence check, the Rust tests and `cargo deny` |
| `npm run release` | Builds this computer's installers and copies them into `releases/<version>/` (`scripts/release.mjs`) |
| `npm run release:publish` | Puts `releases/<version>/` into the version's GitHub release, with `NOTES.md` there as its notes; a draft until `-- --final` (`scripts/release-publish.mjs`) |

The steps to click through are in each build guide:

- **Mac:** [BUILD-macOS.md, Release, step by step](BUILD-macOS.md#release-step-by-step)
- **Windows:** [BUILD-Windows.md, Release, step by step](BUILD-Windows.md#release-step-by-step)

## How it fits together

1. **The version, once, on one computer.** `npm run release:version -- 0.3.0` changes `package.json`,
   `package-lock.json` (twice), `src-tauri/tauri.conf.json` (which names the installers), `src-tauri/Cargo.toml` and
   the app's entry in `src-tauri/Cargo.lock`, and commits them as "Version 0.3.0". Push it; the other computer pulls.
2. **Build on each computer.** `npm run release` builds and collects:

   | Computer | Installer | Signed with |
   |---|---|---|
   | Mac | `SuperXTerminal_<version>_aarch64.dmg` | the Keychain's Developer ID Application certificate, else its Apple Development one, else ad hoc |
   | Windows | `SuperXTerminal_<version>_x64_en-US.msi` | not signed yet |
   | Windows | `SuperXTerminal_<version>_x64-setup.exe` | not signed yet |
   | Windows, in Docker | `SuperXTerminal_<version>_amd64.deb`, `SuperXTerminal-<version>-1.x86_64.rpm`, `SuperXTerminal_<version>_amd64.AppImage` | not signed |

   The Linux packages come from `node scripts/build-linux.mjs --into releases/<version>` ([BUILD-Linux.md](BUILD-Linux.md)),
   which `release.bat` runs after Windows' build.

   Git ignores `releases/`, so it never syncs between the computers.
3. **The notes.** On the first computer, write `releases/<version>/NOTES.md`: what's new, the download table, and the
   "Before you install" warnings for Gatekeeper and SmartScreen. `gh release view --json body -q .body` prints the
   last release's to start from. It's never uploaded as a file. Without it the draft gets GitHub's generated notes,
   which are built from pull requests and so are little more than a changelog link here.
4. **Publish.** The first computer's `npm run release:publish` tags the commit (`v<version>`, pushed) and makes the
   GitHub release as a **draft**, with the notes. Before tagging it checks that nothing tracked is uncommitted and
   the commit is `origin/main`, so the tag is on the code that was built and pushed. The second computer's adds its
   installers to that draft, and each run puts `NOTES.md` in again when the folder has it.
   `npm run release:publish -- --final` then makes it public: that's when the app's update check
   (Settings → General → Updates) offers it, so it refuses until the Mac's `.dmg`, Windows' MSI and setup `.exe`
   and Linux's `.deb`, `.rpm` and AppImage are all in.

   Copying one computer's installers into the other's `releases/<version>/` and publishing from there works too: it
   uploads whatever is in the folder.

`release:publish` needs GitHub's CLI, signed in once per computer: `brew install gh` or `winget install GitHub.cli`,
then `gh auth login`. Keep it up to date: older versions can't find a draft by its tag. Nothing it does runs a GitHub Actions workflow.

## Doing it by hand

What the scripts do, for when one of them can't:

- **The version:** the five places above, kept the same.
- **The build:** `npm run tauri build` (on the Mac with `APPLE_SIGNING_IDENTITY` set to the certificate's name, from
  `security find-identity -v -p codesigning`). The installers are in `src-tauri/target/release/bundle/` (`dmg/`, or
  `msi/` and `nsis/`).
- **Publishing:** `git tag v0.3.0 && git push origin v0.3.0`, then
  `gh release create v0.3.0 releases/0.3.0/*.dmg --draft --title "SuperXTerminal 0.3.0" --notes-file releases/0.3.0/NOTES.md`,
  from the other computer `gh release upload v0.3.0 releases/0.3.0/*.msi releases/0.3.0/*-setup.exe` and the Linux
  packages, and once all six are in, `gh release edit v0.3.0 --draft=false`.

## Signing

- **Mac:** builds are signed with an Apple Development certificate. Other Macs warn about them until there's a
  Developer ID and notarisation ([BUILD-macOS.md](BUILD-macOS.md), sections 2 and 7).
- **Windows:** builds aren't signed yet, so SmartScreen warns on the first run ([BUILD-Windows.md](BUILD-Windows.md#release-build)).
