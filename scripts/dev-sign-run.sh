#!/bin/sh
# Cargo's runner for development builds on macOS (src-tauri/.cargo/config.toml): sign the program with the
# "SuperXTerminal Dev" certificate, if this Mac has one, then run it. A fresh build is otherwise a new app to macOS,
# so the Keychain asks again for every password after each rebuild; signed with one certificate, "Always Allow"
# lasts. Without the certificate it just runs. How to make one: docs/PLAN.md, "Developing on a Mac".
identity="SuperXTerminal Dev"
if security find-identity -p codesigning 2>/dev/null | grep -q "\"$identity\""; then
    codesign --force --sign "$identity" "$1" 2>/dev/null || echo "dev-sign-run: couldn't sign $1 with \"$identity\"" >&2
fi
exec "$@"
