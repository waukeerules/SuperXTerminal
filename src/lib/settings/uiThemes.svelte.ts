/**
 * The window's own themes, beside Light and Dark (Settings > General > Look > Theme, and Edit Themes…): a name, the
 * built-in theme it starts from, and its colours. They're kept in UiThemes.json in the user's folder
 * (src-tauri/src/ui_themes.rs); the terminal's themes are separate (Settings > Terminal).
 *
 * A theme is applied by setting the window's colour tokens (app.css's --bg-window, --accent...) on the page itself,
 * over its base's, and marking the page with its base (data-theme), so what follows the app's darkness (the
 * terminal's Follow the App's Look, the system's own controls) goes by that. Settings' theme is then the theme's
 * name, which can't be "system", "light" or "dark".
 */
import { invoke } from "@tauri-apps/api/core";

/** One theme, as UiThemes.json keeps it */
export interface UiTheme {
  name: string;
  /** "light" or "dark": the built-in theme it starts from, for the colours it doesn't set */
  base: "light" | "dark";
  /** Its colours, "#rrggbb", by the names in COLOUR_TOKENS */
  colors: Record<string, string>;
}

/** A colour a theme can set: its name in the file, app.css's token, and what it colours, as Edit Themes… labels it */
export interface ColourToken {
  key: string;
  css: string;
  label: string;
}

/** The colours a theme can set, in the order Edit Themes… lists them */
export const COLOUR_TOKENS: ColourToken[] = [
  { key: "bgWindow", css: "--bg-window", label: "Window background" },
  { key: "bgBar", css: "--bg-bar", label: "Bars: menu, tabs, status" },
  { key: "bgPane", css: "--bg-pane", label: "Panes and lists" },
  { key: "bgInput", css: "--bg-input", label: "Fields and buttons" },
  { key: "border", css: "--border", label: "Borders" },
  { key: "text", css: "--text", label: "Text" },
  { key: "textMuted", css: "--text-muted", label: "Muted text" },
  { key: "accent", css: "--accent", label: "Selected rows and highlights" },
  { key: "check", css: "--check", label: "Checkboxes" },
];

/**
 * Each built-in theme's colours, as app.css has them: what a new theme starts with, and what Edit Themes… shows for
 * a colour a theme leaves to its base. Keep them as app.css's
 */
export const BASE_COLOURS: Record<"light" | "dark", Record<string, string>> = {
  light: {
    bgWindow: "#f5f5f5", bgBar: "#f0f0f0", bgPane: "#ffffff", bgInput: "#ffffff", border: "#d4d4d4",
    text: "#1e1e1e", textMuted: "#6d6d6d", accent: "#007acc", check: "#007acc",
  },
  dark: {
    bgWindow: "#2d2d30", bgBar: "#2d2d30", bgPane: "#252526", bgInput: "#333337", border: "#3f3f46",
    text: "#f1f1f1", textMuted: "#a0a0a0", accent: "#3399ff", check: "#3399ff",
  },
};

/** The themes saved, read when the settings are (store.svelte.ts) and again when Edit Themes… saves */
export const uiThemes = $state<{ list: UiTheme[] }>({ list: [] });

/** Read the themes from UiThemes.json */
export async function loadUiThemes(): Promise<void> {
  uiThemes.list = await invoke<UiTheme[]>("ui_themes_load").catch(() => []);
}

/**
 * Save the themes, whole; what's saved, or why they can't be ("There are two themes named...")
 * @param themes Every theme, as Edit Themes… has them
 */
export async function saveUiThemes(themes: UiTheme[]): Promise<UiTheme[]> {
  uiThemes.list = await invoke<UiTheme[]>("ui_themes_save", { themes });
  return uiThemes.list;
}

/**
 * A theme by its name, whatever its case; undefined for a built-in one ("system", "light", "dark") or none
 * @param name Settings' theme
 */
export function uiThemeNamed(name: string): UiTheme | undefined {
  return uiThemes.list.find((t) => t.name.toLowerCase() === name.trim().toLowerCase());
}

/**
 * Draw the page in a theme: its base marked (data-theme) and its colours set over the base's; or, for none, every
 * colour the page set taken off, back to app.css's
 * @param theme The theme, or undefined for a built-in one
 */
export function applyUiTheme(theme: UiTheme | undefined): void {
  const style = document.documentElement.style;
  for (const token of COLOUR_TOKENS) {
    const value = theme?.colors[token.key];
    if (value) style.setProperty(token.css, value);
    else style.removeProperty(token.css);
  }
}
