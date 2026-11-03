# Building SuperXTerminal on macOS

How to run SuperXTerminal from source, check it, and make a release: the `.app` and the `.dmg` that installs it. Builds
are made on a Mac, locally; nothing builds on GitHub automatically (see [PLAN.md](PLAN.md), "Developing on a Mac").

The Windows build is in [BUILD-Windows.md](BUILD-Windows.md).

## 1. What you need

| Tool | Version | How to get it |
|---|---|---|
| macOS | 12 or later to run the app; a recent macOS to build | |
| Xcode command line tools | current | `xcode-select --install` (full Xcode also works) |
| Rust | stable | [rustup.rs](https://rustup.rs), then `rustup update stable` |
| Node.js | LTS (22 or later) | [nodejs.org](https://nodejs.org) or `brew install node` |
| cargo-deny | 0.20 or later | `cargo install cargo-deny` (licence checks) |

Apple silicon and Intel Macs both build; a build runs on the kind of Mac it was made on (see "Intel and universal
builds" below).

## 2. The signing certificate (once per Mac)

Builds are signed so macOS knows them as the same app from one build to the next. Without it the Keychain asks again
for every saved password after each rebuild.

- **For development and your own release builds:** an **Apple Development** certificate. It's free with any Apple ID:
  1. Xcode → Settings → Accounts → **+** → Apple ID, and sign in.
  2. Select the account's team (a "Personal Team" for an Apple ID without a paid membership) → **Manage
     Certificates…** → **+** → **Apple Development**.
  3. Check it's there:

     ```bash
     security find-identity -v -p codesigning
     ```

- **For releases other people install:** a **Developer ID Application** certificate, which needs an Apple Developer
  Program membership (USD 99 a year), and notarisation (section 7). Until then, other Macs warn that the app is from an
  unidentified developer.

Development builds are signed automatically before they run (`scripts/dev-sign-run.sh`, set as Cargo's runner in
`src-tauri/.cargo/config.toml`). With no certificate they run unsigned.

## 3. Get the code and install the packages

```bash
git clone https://github.com/waukeerules/SuperXTerminal.git
```

```bash
cd SuperXTerminal && npm install
```

Run `npm install` again after `package.json` changes. Rust crates download on the first build.

## 4. Run it while developing

```bash
npm run tauri dev
```

- **What it does:** starts the web part's dev server (Vite, on 127.0.0.1:1420) and the app. Changes to `src/` reload in
  the window; changes to `src-tauri/` rebuild the app and restart it, closing its tabs.
- **One copy at a time:** the app is single instance, so quit an installed or release copy first. Otherwise the new one
  hands over to it and quits.
- **Its data:** settings and Private sessions are in `~/Library/Application Support/SuperXTerminal`, the log in
  `~/Library/Logs/SuperXTerminal`. Development and release builds share them.

## 5. Check it

Before a release, all of these should pass:

```bash
npm run check
```

```bash
npm test
```

```bash
npm run test:browser
```

```bash
cd src-tauri && cargo test
```

```bash
cd src-tauri && cargo deny check
```

```bash
npm run licenses
```

- **`npm run check`:** Svelte and TypeScript.
- **`npm test`:** the front end's unit and component tests (Vitest), a few seconds.
- **`npm run test:browser`:** the terminal page in WebKit and Chromium (Playwright). The first time, install the
  browsers: `npx playwright install webkit chromium`.
- **`cargo test`:** the Rust unit tests. See [TESTING.md](TESTING.md) for where each kind of test lives.
- **`cargo deny check`:** Rust licences against the allowed list in `src-tauri/deny.toml` (permissive only), known
  vulnerabilities and sources.
- **`npm run licenses`:** the same licence rule for the npm packages the app ships.

## 6. Make a release build

Quit any running SuperXTerminal first (section 4). Then, with your certificate's name from
`security find-identity -v -p codesigning`:

```bash
APPLE_SIGNING_IDENTITY="Apple Development: Your Name (TEAMID)" npm run tauri build
```

It takes a few minutes. What happens:

1. **`npm run notices`** writes `THIRD-PARTY-NOTICES.txt`: every component the app ships and its licence text
   (`scripts/notices.mjs`). It stops the build if a component has no licence text.
2. **`npm run build`** builds the web part into `dist/`.
3. **Cargo** builds the release program.
4. **The bundler** makes the `.app` and signs it with the hardened runtime and `src-tauri/Entitlements.plist`, then
   makes the `.dmg`. It opens the `.dmg` in Finder briefly to lay out its window; leave it alone until the build ends.

The results:

| File | What it is |
|---|---|
| `src-tauri/target/release/bundle/macos/SuperXTerminal.app` | The app |
| `src-tauri/target/release/bundle/dmg/SuperXTerminal_<version>_aarch64.dmg` | The installer: drag the app onto Applications |

"Skipping app notarization" in the output is expected without a Developer ID (section 7).

**Without a certificate:** leave out `APPLE_SIGNING_IDENTITY`. The app is then signed ad hoc, which runs on the Mac that
built it.

**The version** is `version` in `src-tauri/tauri.conf.json` (and `package.json` and `src-tauri/Cargo.toml`, kept the
same). Change all three for a new release.

### Check the build

```bash
codesign --verify --deep --strict --verbose=2 src-tauri/target/release/bundle/macos/SuperXTerminal.app
```

Then quit the development app, open the built one and try:

- a Local Shell tab shows its prompt, and an SSH session connects;
- File transfer (SFTP) lists both sides;
- an RDP session connects (if you have a test machine);
- Connect external: the first time, macOS asks to let SuperXTerminal control Terminal;
- SuperXTerminal → Third-Party Notices opens the notices.

Release builds load their pages from Tauri's `tauri://` scheme, not the dev server. A page that works in development
can fail there, which is why it's worth trying the built app and not only `npm run tauri dev`.

## 7. Notarise (Developer ID only)

With a Developer ID Application certificate, the bundler notarises the app and staples the ticket when these are set:

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Company (TEAMID)"
```

```bash
export APPLE_ID="you@example.com" APPLE_PASSWORD="app-specific-password" APPLE_TEAM_ID="TEAMID"
```

Then run `npm run tauri build` as in section 6. The password is an app-specific password from
[account.apple.com](https://account.apple.com), not your Apple ID's own. An App Store Connect API key
(`APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_KEY_PATH`) works instead. Keep these out of the repository and out of
shell history, for example in the Keychain or a file your shell reads.

Check the result:

```bash
spctl --assess --type execute -v src-tauri/target/release/bundle/macos/SuperXTerminal.app
```

It should say "accepted, source=Notarized Developer ID".

## 8. Publish a release

See [RELEASE.md](RELEASE.md): setting the version, collecting the Mac and Windows installers in `releases/`,
tagging, and the GitHub release. The app's update check (Settings → General → Updates) offers the repository's
latest release.

## 9. Installing and running a build

- **From the `.dmg`:** open it and drag SuperXTerminal onto Applications.
- **An unsigned or Apple Development build on another Mac:** right-click the app → Open, or
  `xattr -dr com.apple.quarantine /Applications/SuperXTerminal.app`.
- **Permissions macOS asks for:** local network access (to reach your devices), controlling Terminal (Connect
  external), and Keychain access for saved passwords (choose Always Allow).

## The installer window

The `.dmg` opens a 660 × 400 window with the app on the left, Applications on the right and an arrow between them.
Its background is `src-tauri/dmg/background.tiff` (1x and 2x), drawn by `scripts/dmg-background.swift`. The window
size and icon positions are `bundle.macOS.dmg` in `src-tauri/tauri.conf.json`; keep them and the script's positions
the same. To redraw it after changing the script:

```bash
swift scripts/dmg-background.swift && tiffutil -cathidpicheck src-tauri/dmg/background.png src-tauri/dmg/background@2x.png -out src-tauri/dmg/background.tiff && rm src-tauri/dmg/background*.png
```

## The app icon

`src-tauri/icons/app-icon.png` (1024 × 1024) is drawn by `scripts/app-icon.swift`; every size the app needs
(`icon.icns`, `icon.ico`, the PNGs) is made from it. To change it, edit the script, then:

```bash
swift scripts/app-icon.swift && npx tauri icon src-tauri/icons/app-icon.png && rm -rf src-tauri/icons/android src-tauri/icons/ios
```

The menu bar icon is separate: `src-tauri/icons/tray-template@2x.png`, black on transparent, which macOS tints.
Finder and the Dock cache icons, so a new one can take a restart of the Dock (`killall Dock`) to show.

## Intel and universal builds

A build runs on the kind of Mac it was made on (`aarch64` on Apple silicon). For one `.dmg` that runs on both:

```bash
rustup target add x86_64-apple-darwin
```

```bash
APPLE_SIGNING_IDENTITY="..." npm run tauri build -- --target universal-apple-darwin
```

The result is in `src-tauri/target/universal-apple-darwin/release/bundle/`. This hasn't been tried yet.

## When something goes wrong

- **The app starts and quits at once:** another copy is running (single instance). Quit it, including the development
  app.
- **Tabs are blank in a release build but fine in development:** a page relying on the dev server's
  `http://localhost:1420` (its origin, an absolute URL). Release builds use `tauri://localhost`.
- **The Keychain asks for passwords after every build:** the build isn't signed with a certificate that has a team ID
  (section 2).
- **"SuperXTerminal can't be opened because Apple cannot check it":** a build that isn't notarised, on another Mac
  (section 9).
- **The build stops at `npm run notices`:** a new dependency has no licence file, and its licence has no standard text
  in `scripts/licences/`. Add the text there (and its id to `scripts/notices.mjs`) after checking the licence is
  allowed.
- **`cargo deny` fails on a licence:** the dependency's licence isn't on the permissive list in `src-tauri/deny.toml`.
  Pick another crate rather than widening the list.
