#!/usr/bin/env bash
# Builds SuperXTerminal's Linux packages (.deb, .rpm and AppImage) in Docker, from the Mac: `npm run build:linux`.
# docs/BUILD-Linux.md has the steps around it.
#
# Tauri can't make Linux packages on macOS (they link against WebKitGTK and other Linux libraries), so this runs the
# build in an Ubuntu 22.04 container, made from scripts/linux-build.Dockerfile the first time it's needed. The
# repository is mounted at /src; node_modules, src-tauri/target and Cargo's downloads live on Docker volumes, one set
# per CPU type, so the Mac's own node_modules and target are left alone and a second build only compiles what changed.
# The packages are copied to releases/linux/<version>/ (not releases/<version>/, which release:publish uploads whole).
# The build writes Linux's THIRD-PARTY-NOTICES.txt into the repository's folder, as every build writes its own; it's
# not in git.
#
#   scripts/build-linux.sh                 packages for x86_64 PCs (amd64), the usual Ubuntu machine
#   scripts/build-linux.sh --arm64         packages for ARM machines; runs at full speed on Apple silicon
#   scripts/build-linux.sh --shell         a shell in the same container, to look around or build by hand
#   scripts/build-linux.sh --new-image     remake the image first (newer Ubuntu updates, Node or Rust)
#   scripts/build-linux.sh -- --bundles deb    anything after -- goes to `tauri build`
#
# amd64 on Apple silicon runs under emulation: the first build takes 10-20 minutes, later ones a few.
set -euo pipefail

arch=amd64
shell=false
new_image=false
tauri_args=()
while [ $# -gt 0 ]; do
  case "$1" in
    --arm64) arch=arm64 ;;
    --amd64) arch=amd64 ;;
    --shell) shell=true ;;
    --new-image) new_image=true ;;
    --) shift; tauri_args=("$@"); break ;;
    *) echo "build-linux: unknown option $1 (the options are at the top of scripts/build-linux.sh)" >&2; exit 2 ;;
  esac
  shift
done

root="$(cd "$(dirname "$0")/.." && pwd)"
image="superxterminal-linux-build:$arch"

# On Windows, npm runs this with Git's or MSYS2's bash. Docker there wants the repository as a Windows path (C:/...),
# and MSYS would otherwise rewrite the container's own paths (/src, /usr/local/cargo/registry) into Windows ones.
case "$(uname -s)" in
  MINGW*|MSYS*) root="$(cd "$root" && pwd -W)"; export MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*' ;;
esac

if ! docker info >/dev/null 2>&1; then
  echo "build-linux: Docker isn't running. Start Docker Desktop (or OrbStack on the Mac), then try again." >&2
  exit 1
fi

# The image: made once, or again when asked. Built from the Dockerfile alone (no context), as it copies nothing in.
if $new_image || ! docker image inspect "$image" >/dev/null 2>&1; then
  echo "Making the Linux build image $image (a few minutes, once)..."
  docker build --pull --platform "linux/$arch" -t "$image" - < "$root/scripts/linux-build.Dockerfile"
fi

run=(docker run --rm -i --platform "linux/$arch"
  -v "$root":/src
  -v "sxt-linux-node-modules-$arch":/src/node_modules
  -v "sxt-linux-target-$arch":/src/src-tauri/target
  -v "sxt-linux-cargo-registry-$arch":/usr/local/cargo/registry
  -e CARGO_BUILD_JOBS
  -w /src)
[ -t 0 ] && [ -t 1 ] && run+=(-t)

if $shell; then
  echo "A shell in the Linux build container ($arch). The repository is /src; exit to leave."
  "${run[@]}" "$image" bash
  exit
fi

# Inside the container: install the packages for Linux, build, then copy whatever was bundled, even when one bundle
# (usually the AppImage) failed after the others were made
"${run[@]}" "$image" bash -c '
  npm ci || exit 1
  npm run tauri build -- "$@"
  status=$?
  v=$(node -p "require(\"./src-tauri/tauri.conf.json\").version")
  out="releases/linux/$v"
  mkdir -p "$out"
  find src-tauri/target/release/bundle -maxdepth 2 -type f \
    \( -name "*_${v}_*.deb" -o -name "*-${v}-*.rpm" -o -name "*_${v}_*.AppImage" \) -exec cp {} "$out/" \;
  echo
  echo "Linux packages in $out:"
  ls -lh "$out"
  exit $status
' bash "${tauri_args[@]+"${tauri_args[@]}"}"
