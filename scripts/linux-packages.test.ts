/**
 * What the Linux .deb and .rpm ask the package manager to install with them. Tauri asks only for WebKitGTK, GTK and
 * the tray icon's library, but the app also links against OpenSSL 3 (native-tls, for Remote Desktop and PostgreSQL)
 * and ALSA (Remote Desktop's sound), which those don't bring. Read from tauri.conf.json itself; nothing is stood in
 * for. That every library the built app needs is declared is checked by hand, with `objdump -p` (docs/BUILD-Linux.md).
 */
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

/** Tauri's settings, which hold each package's dependencies */
const conf = JSON.parse(readFileSync(join(process.cwd(), "src-tauri/tauri.conf.json"), "utf8"));

describe("the Linux packages", () => {
  it("the .deb asks for OpenSSL 3 and ALSA, so it starts on an Ubuntu that ships OpenSSL 4 (26.04 and later)", () => {
    // Ubuntu 24.04 and later name them with t64; 22.04 and Debian 12 without
    expect(conf.bundle.linux.deb.depends).toEqual(expect.arrayContaining(["libssl3t64 | libssl3", "libasound2t64 | libasound2"]));
  });

  it("the .rpm asks for OpenSSL and ALSA, as Fedora and RHEL name them", () => {
    expect(conf.bundle.linux.rpm.depends).toEqual(expect.arrayContaining(["openssl-libs", "alsa-lib"]));
  });
});
