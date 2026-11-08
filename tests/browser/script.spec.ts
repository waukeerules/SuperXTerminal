// The script box's page (public/web/script) on its own, as Edit Button and the Script Editor show it in an iframe:
// SPSL's and Rhai's colours (Monaco's own tokenizer, run on a script), going to a line, Wrap, inserting at the cursor, and each edit sent back
// to the window, and a theme of the user's own's colours. The main window is stood in for by the page itself: host/webview.js posts to window.parent, which
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

/** Each line's words with the colour class Monaco gives them in a language, e.g. "WAITFOR=keyword" */
const tokens = (page: Page, text: string, language = "spsl") => page.evaluate(({ text, language }) => {
  const monaco = (window as any).monaco;
  const lines = text.split("\n");
  return monaco.editor.tokenize(text, language).map((line: { offset: number; type: string }[], i: number) =>
    line.map((t, j) => [lines[i].slice(t.offset, line[j + 1]?.offset).trim(), t.type.replace(new RegExp(`\\.${language}$`), "")])
      // white space has no text; an = operator does
      .filter(([word]: string[]) => word !== "")
      .map(([word, type]: string[]) => `${word}=${type}`));
}, { text, language });

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

test("colours Rhai: its keywords, the tab's functions, session, strings with escapes and ${...}, numbers and comments", async ({ page }) => {
  await openScript(page, "", { language: "rhai" });
  const script = [
    "// the prompt",
    "let n = 0x1F + 2.5;",
    'if !wait_for("p\\tw:", #{ timeout: 10 }) { throw "none"; }',
    "send_line(`ssh ${session.host_address}`);",
    "for row in rows { n += 1; } /* a /* nested */ comment */",
    "fn must(found) { return found == true; }",
  ].join("\n");
  expect(await tokens(page, script, "rhai")).toEqual([
    ["// the prompt=comment"],
    ["let=keyword", "n=identifier", "==operator", "0x1F=number", "+=operator", "2.5=number", ";=delimiter"],
    ["if=keyword.flow", "!=operator", "wait_for=type.identifier", "(=delimiter.parenthesis", '"p=string', "\\t=string.escape", 'w:"=string',
      ",=delimiter", "#{=delimiter.bracket", "timeout=identifier", ":=delimiter", "10=number", "}=delimiter.curly", ")=delimiter.parenthesis",
      "{=delimiter.curly", "throw=keyword.flow", '"none"=string', ";=delimiter", "}=delimiter.curly"],
    ["send_line=type.identifier", "(=delimiter.parenthesis", "`ssh=string", "${=delimiter.bracket", "session=variable.predefined", ".=delimiter",
      "host_address=identifier", "}=delimiter.bracket", "`=string", ")=delimiter.parenthesis", ";=delimiter"],
    ["for=keyword.flow", "row=identifier", "in=keyword.flow", "rows=identifier", "{=delimiter.curly", "n=identifier", "+==operator", "1=number",
      ";=delimiter", "}=delimiter.curly", "/* a /* nested */ comment */=comment"],
    ["fn=keyword", "must=identifier", "(=delimiter.parenthesis", "found=identifier", ")=delimiter.parenthesis", "{=delimiter.curly",
      "return=keyword.flow", "found=identifier", "===operator", "true=constant", ";=delimiter", "}=delimiter.curly"],
  ]);
});

test("a Rhai script takes Rhai's colours, and Send lines none, as the window switches", async ({ page }) => {
  await openScript(page, "send_line(\"x\");", { language: "rhai" });
  const language = () => page.evaluate(() => (window as any).scriptDev.editor.getModel().getLanguageId());
  expect(await language()).toBe("rhai");
  await page.evaluate(() => (window as any).scriptDev.message({ type: "language", language: "plain" }));
  expect(await language()).toBe("plaintext");
  await page.evaluate(() => (window as any).scriptDev.message({ type: "language", language: "spsl" }));
  expect(await language()).toBe("spsl");
});

test("hovering one of the tab's functions in a Rhai script, or session, says what it does", async ({ page }) => {
  await openScript(page, "", { language: "rhai" });
  const help = (line: string, column: number) => page.evaluate(({ line, column }) => (window as any).scriptDev.rhaiHelpAt(line, column), { line, column });
  expect(await help('if !wait_for("#", 10) {', 7)).toMatch(/^\*\*wait_for\(/);
  expect(await help("send_line(`ssh ${session.host_address}`);", 3)).toMatch(/^\*\*send_line/);
  expect(await help("send_line(`ssh ${session.host_address}`);", 20)).toMatch(/^\*\*session/);
  // a variable, a method of the same name, and the language's own words have none
  expect(await help("let status = 1;", 7)).toMatch(/^\*\*status/);
  expect(await help("x.status();", 4)).toBeNull();
  expect(await help("let n = 1;", 2)).toBeNull();
});

test("goes to a line when the window asks (a problem clicked under the box): the cursor at its start, in view", async ({ page }) => {
  const lines = Array.from({ length: 200 }, (_, i) => `SENDLINE line ${i + 1}`).join("\n");
  await openScript(page, lines);
  await page.evaluate(() => (window as any).scriptDev.message({ type: "reveal", line: 150 }));
  const where = await page.evaluate(() => {
    const editor = (window as any).scriptDev.editor;
    const ranges = editor.getVisibleRanges();
    return { position: editor.getPosition(), from: ranges[0].startLineNumber, to: ranges[ranges.length - 1].endLineNumber };
  });
  expect(where.position).toMatchObject({ lineNumber: 150, column: 1 });
  expect(where.from).toBeLessThanOrEqual(150);
  expect(where.to).toBeGreaterThanOrEqual(150);
});

test("colours env as session is, and hovering it says it's Environment Variables", async ({ page }) => {
  await openScript(page, "", { language: "rhai" });
  expect(await tokens(page, "send_line(env.ad_user);", "rhai")).toEqual([
    ["send_line=type.identifier", "(=delimiter.parenthesis", "env=variable.predefined", ".=delimiter", "ad_user=identifier", ")=delimiter.parenthesis", ";=delimiter"],
  ]);
  const help = (line: string, column: number) => page.evaluate(({ line, column }) => (window as any).scriptDev.rhaiHelpAt(line, column), { line, column });
  expect(await help("send_line(env.ad_user);", 12)).toMatch(/^\*\*env\*\*: Environment Variables/);
});

test("is drawn in a theme of your own's colours, and in Monaco's own again for Light", async ({ page }) => {
  await openScript(page, "SEND show version\nSEND show clock");
  /** The editor's background and text, and the colour of a line number other than the cursor's, as drawn */
  const drawn = () => page.evaluate(() => {
    const look = (selector: string) => getComputedStyle(document.querySelector(selector)!);
    return { background: look(".monaco-editor-background").backgroundColor, text: look(".monaco-editor").color,
      lineNumbers: look(".line-numbers:not(.active-line-number)").color };
  });
  const colors = { bgInput: "#882b2b", bgPane: "#bb3e3e", border: "#000000", text: "#ffffff", textMuted: "#cdb1b1", accent: "#6b0778" };
  await page.evaluate((colors) => (window as any).scriptDev.message({ type: "dark", dark: false, colors }), colors);
  await expect.poll(drawn).toEqual({ background: "rgb(136, 43, 43)", text: "rgb(255, 255, 255)", lineNumbers: "rgb(205, 177, 177)" });

  await page.evaluate(() => (window as any).scriptDev.message({ type: "dark", dark: false, colors: null }));
  await expect.poll(async () => (await drawn()).background).toBe("rgb(255, 255, 254)");
});
