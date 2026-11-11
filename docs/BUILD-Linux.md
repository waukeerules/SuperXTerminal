# Building SuperXTerminal for Linux

How to make the Linux packages (`.deb`, `.rpm` and AppImage) in Docker, on the Mac or the Windows PC, and try them on
a Linux machine. A release builds them on the Windows PC, with Windows' installers (`release.bat`,
[RELEASE.md](RELEASE.md)); this is for building them on their own, to test.

The Mac build is in [BUILD-macOS.md](BUILD-macOS.md), the Windows one in [BUILD-Windows.md](BUILD-Windows.md).

## Why Docker

Tauri can't make Linux packages on macOS or Windows: the app links against Linux's own libraries (WebKitGTK, GTK,
libsecret, ALSA) and is bundled with Linux tools (`dpkg`, `rpmbuild`, linuxdeploy). So the build runs in an Ubuntu
22.04 container, and the packages are copied out to install on a real Linux machine.

Everything the container needs is written down in [`scripts/linux-build.Dockerfile`](../scripts/linux-build.Dockerfile),
and [`scripts/build-linux.mjs`](../scripts/build-linux.mjs) makes the image when it's missing. It's Node, not a
shell script, so the same command works on the Mac and Windows. Deleting the image, the
volumes or Docker itself loses nothing but time: the next build makes them again.

## What you'll need

- Docker Desktop (`open -a Docker` on the Mac) or OrbStack, running. On Windows, Docker Desktop in its usual Linux
  containers mode; amd64 builds run natively there (about 6 minutes for Rust), quicker than on Apple silicon.
- Enough memory for Docker. Docker Desktop's 2 GB default on Windows isn't: the compiles are killed ("signal: 9,
  SIGKILL"). Give it 16 GB (Settings > Resources > Advanced, then Apply & restart) and check with
  `docker info --format "{{.MemTotal}}"`. On a smaller machine, set `CARGO_BUILD_JOBS` (e.g. `4`) before the build,
  and the container uses it.
- On Apple silicon, for x86_64 packages: Docker Desktop's Settings > General > "Use Rosetta for x86_64/amd64
  emulation on Apple Silicon" turned on. It works without, but slower.
- The machine to try it on: check its CPU with `uname -m`. `x86_64` is the usual PC (the default build, amd64);
  `aarch64` is ARM (build with `--arm64`).

## Build

```bash
npm run build:linux
```

The first time it makes the image (a few minutes), then installs the npm packages and compiles everything: 10-20
minutes for amd64 on Apple silicon, as it's emulated, and less on an x86 Windows PC, where it isn't. Later builds only
compile what changed.

The packages land in `releases/linux/<version>/`:

```
SuperXTerminal_0.7.0_amd64.deb
SuperXTerminal-0.7.0-1.x86_64.rpm
SuperXTerminal_0.7.0_amd64.AppImage
```

They're kept apart from `releases/<version>/`, as `npm run release:publish` uploads everything in that folder. A
release puts them there instead (`--into releases/<version>`).

Other ways to run it:

```bash
npm run build:linux -- --arm64
```

```bash
npm run build:linux -- -- --bundles deb
```

```bash
npm run build:linux -- --shell
```

```bash
npm run build:linux -- --new-image
```

`--arm64` makes ARM packages (full speed on Apple silicon). The second passes `--bundles deb` on to `tauri build`,
for just the `.deb` (the first `--` is npm's, the second the script's). `--shell` opens a shell in the same container,
with the same volumes, to look around or run `npm run tauri build` by hand. `--new-image` remakes the image, picking
up Ubuntu's updates and the latest Node LTS and Rust.

## Install and try it

The packages are built on Ubuntu 22.04 and use the system's own C library (glibc 2.35 or later), as every Tauri app
does: they run on Ubuntu 22.04, Debian 12, Fedora 36, RHEL 10 and later, not on older ones. The AppImage too: it
carries the app's other libraries, but not glibc. (The AppImage catalog's test says so as "not self-contained".)

Copy the package to the Linux machine, then on Ubuntu or Debian:

```bash
scp releases/linux/0.7.0/SuperXTerminal_0.7.0_amd64.deb you@ubuntu-host:
```

```bash
sudo apt install ./SuperXTerminal_0.7.0_amd64.deb
```

