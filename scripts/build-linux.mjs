// Builds SuperXTerminal's Linux packages (.deb, .rpm and AppImage) in Docker, from the Mac or the Windows PC:
// `npm run build:linux`. scripts/release-all.mjs runs it as part of a Windows release. docs/BUILD-Linux.md has the
// steps around it.
//
// Tauri can't make Linux packages on macOS or Windows (they link against WebKitGTK and other Linux libraries), so this
// runs the build in an Ubuntu 22.04 container, made from scripts/linux-build.Dockerfile the first time it's needed.
// The repository is mounted at /src; node_modules, src-tauri/target and Cargo's downloads live on Docker volumes, one
// set per CPU type, so this computer's own node_modules and target are left alone and a second build only compiles
// what changed. The build writes Linux's THIRD-PARTY-NOTICES.txt into the repository's folder, as every build writes
// its own; it's not in git.
//
// Node rather than a shell script, so the same file runs on Windows, where `bash` may be WSL's, Git's or none.
//
//   npm run build:linux                          packages for x86_64 PCs (amd64), the usual Ubuntu machine
//   npm run build:linux -- --arm64               packages for ARM machines; full speed on Apple silicon
//   npm run build:linux -- --shell               a shell in the same container, to look around or build by hand
//   npm run build:linux -- --new-image           remake the image first (newer Ubuntu updates, Node or Rust)
//   npm run build:linux -- --into releases/0.8.0 where the packages go (else releases/linux/<version>/)
//   npm run build:linux -- -- --bundles deb      anything after a second -- goes to `tauri build`
//
// amd64 is emulated on Apple silicon: the first build takes 10-20 minutes there, later ones a few. On an x86 Windows
// PC it runs at full speed.
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join, dirname, isAbsolute, relative } from "node:path";
import { fileURLToPath } from "node:url";

/** The repository's folder (fileURLToPath, not the URL's pathname: on Windows that's "/C:/...") */
const root = join(dirname(fileURLToPath(import.meta.url)), "..");

/** Stop with a message */
function fail(message, code = 1) {
  console.error(`build-linux: ${message}`);
  process.exit(code);
}

/**
 * The options, from the command line
 * @returns {{ arch: string, shell: boolean, newImage: boolean, into: string | null, tauriArgs: string[] }}
 */
function options(args) {
  const o = { arch: "amd64", shell: false, newImage: false, into: null, tauriArgs: [] };
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a === "--arm64" || a === "--amd64") o.arch = a.slice(2);
    else if (a === "--shell") o.shell = true;
    else if (a === "--new-image") o.newImage = true;
    else if (a === "--into") o.into = args[++i] ?? fail("--into needs a folder", 2);
    else if (a === "--") {
      o.tauriArgs = args.slice(i + 1);
      break;
    } else fail(`unknown option ${a} (the options are at the top of scripts/build-linux.mjs)`, 2);
  }
  return o;
}

/** Whether a command succeeds, its output kept quiet */
function succeeds(words) {
  return spawnSync(words[0], words.slice(1), { stdio: "ignore" }).status === 0;
}

/** Run docker with these arguments, its output shown; returns its exit code */
function docker(args, input) {
  const result = spawnSync("docker", args, { stdio: input === undefined ? "inherit" : ["pipe", "inherit", "inherit"], input });
  if (result.error) fail(`couldn't run docker: ${result.error.message}`);
  return result.status ?? 1;
}

function main() {
  const o = options(process.argv.slice(2));
  const image = `superxterminal-linux-build:${o.arch}`;
  const version = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8")).version;
  // a folder in the repository, as the container sees it (/src/...), with forward slashes on Windows too
  const intoAbsolute = o.into ? (isAbsolute(o.into) ? o.into : join(root, o.into)) : join(root, "releases", "linux", version);
  const into = relative(root, intoAbsolute).split(/[\\/]/).join("/");
  if (into.startsWith("..") || isAbsolute(into)) fail(`--into must be inside the repository (${o.into})`, 2);

  if (!succeeds(["docker", "info"])) {
    fail("Docker isn't running. Start Docker Desktop (or OrbStack on the Mac), in Linux containers mode, then try again.");
  }

  // The image: made once, or again when asked. Built from the Dockerfile alone (no context), as it copies nothing in.
  if (o.newImage || !succeeds(["docker", "image", "inspect", image])) {
    console.log(`Making the Linux build image ${image} (a few minutes, once)...`);
    const dockerfile = readFileSync(join(root, "scripts/linux-build.Dockerfile"));
    if (docker(["build", "--pull", "--platform", `linux/${o.arch}`, "-t", image, "-"], dockerfile) !== 0) fail("the image couldn't be made");
  }

  const run = ["run", "--rm", "-i", "--platform", `linux/${o.arch}`,
    "-v", `${root}:/src`,
    "-v", `sxt-linux-node-modules-${o.arch}:/src/node_modules`,
    "-v", `sxt-linux-target-${o.arch}:/src/src-tauri/target`,
    "-v", `sxt-linux-cargo-registry-${o.arch}:/usr/local/cargo/registry`,
    // passed on when it's set: fewer parallel compiles, for a Docker with little memory (found on the Windows PC)
    "-e", "CARGO_BUILD_JOBS",
    "-w", "/src"];
  if (process.stdin.isTTY && process.stdout.isTTY) run.push("-t");

  if (o.shell) {
    console.log(`A shell in the Linux build container (${o.arch}). The repository is /src; exit to leave.`);
    process.exit(docker([...run, image, "bash"]));
  }

  // Inside the container: install the npm packages for Linux, build, then copy whatever was bundled, even when one
  // bundle (it was the AppImage) failed after the others were made
  const script = `
    npm ci || exit 1
    npm run tauri build -- "$@"
    status=$?
    out="$SXT_INTO"
    mkdir -p "$out"
    find src-tauri/target/release/bundle -maxdepth 2 -type f \\
      \\( -name "*_\${SXT_VERSION}_*.deb" -o -name "*-\${SXT_VERSION}-*.rpm" -o -name "*_\${SXT_VERSION}_*.AppImage" \\) \\
      -exec cp {} "$out/" \\;
    echo
    echo "Linux packages in $out:"
    ls -lh "$out" | grep -E '\\.(deb|rpm|AppImage)$'
    exit $status`;
  const status = docker([...run, "-e", `SXT_INTO=${into}`, "-e", `SXT_VERSION=${version}`, image, "bash", "-c", script, "bash", ...o.tauriArgs]);
  process.exit(status);
}

main();
