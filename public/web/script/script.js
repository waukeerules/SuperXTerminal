/*
 * The script box's page (index.html): Monaco editing a script, with SPSL's own colours (the "spsl" language below),
 * Rhai's ("rhai", for a Rhai script: src-tauri/src/rhai_scripts.rs), or as plain lines ("plaintext", for Send lines). The main window (src/lib/scripting/ScriptBox.svelte) sends:
 *   { type: "set", text, language, wrap, dark, colors }  the whole state, as the box opens
 *   { type: "text", text }                       new text from outside (Revert, a saved script opened)
 *   { type: "insert", text }                     text at the cursor, in place of the selection (the SPSL panel)
 *   { type: "language", language } / { type: "wrap", wrap } / { type: "dark", dark, colors }
 * (colors: a theme of the user's own's, Settings > General > Look > Edit Themes…, or null for Monaco's own)
 *   { type: "focus" }
 *   { type: "reveal", line }                     the cursor to a line's start, shown (a problem clicked under the box)
 * and the page sends back { type: "ready" } once Monaco is up, and { type: "change", text } at every edit.
 *
 * Hovering a command, an option or a {variable} shows what it does (HELP, from docs/SPSL.md). The keywords are
 * spsl.ts's (COMMANDS); keep the two lists, and HELP, the same as it. In a Rhai script, hovering one of the tab's
 * functions does the same (RHAI_HELP); keep it and RHAI_FUNCTIONS the same as rhai_scripts.rs registers them, and
 * as the script panel lists them (SpslHelp.svelte's RHAI_COMMANDS).
 */
