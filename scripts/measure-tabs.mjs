// Runs the many-tabs measurement (tests/browser/manyTabs.spec.ts, its "measure" test) and prints its numbers, the
// same way in PowerShell, cmd and a Unix shell, which set environment variables differently:
//
//   npm run measure:tabs                         100 tabs, the first 10 printing at once (the one in front among them)
//   npm run measure:tabs -- 40 10                40 tabs, 10 printing
//   npm run measure:tabs -- 100 10 behind        only tabs behind the front one print
//   npm run measure:tabs -- 16 16 grid           16 tabs side by side, all showing and all printing (Grid View)
//   npm run measure:tabs -- 12 1 front profile   also where the window's thread spent its time, function by function
//
// Run it before and after a change to the terminal page or its host (`git stash` the change for the before) to see
// what the change did; docs/TESTING.md says what the numbers are.
import { execSync } from "node:child_process";

const [tabs = "100", busy = "10", how = "front", extra = ""] = process.argv.slice(2);
if (!/^\d+$/.test(tabs) || !/^\d+$/.test(busy) || !["front", "behind", "grid"].includes(how) || !["", "profile"].includes(extra)) {
  console.error("Usage: npm run measure:tabs -- [tabs] [printing] [front|behind|grid] [profile]");
  process.exit(2);
}
const env = { ...process.env, SXT_MEASURE: "1", SXT_TABS: tabs, SXT_BUSY: busy };
if (how === "behind") env.SXT_SKIP_FRONT = "1";
if (how === "grid") env.SXT_GRID = "1";
if (extra === "profile") env.SXT_PROFILE = "1";
try {
  // through the shell: npx is npx.cmd on Windows, which only a shell runs
  execSync("npx playwright test manyTabs --project=chromium -g measure", { env, stdio: "inherit" });
} catch {
  process.exit(1);
}
