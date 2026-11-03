// Browser tests for the terminal page (npm run test:browser): public/web/terminal served by Vite on its own port, so
// `tauri dev` (1420) can run beside it, and driven through the page's terminalDev hooks. WebKit is the Mac app's
// engine (WKWebView); Chromium is Windows' (WebView2).
import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "tests/browser",
  fullyParallel: true,
  reporter: "list",
  use: { baseURL: "http://127.0.0.1:1421", viewport: { width: 700, height: 400 } },
  projects: [
    { name: "webkit", use: { ...devices["Desktop Safari"], viewport: { width: 700, height: 400 } } },
    { name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 700, height: 400 } } },
  ],
  webServer: {
    command: "npx vite --port 1421 --strictPort",
    url: "http://127.0.0.1:1421/web/terminal/index.html",
    reuseExistingServer: true,
  },
});
