// The script box's page (public/web/script) on its own, as Edit Button and the Script Editor show it in an iframe:
// SPSL's colours (Monaco's own tokenizer, run on a script), Wrap, inserting at the cursor, and each edit sent back
// to the window. The main window is stood in for by the page itself: host/webview.js posts to window.parent, which
// for a page on its own is the page, so the test listens there; the window's messages go in through scriptDev.
import { expect, test, type Page } from "@playwright/test";

/** Open the page with a script, as the window opens it */
async function openScript(page: Page, text: string, options: { language?: string; wrap?: boolean } = {}) {
  await page.goto("/web/script/index.html");
  await page.waitForFunction(() => !!(window as any).scriptDev?.editor);
  await page.evaluate(() => {
    (window as any).sent = [];
    window.addEventListener("message", (e) => { if (e.data?.channel === "sxt-page") (window as any).sent.push(e.data.message); });
  });
  await page.evaluate(({ text, language, wrap }) => (window as any).scriptDev.message({ type: "set", text, language, wrap, dark: true }),
    { text, language: options.language ?? "spsl", wrap: options.wrap ?? true });
}

/** Each line's words with the colour class Monaco gives them, e.g. "WAITFOR=keyword" */
const tokens = (page: Page, text: string) => page.evaluate((text) => {
  const monaco = (window as any).monaco;
  const lines = text.split("\n");
  return monaco.editor.tokenize(text, "spsl").map((line: { offset: number; type: string }[], i: number) =>
    line.map((t, j) => `${lines[i].slice(t.offset, line[j + 1]?.offset).trim()}=${t.type.replace(/\.spsl$/, "")}`)
      .filter((t: string) => !t.startsWith("=")));
}, text);

test("colours SPSL: commands, options, regular expressions, CASE patterns, labels and placeholders", async ({ page }) => {
  await openScript(page, "");
  const script = [
    "# log in",
    "WAITFOR /t:5 /re /else:none [#$]\\s*$",
    "EXPECT /i",
    '  CASE "(yes/no"',
    "  CASE /assword:/i",
    "    SENDLINE {ad_user} and {secret:Password}",
    "    AGAIN",
    "END",
    ":none",
    "GOTO none",
    "SLEEP 500",
    "SENDKEY ^C",
  ].join("\n");
  expect(await tokens(page, script)).toEqual([
    ["# log in=comment"],
    ["WAITFOR=keyword", "/t:5=attribute.name", "/re=attribute.name", "/else:none=attribute.name", "[#$]\\s*$=regexp"],
    ["EXPECT=keyword", "/i=attribute.name"],
    ["CASE=keyword.flow", '"(yes/no"=string'],
    ["CASE=keyword.flow", "/assword:/i=regexp"],
    ["SENDLINE=keyword", "{ad_user}=variable", "and=string", "{secret:Password}=variable.predefined"],
    ["AGAIN=keyword.flow"],
    ["END=keyword.flow"],
    [":none=type.identifier"],
    ["GOTO=keyword.flow", "none=type.identifier"],
    ["SLEEP=keyword", "500=number"],
    ["SENDKEY=keyword", "^=operator", "C=constant"],
  ]);
});

test("wraps long lines with Wrap on, and not with it off", async ({ page }) => {
  await openScript(page, "SENDLINE " + "x".repeat(400), { wrap: true });
  const wrap = () => page.evaluate(() => (window as any).scriptDev.editor.getOption((window as any).monaco.editor.EditorOption.wordWrap));
  expect(await wrap()).toBe("on");
  await page.evaluate(() => (window as any).scriptDev.message({ type: "wrap", wrap: false }));
  expect(await wrap()).toBe("off");
});

test("inserts at the cursor, and sends each change back to the window", async ({ page }) => {
  await openScript(page, "WAITFOR #\nSENDLINE show clock");
  await page.evaluate(() => (window as any).scriptDev.editor.setPosition({ lineNumber: 2, column: 1 }));
  await page.evaluate(() => (window as any).scriptDev.message({ type: "insert", text: "SLEEP 500\n" }));
  const text = "WAITFOR #\nSLEEP 500\nSENDLINE show clock";
  expect(await page.evaluate(() => (window as any).scriptDev.editor.getValue())).toBe(text);
  await expect.poll(() => page.evaluate(() => (window as any).sent.at(-1))).toEqual({ type: "change", text });
});

