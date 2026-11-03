// Writes THIRD-PARTY-NOTICES.txt: every third-party component the app ships, with its licence text. The Rust crates
// built into the release program (cargo metadata: normal dependencies of superxterminal, on this Mac's target), the npm
// packages the window uses (production dependencies), and the libraries bundled as files (public/web/licenses).
// Packages with the same licence text share one copy of it. Run with `npm run notices`; the release build runs it
// first (tauri.conf.json's beforeBuildCommand), and the file goes into the app (Help > Third-Party Notices).
import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";

const root = join(dirname(new URL(import.meta.url).pathname), "..");
const LICENSE_FILE = /^(LICEN[CS]E|COPYING|NOTICE|UNLICENSE)([-._].*)?$/i;

/** The licence files in a package's folder, their text (NOTICE files are kept too, as Apache-2.0 asks) */
function licenceTexts(folder) {
  if (!folder || !existsSync(folder)) return [];
  return readdirSync(folder)
    .filter((name) => LICENSE_FILE.test(name))
    .sort()
    .map((name) => readFileSync(join(folder, name), "utf8").replace(/\r\n/g, "\n").trim())
    .filter(Boolean);
}

/** @type {{ name: string, version: string, license: string, source: string, texts: string[], authors?: string[] }[]} */
const components = [];

// ---- Rust: what the release program links, for this Mac ----
const target = execFileSync("rustc", ["-vV"], { encoding: "utf8" }).match(/host: (\S+)/)[1];
const metadata = JSON.parse(
  execFileSync("cargo", ["metadata", "--format-version", "1", "--filter-platform", target, "--manifest-path", join(root, "src-tauri/Cargo.toml")], {
    encoding: "utf8",
    maxBuffer: 256 * 1024 * 1024,
  }),
);
const packages = new Map(metadata.packages.map((p) => [p.id, p]));
const nodes = new Map(metadata.resolve.nodes.map((n) => [n.id, n]));
const self = metadata.packages.find((p) => p.name === "superxterminal");
const shipped = new Set();
const queue = [self.id];
while (queue.length) {
  const id = queue.pop();
  for (const dep of nodes.get(id)?.deps ?? []) {
    // only what's built into the program: not build scripts' or tests' dependencies
    if (!dep.dep_kinds.some((k) => k.kind === null)) continue;
    if (!shipped.has(dep.pkg)) {
      shipped.add(dep.pkg);
      queue.push(dep.pkg);
    }
  }
}
for (const id of shipped) {
  const p = packages.get(id);
  components.push({
    name: p.name,
    version: p.version,
    license: p.license ?? (p.license_file ? `see ${p.license_file}` : "unknown"),
    source: p.repository ?? `https://crates.io/crates/${p.name}`,
    texts: licenceTexts(dirname(p.manifest_path)),
    authors: p.authors,
  });
}

// ---- npm: the window's packages ----
const tree = JSON.parse(execFileSync("npm", ["ls", "--omit=dev", "--all", "--json", "--long"], { cwd: root, encoding: "utf8" }));
const seen = new Set();
(function visit(deps) {
  for (const [name, node] of Object.entries(deps ?? {})) {
    if (!node.version || seen.has(`${name}@${node.version}`)) continue;
    seen.add(`${name}@${node.version}`);
    components.push({
      name,
      version: node.version,
      license: typeof node.license === "string" ? node.license : node.license?.type ?? "unknown",
      source: (typeof node.repository === "string" ? node.repository : node.repository?.url) ?? `https://www.npmjs.com/package/${name}`,
      texts: licenceTexts(node.path),
      authors: node.author ? [typeof node.author === "string" ? node.author : node.author.name] : [],
    });
    visit(node.dependencies);
  }
})(tree.dependencies);

