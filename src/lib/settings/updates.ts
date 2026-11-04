/**
 * The update check, as SuperXPuTTY's checkForSuperXPuttyUpdates: the latest release of the repository in the
 * settings (`updateRepository`, as its SuperPuTTY.UpdateRepository) compared with this version; a newer one offers its
 * download page. At start (Settings > General, as its AutoUpdateCheck) only a newer version is mentioned; asked for
 * (SuperXTerminal > Check for Updates…), it always says how it went. Either way it goes in the app's log.
 */
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { ask, message } from "@tauri-apps/plugin-dialog";
import { settings } from "./store.svelte";

/** The title of the update check's messages */
const TITLE = "SuperXTerminal update check";

/** Add a line to the app's log (Help > Show Log); a log that can't be written is no reason to stop */
function log(level: "INFO" | "WARN", text: string): void {
  void invoke("applog_write", { level, text }).catch(() => {});
}

/** "v1.2.10" or "1.2.10" as numbers; [] when it isn't a version */
function parts(version: string): number[] {
  const match = /^v?(\d+(?:\.\d+)*)/i.exec(version.trim());
  return match ? match[1].split(".").map(Number) : [];
}

/** Whether `a` is a later version than `b` */
export function isNewer(a: string, b: string): boolean {
  const [x, y] = [parts(a), parts(b)];
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (x[i] ?? 0) - (y[i] ?? 0);
    if (d !== 0) return d > 0;
  }
  return false;
}

/** Look for a newer release; `asked` when the user chose Check for Updates… (not the check at start) */
export async function checkForUpdates(asked: boolean): Promise<void> {
  const repository = (settings.updateRepository || "waukeerules/SuperXTerminal").trim().replace(/^\/+|\/+$/g, "");
  const current = await getVersion().catch(() => "0.0.0");
  log("INFO", `Checking for an update from ${repository} (running ${current})`);
  let response: Response;
  try {
    response = await fetch(`https://api.github.com/repos/${repository}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } });
  } catch (e) {
    log("WARN", `The update check couldn't reach GitHub: ${e}`);
    if (asked) await message(`GitHub couldn't be reached to check for updates. Try again later.\n\n${e}`, { title: TITLE, kind: "warning" });
    return;
  }
  if (response.status === 404) {
    // no release yet, or a private repository GitHub won't show without signing in
    log("INFO", `${repository} has no release that can be seen`);
    if (asked) await message(`${repository} has no release that can be seen: none has been published yet, or the repository is private. You're running ${current}.`, { title: TITLE });
    return;
  }
  if (!response.ok) {
    log("WARN", `The update check failed: GitHub answered ${response.status}`);
    if (asked) await message(`There was an error while checking for updates (GitHub answered ${response.status}). Please try again later.`, { title: TITLE, kind: "warning" });
    return;
  }
  const latest = (await response.json()) as { tag_name?: string; html_url?: string; name?: string };
  const tag = latest.tag_name ?? "";
  if (tag && isNewer(tag, current)) {
    log("INFO", `A newer version is available: ${tag}`);
    const open = await ask(`An updated version of SuperXTerminal (${tag}) is available; you have ${current}. Would you like to visit the download page?`,
      { title: "SuperXTerminal update found", okLabel: "Download Page", cancelLabel: "Not Now" });
    if (open && latest.html_url) await invoke("plugin:opener|open_url", { url: latest.html_url }).catch(() => {});
  } else {
    log("INFO", `No newer version (latest ${tag || "unknown"}, running ${current})`);
    if (asked) await message(`You are running the latest version of SuperXTerminal (${current}).`, { title: TITLE });
  }
}
