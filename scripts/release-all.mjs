// A whole release on this computer, in one command: `./release.sh --release 0.8.0` on the Mac,
// `release.bat --release 0.8.0` on the Windows PC (both just run this). It runs the steps docs/RELEASE.md lists, in
// order, and stops at the first that fails; running it again carries on, as each step is safe to repeat:
//
//   1. git pull, so this computer has the other's commits (the version, if the other set it)
//   2. the version: set and pushed (npm run release:version) when it isn't already the one asked for
//   3. npm run release:checks (skipped with --skip-checks)
//   4. npm run release: this computer's installers, into releases/<version>/
//   5. on Windows, the Linux packages, built in Docker (scripts/build-linux.mjs), into the same folder
//   6. npm run release:publish: the first computer tags the commit and makes the GitHub release as a draft; the
//      second adds its installers to it
//   7. with --final, npm run release:publish -- --final: the release made public, once everything is in
//
// Either computer can go first; the Mac usually does. The notes, releases/<version>/NOTES.md, are still written by
// hand (docs/RELEASE.md); running `npm run release:publish` again after writing them puts them in.
//
// Options: --release <version> (required), --skip-checks, --linux / --no-linux (build the Linux packages here or not;
// the default is yes on Windows, no on the Mac), --final.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

/** The repository's folder (fileURLToPath, not the URL's pathname: on Windows that's "/C:/...") */
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const windows = process.platform === "win32";
const mac = process.platform === "darwin";

/** Stop with a message */
function fail(message, code = 1) {
  console.error(`\n${message}`);
  process.exit(code);
}

/**
 * The options, from the command line
 * @returns {{ version: string, checks: boolean, linux: boolean, final: boolean }}
 */
function options(args) {
  const o = { version: "", checks: true, linux: windows, final: false };
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a === "--release") o.version = args[++i] ?? "";
    else if (a === "--skip-checks") o.checks = false;
    else if (a === "--linux") o.linux = true;
    else if (a === "--no-linux") o.linux = false;
    else if (a === "--final") o.final = true;
    else fail(`Unknown option ${a}. The options are at the top of scripts/release-all.mjs.`, 2);
  }
  if (!/^\d+\.\d+\.\d+$/.test(o.version)) fail("Give the version as three numbers: --release 0.8.0", 2);
  return o;
}

/**
 * A step: say what it is, run it with its output shown, and stop the release if it fails
 * @param {string} title what the step is, as the heading printed before it
 * @param {string} command run through the shell (npm is npm.cmd on Windows)
 */
function step(title, command) {
  console.log(`\n=== ${title}\n> ${command}\n`);
  const result = spawnSync(command, { cwd: root, stdio: "inherit", shell: true });
  if (result.status !== 0) fail(`${title}: failed (exit code ${result.status}). Fix it and run the release again; the steps before it are safe to repeat.`);
}

/** Whether a command succeeds, its output kept quiet */
function succeeds(command) {
  return spawnSync(command, { cwd: root, stdio: "ignore", shell: true }).status === 0;
}

/** A command's output, trimmed; "" when it fails */
function output(command) {
  const result = spawnSync(command, { cwd: root, encoding: "utf8", shell: true });
  return result.status === 0 ? result.stdout.trim() : "";
}

/** The version the repository has now (tauri.conf.json's, which names the installers) */
function currentVersion() {
  return JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8")).version;
}

function main() {
  const o = options(process.argv.slice(2));
  const where = mac ? "the Mac" : windows ? "Windows" : process.platform;
  console.log(`SuperXTerminal ${o.version} on ${where}: ${mac ? "the .dmg" : windows ? "the MSI and setup .exe" : "this system's installers"}${o.linux ? ", and the Linux packages" : ""}.`);

  // What would otherwise stop the release halfway, checked before anything is built
  if (output("git status --porcelain --untracked-files=no")) {
    fail("Some tracked files have changes that aren't committed. Commit and push them (or put them back) first: the release is tagged on what's on GitHub.");
  }
  if (!succeeds("gh auth status")) fail("GitHub's CLI isn't installed or signed in: install it (brew install gh, or winget install GitHub.cli), then gh auth login.");
  if (o.linux && !succeeds("docker info")) fail("Docker isn't running, and the Linux packages are built in it. Start Docker Desktop, or leave Linux out with --no-linux.");

  step("Getting the latest code", "git pull --ff-only");

  if (currentVersion() !== o.version) {
    step(`Setting the version to ${o.version}`, `npm run release:version -- ${o.version}`);
    step("Pushing the version", "git push");
  } else {
    console.log(`\n=== The version is already ${o.version}`);
  }

  if (o.checks) step("Checks", "npm run release:checks");
  else console.log("\n=== Checks skipped (--skip-checks)");

  if (mac) console.log("\nQuit any running SuperXTerminal: the build replaces the app.");
  step(`Building ${where}'s installers`, "npm run release");

  if (o.linux) step("Building the Linux packages in Docker", `node scripts/build-linux.mjs --into releases/${o.version}`);

  step("Putting them in the GitHub release", "npm run release:publish");
  if (o.final) step("Making the release public", "npm run release:publish -- --final");

  const notes = existsSync(join(root, "releases", o.version, "NOTES.md"));
  console.log(`\nDone on ${where}.`);
  if (!notes) console.log(`No releases/${o.version}/NOTES.md here: write it (docs/RELEASE.md), then npm run release:publish to put it in.`);
  if (!o.final) console.log("Once every installer is in: npm run release:publish -- --final (or run this again with --final).");
}

main();
