// Fails when a package the app ships (npm dependencies, not dev tools) has a licence outside the allowed list.
// The allowed list matches src-tauri/deny.toml, so the Rust and JavaScript sides follow one rule (docs/PLAN.md §1
// Licensing). Run with `npm run licenses` before each release.
import { execSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";

/** The licences a shipped package may have (SPDX ids), as src-tauri/deny.toml allows */
const ALLOWED = new Set([
  "MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "CC0-1.0", "0BSD", "Unicode-3.0", "MPL-2.0",
]);

/** Whether an SPDX expression is acceptable: every alternative of an OR needs only one allowed side */
function allowed(expression) {
  const text = expression.replace(/[()]/g, " ").trim();
  return text.split(/\s+OR\s+/).some((alternative) =>
    alternative.split(/\s+AND\s+/).every((id) => ALLOWED.has(id.trim())),
  );
}

/** The production dependency tree, with each package's licence and folder */
// through the shell: npm is npm.cmd on Windows, which only a shell runs
const tree = JSON.parse(execSync("npm ls --omit=dev --all --json --long", { encoding: "utf8" }));
/** Each package with a licence that isn't allowed, as "name@version: licence" */
const problems = [];
/** The packages checked, as "name@version", so a package used twice is checked once */
const seen = new Set();

/** Check a package and everything it depends on */
function visit(name, node) {
  if (!node || seen.has(`${name}@${node.version}`)) return;
  seen.add(`${name}@${node.version}`);
  let license = node.license;
  if (!license && node.path) {
    try {
      license = JSON.parse(readFileSync(join(node.path, "package.json"), "utf8")).license;
    } catch {
      // no package.json: reported as unknown below
    }
  }
  if (typeof license !== "string" || !allowed(license)) {
    problems.push(`${name}@${node.version}: ${license ?? "unknown"}`);
  }
  for (const [child, value] of Object.entries(node.dependencies ?? {})) visit(child, value);
}

for (const [name, node] of Object.entries(tree.dependencies ?? {})) visit(name, node);

if (problems.length) {
  console.error("Licences not allowed:\n  " + problems.join("\n  "));
  process.exit(1);
}
console.log(`Licences OK: ${seen.size} shipped packages`);
