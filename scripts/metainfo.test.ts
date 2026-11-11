/**
 * The app's AppStream metainfo (src-tauri/linux/com.waukeerules.superxterminal.metainfo.xml), which software centres
 * and the AppImage catalog show the app by: it's well-formed, it names the app as Tauri's packages do (its id, its
 * .desktop file, its licence), every Linux package installs it where they look, and each screenshot it names is one
 * of the README's, in the repository. Read from the files themselves; nothing is stood in for. `appstreamcli validate`
 * checks the rest when the AppImage is made (scripts/linux-build.Dockerfile).
 */
import { describe, expect, it } from "vitest";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

/** The repository's folder (vitest runs from it) */
const root = process.cwd();
/** Tauri's settings, which name the app and its packages */
const conf = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"));
/** Where the metainfo is kept, beside tauri.conf.json, as the packages' files name it */
const source = `linux/${conf.identifier}.metainfo.xml`;
/** The metainfo, parsed as software centres read it */
const xml = new DOMParser().parseFromString(readFileSync(join(root, "src-tauri", source), "utf8"), "application/xml");
/** The text of the first element named so */
const text = (selector: string) => xml.querySelector(selector)?.textContent?.trim() ?? "";

describe("the AppStream metainfo", () => {
  it("is well-formed, and names the app as its packages do", () => {
    expect(xml.querySelector("parsererror")).toBeNull();
    // the id is the app's identifier, and the file is named after it, as AppStream asks
    expect(text("component > id")).toBe(conf.identifier);
    // Tauri names the .desktop file after the product (usr/share/applications/SuperXTerminal.desktop)
    expect(text("launchable[type='desktop-id']")).toBe(`${conf.productName}.desktop`);
    expect(text("component > name")).toBe(conf.productName);
    expect(text("project_license")).toBe(conf.bundle.license);
  });

  it("is installed by the .deb, the .rpm and the AppImage where software centres look", () => {
    const where = `/usr/share/metainfo/${conf.identifier}.metainfo.xml`;
    for (const kind of ["deb", "rpm", "appimage"]) expect(conf.bundle.linux[kind].files).toEqual({ [where]: source });
  });

  it("shows the README's screenshots, one first, each a file in the repository", () => {
    const shots = [...xml.querySelectorAll("screenshot")];
    expect(shots.filter((s) => s.getAttribute("type") === "default")).toHaveLength(1);
    expect(shots[0].getAttribute("type")).toBe("default");
    const prefix = "https://raw.githubusercontent.com/waukeerules/SuperXTerminal/main/";
    for (const shot of shots) {
      const url = shot.querySelector("image")?.textContent?.trim() ?? "";
      expect(url.startsWith(prefix), url).toBe(true);
      expect(existsSync(join(root, url.slice(prefix.length))), url).toBe(true);
      expect(shot.querySelector("caption")?.textContent?.trim()).toBeTruthy();
    }
  });
});