test("colours the variables, FAILON, the new waits, SAVE and CALL", async ({ page }) => {
  await openScript(page, "");
  const script = [
    "SET host {1}",
    "PROMPT /var:vlan Which VLAN?",
    "FAILON % Invalid",
    "FAILON OFF",
    "WAITPROMPT /t:5",
    "WAITQUIET 1500",
    "  CASE PROMPT",
    "SAVE /append run-{host}.txt",
    "ECHO done",
    "CALL login",
  ].join("\n");
  expect(await tokens(page, script)).toEqual([
    ["SET=keyword", "host=variable", "{1}=variable"],
    ["PROMPT=keyword", "/var:vlan=attribute.name", "Which VLAN?=string"],
    ["FAILON=keyword", "% Invalid=string"],
    ["FAILON=keyword", "OFF=keyword"],
    ["WAITPROMPT=keyword", "/t:5=attribute.name"],
    ["WAITQUIET=keyword", "1500=number"],
    ["CASE=keyword.flow", "PROMPT=keyword"],
    ["SAVE=keyword", "/append=attribute.name", "run-=string", "{host}=variable", ".txt=string"],
    ["ECHO=keyword", "done=string"],
    ["CALL=keyword.flow", "login=type.identifier"],
  ]);
});

test("hovering a command, an option or a {variable} says what it does", async ({ page }) => {
  await openScript(page, "");
  const help = (line: string, column: number) => page.evaluate(({ line, column }) => (window as any).scriptDev.helpAt(line, column), { line, column });
  expect(await help("  WAITFOR /re /t:5 [#$]$", 4)).toMatch(/^\*\*WAITFOR/);
  expect(await help("  WAITFOR /re /t:5 [#$]$", 16)).toMatch(/^\*\*\/t:seconds/);
  expect(await help("SENDLINE {host} {1}", 12)).toMatch(/^\*\*\{name\}/);
  expect(await help("SENDLINE {host} {1}", 18)).toMatch(/^\*\*\{0\} … \{9\}/);
  expect(await help("SENDLINE {host} {1}", 1)).toMatch(/^\*\*SENDLINE/);
  expect(await help("# just a comment", 3)).toBeNull();
});

test("colours REPEAT and LIMIT, and says what the session's values are", async ({ page }) => {
  await openScript(page, "");
  expect(await tokens(page, "REPEAT 5\nLIMIT 10m\nEND")).toEqual([
    ["REPEAT=keyword.flow", "5=number"],
    ["LIMIT=keyword", "10m=number"],
    ["END=keyword.flow"],
  ]);
  const help = (line: string, column: number) => page.evaluate(({ line, column }) => (window as any).scriptDev.helpAt(line, column), { line, column });
  expect(await help("SENDLINE ssh {host_address}", 16)).toMatch(/^\*\*The session's values/);
  expect(await help("SENDLINE {repeat}", 12)).toMatch(/^\*\*\{repeat\}/);
  expect(await help("REPEAT 5", 2)).toMatch(/^\*\*REPEAT n/);
});

test("colours CHECK, REQUIRE and PAGER, and explains them", async ({ page }) => {
  await openScript(page, "");
  expect(await tokens(page, "CHECK ios /not /re ^Version 16\nREQUIRE /else:old ios nobgp\nREQUIRE\nPAGER /key:ENTER --More--\nPAGER OFF")).toEqual([
    ["CHECK=keyword", "ios=type.identifier", "/not=attribute.name", "/re=attribute.name", "^Version 16=regexp"],
    ["REQUIRE=keyword.flow", "/else:old=attribute.name", "ios nobgp=type.identifier"],
    ["REQUIRE=keyword.flow"],
    ["PAGER=keyword", "/key:ENTER=attribute.name", "--More--=string"],
    ["PAGER=keyword", "OFF=keyword"],
  ]);
  const help = (line: string, column: number) => page.evaluate(({ line, column }) => (window as any).scriptDev.helpAt(line, column), { line, column });
  expect(await help("CHECK ios /not x", 2)).toMatch(/^\*\*CHECK name/);
  expect(await help("CHECK ios /not x", 12)).toMatch(/^\*\*\/not/);
  expect(await help("PAGER /key:ENTER x", 9)).toMatch(/^\*\*\/key/);
});

test("colours COLLECT, FOREACH, IF and /idle:, and explains them", async ({ page }) => {
  await openScript(page, "");
  expect(await tokens(page, "COLLECT bins /re (\\S+\\.bin)$\nFOREACH file IN bins\nIF {file} != {boot}\nIF {version} ~ /^17\\./\nELSE\nEXPECT /t:1800 /idle:60")).toEqual([
    ["COLLECT=keyword", "bins=variable", "/re=attribute.name", "(\\S+\\.bin)$=regexp"],
    ["FOREACH=keyword.flow", "file=variable", "IN=keyword.flow", "bins=string"],
    ["IF=keyword.flow", "{file}=variable", "!==operator", "{boot}=variable"],
    ["IF=keyword.flow", "{version}=variable", "~=operator", "/^17\\./=regexp"],
    ["ELSE=keyword.flow"],
    ["EXPECT=keyword", "/t:1800=attribute.name", "/idle:60=attribute.name"],
  ]);
  const help = (line: string, column: number) => page.evaluate(({ line, column }) => (window as any).scriptDev.helpAt(line, column), { line, column });
  expect(await help("FOREACH f IN bins", 3)).toMatch(/^\*\*FOREACH/);
  expect(await help("EXPECT /t:10 /idle:5", 16)).toMatch(/^\*\*\/idle/);
});
