# The Linux build machine for SuperXTerminal: Ubuntu 22.04 with everything `npm run tauri build` needs, so the .deb,
# .rpm and AppImage can be made on the Mac or the Windows PC. scripts/build-linux.mjs builds this image the
# first time it's needed and runs the build in it; docs/BUILD-Linux.md has the steps.
#
# The image holds only tools and libraries. The source isn't copied in: the script mounts the repository at /src, with
# node_modules, src-tauri/target and Cargo's download cache on Docker volumes of their own. Those keep the Mac's
# macOS-only node_modules out of the container, and let a second build pick up where the first stopped.
#
# Ubuntu 22.04 rather than a newer one: a program built against an older glibc runs on newer systems, not the other
# way round, so the packages install on 22.04 and everything after it.
FROM ubuntu:22.04

# tzdata, pulled in by the libraries, otherwise stops to ask for a time zone
ENV DEBIAN_FRONTEND=noninteractive

# Tauri's own needs (WebKitGTK, the tray icon, SVG icons), then SuperXTerminal's:
# - libsecret and D-Bus: saved passwords go to the Secret Service (GNOME Keyring, KWallet)
# - ALSA and Opus: Remote Desktop's sound (ironrdp-rdpsnd-native, through cpal and opus2)
# - cmake: libopus_sys builds Opus itself when pkg-config can't find it
# - rpm: the .rpm bundle; file, patchelf: the AppImage bundle
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git build-essential pkg-config file xdg-utils patchelf rpm cmake \
      libssl-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev \
      libsecret-1-dev libdbus-1-dev libasound2-dev libopus-dev \
    && rm -rf /var/lib/apt/lists/*

# Node's current LTS, as .nvmrc asks for (lts/*)
RUN curl -fsSL https://deb.nodesource.com/setup_lts.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# Rust, installed outside /root so the cache volume mounted at /usr/local/cargo/registry doesn't hide the toolchain.
# rustfmt and clippy because rust-toolchain.toml asks for them; without them rustup fetches them on every new container.
ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable \
      --component rustfmt --component clippy

# linuxdeploy, which Tauri runs to make the AppImage, is itself an AppImage, and those need FUSE, which a container
# hasn't got. This makes it unpack itself and run instead.
ENV APPIMAGE_EXTRACT_AND_RUN=1

# linuxdeploy's AppImage plugin, put where Tauri looks for it, so Tauri uses this copy rather than downloading its own.
# An AppImage marks itself with "AI" and a version in its ELF header's padding (bytes 8-10), and Docker's x86 emulation
# on Apple silicon (Rosetta, QEMU) refuses to run a file with them ("Exec format error"), so the AppImage bundle
# failed with "failed to run linuxdeploy". Zeroing them changes nothing else. Tauri does the same to linuxdeploy
# itself, but not to the plugin. Harmless on a real x86 or ARM machine.
RUN arch=$(dpkg --print-architecture | sed 's/amd64/x86_64/; s/arm64/aarch64/') \
    && mkdir -p /root/.cache/tauri \
    && plugin=/root/.cache/tauri/linuxdeploy-plugin-appimage.AppImage \
    && curl -fsSLo "$plugin" "https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/continuous/linuxdeploy-plugin-appimage-$arch.AppImage" \
    && printf '\0\0\0' | dd of="$plugin" bs=1 seek=8 count=3 conv=notrunc \
    && chmod +x "$plugin"

WORKDIR /src
