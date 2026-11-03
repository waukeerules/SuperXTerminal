// Unit and component tests (npm test): Vite's own setup (the Svelte plugin), with the DOM from jsdom and the Rust side
// stood in for by Tauri's mockIPC (src/test/setup.ts). The terminal page's browser tests are Playwright's
// (playwright.config.ts, npm run test:browser).
import { defineConfig, mergeConfig } from "vitest/config";
import { svelteTesting } from "@testing-library/svelte/vite";
import viteConfig from "./vite.config";

export default mergeConfig(
  viteConfig,
  defineConfig({
    plugins: [svelteTesting()],
    test: {
      environment: "jsdom",
      include: ["src/**/*.test.ts"],
      setupFiles: ["src/test/setup.ts"],
    },
  }),
);
