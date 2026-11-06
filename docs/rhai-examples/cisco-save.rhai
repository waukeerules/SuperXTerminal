#!/bin/rhai
// cisco-save.rhai: saves a Cisco switch's running config (copy running-config startup-config), answering its questions
// with their defaults if it asks them. Whether it asks depends on the IOS version and its settings (file prompt):
// "Destination filename [startup-config]?", or "Overwrite the previous NVRAM configuration?[confirm]", or nothing.
// Each is answered with Enter, which takes the default in brackets, so the config goes to startup-config.
//
// To use it: run it in a tab logged in to the switch (Script Editor > Run as: Rhai script), or make it a button
// (Run Rhai script). It needs enable mode: a prompt ending in #.

// ---- 1. At the switch's prompt, in enable mode ---------------------------------------------------------------------

// a prompt already on screen counts; if there's none, Enter brings one
if !wait_prompt(3) {
    send_line("");
    if !wait_prompt(10) {
        throw "no prompt from the switch";
    }
}
let prompt = matched()[0];
prompt.trim();
if !prompt.ends_with("#") {
    throw "not in enable mode (the prompt ends in >): run enable first";
}

// ---- 2. The copy, and any questions it asks -------------------------------------------------------------------------

send_line("copy running-config startup-config");
// each pass waits for whichever comes first: a question (answered, then wait again), "[OK]" (saved), or "%" (IOS's
// error lines, "%Error opening..." or "% Invalid input"). A question that doesn't come is simply never waited past
let questions = 0;
loop {
    let i = wait_for_any(["]?", "[confirm]", "[OK]", "%"], 60);
    if i == 0 || i == 1 {
        // "[startup-config]?" or "?[confirm]": Enter takes the default
        send_line("");
        questions += 1;
        if questions > 3 {
            throw "the copy kept asking questions; look at the tab";
        }
    } else if i == 2 {
        break;
    } else if i == 3 {
        throw "the switch didn't save the config: see its % line in the tab";
    } else {
        throw "the copy didn't finish within 60 s";
    }
}

// ---- 3. Back at the prompt ----------------------------------------------------------------------------------------

if !wait_prompt(15) {
    throw "no prompt after the copy";
}
status(`Saved the running config on ${session.session_name}`);