(function () {
    'use strict';

    var host = window.chrome && window.chrome.webview;
    var editor = null;

    /** Send a message to the main window */
    function post(message) {
        if (host) { host.postMessage(message); }
    }

    /** The options lines take: /t:10, /i, /else:label, /var:name, /append (/re is read on its own) */
    var OPTION = /\/(t:\d+|i|else:\S+|var:\w+|append|not|key:\S+|idle:\d+)/;

    /**
     * SPSL as Monaco's Monarch tokenizer reads it, line by line: a command word at the start of a line; WAITFOR's and
     * EXPECT's options (/t:10, /i, /re, /else:label) and then the text (a regular expression after /re); CASE's
     * "text", /regex/ or text; GOTO's label; SLEEP's number; SENDKEY's key; labels (:name), comments (#), and
     * {placeholders}. Monarch can't match a line's end on its own, so a command enters a state only when something
     * follows it, and each state's last token on a line is matched up to the end ($), which leaves the state;
     * otherwise the state would carry on into the next line.
     */
    var SPSL = {
        ignoreCase: true,
        tokenizer: {
            root: [
                [/^\s*#.*$/, 'comment'],
                [/^(\s*)(:\S+)/, ['white', 'type.identifier']],
                [/^(\s*)(FAILON|PAGER)(\s+)(OFF)\s*$/, ['white', 'keyword', 'white', 'keyword']],
                [/^(\s*)(CHECK)(\s+)(\w+)(?=\s+\S)/, ['white', 'keyword', 'white', { token: 'type.identifier', next: '@options' }]],
                [/^(\s*)(COLLECT)(\s+)(\w+)(?=\s+\S)/, ['white', 'keyword', 'white', { token: 'variable', next: '@options' }]],
                [/^(\s*)(FOREACH)(\s+)(\w+)(\s+)(IN)(?=\s+\S)/, ['white', 'keyword.flow', 'white', 'variable', 'white', { token: 'keyword.flow', next: '@text' }]],
                [/^(\s*)(IF)(?=\s+\S)/, ['white', { token: 'keyword.flow', next: '@condition' }]],
                [/^(\s*)(ELSE)\s*$/, ['white', 'keyword.flow']],
                [/^(\s*)(REQUIRE)(\s+)(\/else:\S+)(\s*)([\w ,]*)$/, ['white', 'keyword.flow', 'white', 'attribute.name', 'white', 'type.identifier']],
                [/^(\s*)(REQUIRE)(\s*)([\w ,]*)$/, ['white', 'keyword.flow', 'white', 'type.identifier']],
                [/^(\s*)(WAITFOR|EXPECT|FAILON|PAGER)(?=\s+\S)/, ['white', { token: 'keyword', next: '@options' }]],
                [/^(\s*)(WAITPROMPT|WAITQUIET|PROMPT|PWDPROMPT|SAVE)(?=\s+\/)/, ['white', { token: 'keyword', next: '@options' }]],
                [/^(\s*)(CASE)(\s+)(PROMPT)\s*$/, ['white', 'keyword.flow', 'white', 'keyword']],
                [/^(\s*)(SET)(\s+)(\w+)(?=\s+\S)/, ['white', 'keyword', 'white', { token: 'variable', next: '@text' }]],
                [/^(\s*)(SET)(\s+)(\w+)/, ['white', 'keyword', 'white', 'variable']],
                [/^(\s*)(WAITQUIET)(\s+)(\d+)/, ['white', 'keyword', 'white', 'number']],
                [/^(\s*)(REPEAT)(\s+)(\d+)/, ['white', 'keyword.flow', 'white', 'number']],
                [/^(\s*)(LIMIT)(\s+)(\d+\s*[smh]?)/, ['white', 'keyword', 'white', 'number']],
                [/^(\s*)(CALL)(\s+)(.+)$/, ['white', 'keyword.flow', 'white', 'type.identifier']],
                [/^(\s*)(CASE)(?=\s+\S)/, ['white', { token: 'keyword.flow', next: '@pattern' }]],
                [/^(\s*)(GOTO)(\s+)(\S+)/, ['white', 'keyword.flow', 'white', 'type.identifier']],
                [/^(\s*)(STOP)(?=\s+\S)/, ['white', { token: 'keyword.flow', next: '@text' }]],
                [/^(\s*)(AGAIN|END|STOP|CASE|GOTO|REPEAT|FOREACH|IF|ELSE)\b/, ['white', 'keyword.flow']],
                [/^(\s*)(SLEEP)(\s+)(\d+)/, ['white', 'keyword', 'white', 'number']],
                [/^(\s*)(SENDKEY)(?=\s+\S)/, ['white', { token: 'keyword', next: '@key' }]],
                [/^(\s*)(SENDLINE|SENDCHAR|PROMPT|OPENSESSION|ECHO|SAVE)(?=\s+\S)/, ['white', { token: 'keyword', next: '@text' }]],
                [/^(\s*)(SENDLINE|SENDCHAR|SENDKEY|PROMPT|PWDPROMPT|OPENSESSION|SENDBREAK|CLOSESESSION|SLEEP|WAITFOR|EXPECT|WAITPROMPT|WAITQUIET|SET|FAILON|ECHO|SAVE|CALL|LIMIT|CHECK|PAGER|COLLECT)\b/, ['white', 'keyword']],
                [/\s+/, 'white'],
                // anything else isn't SPSL: Check script says why
                [/./, 'invalid']
            ],
            // what a line types or shows: text, with its {placeholders}
            text: [
                [/\{(prompt|secret):[^{}]*\}\s*$/, 'variable.predefined', '@pop'],
                [/\{\w+\}\s*$/, 'variable', '@pop'],
                [/\{(prompt|secret):[^{}]*\}/, 'variable.predefined'],
                [/\{\w+\}/, 'variable'],
                [/[^{]*$/, 'string', '@pop'],
                [/[^{]+/, 'string'],
                [/\{$/, 'string', '@pop'],
                [/\{/, 'string']
            ],
            // SENDKEY's key: ^ % + and a name, a character or {NAME count}
            key: [
                [/\s+/, 'white'],
                [/[\^%+]+\s*$/, 'operator', '@pop'],
                [/[\^%+]+/, 'operator'],
                [/\S+\s*$/, 'constant', '@pop'],
                [/\S+/, 'constant']
            ],
            // WAITFOR's and EXPECT's options, then WAITFOR's text (a regular expression after /re)
            options: [
                [/\s+$/, 'white', '@pop'],
                [/\s+/, 'white'],
                [/\/re\s*$/, 'attribute.name', '@pop'],
                [/\/re(?=\s)/, { token: 'attribute.name', switchTo: '@regexOptions' }],
                [new RegExp(OPTION.source + '\\s*$'), 'attribute.name', '@pop'],
                [new RegExp(OPTION.source + '(?=\\s)'), 'attribute.name'],
                [/./, { token: '@rematch', switchTo: '@text' }]
            ],
            regexOptions: [
                [/\s+$/, 'white', '@pop'],
                [/\s+/, 'white'],
                [new RegExp(OPTION.source + '\\s*$'), 'attribute.name', '@pop'],
                [new RegExp(OPTION.source + '(?=\\s)'), 'attribute.name'],
                [/.+$/, 'regexp', '@pop']
            ],
            // IF's test: its sides (text and {variables}), the comparison, and a regular expression after ~ or !~
            condition: [
                [/\s+$/, 'white', '@pop'],
                [/\s+/, 'white'],
                [/(!~|~)(\s+)(.+)$/, ['operator', 'white', { token: 'regexp', next: '@pop' }]],
                [/(==|!=|<=|>=|<|>)(?=\s)/, 'operator'],
                [/\{\w+\}\s*$/, 'variable', '@pop'],
                [/\{\w+\}/, 'variable'],
                [/[^\s{]+\s*$/, 'string', '@pop'],
                [/[^\s{]+/, 'string']
            ],
            // CASE "text", /regex/ (or /regex/i), or text to the end of the line
            pattern: [
                [/\s+/, 'white'],
                [/"(?:[^"\\]|\\.)*"\s*$/, 'string', '@pop'],
                [/\/.*\/i?\s*$/, 'regexp', '@pop'],
                [/./, { token: '@rematch', switchTo: '@text' }]
            ]
        }
    };

    /** The functions SuperXTerminal gives a Rhai script (rhai_scripts.rs), coloured apart from the language's own */
    var RHAI_FUNCTIONS = ['send', 'send_line', 'wait_for', 'wait_for_any', 'wait_prompt', 'matched', 'sleep', 'status',
        'prompt', 'password', 'print', 'debug'];

    /**
     * Rhai as Monaco's Monarch tokenizer reads it (https://rhai.rs/book): its keywords, the tab's functions
     * (RHAI_FUNCTIONS) and `session`, numbers, "strings" with their \escapes, `backquoted strings` with their
     * ${interpolations} (coloured as code), 'c'haracters, #{ maps }, // and /* comments *\/ (which nest, as Rhai's do).
     * Unlike SPSL's, Rhai's statements run over lines, so the states here follow brackets and quotes, not lines.
     */
    var RHAI = {
        defaultToken: '',
        functions: RHAI_FUNCTIONS,
        // declarations, and the words a script's flow is made of
        declarations: ['let', 'const', 'fn', 'private', 'import', 'export', 'as', 'global', 'this'],
        flow: ['if', 'else', 'switch', 'while', 'loop', 'for', 'in', 'do', 'until', 'break', 'continue', 'return',
            'throw', 'try', 'catch'],
        constants: ['true', 'false'],
        // longest first, so => and ** aren't read as = > and * *
        operators: /\*\*=?|<<=?|>>=?|\.\.=?|=>|\?\?|\?\.|::|&&|\|\||[=!<>]=?|[-+*\/%&|^]=?/,
        escapes: /\\(?:[nrt0\\"'`$]|x[0-9A-Fa-f]{2}|u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8})/,
        tokenizer: {
            root: [
                [/\/\/.*$/, 'comment'],
                [/\/\*/, 'comment', '@comment'],
                [/\b(?:session|env)\b/, 'variable.predefined'],
                [/[A-Za-z_]\w*/, {
                    cases: {
                        '@functions': 'type.identifier',
                        '@declarations': 'keyword',
                        '@flow': 'keyword.flow',
                        '@constants': 'constant',
                        '@default': 'identifier'
                    }
                }],
                [/0x[0-9A-Fa-f_]+|0o[0-7_]+|0b[01_]+/, 'number'],
                [/\d[\d_]*\.\d[\d_]*(?:[eE][-+]?\d+)?|\d[\d_]*(?:[eE][-+]?\d+)?/, 'number'],
                [/"/, 'string', '@string'],
                [/`/, 'string', '@backquoted'],
                [/'(?:@escapes|[^\\'])'/, 'string'],
                [/#\{/, 'delimiter.bracket'],
                [/[{}()\[\]]/, '@brackets'],
                [/@operators/, 'operator'],
                [/[;,.:]/, 'delimiter'],
                [/\s+/, 'white']
            ],
            string: [
                [/@escapes/, 'string.escape'],
                [/\\$/, 'string.escape'],
                [/[^\\"]+/, 'string'],
                [/\\./, 'string.escape.invalid'],
                [/"/, 'string', '@pop']
            ],
            // `text ${code} text`: no escapes, and it may run over lines
            backquoted: [
                [/\$\{/, 'delimiter.bracket', '@interpolation'],
                [/[^`$]+/, 'string'],
                [/\$/, 'string'],
                [/`/, 'string', '@pop']
            ],
            interpolation: [
                [/\}/, 'delimiter.bracket', '@pop'],
                { include: 'root' }
            ],
            comment: [
                [/[^/*]+/, 'comment'],
                [/\/\*/, 'comment', '@push'],
                [/\*\//, 'comment', '@pop'],
                [/[/*]/, 'comment']
            ]
        }
    };

    /** What each of the tab's functions does in a Rhai script, for hovering it (the script panel lists them too) */
    var RHAI_HELP = {
        send: '**send(text)**: type the text, without Enter. `\\r` is Enter, `\\t` Tab, `\\x03` Ctrl+C.',
        send_line: '**send_line(text)**: type the text, then Enter. A wait after it looks only at what comes after.',
        wait_for: '**wait_for(text [, seconds | #{ timeout, regex, any_case }])**: wait until the text appears (30 s by default; 0 for as long as it takes). **true** if it came, **false** if not: the script goes on either way, so check it. With `regex: true` it\'s a regular expression (JavaScript\'s; ^ and $ are each line\'s ends), its groups in `matched()`.',
        wait_for_any: '**wait_for_any([texts] [, seconds])**: wait for whichever text appears first (30 s by default): its index, 0 for the first, or **-1** when none came.',
        wait_prompt: '**wait_prompt([seconds])**: wait for the device\'s prompt, a last line ending in > # $ or % (30 s by default): **true** if it came. `matched()[0]` is the prompt line.',
        matched: '**matched()**: what the last wait matched: `[0]` the whole match (the prompt line, for wait_prompt), `[1]`… a regular expression\'s groups.',
        sleep: '**sleep(ms)**: wait that many milliseconds; Stop ends it at once.',
        status: '**status(text)**: show the text in the status bar.',
        print: '**print(text)**: show the text in the status bar, as status.',
        debug: '**debug(value)**: show the value in the status bar, with its line.',
        prompt: '**prompt(question)**: ask for a value: the answer, or "" when cancelled.',
        password: '**password(question)**: ask for a password (hidden): the answer. Cancel stops the script.',
        env: '**env**: Environment Variables, as SPSL\'s `{name}`: `env.ad_user` or `env["ad_user"]`, secrets included (filled in when the script starts, never shown). One with no value stops the script naming it; `env.has("name")` says whether it has one.',
        session: '**session**: the tab\'s session, as the script starts: `session.host_address`, `host_port`, `session_name`, `session_id`, `session_folder`, `session_username`, `session_profile`, `session_protocol`.'
    };

    /** The help for the tab's function (or `session`) under the mouse on a line of a Rhai script; null for none */
    function rhaiHelpAt(line, column) {
        var words = /[A-Za-z_]\w*/g;
        var match;
        while ((match = words.exec(line))) {
            if (column - 1 >= match.index && column - 1 <= match.index + match[0].length) {
                // a field or method of the same name (x.status) isn't the function
                if (match.index > 0 && line[match.index - 1] === '.' && match[0] !== 'session' && match[0] !== 'env') { return null; }
                return Object.prototype.hasOwnProperty.call(RHAI_HELP, match[0]) ? RHAI_HELP[match[0]] : null;
            }
        }
        return null;
    }

    /** The Monaco language for the window's: "spsl", "rhai", or plaintext for anything else (Send lines) */
    function languageOf(language) {
        return language === 'spsl' || language === 'rhai' ? language : 'plaintext';
    }

    /** What each command, option and kind of {variable} does, for hovering it (docs/SPSL.md has the whole language) */
    var HELP = {
        SENDLINE: '**SENDLINE text**: type the text, then Enter. `SENDLINE` alone presses Enter. {variables} are filled in.',
        SENDCHAR: '**SENDCHAR text**: type the text, without Enter. A lone space can\'t be typed this way: use `SENDKEY SPACE`.',
        SENDKEY: '**SENDKEY key**: press one key: ENTER, TAB, SPACE, ESC, UP, F5…, or a character. `^` Ctrl, `%` Alt, `+` Shift (`^C`); `{TAB 3}` presses it 3 times.',
        WAITFOR: '**WAITFOR [/t:s] [/i] [/re] [/else:…] text**: wait until the text appears in the output since the last thing typed (30 s), else stop. `/re` makes it a regular expression; what it matched is `{0}`, its groups `{1}`…',
        EXPECT: '**EXPECT [/t:s] [/i] [/else:…]** … **CASE** … **END**: wait for whichever CASE appears first in the output, and run the lines under it.',
        CASE: '**CASE "text" | /regex/ | text | PROMPT**: one thing an EXPECT waits for; the lines under it run when it\'s seen first.',
        AGAIN: '**AGAIN**: under a CASE, wait again with the same cases (a login whose prompts come in any order).',
        END: '**END**: closes an EXPECT; the end of a CASE goes on after it.',
        GOTO: '**GOTO label**: go on from the line `:label`.',
        STOP: '**STOP message**: end the script, with the message in the status bar.',
        SLEEP: '**SLEEP ms**: wait that many milliseconds.',
        PROMPT: '**PROMPT [/var:name] question**: ask for a value and type it, then Enter; with `/var:` keep it in `{name}` instead.',
        PWDPROMPT: '**PWDPROMPT [/var:name]**: ask for a password (hidden) and type it, then Enter; Cancel stops the script.',
        OPENSESSION: '**OPENSESSION Folder/name**: open a saved session in a new tab.',
        SENDBREAK: '**SENDBREAK**: send a Break (Serial and Telnet tabs).',
        WAITPROMPT: '**WAITPROMPT [/t:s] [/else:…]**: wait for the device\'s prompt: a last line ending in > # $ or %. `SET prompt` to a regular expression for another.',
        WAITQUIET: '**WAITQUIET ms [/t:s] [/else:…]**: wait until the output has stopped changing for that long.',
        SET: '**SET name value**: a variable; `{name}` in later lines is its value. `SET name` alone forgets it.',
        FAILON: '**FAILON [/i] [/re] text**: from here on, any wait that sees it stops the script, naming the device\'s line. `FAILON OFF` forgets them.',
        ECHO: '**ECHO text**: show the text in the status bar.',
        SAVE: '**SAVE [/append] file**: save what the last command printed (without its echo and prompt); a bare name goes in Documents/SuperXTerminal/output.',
        CALL: '**CALL name**: run a saved script here, with the same variables, then go on.',
        REPEAT: '**REPEAT n** … **END**: run the lines up to END n times; `{repeat}` is the round (1, 2…).',
        LIMIT: '**LIMIT 90s | 10m | 1h**: the whole script may run that long, its waits included; then it stops.',
        COLLECT: '**COLLECT name [/i] [/re] text**: every match in the last command\'s output (a regular expression\'s first group, else the whole match), as a list for FOREACH; `{name}` is them all, `{name_count}` how many.',
        FOREACH: '**FOREACH item IN list** … **END**: run the lines once per item, `{item}` set to it: a COLLECT\'s list, or a written one (`10, 20, 30`).',
        IF: '**IF a == b** (`!=`, `<`, `>`, `<=`, `>=`; as numbers when both are), **IF a ~ /regex/** (`!~`), or **IF a** (not empty) … **ELSE** … **END**.',
        ELSE: '**ELSE**: in an IF, the lines run when its test doesn\'t hold.',
        '/idle': '**/idle:seconds**: end the wait if no new output comes for that long (a stalled copy), whatever /t: says; /else: decides what then.',
        CHECK: '**CHECK name [/not] [/i] [/re] text**: a prerequisite, watched in all the output from here on; REQUIRE judges it. `/not`: met when the text does NOT appear. Once met, `{name}` is what it matched (its first group, if any).',
        REQUIRE: '**REQUIRE [/else:…] [names]**: stop here unless these CHECKs (or all of them) are met, saying which isn\'t and why; `/else:label` goes there instead.',
        PAGER: '**PAGER [/key:NAME] [/i] [/re] text**: from here on, a wait that sees the pager\'s prompt (`--More--`) presses Space (or the key) and keeps waiting, once per prompt. `PAGER OFF` forgets them.',
        '/not': '**/not**: (CHECK) met when the text does NOT appear.',
        '/key': '**/key:NAME**: (PAGER) the key to press, as SENDKEY writes it: ENTER, SPACE, q…',
        '/t': '**/t:seconds**: how long to wait (30 by default).',
        '/i': '**/i**: ignore case.',
        '/re': '**/re**: the text is a regular expression (JavaScript\'s; ^ and $ are each line\'s ends).',
        '/else': '**/else:continue | stop | label**: what a timeout does: carry on, stop (the default), or go to `:label`.',
        '/var': '**/var:name**: keep the answer in `{name}` instead of typing it.',
        '/append': '**/append**: add to the file instead of replacing it.',
        group: '**{0} … {9}**: what the last wait matched ({0}), then a regular expression\'s groups.',
        repeat: '**{repeat}**: the REPEAT\'s round: 1, 2…',
        session: '**The session\'s values**: {host_address}, {host_port}, {session_name}, {session_id}, {session_folder}, {session_username}, {session_profile}, {session_protocol}, from the tab\'s session as the script starts.',
        variable: '**{name}**: a variable the script SETs, a /var: answer, a named group, else an Environment Variable.'
    };

    /** The help for what's under the mouse on a line: its command, an option, or a {variable}; null for none */
    function helpAt(line, column) {
        var found = null;
        var command = /^\s*([A-Za-z]+)/.exec(line);
        if (command && column <= command[0].length + 1 && HELP[command[1].toUpperCase()]) {
            found = HELP[command[1].toUpperCase()];
        }
        var tokens = /\/(t|i|re|else|var|append|not|key|idle)\b[^\s]*|\{(\w+)\}/g;
        var match;
        while ((match = tokens.exec(line))) {
            if (column - 1 >= match.index && column - 1 <= match.index + match[0].length) {
                if (match[1]) { found = HELP['/' + match[1]]; }
                else {
                    var name = match[2].toLowerCase();
                    found = /^\d$/.test(name) ? HELP.group
                        : name === 'repeat' ? HELP.repeat
                        : /^(host_address|host_port|session_\w+)$/.test(name) ? HELP.session
                        : HELP.variable;
                }
            }
        }
        return found;
    }

    /**
     * The page's Monaco theme, as the app is: light or dark, or, for a theme of the user's own, one made from its
     * colours (the app's names for them: bgInput, text, textMuted, accent...) on its base, as the app's fields are
     * drawn: the box the fields' colour, the text and cursor the theme's, line numbers muted, and the selection and
     * current line in its accent. The words' own colours (SPSL's commands, Rhai's strings...) stay the base's
     */
    function theme(dark, colors) {
        var base = dark ? 'vs-dark' : 'vs';
        if (!colors) { return base; }
        monaco.editor.defineTheme('sxt-own', {
            base: base,
            inherit: true,
            rules: [],
            colors: {
                'editor.background': colors.bgInput,
                'editor.foreground': colors.text,
                'editorGutter.background': colors.bgInput,
                'editorLineNumber.foreground': colors.textMuted,
                'editorLineNumber.activeForeground': colors.text,
                'editorCursor.foreground': colors.text,
                'editor.selectionBackground': colors.accent + '80',
                'editor.inactiveSelectionBackground': colors.accent + '40',
                'editor.lineHighlightBackground': colors.accent + '1a',
                'editor.lineHighlightBorder': colors.accent + '33',
                'editorWidget.background': colors.bgPane,
                'editorWidget.foreground': colors.text,
                'editorWidget.border': colors.border,
                'editorHoverWidget.background': colors.bgPane,
                'editorHoverWidget.border': colors.border,
                'scrollbarSlider.background': colors.textMuted + '66',
                'scrollbarSlider.hoverBackground': colors.textMuted + '99',
                'scrollbarSlider.activeBackground': colors.textMuted + 'cc'
            }
        });
        return 'sxt-own';
    }

    /** One message from the main window */
    function onMessage(message) {
        if (!message || !editor) { return; }
        var model = editor.getModel();
        switch (message.type) {
            case 'set':
                monaco.editor.setModelLanguage(model, languageOf(message.language));
                editor.updateOptions({ wordWrap: message.wrap ? 'on' : 'off' });
                monaco.editor.setTheme(theme(message.dark, message.colors));
                if (model.getValue() !== message.text) { model.setValue(message.text || ''); }
                break;
            case 'text':
                // only when it differs, so the cursor stays put while the user types
                if (model.getValue() !== message.text) { model.setValue(message.text || ''); }
                break;
            case 'insert': {
                var selection = editor.getSelection();
                editor.executeEdits('insert', [{ range: selection, text: message.text, forceMoveMarkers: true }]);
                editor.focus();
                break;
            }
            case 'language':
                monaco.editor.setModelLanguage(model, languageOf(message.language));
                break;
            case 'wrap':
                editor.updateOptions({ wordWrap: message.wrap ? 'on' : 'off' });
                break;
            case 'dark':
                monaco.editor.setTheme(theme(message.dark, message.colors));
                break;
            case 'focus':
                editor.focus();
                break;
            case 'reveal':
                editor.setPosition({ lineNumber: message.line, column: 1 });
                editor.revealLineInCenterIfOutsideViewport(message.line);
                editor.focus();
                break;
        }
    }

    /** Create the editor, listen to the main window, and say the page is ready */
    function start() {
        monaco.languages.register({ id: 'spsl' });
        monaco.languages.setMonarchTokensProvider('spsl', SPSL);
        monaco.languages.setLanguageConfiguration('spsl', { comments: { lineComment: '#' } });
        monaco.languages.registerHoverProvider('spsl', {
            provideHover: function (model, position) {
                var help = helpAt(model.getLineContent(position.lineNumber), position.column);
                return help ? { contents: [{ value: help }] } : null;
            }
        });
        monaco.languages.register({ id: 'rhai' });
        monaco.languages.setMonarchTokensProvider('rhai', RHAI);
        monaco.languages.setLanguageConfiguration('rhai', {
            comments: { lineComment: '//', blockComment: ['/*', '*/'] },
            brackets: [['{', '}'], ['[', ']'], ['(', ')']],
            autoClosingPairs: [{ open: '{', close: '}' }, { open: '[', close: ']' }, { open: '(', close: ')' },
                { open: '"', close: '"', notIn: ['string'] }, { open: '`', close: '`', notIn: ['string'] }],
            surroundingPairs: [{ open: '{', close: '}' }, { open: '[', close: ']' }, { open: '(', close: ')' },
                { open: '"', close: '"' }, { open: '`', close: '`' }]
        });
        monaco.languages.registerHoverProvider('rhai', {
            provideHover: function (model, position) {
                var help = rhaiHelpAt(model.getLineContent(position.lineNumber), position.column);
                return help ? { contents: [{ value: help }] } : null;
            }
        });
        editor = monaco.editor.create(document.getElementById('editor'), {
            value: '',
            language: 'spsl',
            automaticLayout: true,
            scrollBeyondLastLine: false,
            minimap: { enabled: false },
            lineNumbers: 'on',
            fontFamily: 'Menlo, Consolas, "Courier New", monospace',
            fontSize: 12,
            tabSize: 2,
            renderLineHighlight: 'line',
            unicodeHighlight: { ambiguousCharacters: false, invisibleCharacters: false },
            occurrencesHighlight: 'off',
            links: false,
            // the SPSL panel beside the box lists the commands; Monaco's word suggestions would only get in the way
            quickSuggestions: false,
            wordBasedSuggestions: 'off'
        });
        editor.onDidChangeModelContent(function () { post({ type: 'change', text: editor.getValue() }); });
        if (host) {
            host.addEventListener('message', function (event) { onMessage(event.data); });
        }
        // for testing the page in a plain browser: scriptDev.message({...}), scriptDev.editor
        window.scriptDev = { message: onMessage, helpAt: helpAt, rhaiHelpAt: rhaiHelpAt, get editor() { return editor; } };
        post({ type: 'ready' });
    }

    window.require.config({ paths: { vs: '../lib/monaco/vs' } });
    window.require(['vs/editor/editor.main'], start);
})();
