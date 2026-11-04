// Puts the installers in releases/<version>/ into the version's GitHub release: `npm run release:publish`. The first
// computer to run it tags the commit (v<version>, pushed) and makes the release as a draft, with GitHub's notes from
// the commits since the last tag; the other adds its installers to it (one already there is replaced).
// `npm run release:publish -- --final` then makes the draft public, which is when the app's update check offers it, so
// it's done once both computers' installers are in. Needs GitHub's CLI, signed in (gh auth login). docs/RELEASE.md.
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

/** The repository's folder (fileURLToPath, not the URL's pathname: on Windows that's "/C:/...") */
const root = join(dirname(fileURLToPath(import.meta.url)), "..");

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

function main() {
  const version = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8")).version;
  const tag = `v${version}`;
  const folder = join(root, "releases", version);
  const files = existsSync(folder) ? readdirSync(folder).map((f) => join(folder, f)) : [];
  const final = process.argv.includes("--final");
  if (files.length === 0 && !final) {
    console.error(`releases/${version}/ has no installers: run npm run release first.`);
    process.exit(1);
  }
  if (!succeeds(["gh", "auth", "status"])) {
    console.error("GitHub's CLI isn't installed or signed in: install it (brew install gh, or winget install GitHub.cli), then gh auth login.");
    process.exit(1);
  }

  if (succeeds(["gh", "release", "view", tag])) {
    if (files.length) {
      console.log(`Adding to the ${tag} release: ${files.map((f) => f.split(/[\\/]/).pop()).join(", ")}`);
      run(["gh", "release", "upload", tag, ...files, "--clobber"]);
    }
  } else {
    // the commit that was built, tagged here unless the other computer already has
    if (!succeeds(["git", "rev-parse", "--verify", "--quiet", `refs/tags/${tag}`])) {
      run(["git", "fetch", "--tags", "--quiet"]);
    }
    if (!succeeds(["git", "rev-parse", "--verify", "--quiet", `refs/tags/${tag}`])) run(["git", "tag", tag]);
    run(["git", "push", "origin", `refs/tags/${tag}`]);
    console.log(`Making the ${tag} release, as a draft, with: ${files.map((f) => f.split(/[\\/]/).pop()).join(", ")}`);
    run(["gh", "release", "create", tag, ...files, "--draft", "--title", `SuperXTerminal ${version}`, "--generate-notes"]);
  }

  if (final) {
    run(["gh", "release", "edit", tag, "--draft=false"]);
    console.log(`\n${tag} is public: the app's update check offers it now.`);
  } else {
    console.log(`\n${tag} is a draft. Once both computers' installers are in: npm run release:publish -- --final`);
  }
}

main();
