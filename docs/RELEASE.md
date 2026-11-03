# Making a release

Releases are built locally, on a Mac and on a Windows PC, and published to GitHub (`waukeerules/SuperXTerminal`).
Nothing builds on GitHub automatically: the manual workflow in `.github/workflows/build.yml` is only for an occasional
check, as its runs cost money.

How to build on each system:

- **Mac:** [BUILD-macOS.md](BUILD-macOS.md), sections 6 (release build), 7 (notarise) and 9 (installing).
- **Windows:** [BUILD-Windows.md](BUILD-Windows.md#release-build).

## 1. Set the version

The version is in three places, kept the same:

- `package.json`: `"version"`
- `src-tauri/tauri.conf.json`: `"version"` (this one names the installers)
- `src-tauri/Cargo.toml`: `version` under `[package]`

Run the checks ([BUILD-macOS.md](BUILD-macOS.md), section 5), then commit and push to `main`.

## 2. Build on both systems

On each one, pull `main`, make the release build, and copy the installers into `releases/<version>/` at the top of
the repository. Git ignores `releases/`, so this folder never syncs between the two systems. Bring each system's
installers into one folder before you publish, or upload them from each system (step 3).

| System | Installer | Built at |
|---|---|---|
| Mac | `SuperXTerminal_<version>_aarch64.dmg` | `src-tauri/target/release/bundle/dmg/` |
| Windows | `SuperXTerminal_<version>_x64_en-US.msi` | `src-tauri\target\release\bundle\msi\` |
| Windows | `SuperXTerminal_<version>_x64-setup.exe` | `src-tauri\target\release\bundle\nsis\` |

On the Mac, for example:

```bash
mkdir -p releases/0.1.0 && cp src-tauri/target/release/bundle/dmg/*.dmg releases/0.1.0/
```

On Windows, in PowerShell:

```powershell
mkdir releases\0.1.0 -Force; cp src-tauri\target\release\bundle\msi\*.msi, src-tauri\target\release\bundle\nsis\*-setup.exe releases\0.1.0\
```

After a Windows build, run `git checkout THIRD-PARTY-NOTICES.txt`. The repository keeps the Mac's copy
([BUILD-Windows.md](BUILD-Windows.md#release-build)).

## 3. Tag and publish

Tag the commit that was built, push the tag, and make a GitHub release with the installers attached:

```bash
git tag v0.1.0 && git push origin v0.1.0
```

```bash
gh release create v0.1.0 releases/0.1.0/* --title "SuperXTerminal 0.1.0" --notes "..."
```

To add the other system's installers to the release afterwards, from that system:

```bash
gh release upload v0.1.0 releases/0.1.0/*
```

Neither command runs a GitHub Actions workflow. The app's update check (Settings → General → Updates) offers the
repository's latest release, so publish only builds that are ready for people to install.

## Signing

- **Mac:** builds are signed with an Apple Development certificate. Other Macs warn about them until there's a
  Developer ID and notarisation ([BUILD-macOS.md](BUILD-macOS.md), sections 2 and 7).
- **Windows:** builds aren't signed yet, so SmartScreen warns on the first run ([BUILD-Windows.md](BUILD-Windows.md#release-build)).
