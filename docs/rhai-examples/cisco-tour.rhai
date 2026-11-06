// cisco-tour.rhai: a tour of a Cisco switch, and of Rhai scripting in SuperXTerminal.
// Run it in a tab already logged in to the switch: Script Editor > Run as > Rhai script, or as a button's
// Run Rhai script (Edit Button). The status bar shows each step; Stop ends it at once.
//
// What it does, all read only (conf t is entered and left with nothing changed):
//   1. Finds the prompt, and goes to enable mode if it's at > (asking for the enable secret, or enable_secret below).
//   2. terminal length 0, so no --More-- stops the output (this login only).
//   3. show version: picks out the version, the uptime and the model.
//   4. configure terminal, a "do show clock" from config mode, then end.
//   5. show ip int bri: counts the interfaces up and down, and lists those with an address.
//   6. Puts it all in the status bar.
//
// What it shows of Rhai: variables (let), functions of your own (fn), waits that return true or false so the script
// decides, regular expressions and matched(), wait_for_any for "whichever comes first", loops, arrays, and throw to
// stop with a message.

// ---- Settings -----------------------------------------------------------------------------------------------------

// The enable secret, for a switch at >. "" asks for it (hidden) as the script runs, which keeps it out of the script;
// a secret written here is for your own copy only, never one you share or save in the team's scripts folder.
let enable_secret = "";

// ---- Helpers ------------------------------------------------------------------------------------------------------

// Stop the script, saying what didn't come, when a wait comes back false
fn must(found, what) {
    if !found {
        throw `${what} didn't come`;
    }
}

// Type a command and wait for the prompt after its output: the prompt line, e.g. "core-sw1#"
fn run(command) {
    send_line(command);
    must(wait_prompt(30), `The prompt after "${command}"`);
    matched()[0]
}

// ---- 1. The prompt, and enable mode -------------------------------------------------------------------------------

// A prompt already on screen counts; if there's none, Enter brings one
if !wait_prompt(3) {
    send_line("");
    must(wait_prompt(10), "The switch's prompt");
}
// trim() tidies a variable in place (it gives nothing back)
let prompt = matched()[0];
prompt.trim();
status(`Found the prompt ${prompt}`);

if prompt.ends_with(">") {
    send_line("enable");
    must(wait_for("password:", #{ any_case: true, timeout: 10 }), "enable's Password:");
    if enable_secret == "" {
        send_line(password("Enable secret"));
    } else {
        send_line(enable_secret);
    }
    // the prompt after it says how it went: # is in; a refused secret ("% Access denied") comes back to >. Waiting for
    // the prompt itself, not its "#": a wait moves past what it finds, so wait_for("#") would use up the prompt
    must(wait_prompt(10), "The prompt after the enable secret");
    prompt = matched()[0];
    prompt.trim();
    if !prompt.ends_with("#") {
        throw "the enable secret was turned down";
    }
}
// "core-sw1#" -> "core-sw1"
let host = prompt.sub_string(0, prompt.len - 1);

// ---- 2. No pager --------------------------------------------------------------------------------------------------

run("terminal length 0");

// ---- 3. show version ----------------------------------------------------------------------------------------------

status("show version");
send_line("show version");
// In the order they come; each pattern ends at its line's end (a comma, or the line break), so a line still
// arriving isn't taken half-read
let version = "?";
if wait_for(`, Version ([^\s,]+),`, #{ regex: true, timeout: 15 }) {
    version = matched()[1];
}
let uptime = "?";
if wait_for(`^\S+ uptime is ([^\r\n]+)\r?\n`, #{ regex: true, timeout: 15 }) {
    uptime = matched()[1];
}
let model = "?";
if wait_for(`^[Cc]isco (\S+) \(.*\) processor`, #{ regex: true, timeout: 15 }) {
    model = matched()[1];
}
must(wait_prompt(30), "The prompt after show version");
status(`${host}: ${model}, version ${version}, up ${uptime}`);

// ---- 4. Config mode, and back ---------------------------------------------------------------------------------------

send_line("configure terminal");
let into = wait_for_any([`(config)#`, "% "], 10);
switch into {
    0 => status("In config mode"),
    1 => { throw "configure terminal was refused"; }
    _ => { throw "config mode's prompt didn't come"; }
}
// a show command from config mode, its answer picked out
send_line("do show clock");
let clock = "?";
if wait_for(`(\d{1,2}:\d{2}:\d{2})`, #{ regex: true, timeout: 10 }) {
    clock = matched()[1];
}
must(wait_for(`(config)#`, 10), "Config mode's prompt after do show clock");
// nothing changed, so end leaves it with nothing to save
let back = run("end");
if back.contains("(config") {
    throw "end didn't leave config mode";
}
status(`Config mode in and out at ${clock}`);

// ---- 5. show ip int bri -------------------------------------------------------------------------------------------

status("show ip interface brief");
send_line("show ip interface brief");
must(wait_for("Protocol", 15), "show ip int bri's heading");

// One pattern for an interface's whole line, or (group 5) the prompt that ends the list, so the loop needn't
// wait for a timeout to know it's done. Each wait moves past what it found, so the next finds the next line.
let row = `^(\S+)\s+(\S+)\s+(?:YES|NO)\s+\S+\s+(up|down|administratively down)\s+(up|down)[ \t]*\r?\n|^([\w.-]+#)[ \t]*$`;
let up = 0;
let down = 0;
let addressed = [];
loop {
    if !wait_for(row, #{ regex: true, timeout: 15 }) {
        throw "show ip int bri's list didn't end with the prompt";
    }
    let m = matched();
    if m[5] != "" {
        break;
    }
    if m[3] == "up" && m[4] == "up" {
        up += 1;
    } else {
        down += 1;
    }
    if m[2] != "unassigned" {
        addressed.push(`${m[1]} ${m[2]} (${m[3]}/${m[4]})`);
    }
}

// ---- 6. The summary -----------------------------------------------------------------------------------------------

for line in addressed {
    status(`Address: ${line}`);
}
status(`${host}: ${model} ${version}, up ${uptime}; ${up} interfaces up, ${down} not, ${addressed.len} with an address`);
