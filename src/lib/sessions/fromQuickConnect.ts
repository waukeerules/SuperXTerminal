/**
 * A saved session made from a Quick connect tab (its menu's Save session as…): what was typed in the toolbar as
 * Edit Session's fields, read as the core reads it to open the tab (`QuickConnect::session` in
 * `src-tauri/src/launch.rs`). The password typed is left out: Quick connect never stores one, and the session can
 * be given its own in Edit Session.
 */
import type { QuickConnect } from "../terminal/PageHost";
import { PROTOCOLS, blankSession, defaultPort, type SessionFields } from "./api";

/**
 * The fields of a new session for what a Quick connect tab was opened with.
 * @param quick the toolbar's fields; its host may be "host", "host:port", "user@host" or "ssh://host:port"
 * @param title the tab's title, which names the session; a tab still titled with what was typed ("admin@sw1:2222")
 *   is named for its host alone
 * @returns the session, in no folder, on the port typed, else its connection type's usual one
 */
export function sessionFromQuick(quick: QuickConnect, title: string): SessionFields {
  let text = quick.host.trim().replace(/\/+$/, "");
  let protocol = quick.protocol;
  const scheme = text.indexOf("://");
  if (scheme >= 0) {
    // a connection type typed with the host ("telnet://") in Edit Session's spelling of it ("Telnet"), so its list
    // shows it chosen
    const named = text.slice(0, scheme);
    protocol = PROTOCOLS.find((p) => p.toLowerCase() === named.toLowerCase()) ?? named.toUpperCase();
    text = text.slice(scheme + 3);
  }
  const typed = text;
  let username = quick.username.trim();
  const at = text.indexOf("@");
  if (at >= 0) {
    // the Login box comes before a user typed with the host, as it does when the tab is opened
    username ||= text.slice(0, at);
    text = text.slice(at + 1);
  }
  let host = text;
  let port = 0;
  const colon = text.lastIndexOf(":");
  const after = colon >= 0 ? text.slice(colon + 1) : "";
  if (/^\d+$/.test(after) && Number(after) <= 65535) {
    host = text.slice(0, colon);
    port = Number(after);
  }
  return {
    ...blankSession(""),
    name: title === typed || !title.trim() ? host : title,
    protocol,
    host,
    port: port || defaultPort(protocol) || 0,
    username,
    profile: quick.profile.trim() || "Default Settings",
  };
}
