// Runs the Sessions tree's measurement (tests/browser/tree.spec.ts, its "measure" test) and prints its numbers, the
// same way in PowerShell, cmd and a Unix shell, which set environment variables differently:
//
//   npm run measure:tree                    10,000 sessions in one folder: every one a row when it's open
//   npm run measure:tree -- 10000 100       10,000 sessions, 100 to a folder
//
// Run it before and after a change to the tree (`git stash` the change for the before); docs/TESTING.md says what
// the numbers are.
import { execSync } from "node:child_process";

const [sessions = "10000", perFolder = sessions] = process.argv.slice(2);
if (!/^\d+$/.test(sessions) || !/^\d+$/.test(perFolder)) {
  console.error("Usage: npm run measure:tree -- [sessions] [sessions to a folder]");
  process.exit(2);
}
const env = { ...process.env, SXT_MEASURE: "1", SXT_SESSIONS: sessions, SXT_PER_FOLDER: perFolder };
try {
  // through the shell: npx is npx.cmd on Windows, which only a shell runs
  execSync("npx playwright test tree.spec --project=chromium -g measure", { env, stdio: "inherit" });
} catch {
  process.exit(1);
}
