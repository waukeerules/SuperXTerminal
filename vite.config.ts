import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { defineConfig, type Plugin } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

/**
 * dockview 8 ships its stylesheet only inside its standalone build (dist/dockview-core.js), which injects it when
 * loaded; the ES module build Vite uses has none, and there's no .css file. This serves that stylesheet as
 * `virtual:dockview.css`, read from the installed package, so it always matches the dockview in use.
 */
function dockviewCss(): Plugin {
  const id = "virtual:dockview.css";
  return {
    name: "dockview-css",
    resolveId: (source) => (source === id ? "\0" + id : null),
    load(resolved) {
      if (resolved !== "\0" + id) return null;
      const file = createRequire(import.meta.url).resolve("dockview-core/dist/dockview-core.js");
      const match = /s\.textContent = ("(?:[^"\\]|\\.)*");/.exec(readFileSync(file, "utf8"));
      if (!match) throw new Error("dockview's stylesheet wasn't found in " + file);
      return JSON.parse(match[1]) as string;
    },
  };
}

// Tauri's dev server: a fixed port (tauri.conf.json's devUrl), and Rust's own files left to cargo
export default defineConfig({
  plugins: [svelte(), dockviewCss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "safari16",
    outDir: "dist",
  },
});