// ---- libraries bundled as files (public/web/lib, the icons) ----
const BUNDLED = {
  "xterm.js-LICENSE.txt": ["xterm.js (and its addons)", "MIT", "https://github.com/xtermjs/xterm.js"],
  "monaco-editor-LICENSE.txt": ["Monaco Editor", "MIT", "https://github.com/microsoft/monaco-editor"],
  "simple-icons-LICENSE.md": ["Simple Icons", "CC0-1.0", "https://github.com/simple-icons/simple-icons"],
  "SuperXPuTTY-LICENSE.txt": ["SuperXPuTTY (pages, icons and behaviour this app follows)", "MIT", "https://github.com/waukeerules/superputty"],
};
const bundledFolder = join(root, "public/web/licenses");
for (const file of readdirSync(bundledFolder).sort()) {
  const [name, license, source] = BUNDLED[file] ?? [file.replace(/-LICEN[CS]E.*$/i, ""), "see text", ""];
  components.push({ name, version: "", license, source, texts: [readFileSync(join(bundledFolder, file), "utf8").trim()] });
}

// ---- a package with no licence file: its authors' copyright and the standard text of a licence it offers ----
const STANDARD = Object.fromEntries(
  ["MIT", "Apache-2.0", "BSD-3-Clause", "Zlib", "MPL-2.0"].map((id) => [id, readFileSync(join(root, "scripts/licences", `${id}.txt`), "utf8").trim()]),
);
const missing = [];
for (const c of components) {
  if (c.texts.length) continue;
  // the first licence offered that there's a standard text for (MIT before Apache-2.0: it carries the copyright)
  const offered = c.license.split(/\s+OR\s+|\/|[()]/).map((x) => x.trim()).filter(Boolean);
  const id = ["MIT", "Apache-2.0", "BSD-3-Clause", "Zlib", "MPL-2.0"].find((l) => offered.includes(l));
  if (!id) {
    missing.push(c);
    continue;
  }
  const holders = (c.authors ?? []).map((a) => a.replace(/\s*<[^>]*>/, "").trim()).filter(Boolean).join(", ") || `the ${c.name} authors`;
  c.texts = [id === "Apache-2.0" || id === "MPL-2.0" ? STANDARD[id] : `${id} licence\n\nCopyright (c) ${holders}\n\n${STANDARD[id]}`];
}

// ---- write it: the list, then each distinct licence text once, with the components that use it ----
components.sort((a, b) => a.name.localeCompare(b.name, "en", { sensitivity: "base" }) || a.version.localeCompare(b.version));
const label = (c) => `${c.name}${c.version ? ` ${c.version}` : ""}`;
/** each text, and who uses it; a component's own licence files are each a text of their own */
const texts = new Map();
for (const c of components) {
  for (const text of c.texts) {
    if (!texts.has(text)) texts.set(text, []);
    texts.get(text).push(c);
  }
}
const rule = "=".repeat(78);
let out = `SuperXTerminal: third-party notices

SuperXTerminal includes the components below. Each is used under its own licence, given in full further down: each
distinct licence text appears once, with the components it belongs to. Generated by scripts/notices.mjs.

${rule}
COMPONENTS (${components.length})
${rule}

`;
for (const c of components) out += `${label(c)}  [${c.license}]${c.source ? `  ${c.source.replace(/^git\+/, "")}` : ""}\n`;
let n = 0;
for (const [text, users] of texts) {
  n++;
  out += `\n\n${rule}\nLICENCE TEXT ${n}, for: ${users.map(label).join(", ")}\n${rule}\n\n${text}\n`;
}
writeFileSync(join(root, "THIRD-PARTY-NOTICES.txt"), out);
console.log(`THIRD-PARTY-NOTICES.txt: ${components.length} components, ${texts.size} licence texts, ${Math.round(out.length / 1024)} KB`);
if (missing.length) {
  console.error(`No licence text for: ${missing.map((c) => `${label(c)} [${c.license}]`).join(", ")}`);
  process.exit(1);
}
