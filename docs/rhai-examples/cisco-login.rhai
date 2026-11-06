#!/bin/rhai
// cisco-login.rhai: a Rhai login script for a Cisco switch, run as its session opens. Read only: enable, no paging for
// this login, and the switch's name, version and uptime in the status bar.
//
// To use it: Edit Session > Terminal > Login script (or a profile's, Settings > Profiles > Login script), and choose
// this file. Its first line (#!/bin/rhai) or its name (.rhai) is what says it's Rhai.
//
// The enable secret: an Environment Variable named enable_secret (Tools > Environment Variables, as a secret) is used
// if there is one; otherwise it's asked for, hidden. It's never written here.

// ---- 1. The login finishing ---------------------------------------------------------------------------------------

// the app types the session's stored password itself; this waits for the prompt that follows it
if !wait_prompt(45) {
    throw "the switch's prompt didn't come (login still going, or refused?)";
}
// trim() tidies a variable in place (it gives nothing back)
let prompt = matched()[0];
prompt.trim();

// ---- 2. Enable mode, if it's at > ---------------------------------------------------------------------------------

if prompt.ends_with(">") {
    send_line("enable");
    if !wait_for("password:", #{ any_case: true, timeout: 10 }) {
        throw "enable didn't ask for the secret";
    }
    let secret = if env.has("enable_secret") { env.enable_secret } else { password("Enable secret") };
    send_line(secret);
    // the prompt after it says how it went: # is in; a refused secret ("% Access denied") comes back to >. Waiting for
    // the prompt itself, not its "#": a wait moves past what it finds, and a wait_for("#") would use up the prompt, so
    // a wait_prompt() after it would find nothing left
    if !wait_prompt(10) {
        throw "no prompt after the enable secret";
    }
    prompt = matched()[0];
    prompt.trim();
    if !prompt.ends_with("#") {
        throw "the enable secret was turned down";
    }
}

// "core-sw1#" -> "core-sw1"
let host = prompt.sub_string(0, prompt.len - 1);

// ---- 3. No pager for this login ----------------------------------------------------------------------------------

send_line("terminal length 0");
if !wait_prompt(10) {
    throw "no prompt after terminal length 0";
}

// ---- 4. What it is ------------------------------------------------------------------------------------------------

send_line("show version");
let version = "?";
// ", Version 15.2(7)E8," (each pattern ends at a comma or the line's end, so a line still arriving isn't half-read)
if wait_for(`, Version ([^\s,]+),`, #{ regex: true, timeout: 15 }) {
    version = matched()[1];
}
let uptime = "?";
if wait_for(`^\S+ uptime is ([^\r\n]+)\r?\n`, #{ regex: true, timeout: 15 }) {
    uptime = matched()[1];
}
if !wait_prompt(30) {
    throw "no prompt after show version";
}

status(`Logged in to ${host} (${session.host_address}): IOS ${version}, up ${uptime}`);
