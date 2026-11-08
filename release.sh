#!/bin/sh
# A whole Mac release in one command: ./release.sh --release 0.8.0
# Pulls, sets the version if it's new, runs the checks, builds the .dmg and puts it in the GitHub release (a draft).
# Everything is done by scripts/release-all.mjs, which lists the steps and the other options (--skip-checks, --linux,
# --final); this only runs it. The Windows PC's is release.bat. docs/RELEASE.md has how the two fit together.
cd "$(dirname "$0")" || exit 1
exec node scripts/release-all.mjs "$@"
