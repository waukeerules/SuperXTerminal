# Rhai: scripts with a real language

[Rhai](https://rhai.rs/book) is SuperXTerminal's second script language, beside [SPSL](SPSL.md). SPSL is one command
per line, made for typing and waiting. Rhai is a small programming language: variables, arithmetic, `if`, loops,
lists, functions of your own. SuperXTerminal gives it the same reach into a tab that SPSL has: type, wait for the
device, ask you something, put a line in the status bar. Use it when a script needs to decide, count or remember;
keep SPSL for a login or a few commands.

This page is what a Rhai script can do in SuperXTerminal, written so a person or an LLM can write a working script
from it. Read [What a script can read](#what-a-script-can-read-session-env-and-its-own) before using a session's
values or Environment Variables: `session` and `env` are different things, and mixing them up is the usual mistake.

It's a prototype: what's here works and is tested, and [Not there yet](#not-there-yet) lists what SPSL does that Rhai
can't yet. The code is `src-tauri/src/rhai_scripts.rs` (the script runs there) and `src/lib/scripting/rhai.ts` (the
tab's half); examples are in [rhai-examples](rhai-examples).

## Where scripts run

| From | How | Sees from the start of |
|---|---|---|
| A button (button bar, Edit Button > Action: Run Rhai script) | Click: the active tab. Shift+click: the Send Commands selection | the cursor's line |
| Script Editor (Tools > Script Editor), Run as: Rhai script | Send: the Send Commands selection | the cursor's line |

Not yet as a session's own script or a profile's (those are SPSL only).

**Before it runs**, it's checked, as you write it (the problems under the script box) and again when you press OK
or Send:
- **A syntax error, or a variable used before it's set** (a misspelt name): Send won't run it; OK asks before saving.
- **An `env.name` with no value** in Environment Variables: you're offered Environment Variables, as SPSL does for a
  missing `{name}`, and nothing is typed.
- **`env.host_address`** (a session value read as an Environment Variable): it doesn't run, and says to write
  `session.host_address`.

A function name spelt wrong (`send_lin`) isn't found until that line runs: Rhai only looks functions up then.

**While it runs**, the status bar shows its line and what it's doing (`wait_for "#"`), and **Stop** ends it at once,
in a wait, a `sleep` or a loop that never waits.

## How a script sees the output

The same as SPSL; see [SPSL's](SPSL.md#how-a-script-sees-the-output) for more.

- **What a wait looks at.** `wait_for`, `wait_for_any` and `wait_prompt` look at the tab's output **since a mark**.
  `send` and `send_line` move the mark to the end of the output just before they type, so the next wait sees only
  what came after. A match moves the mark to just after it, so the next wait looks further on.
- **The echo counts.** What's typed is echoed back, and the echo is output: after `send_line("echo DONE")`,
  `wait_for("DONE")` matches the echoed command at once. Wait for something only the answer has: start a regular
  expression with `^` (the echo starts with the prompt), or split a marker (`echo DO''NE` prints `DONE`).
- **The text is what's on screen**: the terminal's lines joined with `\n`, no colour codes, no `\r`. The cursor's line
  (usually the prompt) is last, with no `\n` after it.
- **Matching.** Text is matched exactly, case and all, anywhere; `any_case: true` ignores case. A regular expression
  is JavaScript's, with the `m` flag: `^` and `$` are each line's start and end.
- **The first wait** sees the cursor's line and after: a prompt already on screen matches at once.
- **A wait that times out doesn't stop the script** (SPSL's WAITFOR does): it gives `false` (or `-1`), and the script
  goes on. Check it, or the next line types into the wrong prompt:

```rhai
if !wait_for("assword:", 10) {
    throw "no password prompt";
}
```

**When the session drops** or the tab reconnects, the script stops at once, as SPSL's does ("the session ended").

## What a script can read: session, env, and its own

A script has three kinds of values. They're kept apart on purpose:

| Written | What it is | Comes from |
|---|---|---|
| `session.host_address` | **The tab's session**: where this tab is connected | The Sessions tree (or what Quick connect was typed), as the script starts |
| `env.ad_user` | **Environment Variables**: your own values, the same for every tab | Tools > Environment Variables, secrets included |
| `let host = "sw1";` | **The script's own variables** | The script |

And `matched()`: what the last wait found.

### `session`: the tab's session

| Name | Holds |
|---|---|
| `session.host_address` | The session's host |
| `session.host_port` | Its port (the protocol's usual one when it has none: 22 for SSH) |
| `session.session_name` | Its name in the Sessions tree |
| `session.session_id` | Folder/name |
| `session.session_folder` | Its folder |
| `session.session_username` | Its username |
| `session.session_profile` | Its profile |
| `session.session_protocol` | SSH, TELNET… |

- These are SPSL's `{host_address}`, `{session_name}`… under `session.`.
- **Any case**: `session.HOST_ADDRESS` works.
- **A Local Shell tab has none**; a Quick connect tab has the host and login typed. Reading one the tab hasn't
  **stops the script**, saying so, rather than typing nothing (`ssh ` with no host):
  `this tab has no session.host_address: it isn't a saved session's (a Local Shell tab, or Quick connect without it)`.
- **A name that isn't one of these** stops it too, listing the right ones: `session has no hostname: its values are…`.
- **`session.has("host_address")`** asks without stopping:

```rhai
let target = if session.has("host_address") { session.host_address } else { prompt("Which host?") };
send_line(`ssh ${target}`);
```

### `env`: Environment Variables

```rhai
send_line(env.ad_user);
wait_for("assword:");
send_line(env.ad_password);
```

- Each is the Environment Variable of that name, as SPSL's `{ad_user}`. `env["ad_user"]` is the same, and takes a name
  worked out as the script runs (`env[name]`).
- **Any case**: `env.AD_User` finds `ad_user`.
- **Secrets work**, and are safe: the values are filled in where the script runs, in the app's core, when it starts.
  A secret's value never passes through the window, and isn't shown. (`echo` it on a device and it is, of course.)
- **One with no value**: before the script runs you're offered Environment Variables, and nothing runs. One the check
  can't see beforehand (`env[name]`) stops the script when it's reached:
  `the Environment Variable jump_host has no value: define it in Environment Variables`.
- **`env.has("jump_host")`** asks without stopping, for an optional one.
- **Not a session value.** `env.host_address` is an error, not an Environment Variable: `host_address` is the tab's
  session's, and Environment Variables won't take that name. Write `session.host_address`.

### The difference, side by side

| You want | Write | Not |
|---|---|---|
| The host this tab is connected to | `session.host_address` | ~~`env.host_address`~~ |
| The username this session logs in with | `session.session_username` | ~~`env.session_username`~~ |
| Your own login, the same for every device | `env.ad_user` | ~~`session.ad_user`~~ |
| A jump host you set up once | `env.jump_host` | |
| Something to ask each time | `prompt("VLAN?")` or `password("Password")` | ~~`{prompt:VLAN}`~~ |

**No `{name}` placeholders in Rhai.** Braces are Rhai's own (blocks and `#{ maps }`), so SPSL's `{ad_user}`,
`{prompt:Name}` and `{secret:Name}` aren't filled in. Use `env.ad_user`, `prompt("Name")` and `password("Name")`.

Both `session` and `env` work everywhere, inside your own functions too. Don't name a variable of your own `session`
or `env`.

## Functions

The functions SuperXTerminal adds. The script panel beside the script lists them (double-click inserts one), and
hovering one in the script box says what it does.

### Typing

| Function | Does |
|---|---|
| `send_line(text)` | Types the text, then Enter. |
| `send(text)` | Types the text, without Enter. In `"double quotes"`, `\r` is Enter, `\t` Tab, `\x03` Ctrl+C. |

### Waiting

| Function | Gives |
|---|---|
| `wait_for(text)` | `true` when the text appears (up to 30 s), `false` if it doesn't |
| `wait_for(text, 10)` | The same, up to 10 seconds; `0` waits for as long as it takes |
| `wait_for(text, #{ timeout: 10, regex: true, any_case: true })` | The same with options, any of them: `timeout` (seconds), `regex` (a regular expression), `any_case` |
| `wait_for_any(["#", ">", "denied"])` | Whichever text appears first: its index (`0` for the first), or `-1` if none comes in 30 s. Texts only, not regular expressions |
| `wait_for_any([…], 10)` | The same, up to 10 seconds |
| `wait_prompt()` | `true` when the device's prompt comes (a last line ending in `>`, `#`, `$` or `%`), up to 30 s |
| `wait_prompt(10)` | The same, up to 10 seconds |
| `matched()` | What the last wait found, as a list: `matched()[0]` the whole match (the prompt line, for `wait_prompt`), `matched()[1]`… a regular expression's groups (`""` for one that didn't take part) |

### Asking and telling

| Function | Does |
|---|---|
| `prompt("question")` | Asks; gives the answer, or `""` when cancelled |
| `password("question")` | Asks, hidden; gives the answer. Cancel stops the script |
| `status("text")` | A line in the status bar. `print("text")` does the same; `debug(value)` shows any value |
| `sleep(500)` | Waits 500 ms. For pacing; to wait for a device, wait for its prompt |
| `throw "message"` | Stops the script, with the message in the status bar |

## The language, in brief

Rhai is close to JavaScript and Rust; [the Rhai book](https://rhai.rs/book) has all of it. What device scripts need:

```rhai
// a comment; /* this too */
let count = 0;                       // a variable; it must be set before it's used
const LIMIT = 5;                     // a constant
let names = ["Gi1/0/1", "Gi1/0/2"];  // a list: names[0], names[-1] (the last), names.len, names.push("x")
let device = #{ host: "sw1", port: 22 };  // a map: device.host

if count < LIMIT && names.len > 0 {
    count += 1;
} else {
    status("enough");
}

for name in names {                  // once per item; for n in 1..=5 counts 1 to 5
    send_line(`show interface ${name} status`);
    wait_prompt();
}

let i = wait_for_any(["#", ">"], 10);
switch i {
    0 => status("enable mode"),
    1 => status("user mode"),
    _ => { throw "no prompt"; }      // throw needs braces here
}

fn interface_line(name) {           // a function of your own: it sees its arguments, session and env,
    `interface ${name}`              //   not the script's own variables; its last line is what it gives
}
send_line(interface_line("Gi1/0/1"));
```

**Text:**
- `"double quotes"` take escapes (`\r`, `\t`, `\x03`, `\"`).
- `` `backquotes` `` take `${expression}` (`` `ssh ${session.host_address}` ``) and no escapes, so a regular expression
  is written as it is: `` `^(\S+)\s+up` ``. They can run over lines.
- `+` joins text, and text with a number: `"VLAN " + 10`.
- `text.contains("x")`, `starts_with`, `ends_with`, `split(",")` (a list), `to_upper()`, `to_lower()`, `len`,
  `sub_string(start, length)`, `"x" in text`; `parse_int("42")` makes a number.
- **`trim()` and `replace(a, b)` change the variable itself and give nothing back**: write `name.trim();` on its own
  line, not `let t = name.trim();` (that's empty).

## Patterns that work

### Start from a prompt

A prompt already on screen counts; if there's none, Enter brings one.

```rhai
if !wait_prompt(3) {
    send_line("");
    if !wait_prompt(10) {
        throw "no prompt";
    }
}
let prompt = matched()[0];
prompt.trim();
status(`at ${prompt}`);
```

### Wait for the prompt after every command

After a command's answer, the device still prints its prompt. Wait for it before typing the next command, or the next
wait may find the old one. A function keeps that short:

```rhai
fn run(command) {
    send_line(command);
    if !wait_prompt(30) {
        throw `no prompt after "${command}"`;
    }
}
run("terminal length 0");
run("show clock");
```

### Pick a value out of an answer

```rhai
send_line("show version");
let version = "?";
// ends at a comma, so a line still arriving isn't taken half-read
if wait_for(`, Version ([^\s,]+),`, #{ regex: true, timeout: 15 }) {
    version = matched()[1];
}
wait_prompt();
status(`version ${version}`);
```

### A login whose prompts come in any order

```rhai
loop {
    let i = wait_for_any(["(yes/no", "sername:", "assword:", "#", ">"], 20);
    if i == 0 { send_line("yes"); }
    else if i == 1 { send_line(env.ad_user); }
    else if i == 2 { send_line(password("Password")); }
    else if i == 3 || i == 4 { break; }
    else { throw "the login didn't finish"; }
}
```

### Configuration that stops at the first refusal

```rhai
let commands = ["interface Gi1/0/5", "description uplink", "end"];
send_line("configure terminal");
if wait_for_any(["(config)#", "% "], 10) != 0 {
    throw "configure terminal was refused";
}
for command in commands {
    send_line(command);
    // a % line is a Cisco refusal; a prompt is the device ready for the next
    if wait_for_any(["% ", "#"], 10) != 1 {
        throw `"${command}" was refused`;
    }
}
```

### Every line of a table

One pattern for a row, or (another group) the prompt that ends the table, so the loop ends on the prompt rather than
waiting for a timeout. Each wait moves past what it found, so the next finds the next row:

```rhai
send_line("show ip interface brief");
let row = `^(\S+)\s+(\S+)\s+(?:YES|NO)\s+\S+\s+(up|down|administratively down)\s+(up|down)[ \t]*\r?\n|^([\w.-]+#)[ \t]*$`;
let up = 0;
loop {
    if !wait_for(row, #{ regex: true, timeout: 15 }) {
        throw "the table didn't end with the prompt";
    }
    let m = matched();
    if m[5] != "" { break; }
    if m[3] == "up" && m[4] == "up" { up += 1; }
}
status(`${up} interfaces up`);
```

### Paging (`--More--`)

There's no PAGER yet: turn the device's paging off for this login first (`terminal length 0` on Cisco, `| cat` or
`--no-pager` on Linux).

## SPSL and Rhai

| SPSL | Rhai |
|---|---|
| `SENDLINE text` | `send_line("text");` |
| `SENDCHAR text` | `send("text");` |
| `SENDKEY ^C` | `send("\x03");` |
| `WAITFOR text` (stops on timeout) | `if !wait_for("text") { throw "…"; }` |
| `WAITFOR /t:10 /i /re …` | `wait_for(…, #{ timeout: 10, any_case: true, regex: true })` |
| `WAITFOR /else:continue text` | `wait_for("text");` (its answer not checked) |
| `EXPECT … CASE … END` | `wait_for_any([…])` and `switch` or `if` |
| `WAITPROMPT` | `wait_prompt()` |
| `SET name value`, `{name}` | `let name = "value";`, `name` (or `` `${name}` `` in text) |
| `{0}`, `{1}` | `matched()[0]`, `matched()[1]` |
| `{host_address}`, `{session_name}`… | `session.host_address`, `session.session_name`… |
| `{ad_user}` (an Environment Variable) | `env.ad_user` |
| `PROMPT question`, `PWDPROMPT` | `prompt("question")`, `password("Password")` |
| `{prompt:Name}`, `{secret:Name}` | `prompt("Name")`, `password("Name")` |
| `ECHO text` | `status("text");` |
| `STOP message` | `throw "message";` |
| `SLEEP 500` | `sleep(500);` |
| `IF … ELSE … END`, `REPEAT`, `FOREACH`, `GOTO` | `if … else`, `for`, `while`, `loop`, functions |
| `COLLECT` | A loop of waits, as [Every line of a table](#every-line-of-a-table) |

## Not there yet

What SPSL has that Rhai scripts don't yet: running as a session's or a profile's own script; Step (a line at a time);
FAILON; PAGER; WAITQUIET; SAVE; CALL (Saved scripts > Insert puts a saved script's text in instead); LIMIT (Stop
ends one); CHECK and REQUIRE; SENDBREAK; OPENSESSION.

## Pitfalls

- **A wait that times out doesn't stop the script.** Check what `wait_for` gives, or `throw`.
- **`session` and `env` are different.** The tab's host is `session.host_address`; `env` is Environment Variables.
- **`trim()` and `replace()` give nothing back**: they change the variable itself.
- **The echo of what you typed is output.** Anchor with `^`, or wait for something only the answer has.
- **`#`, `>` and `$` anywhere.** `wait_for("#")` matches the first `#` in any output; `wait_prompt()` or an anchored
  regular expression is safer.
- **Regular expressions in `"double quotes"`** need every `\` doubled (`"\\d+"`). Write them in `` `backquotes` ``.
- **Your own functions** see their arguments, `session` and `env`, not the script's `let` variables: pass what they
  need.
- **A secret `echo`ed or typed into a command** is shown by the device and kept in its history, as it would be if you
  typed it.
- **Line numbers** in messages are the script's own lines, comments and blank lines counted, as the editor shows them.

## Checklist before handing a script over

1. No problems under the script box.
2. Starts by waiting for a prompt.
3. Every `wait_for` that must succeed is checked (`if !wait_for(…) { throw …; }`).
4. A prompt is waited for after every command, before the next.
5. The tab's values come from `session`, your own from `env`; no `{name}` placeholders.
6. No secrets written in the script: `env` (a secret Environment Variable) or `password()`.
7. Paging is off before a command with a long answer.
8. Every loop has a way out (`break`, a `throw`, or a list that ends).

## Examples

- [rhai-examples/cisco-tour.rhai](rhai-examples/cisco-tour.rhai): a Cisco switch, read only: enable, `show version`,
  config mode in and out, `show ip interface brief` counted.
- [rhai-examples/linux-values.rhai](rhai-examples/linux-values.rhai): what a script sees of `session` and `env`, each
  echoed on a Linux host and read back.
