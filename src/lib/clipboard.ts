/**
 * The clipboard's text for a paste, read by the system (clipboard_text, src-tauri/src/clipboard.rs) rather than the
 * page's navigator.clipboard.readText. WebView2 treats the app like a website, and reading the clipboard from the page
 * made it ask "localhost wants to see text and images copied to the clipboard"; the Rust side reading it isn't asked.
 * Writing stays with navigator.clipboard, which never asks.
 */
import { invoke } from "@tauri-apps/api/core";

/** The text on the clipboard; "" when there's none (nothing copied, or a picture) */
export async function clipboardText(): Promise<string> {
  return invoke<string>("clipboard_text").catch(() => "");
}
