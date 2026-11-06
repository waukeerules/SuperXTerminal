# SPSL: scripts that drive a terminal tab

SPSL (SuperPuTTY Script Language) is how SuperXTerminal types into a session and reacts to what comes back: log in,
answer prompts, run commands, wait for the device. It started as SuperPuTTY's and SuperXPuTTY's; SuperXTerminal runs
everything they did and adds EXPECT, regular expressions, any-case matching, timeout handling, labels, GOTO, STOP,
variables (the session's own too), FAILON, WAITPROMPT, WAITQUIET, ECHO, SAVE, CALL, REPEAT, LIMIT, CHECK and
REQUIRE (prerequisites), PAGER, COLLECT, FOREACH, IF and ELSE, `/idle:` and stepping a line at a time.

This page is the whole language, written so a person or an LLM can write a working script from it alone. If you're
an LLM writing a script: read [How a script sees the output](#how-a-script-sees-the-output) and
[Patterns that work](#patterns-that-work) before anything else, and finish with the
[checklist](#checklist-before-handing-a-script-over).

For scripts that need variables, arithmetic, loops or functions of their own, there's also Rhai, a small programming
language: see [RHAI.md](RHAI.md), which has a table of SPSL's commands and their Rhai equivalents.

The code is `src/lib/scripting/spsl.ts` (parser, compiler, runner, checker), with its tests in `spsl.test.ts`. The
script box's colours are `public/web/script/script.js`.

## Where scripts run

| From | How | Sees from the start of | Placeholders filled first |
|---|---|---|---|
| A button (button bar, Action: Run SPSL script) | Click: the active tab. Shift+click: the Send Commands selection | the cursor's line | Yes |
| Script Editor (Tools > Script Editor), Run as: SPSL script | Send: the Send Commands selection | the cursor's line | Yes |
| A session's own script (Edit Session > Terminal > Login script: a file, a path in the settings folder, or a URL) | Runs as the session starts, if the file starts with `#!/bin/spsl` | everything since the session started | No |
| A profile's script (Settings > Profiles > Terminal > Login script, named the same way), for its sessions that have no script of their own | The same as a session's own | everything since the session started | No |

The Login script fields take Rhai scripts too ([RHAI.md](RHAI.md#where-scripts-run)): one starting `#!/bin/rhai`, or
named `.rhai`.

**A profile's script** lets a session be just a name and a host: the profile logs in and sets up, for every session
that uses it (or whose folder does). A session's own script wins over its profile's; a profile with none uses Default
Settings'. It starts as the connection does, so begin with WAITPROMPT to run once the login is done:

```
#!/bin/spsl
WAITPROMPT /t:30
SENDLINE echo "Logged in to {session_name} at {host_address}"
WAITPROMPT
```

**A jump server profile.** Tick **Log in here** on a profile (Settings > Profiles, beside Host) and its sessions' tabs
log in to the profile's Host, with the profile's username, password and key, rather than to the session's host. The
session's host, username and port are left to the profile's script, as `{host_address}`, `{session_username}` and
`{host_port}`, so it can go on from the jump server:

```
#!/bin/spsl
WAITPROMPT /t:30
SENDLINE ssh {session_username}@{host_address}
EXPECT /t:40
  CASE "(yes/no"
    SENDLINE yes
    AGAIN
  CASE /password:/i
    PWDPROMPT
    AGAIN
  CASE /^([\w.-]+)[>#]\s?$/
END
```

A session is then just a name, the target's address and your username there. `docs/spsl-examples/cisco-jump.spsl`
does it for a Cisco switch, and `jump-host.spsl` for a Linux host. (A profile's **Jump host** field is the other way
through a jump server: ssh's own `-J`, with both logins made from this computer and no script.)

Each run belongs to one tab. With several tabs selected, the same script runs in each, separately, each with its own
variables. The status bar shows "name (tab): line 4/40 · WAITFOR … · 12s" while it runs, with Stop, and a line when it
ends saying how. Saved scripts live in the settings folder's `scripts` folder (`.spsl` for SPSL, `.txt` for plain lines
to send).

**Stepping.** The Script Editor's **Step** runs the script a line at a time: it pauses before each line, and **Next
line** in the status bar (or its menu) runs it. Stop ends it. Use it to try a new script on a real device.

**In the editor.** Hovering a command, an option or a `{variable}` in the script box says what it does.

## How a script sees the output

This is what makes scripts work or not.

- **What a wait looks at.** WAITFOR and EXPECT look at the tab's output **since a mark**. Every line that types
  (SENDLINE, SENDCHAR, SENDKEY, PROMPT, PWDPROMPT) moves the mark to the end of the output just before it types, so
  the next wait sees only what came after. A match moves the mark to just after it, so the next wait looks further on.
- **The echo counts.** What's typed is echoed back by the device, and the echo is output. After `SENDLINE echo DONE`,
  `WAITFOR DONE` matches the echoed command line itself, at once, before the command has run. Wait for something only
  the device's answer contains (see [markers](#markers-wait-for-the-answer-not-the-echo)).
- **The text is what's on screen.** The output is the terminal's lines as drawn, joined with `\n`: no colour codes,
  no `\r`. The cursor's line (usually the prompt) is the last line, without a `\n` after it.
- **Matching.** Plain text is matched exactly (case and all) anywhere in that output; `/i` ignores case. A regular
  expression is JavaScript's, always with the `m` flag (`^` and `$` are each line's start and end), plus `i` with
  `/i`. A match of no characters (`x*`) doesn't count.
- **Polling.** A wait looks every 100 ms until it matches, its time runs out (30 s unless `/t:` says), or Stop.
- **A button's first wait** sees the cursor's line and after: a prompt already on screen matches at once, older
  output doesn't. A session's own script's first wait sees everything since the session started.

## When the session drops

A script belongs to the connection it started on:
- **Started on a tab whose session is down:** it doesn't start ("the session isn't connected").
- **The session ends while it runs:** it stops at once, during a wait too, without waiting out the wait's time ("stopped at line N: the session ended"). `/else:` doesn't apply: it's for a device that doesn't answer, not for a session that's gone.
- **The tab reconnects, by itself or by Reconnect:** the script stops instead of typing into the new connection, which is back at a login prompt rather than where the script left it ("the session reconnected, so the script stopped…"). Run it again once the tab is logged in.

A session that's still connected but has stopped answering (a hung device, a lost route with no keep-alives yet) looks like a quiet device: the next wait times out, as its `/t:` and `/else:` say. Keep `/t:` realistic for the device.

## Lines

- One command per line: `COMMAND arguments`. Command words are case-insensitive (`sendline` works); arguments are
  taken as written, after one space.
- Blank lines, and lines starting with `#` (after any spaces), are comments. `#!/bin/spsl` on the first line marks a
  file as SPSL (a session's own script needs it).
- `:name` on its own line is a label (one word).
- Indenting is free; use it to show EXPECT's cases.
- A line with an unknown command word is skipped when the script runs (as SuperXPuTTY did), and Check script reports
  it. Don't rely on that: a skipped line is a bug.

## Commands

### Typing

| Command | Does |
|---|---|
| `SENDLINE text` | Types the text, then Enter, then waits 50 ms. `SENDLINE` alone presses Enter. |
| `SENDCHAR text` | Types the text, no Enter. |
| `SENDKEY key` | Presses one key; see [Keys](#keys-for-sendkey). |
| `PROMPT question` | Asks you for a value (a window with the question), types it, then Enter. Cancel types an empty line. |
| `PROMPT /var:name question` | Asks, and keeps the answer in `{name}` instead of typing it. |
| `PWDPROMPT` | Asks for a password (hidden), types it, then Enter. Cancel stops the script. Never shown or logged by the script. |
| `PWDPROMPT /var:name` | Asks for a password, and keeps it in `{name}` (for typing later, e.g. after a jump host). |

`{variables}` in the text are filled in as the line runs ([Variables](#variables)). A line feed in the text (from a
value) is typed as Enter.

### Waiting

| Command | Does |
|---|---|
| `SLEEP ms` | Waits that many milliseconds (a whole number above 0). |
| `WAITFOR [options] text` | Waits until the text appears in the output since the mark. |
| `WAITPROMPT [/t:s] [/else:…]` | Waits for the device's prompt: the output's last line (the cursor's) starting with something other than a space and ending in `>`, `#`, `$` or `%` (and maybe one space): `sw1>`, `sw1(config-if)#`, `user@host:~$`. `SET prompt` to a regular expression puts another in its place (`SET prompt ^Router>\s$`). |
| `WAITQUIET ms [/t:s] [/else:…]` | Waits until the output has stopped changing for `ms` milliseconds: a command's whole answer, when its end can't be named. |
| `EXPECT [options]` … `END` | Waits for whichever of several things comes first; see [EXPECT](#expect). |

What a wait matched is kept as `{0}`, and a regular expression's groups as `{1}`…`{9}` and `{name}` (for
`(?<name>…)`); see [Variables](#variables).

The options come before the text, each followed by a space, in any order (WAITPROMPT and WAITQUIET take `/t:` and
`/else:`; FAILON `/i` and `/re`; PROMPT and PWDPROMPT `/var:`; SAVE `/append`):

| Option | Means |
|---|---|
| `/t:10` | Wait up to 10 seconds (default 30). |
| `/i` | Ignore case: `WAITFOR /i password:` matches `Password:`. |
| `/re` | The text is a regular expression: `WAITFOR /re [>#]\s*$`. (WAITFOR only; EXPECT's cases say it themselves.) |
| `/else:stop` | When the time runs out, stop the script, saying what didn't appear. This is the default. |
| `/else:continue` | When the time runs out, carry on with the next line (after END, for EXPECT). |
| `/else:label` | When the time runs out, go on from the line `:label`. |
| `/idle:60` | End the wait when no new output has come for 60 s, whatever `/t:` says; `/else:` decides what then ("no new output for 60 s (/idle:): stalled?"). For long transfers: a big `/t:` with a small `/idle:` waits as long as progress keeps coming. |
| `/var:name` | (PROMPT, PWDPROMPT) Keep the answer in `{name}` instead of typing it. |
| `/append` | (SAVE) Add to the file instead of replacing it. |

The text is the rest of the line, spaces included, exactly as written (no quotes needed, and quotes would be part of
the text). Something starting with `/` that isn't one of the options is the text itself (`WAITFOR /tmp` waits for
"/tmp").

### Flow

| Command | Does |
|---|---|
| `:name` | A label: a place to go to. One word; each used once. |
| `GOTO name` | Goes on from the line `:name`. |
| `STOP message` | Ends the script here; the status bar says "stopped at line N: message". `STOP` alone, without a message. |
| `EXPECT` / `CASE` / `AGAIN` / `END` | See [EXPECT](#expect). |
| `FAILON [/i] [/re] text` | From this line on, any wait (WAITFOR, WAITPROMPT, WAITQUIET, EXPECT) that sees the text stops the script, naming the device's line: "the device said "% Invalid input detected…" (FAILON)". Several can be in force; one that appears at the same place as what the wait waits for wins. `FAILON OFF` forgets them all. Only waits look: put a wait after each command. |
| `REPEAT n` … `END` | Runs the lines up to its END n times (1 to 100,000); `{repeat}` is the round, 1, 2…, and an inner REPEAT's round is its own (the outer's is `{repeat}` again after it). REPEATs can go inside each other, an EXPECT's CASE, or the other way round. |
| `LIMIT 90s` / `LIMIT 10m` / `LIMIT 1h` | The whole script (from when it started) may run that long, waits and SLEEPs included; then it stops, "the script's LIMIT of 10 min ran out". A number alone is seconds. |
| `CHECK name [/not] [/i] [/re] text` | Defines a prerequisite: watched in **all** the output from this line on, even what waits have moved past. REQUIRE judges it. `/not`: met when the text does **not** appear. Once met, `{name}` is what it matched (its first group, if it has one). See [Prerequisites](#prerequisites). |
| `REQUIRE [/else:…] [names]` | Stops the script here unless these CHECKs (all of them, with no names) are met, saying which isn't and why: "prerequisite "ios" isn't met: /Version 17\./ didn't appear". `/else:label` goes there instead, `/else:continue` carries on. |
| `PAGER [/key:NAME] [/i] [/re] text` | From this line on, a wait (any of them) that sees the pager's prompt presses Space (or `/key:`'s key, as SENDKEY writes it) and keeps waiting, so a long answer pages through by itself. Each prompt is answered once; one at the same place as what the wait waits for is answered first (`<--- More --->` ends in `>` and would look like a prompt). `PAGER OFF` forgets them. |
| `IF test` … `ELSE` … `END` | Runs the lines up to ELSE (or END) when the test holds, else those after ELSE. A test is `a == b`, `!=`, `<`, `>`, `<=`, `>=` (as numbers when both sides are numbers, else as text), `a ~ /regex/` (matches; `/regex/i` for any case) or `!~`, the operator with a space each side; or `a` alone (true when not empty). `{variables}` with no value are empty. ELSE is optional; IFs go inside each other. |
| `FOREACH item IN list` … `END` | Runs the lines once per item, `{item}` set to it: a COLLECT's list by its name, or a written one, `10, 20, 30` (with `{variables}`; an empty one runs nothing). |
| `CALL name` | Runs a saved SPSL script (the scripts folder, by name, without `.spsl`) here, with the same variables and FAILONs, then goes on. Its labels are its own; a STOP in it stops everything; messages say "line N of "name"". Calls may go 8 deep. |

### Output

| Command | Does |
|---|---|
| `SET name value` | Sets a variable: `{name}` in later lines is the value (with its own `{variables}` filled in as SET runs). `SET name` alone forgets it. A name starts with a letter or `_`. |
| `ECHO text` | Shows the text in the status bar: "Script "name" in tab: text". |
| `COLLECT name [/i] [/re] text` | Every match in the last command's output (as SAVE takes it), as a list for FOREACH: a regular expression's first group (else its whole match), or each place plain text appears. `{name}` is them all, comma-separated; `{name_count}` how many. |
| `SAVE [/append] file` | Saves what the last command printed: the output since the last line that typed, without its echo (the rest of the line it was typed on) and without a prompt on the last line. A bare name or a relative path goes in `Documents/SuperXTerminal/output` (folders made as needed); `~/…` is the home folder; a full path is used as it is. `/append` adds to the file. |

Reaching the end of the script ends it ("finished"). A loop made with GOTO runs until something stops it: a wait
that times out with the default `/else:stop`, a STOP, the LIMIT, or Stop in the status bar. A loop that goes round
10,000 times without waiting for anything (no wait or SLEEP between its jumps back) is stopped as a runaway, so a
mistake can't type forever.

### Sessions and serial lines

| Command | Does |
|---|---|
| `OPENSESSION Folder/name` | Opens a saved session, by its ID (Saved, then Private, then Database sessions), in a new tab. The script goes on in its own tab; it doesn't drive the new one. |
| `SENDBREAK` | Sends a Break: Serial and Telnet tabs only (to stop a Cisco switch's boot at ROMMON, say). In an SSH tab the script stops here. |
| `CLOSESESSION` | Listed by SPSL but never implemented: the script stops here. Don't use it. |

## EXPECT

```
EXPECT [/t:seconds] [/i] [/else:stop|continue|label]
  CASE pattern
    lines run when this pattern is seen first
  CASE pattern
    lines…
END
```

- **A CASE's pattern** is one of:
  - `"quoted text"`: exact text; `\"` for a quote and `\\` for a backslash inside.
  - `/regular expression/`, or `/regular expression/i` for any case.
  - `PROMPT`: the device's prompt, as WAITPROMPT sees it (`"PROMPT"` for the word itself).
  - plain text to the end of the line (spaces included).
  
  EXPECT's `/i` makes every case ignore case.
- **Which case wins.** At each look, every case is tried against the output since the mark. The one that **starts
  earliest in the output** wins; on a tie, the case written first. So the order of events on screen decides, not the
  order of the cases. Example: `NOOMV-5` contains `OMV-5`, but `CASE OMV-5` can't win against `CASE NOOMV-5` when the
  output says `NOOMV-5`, because `NOOMV-5` starts two characters earlier.
- The mark moves past the winning match, then the lines under that CASE run.
- **`AGAIN`** (under a CASE) waits again, with the same cases and a fresh timeout, from the new mark. It's how one
  EXPECT answers a sequence of prompts in whatever order they come.
- **The end of a CASE's lines** goes on after END. A CASE with no lines just ends the wait.
- **Timeout.** If no case appears in time, `/else:` decides: stop (the default; "none of its cases appeared within
  N s"), continue after END, or go to a label.
- **Nesting.** An EXPECT can go under another's CASE; AGAIN and END belong to the innermost.
- **Structure is checked before the script starts.** An EXPECT without END, a CASE or AGAIN outside one, lines
  between EXPECT and its first CASE, or an EXPECT with no CASE: the script doesn't start ("didn't start: line N: …")
  and nothing is typed.

## Variables

A script's own values, written `{name}` (case doesn't matter) and filled in **as each line runs**:

| Made by | Holds |
|---|---|
| `SET name value` | The value. |
| `PROMPT /var:name question`, `PWDPROMPT /var:name` | Your answer. |
| The tab's session, as the script starts | `{host_address}` and `{host_port}` (its own host and port, the protocol's usual port when it has none), `{session_name}`, `{session_id}` (Folder/name), `{session_folder}`, `{session_username}`, `{session_profile}`, `{session_protocol}`, as the Sessions tree has them. None in a tab without a session (a Local Shell, Quick connect). |
| REPEAT | `{repeat}`: the round, 1, 2… |
| A wait that matched | `{0}`: what it matched. With a regular expression: `{1}`…`{9}`, its groups (`""` for one that didn't take part), and `{name}` for each `(?<name>…)`. Each match replaces the last match's `{0}`…`{9}`. |

- **Where they're filled in:** the text of SENDLINE, SENDCHAR, PROMPT, ECHO, STOP, SET, SAVE, CALL and OPENSESSION,
  and the patterns of WAITFOR, CASE and FAILON (SENDKEY's braces are keys, not variables).
- **In a regular expression** a variable's value is taken as plain text (its `.` or `(` match themselves). Only names
  starting with a letter or `_` are filled in there, so a quantifier like `\d{2}` stays one.
- **Each match replaces the last one's `{0}`…`{9}`** (a WAITPROMPT included): keep a group you need with SET straight
  after the wait that matched it.
- **A `{name}` with no value** stays as written (in IF and FOREACH it's empty). For a button or the Script Editor, the script's own variables are left
  alone when Environment Variables are filled in (below), so they don't need defining there.

```
WAITFOR /re ^(?<host>[\w.-]+)[>#]\s*$
SENDLINE show version | include Version
WAITFOR /re Version (\S+),
SET version {1}
ECHO {host} runs {version}
```

## Placeholders

Filled in before a button's or the Script Editor's script runs, all at once (not line by line). The script's own
[variables](#variables) (from SET, `/var:`, named groups, `{0}`…`{9}`) are left for it to fill as it runs:

| Written | Becomes |
|---|---|
| `{name}` | The Environment Variable `name` (Tools > Environment Variables). If one isn't defined, the script doesn't run, and the app offers to open Environment Variables. |
| `{prompt:Name}` | Asked for once as the script starts, however often it appears; the last answer is offered next time. |
| `{prompt:Name=default}` | The same, with a default. |
| `{secret:Name}` | Asked for hidden, never remembered. |

A session's own script, or its profile's, isn't filled in: write its values in, or use PROMPT and PWDPROMPT. The
session's own values (`{host_address}`, `{session_name}`…) are [variables](#variables), so they work in both.

Braces are the placeholders' own: shell text with braces (`${HOME}`, `awk '{print $1}'`, `{1..5}`) would be read as
placeholders. Write `$HOME`, `cut -d' ' -f1`, `seq 1 5` instead. `$(...)` and `$((...))` are fine.

## Keys for SENDKEY

`SENDKEY` presses one key, written as one of:

- **A name:** ENTER, TAB, SPACE, ESC (ESCAPE), BACKSPACE (BKSP, BS), DEL (DELETE), INS (INSERT), HOME, END, PGUP, PGDN,
  UP, DOWN, LEFT, RIGHT, F1 to F16, BREAK, CLEAR, HELP, CAPSLOCK, NUMLOCK, SCROLLLOCK, PRTSC, and the keypad's
  MULTIPLY, ADD, SUBTRACT, DIVIDE.
- **A single character:** `SENDKEY y`.
- **A name in braces with a count:** `{TAB 3}` presses Tab three times.

Before any of them: `^` is Ctrl, `%` Alt, `+` Shift.
- Ctrl+C is `SENDKEY ^C`, Ctrl+Z `SENDKEY ^Z`, and the Cisco escape (Ctrl+Shift+6) `SENDKEY ^6`.
- A brace itself is `{{}` or `{}}`.
- To type text, use SENDCHAR or SENDLINE, not SENDKEY.

## Patterns that work

### Markers: wait for the answer, not the echo

Make the device print something the typed command doesn't contain. A shell works out `$((...))`, so the command
line shows the sum and the answer shows the result:

```
SENDLINE ls /srv; echo "DONE-$((6*7))"
WAITFOR DONE-42
```

The same for a choice: print one word or another and EXPECT them.

```
SENDLINE test -d /srv/data && echo "YES-$((1+1))" || echo "NO-$((1+1))"
EXPECT /t:5
  CASE NO-2
    STOP /srv/data isn't there
  CASE YES-2
END
```

On a device without a shell (a Cisco switch), wait for text only the answer has (`Uptime is`, `Building
configuration`), or for the prompt with a regular expression anchored at the line's start (below).

### Prompts

The prompt is the cursor's line, at the end of the output. Anchor it so text inside other output doesn't match:

| Device | Prompt regular expression |
|---|---|
| Linux shell (`user@host:~$ `, `root@host:/# `) | `WAITFOR /re [#$]\s*$` |
| Cisco IOS exec or enable (`sw1>`, `sw1#`) | `WAITFOR /re ^[\w.-]+[>#]\s*$` |
| Cisco config mode (`sw1(config)#`, `sw1(config-if)#`) | `WAITFOR /re ^[\w.-]+\(config[^)]*\)#\s*$` |
| Juniper (`user@router>`, `user@router#`) | `WAITFOR /re ^\S+@\S+[>#]\s*$` |

`^` keeps a `#` in a banner or a command's output from matching. `$` keeps a prompt-like word earlier on the line from
matching. After typing a command, the echoed command sits on the prompt's line, so the line no longer ends in the
prompt character; the next prompt is the one that matches.

### A login that copes with any order of prompts

```
EXPECT /t:20
  CASE "(yes/no"
    SENDLINE yes
    AGAIN
  CASE /username:|login:/i
    SENDLINE {username}
    AGAIN
  CASE /password:/i
    PWDPROMPT
    AGAIN
  CASE /% (Login invalid|Authentication failed|Bad passwords)/i
    STOP The device refused the login
  CASE /^[\w.-]+[>#]\s*$/
END
```

The host-key question, username and password are each answered if they come, in any order. A refusal stops the
script, and the prompt ends the block.

### Paging (`--More--`)

A long command pauses with `--More--`. Either turn paging off first (`terminal length 0` on Cisco, `| cat` on Linux),
or say once, at the top, how the pager asks, and every wait after it answers it:

```
PAGER --More--
SENDLINE show running-config
WAITPROMPT /t:60
SAVE running-config.txt
```

PAGER presses Space at each `--More--` until the prompt comes back. Other pagers: `PAGER /key:ENTER /re <-+ More -+>`.
Linux's `less` (which `more` can be, on Debian) needs two: Space at its `:` between pages, and **q** at its `(END)`,
where it waits rather than ending (without that, the prompt never comes back):

```
PAGER --More--
PAGER /re ^:\s*$
PAGER /key:q (END)
``` (`SENDCHAR` followed by a space types nothing: a line's trailing
spaces are dropped. Use `SENDKEY SPACE`.)

## Prerequisites

Say at the top what must be true before the script changes anything, look at the device, then judge it all at once:

```
CHECK ios /re Version (17\.\d+)
CHECK uplink /re ^Gi1/0/48\s+connected
CHECK nobgp /not /re ^router bgp
PAGER --More--
SENDLINE show version
WAITPROMPT
SENDLINE show interfaces status
WAITPROMPT
SENDLINE show running-config | section bgp
WAITPROMPT
REQUIRE ios uplink nobgp
ECHO prerequisites met on IOS XE {ios}
SENDLINE configure terminal
WAITPROMPT
```

- **What a CHECK sees:** all the output from its own line on, whatever waits do with it. Put CHECKs before the show
  commands they're about.
- **REQUIRE judges them when it runs:** a CHECK is met if its text has appeared by then (or, with `/not`, hasn't). The
  first one that isn't met stops the script, naming it and why, before any configuration is typed.
- **Branching instead of stopping:** `REQUIRE /else:old ios` goes to `:old` when `ios` isn't met (another way to
  configure older software, say).
- **What it found:** a met CHECK sets `{name}`: `{ios}` above is `17.9`, its first group.
- A CHECK can be defined again (the same name) to start watching afresh.

### Copying a file, with `!!!!` progress

```
FAILON /re %Error|Error opening|No such file
SENDLINE copy ftp://{ftp_user}:{secret:FTP password}@10.0.0.9/cat9k_iosxe.17.12.04.SPA.bin flash:
EXPECT /t:1800 /idle:60
  CASE /\[[^\]]*\]\?\s*$/
    SENDLINE
    AGAIN
  CASE /\[confirm\]/
    SENDLINE
    AGAIN
  CASE /(\d+) bytes copied in ([\d.]+) secs/
    ECHO copied {1} bytes in {2} s
END
WAITPROMPT
```

Each `[default]?` question (address, source, destination) takes its default with Enter; the `!!!!` progress builds up on
the line unmatched; the copy may take 30 minutes, but if the `!`s stop for a minute it's stalled and the script stops.
FAILON catches the device's errors at once.

### Cleaning up old images: a list and a loop

```
SENDLINE show boot | include BOOT
WAITFOR /re (\S+\.bin)
SET boot {1}
WAITPROMPT
SENDLINE dir flash:
WAITPROMPT
COLLECT bins /re (\S+\.bin)$
IF {bins_count} == 0
  STOP nothing to clean up
END
PROMPT /var:ok Delete {bins_count} images except {boot}? Type yes
IF {ok} != yes
  STOP not deleted
END
FOREACH file IN bins
  IF flash:{file} != {boot}
    SENDLINE delete /force flash:{file}
    WAITPROMPT
  ELSE
    ECHO keeping {file}
  END
END
```

The boot image's name is kept with SET straight after the wait that found it (the next wait replaces `{1}`). COLLECT
lists every `.bin` in dir's output; the script asks before deleting, and keeps the image the switch boots from.

### Timeouts as decisions

A wait that may not happen is a question, not an error:

```
SENDLINE copy running-config startup-config
WAITFOR /t:5 /else:noconfirm Destination filename
SENDLINE
:noconfirm
WAITFOR /re ^[\w.-]+#\s*$
```

If the device asks for the filename, Enter accepts it; if it doesn't ask within 5 s, the script carries on.

### Configuration that stops at the first refusal

```
FAILON /re ^% (Invalid|Incomplete|Ambiguous)
SENDLINE configure terminal
WAITPROMPT
SENDLINE interface Gi1/0/1
WAITPROMPT
SENDLINE description uplink to core
WAITPROMPT
SENDLINE end
WAITPROMPT
```

Each WAITPROMPT also looks for the refusal: a typo stops the script at that line, naming the device's message, instead
of typing the rest into the wrong mode.

### Saving a command's output

```
# the host's name, from its prompt
WAITFOR /re ^(?<host>[\w.-]+)[>#]\s*$
SENDLINE terminal length 0
WAITPROMPT
SENDLINE show running-config
WAITPROMPT /t:60
SAVE {host}-running-config.txt
ECHO saved {host}'s configuration
```

WAITPROMPT waits for the whole answer (the prompt comes back at its end); SAVE writes it without the command line and
the prompt. When an answer has no clear end, `WAITQUIET 2000` waits until it's been quiet for 2 s.

### Sharing a login

Save the login as its own script (say `login`), then start other scripts with `CALL login`. Variables it sets (the
host from its prompt, say) are there for the rest.

### Loops

GOTO makes a loop; give it a way out: a STOP, a wait that times out, or Stop in the status bar.

REPEAT runs lines a number of times; GOTO makes a loop of any shape. Give a loop a way out (a STOP, a wait that times
out, a LIMIT), and something to wait for in it.

```
LIMIT 10m
REPEAT 120
  SENDLINE show interfaces status | include Gi1/0/1
  EXPECT /t:10
    CASE /Gi1\/0\/1\s.*\bconnected\b/
      STOP Gi1/0/1 is up (check {repeat})
    CASE PROMPT
  END
  SLEEP 5000
END
STOP Gi1/0/1 didn't come up
```

The loop waits for the port to come up, checking every 5 s, at most 120 times and at most 10 minutes.

A login to the tab's own device, through a jump host, using the session's values:

```
SENDLINE ssh {session_username}@{host_address} -p {host_port}
EXPECT /t:20
  CASE "(yes/no"
    SENDLINE yes
    AGAIN
  CASE /password:/i
    PWDPROMPT
    AGAIN
  CASE PROMPT
END
```

## Pitfalls

- **Waiting for text that's in the command you typed.** It matches the echo at once. Use a marker, or anchor with
  `^`.
- **`#`, `>` and `$` anywhere.** `WAITFOR #` matches the first `#` in any output (a banner, an interface name). Anchor
  prompts with a regular expression.
- **Braces in shell text** are read as placeholders; see [Placeholders](#placeholders).
- **Typing before the device is ready.** After a CASE matches the answer, the device still prints its prompt. Wait
  for the prompt before the next SENDLINE, or the next wait may match the old prompt.
- **A command word on its own line.** A pasted `end` or `exit` (meant for the device) is read as SPSL's END (an error)
  or skipped. Write `SENDLINE end`.
- **Regular expressions are JavaScript's.** Escape `/` only inside a CASE's `/…/` (`/Gi1\/0\/1/`); after
  `WAITFOR /re` a `/` needs no escape. `\b`, `\s`, `\d`, `(a|b)`, `[^)]` and lookahead all work.
- **SLEEP to wait for a device** is a guess; WAITFOR the prompt instead, and keep SLEEP for pacing.
- **Line numbers** in messages are the script's own lines, comments and blanks counted, as the editor shows them.
- **FAILON only looks during waits.** A command typed with no wait after it isn't checked until the next wait.
- **WAITPROMPT's prompt** is any last line ending in `>`, `#`, `$` or `%`: output that stops mid-line on one of those
  (a progress "50%") looks like a prompt. SET prompt to the device's own when that matters.
- **Variables and placeholders share braces.** `{name}` is the script's variable if it SETs it (or saves it from a
  match or `/var:`), else an Environment Variable.

## Grammar

```
script     = { line "\n" } ;
line       = blank | comment | label | command ;
comment    = { " " } "#" { any } ;
label      = { " " } ":" word ;
command    = { " " } word [ " " arguments ] ;          (* word is case-insensitive *)

SENDLINE text | SENDCHAR text | SENDKEY key | SLEEP number | ECHO text | CALL name
PROMPT [ "/var:" name " " ] text | PWDPROMPT [ "/var:" name ] | OPENSESSION id | SENDBREAK | GOTO word | STOP [ text ]
WAITFOR { option " " } text | WAITPROMPT { " " option } | WAITQUIET { option " " } number
SET name [ " " text ] | FAILON { option " " } text | FAILON OFF | SAVE [ "/append " ] path
REPEAT number "\n" { line } "END" | LIMIT number [ "s" | "m" | "h" ]
CHECK name { option " " } text | REQUIRE [ "/else:" word ] { name } | PAGER { option " " } text | PAGER OFF
COLLECT name { option " " } text | FOREACH name " IN " list "\n" { line } "END"
IF test "\n" { line } [ "ELSE\n" { line } ] "END"
test       = text [ " " ( "==" | "!=" | "<" | ">" | "<=" | ">=" ) " " text | " " ( "~" | "!~" ) " " regex ] ;
list       = name | text { "," text } ;
EXPECT { " " option } "\n" case { case } "END"
case       = "CASE" " " ( pattern | "PROMPT" ) "\n" { line } ;   (* AGAIN may appear among a case's lines *)
option     = "/t:" digits | "/idle:" digits | "/i" | "/re" | "/not" | "/key:" key | "/var:" name | "/append"
           | "/else:" ( "stop" | "continue" | word ) ;
name       = ( letter | "_" ) { letter | digit | "_" } ;
pattern    = '"' { char | '\"' | '\\' } '"' | "/" regex "/" [ "i" ] | text ;
text       = { char | "{" name "}" | "{" digit "}" } ;            (* variables filled in as the line runs *)
```

## Checklist before handing a script over

1. Starts with `#!/bin/spsl` (needed for a session's own script, harmless elsewhere).
2. Every wait is for something only the device's answer contains: a marker, or a `^`-anchored prompt.
3. Every SENDLINE after an EXPECT or WAITFOR on an answer comes after a wait for the prompt.
4. Every wait that may not happen has `/t:` and an `/else:`; every wait that must happen has a `/t:` that's long
   enough for the slowest device.
5. Every EXPECT has CASEs and an END; every GOTO and `/else:label` has its `:label`; labels are unique.
6. No braces except intended placeholders and variables; secrets come from PWDPROMPT or `{secret:Name}`, never
   written in the script.
7. Loops have a way out, and something to wait for in them; a script that could run long starts with a LIMIT.
8. Configuration scripts start with a FAILON for the device's refusals, CHECKs for what must be true, and a PAGER
   (or paging turned off); they REQUIRE the checks before the first configuration line, and wait after every command.
9. Check script (the SPSL panel) reports nothing.

## A complete example: a Cisco switch's state, read only

```
#!/bin/spsl
# Log in if asked, turn paging off, save the configuration, and report any interface that's err-disabled.
FAILON /re ^% (Invalid|Incomplete|Ambiguous)
EXPECT /t:20 /else:noprompt
  CASE /username:/i
    SENDLINE {username}
    AGAIN
  CASE /password:/i
    PWDPROMPT
    AGAIN
  CASE /^(?<host>[\w.-]+)[>#]\s*$/
END
SENDLINE terminal length 0
WAITPROMPT
SENDLINE show running-config
WAITPROMPT /t:60
SAVE {host}-running-config.txt
SENDLINE show interfaces status | include err-disabled
EXPECT /t:15
  CASE /^(\S+)\s.*err-disabled/
    WAITPROMPT
    STOP {1} on {host} is err-disabled (configuration saved)
  CASE PROMPT
END
STOP {host}: configuration saved, no err-disabled interfaces
:noprompt
STOP No prompt: is this tab logged in to a switch?
```

Two longer ones, read-only tours of a Linux server (an OpenMediaVault NAS), each run there to its end:
[spsl-examples/omv-tour.spsl](spsl-examples/omv-tour.spsl) (who you are, the system, folders, disks, Docker, and
timeouts used as decisions) and [spsl-examples/omv-tour-2.spsl](spsl-examples/omv-tour-2.spsl) (LIMIT, the session's
values, CHECK and REQUIRE, PROMPT /var:, IF, COLLECT and FOREACH, PAGER through less, /idle:, REPEAT and SAVE).

## SuperXTerminal's additions to SuperXPuTTY's SPSL

SuperXPuTTY runs SENDLINE, SENDCHAR, SENDKEY, WAITFOR (exact text, with /t:), SLEEP, PROMPT, PWDPROMPT and
OPENSESSION, and skips SENDBREAK. SuperXTerminal adds:
- SENDBREAK in Serial and Telnet tabs
- WAITFOR's `/i`, `/re` and `/else:`
- EXPECT, CASE, AGAIN and END
- labels, GOTO and STOP
- checking the structure before a script starts
- line numbers that count every line

- variables: SET, PROMPT and PWDPROMPT's `/var:`, and what waits matched
- FAILON, WAITPROMPT, WAITQUIET, CASE PROMPT, ECHO, SAVE and CALL
- REPEAT, LIMIT, and stopping a loop that never waits
- CHECK and REQUIRE (prerequisites), and PAGER
- COLLECT, FOREACH, IF and ELSE, and `/idle:` for transfers that stall
- the tab's session's values: `{host_address}`, `{session_name}`…
- stopping when the session ends or reconnects
- stepping a line at a time, and hover help in the editor
- a profile's script, run by its sessions with none of their own, and Log in here for a jump server

SuperXPuTTY skips the lines it doesn't know, so a script using the additions only runs correctly in SuperXTerminal.
