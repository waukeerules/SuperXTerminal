// Sets the app's version everywhere it's written, so a release is one command: `npm run release:version -- 0.3.0`.
// The five places are package.json, package-lock.json (twice: the lock's own and its root package's),
// src-tauri/tauri.conf.json (which names the installers), src-tauri/Cargo.toml and the app's entry in
// src-tauri/Cargo.lock. Each is changed in its text, so the rest of the file stays exactly as it was, and the five are
// committed ("Version 0.3.0"), nothing else. Run it on one computer and push; the other pulls (docs/RELEASE.md).
import { readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

/** The repository's folder (fileURLToPath, not the URL's pathname: on Windows that's "/C:/...") */
const root = join(dirname(fileURLToPath(import.meta.url)), "..");

/**
 * One place the version is written: the file, and the text around the version there
 * @property {string} file relative to the repository
 * @property {RegExp} pattern the version's place, as group 2 between groups 1 and 3
 */
const PLACES = [
  { file: "package.json", pattern: /^(  "version": ")([^"]+)(")/m },
  // the lock's own version, then its root package's ("packages": { "": { ... "version" } })
  { file: "package-lock.json", pattern: /^(  "version": ")([^"]+)(")/m },
  { file: "package-lock.json", pattern: /("packages": \{\s*"": \{[^}]*?"version": ")([^"]+)(")/ },
  { file: "src-tauri/tauri.conf.json", pattern: /^(  "version": ")([^"]+)(")/m },
  // the first version in Cargo.toml is [package]'s; dependencies' are on their own lines further down. \r?: a
  // Windows copy of the repository has Windows line endings
  { file: "src-tauri/Cargo.toml", pattern: /(\[package\][^[]*?\r?\nversion = ")([^"]+)(")/ },
  { file: "src-tauri/Cargo.lock", pattern: /(\r?\nname = "superxterminal"\r?\nversion = ")([^"]+)(")/ },
];

/** Set every place to `version`, or say what's wrong and change nothing */
function main() {
  const version = process.argv[2];
  if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
    console.error("Give the new version as three numbers: npm run release:version -- 0.3.0");
    process.exit(1);
  }
  // read everything first, so a file that doesn't match changes nothing
  const texts = new Map();
  const found = [];
  for (const place of PLACES) {
    const text = texts.get(place.file) ?? readFileSync(join(root, place.file), "utf8");
    const match = place.pattern.exec(text);
    if (!match) {
      console.error(`Couldn't find the version in ${place.file}: nothing changed.`);
      process.exit(1);
    }
    found.push(`${place.file}: ${match[2]}`);
    texts.set(place.file, text.replace(place.pattern, `$1${version}$3`));
  }
  if (found.every((f) => f.endsWith(`: ${version}`))) {
    console.log(`The version is already ${version} everywhere: nothing to change.`);
    return;
  }
  for (const [file, text] of texts) writeFileSync(join(root, file), text);
  console.log(`Version ${version} in:\n  ${found.map((f) => `${f} -> ${version}`).join("\n  ")}`);
  // committed alone: on Windows `git commit -a` would take every file git shows changed for its line endings
  const files = [...texts.keys()];
  const commit = spawnSync("git", ["commit", "-m", `Version ${version}`, "--", ...files], { cwd: root, stdio: "inherit" });
  if (commit.status !== 0) {
    console.error("\nThe files are changed, but the commit didn't happen: commit them yourself.");
    process.exit(1);
  }
  console.log(`\nCommitted as "Version ${version}". Next: git push, then npm run release on each computer.`);
}

main();
