// Builds this computer's release and collects its installers into releases/<version>/: `npm run release`. On the
// Mac the .dmg, signed with the Keychain's signing certificate (a Developer ID Application one if there is one, else
// Apple Development; APPLE_SIGNING_IDENTITY, when set, wins); on Windows the MSI and the setup .exe. The
// version is src-tauri/tauri.conf.json's (npm run release:version sets it). Anything after `--` goes to `tauri build`,
// e.g. `npm run release -- --target universal-apple-darwin`. docs/BUILD-macOS.md and docs/BUILD-Windows.md have the
// steps around it.
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync } from "node:fs";
import { join, dirname, basename } from "node:path";
import { fileURLToPath } from "node:url";
import { execSync, spawnSync } from "node:child_process";

/** The repository's folder (fileURLToPath, not the URL's pathname: on Windows that's "/C:/...") */
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const mac = process.platform === "darwin";
const windows = process.platform === "win32";

/** The version the installers are named for */
function version() {
  const conf = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"));
  const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
  if (conf.version !== pkg.version) {
    console.error(`tauri.conf.json says ${conf.version} and package.json ${pkg.version}: set one with npm run release:version -- <version>.`);
    process.exit(1);
  }
  return conf.version;
}

/**
 * The Mac's signing certificate's name, from the Keychain: a Developer ID Application one (for releases others
 * install) before an Apple Development one; null for none, when the build is signed ad hoc
 */
function signingIdentity() {
  let out = "";
  try {
    out = execSync("security find-identity -v -p codesigning", { encoding: "utf8" });
  } catch {
    return null;
  }
  const names = [...out.matchAll(/"([^"]+)"/g)].map((m) => m[1]);
  return names.find((n) => n.startsWith("Developer ID Application:")) ?? names.find((n) => n.startsWith("Apple Development:")) ?? null;
}

/**
 * The installers a build left for `v`, in the bundle folder (target/<target>/release when a --target was given)
 * @param {string} v the version
 * @param {string[]} args what was given to tauri build
 */
function installers(v, args) {
  const at = args.indexOf("--target");
  const release = at >= 0 && args[at + 1] ? join("src-tauri/target", args[at + 1], "release") : "src-tauri/target/release";
  const bundle = join(root, release, "bundle");
  /** The files in one of the bundle's folders whose names have the version and end with `ending` */
  const of = (folder, ending) => {
    const dir = join(bundle, folder);
    return existsSync(dir) ? readdirSync(dir).filter((f) => f.includes(`_${v}_`) && f.endsWith(ending)).map((f) => join(dir, f)) : [];
  };
  return mac ? of("dmg", ".dmg") : windows ? [...of("msi", ".msi"), ...of("nsis", "-setup.exe")] : [];
}

/** Build, collect, and say what's next */
function main() {
  const v = version();
  const args = process.argv.slice(2);
  const env = { ...process.env };
  if (mac && !env.APPLE_SIGNING_IDENTITY) {
    const identity = signingIdentity();
    if (identity) env.APPLE_SIGNING_IDENTITY = identity;
  }
  console.log(`Building SuperXTerminal ${v} for ${mac ? "the Mac" : windows ? "Windows" : process.platform}${env.APPLE_SIGNING_IDENTITY ? `, signed as ${env.APPLE_SIGNING_IDENTITY}` : mac ? ", signed ad hoc (no certificate in the Keychain)" : ""}.\n`);

  // npm through the shell: on Windows it's npm.cmd
  const build = spawnSync(`npm run tauri build${args.length ? ` -- ${args.join(" ")}` : ""}`, { cwd: root, env, stdio: "inherit", shell: true });
  if (build.status !== 0) {
    console.error(`\nThe build failed (exit code ${build.status}): nothing was collected.`);
    process.exit(build.status ?? 1);
  }

  const found = installers(v, args);
  if (found.length === 0) {
    console.error(`\nThe build finished, but no installer named for ${v} was found in the bundle folder.`);
    process.exit(1);
  }
  const into = join(root, "releases", v);
  mkdirSync(into, { recursive: true });
  for (const file of found) copyFileSync(file, join(into, basename(file)));
  console.log(`\nSuperXTerminal ${v}'s installers, in ${join("releases", v)}:\n  ${found.map((f) => basename(f)).join("\n  ")}`);
  console.log(`\nNext: npm run release:publish, to put them in the GitHub release. Release notes go in ${join("releases", v, "NOTES.md")} (docs/RELEASE.md).`);
}

main();
