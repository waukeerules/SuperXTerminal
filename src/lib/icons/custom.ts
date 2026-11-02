/**
 * Custom icons: one-colour PNGs in `<SettingsFolder>/icons`, shared by a team like Sessions.xml (SuperXPuTTY's
 * CustomIcons). The Rust core reads them (`icons_custom`); each is drawn as a mask, so it takes any colour. Without
 * a colour in its key, it's drawn in the PNG's own average colour.
 */
import { invoke } from "@tauri-apps/api/core";

/** One custom icon: its name as the file spells it, its PNG as a data URL, and its default colour */
export interface CustomIcon {
  title: string;
  dataUrl: string;
  color: string;
}

/** Load every custom icon, by lower-case name ("juniper" for icons/Juniper.png) */
export async function loadCustomIcons(): Promise<Map<string, CustomIcon>> {
  const found = await invoke<[string, string][]>("icons_custom");
  const icons = new Map<string, CustomIcon>();
  for (const [name, dataUrl] of found) {
    icons.set(name.toLowerCase(), { title: name, dataUrl, color: await averageColor(dataUrl) });
  }
  return icons;
}

/** The average colour of a PNG's visible pixels, as CustomIcons.DefaultColorOf */
async function averageColor(dataUrl: string): Promise<string> {
  const image = new Image();
  image.src = dataUrl;
  await image.decode().catch(() => undefined);
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, image.naturalWidth);
  canvas.height = Math.max(1, image.naturalHeight);
  const context = canvas.getContext("2d");
  if (!context) return "#808080";
  context.drawImage(image, 0, 0);
  const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
  let r = 0, g = 0, b = 0, n = 0;
  for (let i = 0; i < pixels.length; i += 4) {
    if (pixels[i + 3] < 128) continue;
    r += pixels[i];
    g += pixels[i + 1];
    b += pixels[i + 2];
    n += 1;
  }
  if (n === 0) return "#808080";
  const hex = (v: number) => Math.round(v / n).toString(16).padStart(2, "0");
  return `#${hex(r)}${hex(g)}${hex(b)}`.toUpperCase();
}
