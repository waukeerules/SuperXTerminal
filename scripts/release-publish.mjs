// Puts the installers in releases/<version>/ into the version's GitHub release: `npm run release:publish`. The first
// computer to run it tags the commit (v<version>, pushed) and makes the release as a draft; the other adds its
// installers to it (one already there is replaced). `npm run release:publish -- --final` then makes the draft public,
// which is when the app's update check offers it, so it refuses until the Mac's .dmg, Windows' .msi and -setup.exe,
// and the Linux .deb, .rpm and AppImage (built in Docker on the Windows PC, scripts/build-linux.mjs) are all in.
//
// Before uploading, the installers in releases/<version>/ are renamed from Tauri's names to ones that say their system
// (SuperXTerminal-0.8.0-macOS-AppleSilicon.dmg, -Windows-x64.msi, -Linux-x64.deb...): scripts/release-names.mjs.
//
// The notes are releases/<version>/NOTES.md when it's there (what's new, the download table, how to get past
// Gatekeeper and SmartScreen), put in whenever the script runs so they can be written or changed while it's a draft;
// without it, GitHub's generated notes, which for commits straight to main are little more than a changelog link.
//
// Tagging checks first that what's tagged is what's on GitHub: no uncommitted changes to tracked files, and the commit
// the same as origin/main. Whether the release exists is read from `gh release list`, which includes drafts, so an
// older GitHub CLI that can't look a draft up by its tag stops with its own error rather than making a second release;
// keep the CLI up to date (docs/BUILD-Windows.md). Needs that CLI, signed in (gh auth login). docs/RELEASE.md.
import { existsSync, readdirSync, readFileSync, renameSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { expectedNames, releaseName } from "./release-names.mjs";

/** The repository's folder (fileURLToPath, not the URL's pathname: on Windows that's "/C:/...") */
const root = join(dirname(fileURLToPath(import.meta.url)), "..");

/** The notes' file name in releases/<version>/; it's never uploaded as an installer */
const NOTES = "NOTES.md";

/**
 * Run a command in the repository, its output shown; stops the script if it fails
 * @param {string[]} words the program and its arguments (passed as they are, no shell)
 */
function run(words) {
  const result = spawnSync(words[0], words.slice(1), { cwd: root, stdio: "inherit" });
  if (result.status !== 0) {
    console.error(`\n${words.slice(0, 3).join(" ")} failed${result.error ? `: ${result.error.message}` : ""}.`);
    process.exit(result.status ?? 1);
  }
}

/** Whether a command succeeds, its output kept quiet */
function succeeds(words) {
  return spawnSync(words[0], words.slice(1), { cwd: root, stdio: "ignore" }).status === 0;
}

/** A command's output, trimmed; "" when it fails */
function output(words) {
  const result = spawnSync(words[0], words.slice(1), { cwd: root, encoding: "utf8" });
  return result.status === 0 ? result.stdout.trim() : "";
}

/** Stop with a message */
function fail(message) {
  console.error(message);
  process.exit(1);
}

/** Whether the version's release exists, a draft or not: from the list, which has drafts in every version of gh */
function releaseExists(tag) {
  const list = output(["gh", "release", "list", "--limit", "100", "--json", "tagName"]);
  return (list ? JSON.parse(list) : []).some((r) => r.tagName === tag);
}

/**
 * Rename the installers in the version's folder to their release names (scripts/release-names.mjs), saying so
 * @param {string} folder releases/<version>
 * @param {string} version the version
 * @returns {string[]} the folder's file names afterwards
 */
function renameForRelease(folder, version) {
  for (const file of readdirSync(folder)) {
    const name = releaseName(file, version);
    if (name && name !== file) {
      renameSync(join(folder, file), join(folder, name));
      console.log(`${file} is ${name} in the release.`);
    }
  }
  return readdirSync(folder);
}

/** Stop unless this checkout is what GitHub has: tracked files as committed, and the commit origin/main's */
function checkPushed() {
  const changed = output(["git", "status", "--porcelain", "--untracked-files=no"]);
  if (changed) fail(`These files have changes that aren't committed, so the tag wouldn't match what was built:\n${changed}\nCommit and push them (or put them back), then run this again.`);
  run(["git", "fetch", "--quiet", "origin", "main"]);
  const head = output(["git", "rev-parse", "HEAD"]);
  const main = output(["git", "rev-parse", "origin/main"]);
  if (head !== main) fail("This commit isn't origin/main: git push (or git pull), then run this again, so the tag is on the code everyone has.");
}

function main() {
  const version = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8")).version;
  const tag = `v${version}`;
  const folder = join(root, "releases", version);
  const names = existsSync(folder) ? renameForRelease(folder, version) : [];
  const files = names.filter((f) => f !== NOTES && !f.startsWith(".")).map((f) => join(folder, f));
  const notes = names.includes(NOTES) ? join(folder, NOTES) : null;
  const final = process.argv.includes("--final");
  if (files.length === 0 && !final) fail(`releases/${version}/ has no installers: run npm run release first.`);
  if (!succeeds(["gh", "auth", "status"])) {
    fail("GitHub's CLI isn't installed or signed in: install it (brew install gh, or winget install GitHub.cli), then gh auth login.");
  }

  if (releaseExists(tag)) {
    if (files.length) {
      console.log(`Adding to the ${tag} release: ${files.map((f) => f.split(/[\\/]/).pop()).join(", ")}`);
      run(["gh", "release", "upload", tag, ...files, "--clobber"]);
    }
    if (notes) run(["gh", "release", "edit", tag, "--notes-file", notes]);
  } else {
    if (final) fail(`There's no ${tag} release yet: run npm run release:publish first.`);
    const hasTag = () => succeeds(["git", "rev-parse", "--verify", "--quiet", `refs/tags/${tag}`]);
    if (!hasTag()) run(["git", "fetch", "--tags", "--quiet"]);
    if (hasTag()) {
      // the other computer tagged it: it should be the commit built here
      if (output(["git", "rev-list", "-n", "1", tag]) !== output(["git", "rev-parse", "HEAD"])) {
        console.warn(`Note: ${tag} is on another commit than this one. The release is that commit's.`);
      }
    } else {
      checkPushed();
      run(["git", "tag", tag]);
    }
    run(["git", "push", "origin", `refs/tags/${tag}`]);
    console.log(`Making the ${tag} release, as a draft, with: ${files.map((f) => f.split(/[\\/]/).pop()).join(", ")}`);
    if (!notes) console.log(`(No releases/${version}/${NOTES}: GitHub's generated notes for now. Write it and run this again to put it in.)`);
    run(["gh", "release", "create", tag, ...files, "--draft", "--title", `SuperXTerminal ${version}`,
      ...(notes ? ["--notes-file", notes] : ["--generate-notes"])]);
  }

  if (final) {
    const assets = output(["gh", "release", "view", tag, "--json", "assets", "--jq", ".assets[].name"]).split(/\r?\n/);
    const missing = expectedNames(version).filter((name) => !assets.includes(name));
    if (missing.length) fail(`\n${tag} stays a draft: it hasn't got ${missing.join(", ")} yet. Run npm run release:publish on the computer that has them.`);
    run(["gh", "release", "edit", tag, "--draft=false"]);
    console.log(`\n${tag} is public: the app's update check offers it now.`);
  } else {
    console.log(`\n${tag} is a draft. Once both computers' installers are in: npm run release:publish -- --final`);
  }
}

main();
