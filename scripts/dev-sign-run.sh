#!/bin/sh
# Cargo's runner for development builds on macOS (src-tauri/.cargo/config.toml): sign the program, then run it.
#
# A fresh build is a new app to macOS, so the Keychain asked again for every saved password after each rebuild. The
# Keychain lets an app in by its Apple team ID (each password's "partition list"); a build with none is let in by its
# exact hash, which every rebuild changes. Signed with an Apple Development certificate (free, from Xcode with any
# Apple ID), every build has the same team ID and "Always Allow" lasts. A self-signed "SuperXTerminal Dev"
# certificate is used if there's no Apple Development one, though without a team ID it doesn't stop the prompts.
# Without either the program just runs. How to make the certificate: docs/PLAN.md, "Developing on a Mac".
identities=$(security find-identity -v -p codesigning 2>/dev/null)
identity=$(printf '%s\n' "$identities" | sed -n 's/.*"\(Apple Development: [^"]*\)".*/\1/p' | head -n 1)
if [ -z "$identity" ] && security find-identity -p codesigning 2>/dev/null | grep -q '"SuperXTerminal Dev"'; then
    identity="SuperXTerminal Dev"
fi
if [ -n "$identity" ]; then
    codesign --force --sign "$identity" "$1" 2>/dev/null || echo "dev-sign-run: couldn't sign $1 with \"$identity\"" >&2
fi
exec "$@"
