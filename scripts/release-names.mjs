// The names the installers have in the GitHub release, saying which system each is for:
// SuperXTerminal-0.8.0-macOS-AppleSilicon.dmg rather than Tauri's SuperXTerminal_0.8.0_aarch64.dmg, which only
// names the processor, and that differently on each system (aarch64, x64, amd64, x86_64). Tauri's own names stay in
// its bundle folders and in releases/<version>/; scripts/release-publish.mjs renames the files there to these just
// before uploading, so the Mac, Windows and the Linux build in Docker all end up named the same way.
// Tested by scripts/release-names.test.ts.

/** Tauri's processor words, as each system's packages spell them, to the one word the release names use */
const ARCH = {
  aarch64: "arm64",
  arm64: "arm64",
  x64: "x64",
  amd64: "x64",
  x86_64: "x64",
  universal: "Universal",
};

/** How a Mac's processor is put to the people downloading: Apple silicon and Intel, as Apple's own pages say */
const MAC_ARCH = { arm64: "AppleSilicon", x64: "Intel", Universal: "Universal" };

/**
 * The release's name for one of Tauri's installers, or null when it isn't one Tauri makes for this version
 * @param {string} file the installer's file name, as Tauri left it (SuperXTerminal_0.8.0_aarch64.dmg)
 * @param {string} version the version, which every installer's name has (0.8.0)
 * @returns {string | null} the name it's uploaded as (SuperXTerminal-0.8.0-macOS-AppleSilicon.dmg); a name that's
 *   already a release name is returned as it is, so renaming twice does nothing
 */
export function releaseName(file, version) {
  const v = version.replace(/\./g, "\\.");
  const patterns = [
    // SuperXTerminal_0.8.0_aarch64.dmg
    [new RegExp(`^(.+)_${v}_(\\w+)\\.dmg$`), (app, arch) => `${app}-${version}-macOS-${MAC_ARCH[ARCH[arch]] ?? arch}.dmg`],
    // SuperXTerminal_0.8.0_x64_en-US.msi: the language is left out, as there's only English
    [new RegExp(`^(.+)_${v}_(\\w+)_[\\w-]+\\.msi$`), (app, arch) => `${app}-${version}-Windows-${ARCH[arch] ?? arch}.msi`],
    // SuperXTerminal_0.8.0_x64-setup.exe
    [new RegExp(`^(.+)_${v}_(\\w+)-setup\\.exe$`), (app, arch) => `${app}-${version}-Windows-${ARCH[arch] ?? arch}-setup.exe`],
    // SuperXTerminal_0.8.0_amd64.deb
    [new RegExp(`^(.+)_${v}_(\\w+)\\.deb$`), (app, arch) => `${app}-${version}-Linux-${ARCH[arch] ?? arch}.deb`],
    // SuperXTerminal-0.8.0-1.x86_64.rpm: the 1 is the package's release number, always 1 here
    [new RegExp(`^(.+)-${v}-\\d+\\.(\\w+)\\.rpm$`), (app, arch) => `${app}-${version}-Linux-${ARCH[arch] ?? arch}.rpm`],
    // SuperXTerminal_0.8.0_amd64.AppImage
    [new RegExp(`^(.+)_${v}_(\\w+)\\.AppImage$`), (app, arch) => `${app}-${version}-Linux-${ARCH[arch] ?? arch}.AppImage`],
  ];
  if (new RegExp(`^.+-${v}-(macOS|Windows|Linux)-`).test(file)) return file;
  for (const [pattern, name] of patterns) {
    const m = file.match(pattern);
    if (m) return name(m[1], m[2]);
  }
  return null;
}

/**
 * The installers that must all be in the release before it's public: the Mac's, Windows' two and Linux's three
 * @param {string} version the version (0.8.0)
 * @returns {string[]} their release names
 */
export function expectedNames(version) {
  return [
    `SuperXTerminal-${version}-macOS-AppleSilicon.dmg`,
    `SuperXTerminal-${version}-Windows-x64.msi`,
    `SuperXTerminal-${version}-Windows-x64-setup.exe`,
    `SuperXTerminal-${version}-Linux-x64.deb`,
    `SuperXTerminal-${version}-Linux-x64.rpm`,
    `SuperXTerminal-${version}-Linux-x64.AppImage`,
  ];
}