apt says it's "selecting 'super-x-terminal'": that's the package's name (Tauri's spelling of SuperXTerminal), the
one to use to remove it. A notice that the download "is performed unsandboxed as root" is harmless: apt's own user
can't read your home folder, so it read the file as root.

Run it from the applications menu (SuperXTerminal), or from a terminal, where anything that goes wrong at start is
printed:

```bash
superxterminal
```

To install a newer build, `apt install` its `.deb` the same way; it replaces the old one. To remove it:

```bash
sudo apt remove super-x-terminal
```

On Fedora or RHEL, install the `.rpm` instead:

```bash
sudo dnf install ./SuperXTerminal-0.7.0-1.x86_64.rpm
```

The AppImage needs no install, just permission to run. Tauri names it `SuperXTerminal_<version>_amd64.AppImage`;
in a release it's `SuperXTerminal-<version>-x86_64.AppImage`, as AppImages are named (`scripts/release-names.mjs`):

```bash
chmod +x SuperXTerminal_0.7.0_amd64.AppImage && ./SuperXTerminal_0.7.0_amd64.AppImage
```

Things that differ from the Mac and Windows, worth checking first:

- **Saved passwords** go to the Secret Service (GNOME Keyring or KWallet). A desktop session has one; over plain SSH
  with no desktop there's none, and saving a password fails.
- **The terminal** runs in WebKitGTK, which is slower than the Mac's WebKit. If xterm.js can't use WebGL there it
  falls back to drawing with the DOM.
- **Serial ports** are `/dev/ttyUSB*` and `/dev/ttyACM*`, and need your user in the `dialout` group.
- **Remote Desktop's sound** goes through ALSA (or PulseAudio and PipeWire's ALSA layer).

## When something goes wrong

- **"The system library `xyz` required by crate `xyz-sys` was not found"**: a new dependency needs a Linux library.
  Add its `-dev` package to the `apt-get install` in `scripts/linux-build.Dockerfile` (and to the Linux step of
  `.github/workflows/build.yml`), then `npm run build:linux -- --new-image`.
- **`failed to run linuxdeploy`** (the AppImage): the `.deb` and `.rpm` are made before it and still copied out.
  The image already deals with the two known causes: no FUSE in a container (`APPIMAGE_EXTRACT_AND_RUN=1`), and
  emulated x86 refusing AppImages' header marker (a patched copy of linuxdeploy's AppImage plugin; the Dockerfile
  says more). If it comes back, an image made before those fixes may be in use (`--new-image`). Otherwise, build
  without it (`npm run build:linux -- -- --bundles deb,rpm`) and run `npx tauri build --verbose --bundles appimage`
  in `--shell` to see what linuxdeploy says.
- **Starting from scratch**: delete the volumes, and the image if you like; the next build makes them again.

  ```bash
  docker volume rm sxt-linux-node-modules-amd64 sxt-linux-target-amd64 sxt-linux-cargo-registry-amd64
  ```

  ```bash
  docker image rm superxterminal-linux-build:amd64
  ```

## What's where

| | Where | What it's for |
| --- | --- | --- |
| Image | `superxterminal-linux-build:amd64` (or `:arm64`) | Ubuntu 22.04, the Linux libraries, Node, Rust |
| Volume | `sxt-linux-node-modules-<arch>` | Linux's `node_modules`, kept away from the Mac's |
| Volume | `sxt-linux-target-<arch>` | `src-tauri/target`: the compiled crates, so the next build is quick |
| Volume | `sxt-linux-cargo-registry-<arch>` | Cargo's downloaded crates |
| Folder | `releases/linux/<version>/` | The packages, on the Mac (not in git) |
| File | `src-tauri/linux/com.waukeerules.superxterminal.metainfo.xml` | The AppStream metainfo: the summary, description and screenshots that GNOME Software, KDE Discover and the AppImage catalog show. Every package installs it in `/usr/share/metainfo/` (`tauri.conf.json`, `bundle > linux`); appimagetool checks it with `appstreamcli` as it makes the AppImage |

The metainfo can be checked on its own, without a build, in a container like the image's:

```bash
docker run --rm -v "$PWD/src-tauri/linux:/m:ro" ubuntu:22.04 bash -c 'apt-get update -qq && apt-get install -y -qq appstream >/dev/null && appstreamcli validate --no-net /m/com.waukeerules.superxterminal.metainfo.xml'
```

The image installs `appstream` for that check. An image made before it was added skips the check; make it again once
with `npm run build:linux -- --new-image`.
