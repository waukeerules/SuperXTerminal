// linux-values.rhai: shows what a Rhai script sees of the tab's session (session) and of Environment Variables
// (env), by echoing each on a Linux host and reading it back. Run it in a tab logged in to a Linux shell (bash, zsh
// or sh): Script Editor > Run as > Rhai script, or as a button's Run Rhai script. Read only: it runs echo, nothing else.
//
// Each value goes through the host and back: "echo 'session.host_address = 10.0.0.5'", then the script waits for that
// line in the host's answer and checks it came back the same. The status bar says what was found, and the summary
// at the end says how many matched.
//
// List your own Environment Variables in env_names below (Tools > Environment Variables has them). Leave secrets
// out: echo would show them on the host's screen and in its shell history.

// ---- Settings -----------------------------------------------------------------------------------------------------

// Environment Variables to show (not secrets); one with no value is said so and skipped, not stopped at
let env_names = ["ad_user", "jump_host"];

// ---- Helpers ------------------------------------------------------------------------------------------------------

// Echo "label = value" on the host and read the value back from its answer: true if it came back the same. The
// pattern starts at a line's start (^), so it finds the host's answer, not the echo of the command typed (which
// starts with the prompt). The label is matched as text: its dots are escaped.
fn echo_back(label, value) {
    send_line(`echo '${label} = ${value}'`);
    // replace() changes a variable in place (it gives nothing back), so on a copy
    let escaped = label;
    escaped.replace(".", "\\.");
    let pattern = "^" + escaped + ` = ([^\r\n]*)\r?\n`;
    if !wait_for(pattern, #{ regex: true, timeout: 10 }) {
        status(`${label}: the host's answer didn't come`);
        return false;
    }
    let back = matched()[1];
    if back == value {
        status(`${label} = ${value}`);
        true
    } else {
        status(`${label}: sent "${value}", the host said "${back}"`);
        false
    }
}

// Who and where, read inside a function: session and env are seen here too, not only at the top of the script
fn who_and_where() {
    let user = if session.has("session_username") { session.session_username } else { "?" };
    let host = if session.has("host_address") { session.host_address } else { "?" };
    let me = if env.has("ad_user") { env.ad_user } else { "(no ad_user)" };
    `${user}@${host}, Environment Variable ad_user: ${me}`
}

// ---- 1. A prompt to start from ------------------------------------------------------------------------------------

if !wait_prompt(3) {
    send_line("");
    if !wait_prompt(10) {
        throw "the shell's prompt didn't come";
    }
}

// ---- 2. The tab's session -----------------------------------------------------------------------------------------

let names = ["host_address", "host_port", "session_name", "session_id", "session_folder", "session_username",
    "session_profile", "session_protocol"];
let matched_count = 0;
let shown = 0;
for name in names {
    if session.has(name) {
        shown += 1;
        if echo_back(`session.${name}`, session[name]) {
            matched_count += 1;
        }
    } else {
        // a Local Shell tab has none; Quick connect has the host and login typed
        status(`session.${name}: this tab has none`);
    }
    if !wait_prompt(10) {
        throw "the prompt didn't come back after an echo";
    }
}

// ---- 3. Environment Variables -------------------------------------------------------------------------------------

for name in env_names {
    if env.has(name) {
        shown += 1;
        if echo_back(`env.${name}`, env[name]) {
            matched_count += 1;
        }
    } else {
        status(`env.${name}: no value in Environment Variables`);
    }
    if !wait_prompt(10) {
        throw "the prompt didn't come back after an echo";
    }
}

// ---- 4. Inside a function, and the summary ------------------------------------------------------------------------

status(who_and_where());
status(`${matched_count} of ${shown} values came back from the host as sent`);
