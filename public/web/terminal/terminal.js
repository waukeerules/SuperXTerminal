/*
 * Built-in terminal page (see index.html and docs/MonacoTerminal.md, phase 2).
 *
 * Gui/TerminalControl runs plink in a pseudo console and sends its output here as base64 chunks, one at a time (each
 * answered with "ack"). xterm.js draws it; typing goes back as "input". It is meant to feel like PuTTY: select to copy,
 * Shift+Insert and middle-click paste, a blinking cursor only while focused, and a right-click menu (Copy, Paste,
 * Find..., Select All, Clear Scrollback, Clear Screen...). Lines are coloured live by the Highlighted Output rules,
 * using xterm.js decorations, so the bytes themselves never change.
 *
 * The session log is written from what this page shows (the pseudo console rewrites the stream, see the design doc):
 * a line is sent to TerminalControl ("log") once the cursor has left it, and the cursor's line is sent as "current",
 * so SPSL's WAITFOR can match a prompt before Enter is pressed.
 *
 * Messages from TerminalControl: config, data, notice, paste, focus, action, cursorColor, shown. To it: ready, size, input,
 * ack, log, copy, pasteRequest (every paste, so TerminalControl can ask first; with "text" for one not from the
 * clipboard), document, bell, tab, openInPutty (only while config.putty isn't false), special (a Special Command, when config.specials offers it), openLink (a link Ctrl+clicked), pref, cleared (the screen was cleared
 * here; Windows' pseudo console keeps its own copy, whose redraw after a resize the page drops: dropConptyRedraw).
 *
 * xterm.js add-ons: web links (Ctrl+click opens) and WebGL drawing (Options > Terminal > Drawing; the DOM renderer
 * takes over without it).
 */
(function () {
    'use strict';

    /** Width of xterm.js's scroll bar, left out when fitting the columns (as its fit addon does) */
    var SCROLLBAR_WIDTH = 14;
    /** Longest part of a line the rules look at */
    var MAX_MATCH_LENGTH = 2000;
    /** Most find matches marked at once */
    var MAX_FIND_MATCHES = 2000;
    /** Wait after output before highlighting and logging, so a burst is handled once */
    var SETTLE_MS = 30;

    var host = window.chrome && window.chrome.webview ? window.chrome.webview : null;
    var term;
    var ruleSets = [];
    var themes = [];
    var theme = null;
    var autoSet = null;
    var darkDefault = true;
    var bellFromProfile = 'visual'; // the PuTTY profile's bell, from TerminalControl
    var cursorFromProfile = { style: 'block', blink: false }; // the PuTTY profile's cursor, from TerminalControl
    var localShell = false;         // cmd, PowerShell or WSL rather than a connection through plink
    var serialSession = false;      // a serial port: Open in PuTTY replaces this tab, as the port can only be open once
    var specials = [];              // Special Commands the host sends itself ({label, command}): then no Open in PuTTY
    // whether there's a PuTTY to open the session in for the Special Commands the host can't send: SuperXPuTTY's
    // plink tabs have one; SuperXTerminal (config.putty false) has none, so its menu says why there are none instead
    var puttyToHand = true;
    var menuKeys = [];              // Send Key's keys ({key, label, hint}), from TerminalKeys.MenuKeys
    // the keys the menu shows for Copy, Paste and Find, as the app has them (SuperXTerminal: the Mac's ⌘C, ⌘V and its
    // Settings > Shortcuts Find); PuTTY's Windows keys where the app gives none (SuperXPuTTY)
    var keyLabels = {};
    var keysFromProfile = { backspace: 'del', homeEnd: 'standard', functionKeys: 'tilde' }; // the PuTTY profile's keys, from TerminalControl
    var sessionRules = '';          // the session's or its folder's highlighting rules; empty: Options > Terminal's
    var tabRules = null;            // rules chosen in this tab's menu, for this tab only; null: the session's or default
    var unlimitedScrollback = 1000000; // scrollback when the preference is 0, from TerminalControl
    var profileScrollback = 0;      // the profile's own lines kept (SuperXTerminal), over the preference's; 0 for none
    var webgl = null;               // xterm.js's WebGL renderer while it's drawing the terminal
    var cursorColor = null;         // the session's environment colour (#rrggbb) for the cursor, or null
    var settleTimer = null;
    var scanFrom = null;        // marker: first line to look at for highlighting next time
    var logged = null;          // marker: first line not yet sent to the log
    var lastCurrent = null;     // the cursor's line as last sent to the log
    var entries = [];           // highlighted lines: {marker, decorations, text}
    var find = { matches: [], index: -1, decorations: [], current: null, selected: null };
    var menuSelection = null;   // the selection when the right button went down: {text, position}
    var hostMenu = false;       // the host draws the right-click menu over its whole window ("config"'s hostMenu)
    var hostMenuItems = null;   // the items of the menu the host is showing, which its "menuChoose" names by place
    var selectionMatches = [];  // marks ({marker, decoration}) on the other places the selected text appears
    var flushMarker = null;     // the marker flushDecorations adds its throwaway decoration on
    var sized = false;
    // Whether the tab is in front (SuperXTerminal's "shown"; SuperXPuTTY's tabs always are). A tab behind another
    // still takes its session's output and logs it, but doesn't draw, highlight or resize until it's in front again
    // (setShown): every tab's page runs on the window's one thread, so a hundred tabs drawing unseen cost the one seen
    var shown = true;
    var owedHighlight = null;   // highlighting asked for while behind: 'all', or 'new' for the lines since
    var owedFocus = false;      // the keyboard was given to the tab while it was behind
    var webglLost = false;      // the browser took this tab's WebGL context: not asked for again until it's next shown
    var webglTimer = null;      // asks again for a WebGL context the browser took from a tab in front
    var webglRetries = 0;       // how often a tab in front has asked again for a context taken from it, since shown
    /** How often a tab in front asks again for a WebGL context the browser took, and how long after (lostWebgl) */
    var MAX_WEBGL_RETRIES = 3;
    var WEBGL_RETRY_MS = 2000;
    var behindTimer = null;     // stops laying the page out once a tab behind has had its size for a moment
    /**
     * How long a tab behind keeps its layout and its WebGL context after its size last changed (applyBehind): long
     * enough to settle on its size as it opens, and not to give them up for a moment's flick to another tab and back
     */
    var BEHIND_MS = 1000;

    /** Send a message to TerminalControl (ignored when the page is opened in a plain browser to test it) */
    function post(message) {
        if (host) {
            host.postMessage(message);
        } else if (window.terminalDev) {
            window.terminalDev.posted.push(message);
        }
    }

    /**
     * The terminal preferences (Utils/TerminalPreferences.cs): rule set, theme, font, cursor, bell, copy on select,
     * marking matches. SuperPuTTY keeps them, so Tools > Options > Terminal and every tab share them; they arrive in
     * "config" and again in "prefs" whenever they change.
     */
    var prefs = {};

    /** One preference, or null when it isn't set (empty counts as not set) */
    function loadPref(name) {
        var value = prefs[name];
        return value === undefined || value === null || value === '' ? null : String(value);
    }

    /** Change a preference here and have SuperPuTTY save it for every terminal tab */
    function savePref(name, value) {
        prefs[name] = String(value);
        post({ type: 'pref', name: name, value: String(value) });
    }

    // ---- size -------------------------------------------------------------------------------------------------

    /**
     * Fit the columns and rows to the tab, as xterm.js's fit addon does (it reads the same cell size), and tell
     * TerminalControl the first time and on every change
     */
    function fit() {
        var dimensions = term._core && term._core._renderService && term._core._renderService.dimensions;
        if (!dimensions || !dimensions.css.cell.width || !dimensions.css.cell.height) {
            return;
        }
        var element = document.getElementById('terminal');
        // before measuring: the gutter's width follows the font, and measured before it changed the terminal would be
        // sized twice for a new font (a theme's), the session told of a size in between
        if (document.body.classList.contains('numbers')) {
            var width = gutterWidthFor(gutterDigits, dimensions);
            if (width !== gutterWidth) {
                gutterWidth = width;
                document.documentElement.style.setProperty('--gutter', width + 'px');
            }
        }
        // before measuring: the side-to-side scroll bar takes its height from the rows
        document.body.classList.toggle('nowrap', wide());
        // a hidden tab has no size yet; wait for the ResizeObserver rather than start the session at 2x1
        if (element.clientWidth < SCROLLBAR_WIDTH + dimensions.css.cell.width * 2 || element.clientHeight < dimensions.css.cell.height) {
            return;
        }
        var cols = Math.max(2, Math.floor((element.clientWidth - SCROLLBAR_WIDTH) / dimensions.css.cell.width));
        var rows = Math.max(1, Math.floor(element.clientHeight / dimensions.css.cell.height));
        if (wide()) {
            var widest = Math.floor(MAX_CANVAS / (dimensions.css.cell.width * (window.devicePixelRatio || 1)));
            cols = Math.max(cols, Math.min(WIDE_COLS, widest));
        }
        if (cols !== term.cols || rows !== term.rows) {
            term.resize(cols, rows);
        }
        document.querySelector('#hscroll > div').style.width = (term.cols * dimensions.css.cell.width) + 'px';
        applyPan(wide() ? pan : 0);
        if (!sized) {
            sized = true;
            post({ type: 'size', cols: term.cols, rows: term.rows });
            // a tab opened behind another has its size now, for its session to start with: it can stop drawing
            applyBehind();
        }
    }

    // ---- behind another tab ---------------------------------------------------------------------------------------

    /**
     * Stop laying the page out while its tab is behind another (terminal.css's body.behind). xterm.js then stops
     * drawing by itself (it watches whether its screen shows), nothing is fitted (the page has no size: fit waits, so
     * the session isn't resized as the window is dragged), and it draws the screen afresh, and is fitted once, when
     * the page is laid out again.
     *
     * Not until the tab has had its size and that has been still for BEHIND_MS: a tab opened behind another
     * (Connect all, a layout's tabs) is sized twice as it starts, for the default font and then the theme's or the
     * profile's, and its session must start with the second, not be resized to it when the tab is first looked at.
     * It keeps its WebGL context until then too, and gives it up as it goes: the two renderers' cells differ in width
     * (WebGL's are whole pixels), so a tab sized with the DOM renderer would have other columns in front.
     */
    function applyBehind() {
        window.clearTimeout(behindTimer);
        if (shown || !sized) {
            document.body.classList.remove('behind');
            return;
        }
        behindTimer = window.setTimeout(function () {
            if (shown) { return; }
            document.body.classList.add('behind');
            dropWebgl();
        }, BEHIND_MS);
    }

    /** Whether the page has stopped being laid out, its tab behind another (applyBehind) */
    function behind() {
        return document.body.classList.contains('behind');
    }

    /**
     * Keep xterm.js from redrawing the selection while the page isn't laid out. It stops its ordinary drawing by
     * itself then, but not this: every scroll asks for the selection to be redrawn, there being one or not, and its
     * DOM renderer (which a tab behind another has, its WebGL context given up) answers by building every row of the
     * screen again, measuring each character in a page with no layout to measure in. Five tabs printing behind
     * another spent most of the window's time there. Held, the redraw is done when xterm.js next draws, once the
     * tab is in front (it keeps what was asked for, as it does for a redraw asked for before its first drawing).
     *
     * Through xterm.js's own render service, as fit reads the cell size from it: if a later xterm.js names these
     * differently, nothing is held and the page works as it did.
     */
    function holdSelectionRedraws() {
        var service = term._core && term._core._renderService;
        if (!service || typeof service.handleSelectionChanged !== 'function' || !service._selectionState) { return; }
        var redraw = service.handleSelectionChanged.bind(service);
        service.handleSelectionChanged = function (start, end, columnSelectMode) {
            if (!behind()) {
                redraw(start, end, columnSelectMode);
                return;
            }
            service._selectionState.start = start;
            service._selectionState.end = end;
            service._selectionState.columnSelectMode = columnSelectMode;
            service._needsSelectionRefresh = true;
        };
    }

    /**
     * The tab came to the front or went behind another ("shown", from SuperXTerminal's PageHost). Behind, the output
     * still goes into the terminal and the log, but nothing is highlighted and, after a moment (applyBehind), nothing
     * is drawn or fitted and the WebGL context is given up: a browser keeps about 16, so with more tabs than that
     * the newest took the oldest's.
     * In front again, it draws with WebGL again, is fitted to the tab as it is now, and the lines that came meanwhile
     * are highlighted.
     */
    function setShown(visible) {
        if (visible === shown) { return; }
        shown = visible;
        applyBehind();
        window.clearTimeout(webglTimer);
        if (!visible) { return; }
        webglLost = false;
        webglRetries = 0;
        applyRenderer();
        // the whole screen drawn once the page is laid out again, with what came and was selected meanwhile
        term.refresh(0, term.rows - 1);
        fit();
        if (owedHighlight) {
            var all = owedHighlight === 'all';
            owedHighlight = null;
            highlight(all);
        }
        drawNumbers();
        if (owedFocus) {
            owedFocus = false;
            term.focus();
        }
    }

    // ---- wrapping -------------------------------------------------------------------------------------------------

    /** Columns when Wrap Lines is off: lines up to this long stay whole, and the view pans along them */
    var WIDE_COLS = 300;
    /** Widest canvas WebGL draws, in device pixels (16384 on the Mac); very large fonts get fewer columns */
    var MAX_CANVAS = 16384;
    /** Cells kept between the cursor and the right edge when the view follows it along a long line */
    var FOLLOW_MARGIN = 8;
    /** How far the view is panned to the right, in pixels, while lines aren't wrapped */
    var pan = 0;
    /** When the user last typed, for following the cursor as the echo moves it */
    var typedAt = 0;
    /** Whether the cursor showed before it last moved: output then follows it, but not after the user panned away */
    var cursorShowed = true;

    /** Whether lines wrap at the tab's width (Wrap Lines, on unless turned off) */
    function wrapping() {
        return prefOn('wrap');
    }

    /**
     * Whether the terminal is wider than the tab: Wrap Lines off, and not in a full-screen program (vim, less and top
     * use the alternate screen), which gets the tab's own width so it draws to fit
     */
    function wide() {
        return !wrapping() && term.buffer.active.type === 'normal';
    }

    /** Width of one cell, in pixels; 0 before the renderer has measured it */
    function cellWidth() {
        var dimensions = term._core && term._core._renderService && term._core._renderService.dimensions;
        return dimensions ? dimensions.css.cell.width : 0;
    }

    /** Width of the part of the terminal that shows, in pixels (left of xterm.js's own scroll bar) */
    function visibleWidth() {
        return Math.max(0, document.getElementById('terminal').clientWidth - SCROLLBAR_WIDTH);
    }

    /**
     * Pan the view to `x` pixels from the start of the lines: the screen slides left under xterm.js's box, which
     * stays the tab's size so its own scroll bar stays at the edge, and is clipped to what shows. xterm.js reads mouse
     * positions from the screen's place on the page, so selecting still finds the right cells.
     */
    function applyPan(x) {
        var screen = document.querySelector('#terminal .xterm-screen');
        if (!screen) { return; }
        var visible = visibleWidth();
        var width = term.cols * cellWidth();
        pan = Math.round(Math.max(0, Math.min(x, width - visible)));
        screen.style.transform = pan ? 'translateX(' + (-pan) + 'px)' : '';
        screen.style.clipPath = wide() ? 'inset(0 ' + Math.max(0, width - pan - visible) + 'px 0 ' + pan + 'px)' : '';
        var bar = document.getElementById('hscroll');
        if (bar.scrollLeft !== pan) { bar.scrollLeft = pan; }
        cursorShowed = cursorShows();
    }

    /** Pan just far enough to show `width` pixels at `x`, keeping a margin; back to the start when they fit there */
    function panTo(x, width) {
        var visible = visibleWidth(), margin = FOLLOW_MARGIN * cellWidth();
        if (x + width <= visible) {
            applyPan(0);
        } else if (x + width > pan + visible) {
            applyPan(x + width - visible + margin);
        } else if (x < pan) {
            applyPan(x - margin);
        }
    }

    /** Whether the cursor is within the part of the line that shows */
    function cursorShows() {
        var x = term.buffer.active.cursorX * cellWidth();
        return x >= pan && x + cellWidth() <= pan + visibleWidth();
    }

    /**
     * Keep the cursor in view while lines aren't wrapped: at the prompt the view goes back to the start of the line,
     * and along a long command it follows the cursor
     */
    function followCursor() {
        if (!wide()) { return; }
        panTo(term.buffer.active.cursorX * cellWidth(), cellWidth());
    }

    /** Wrap Lines changed (this tab's menu, another tab's, or Settings > Terminal): refit the columns */
    function applyWrap() {
        fit();
        followCursor();
    }

    // ---- line numbers ---------------------------------------------------------------------------------------------

    /** Rows between the counts kept for numbering, so a scroll up a long scrollback doesn't count it all again */
    var NUMBER_STEP = 512;
    /** numberCounts[k]: lines (not wrapped rows) above row k * NUMBER_STEP; kept only above the screen, where rows stay put */
    var numberCounts = [0];
    /**
     * Lines dropped off the top of the scrollback since it began (or was cleared), so a line keeps its number. Counted
     * whether or not Line Numbers is on, and from before the gutter is first drawn: output that fills the scrollback
     * before then would otherwise leave the numbers starting again near 1
     */
    var droppedLines = 0;
    /** The scrollback's rows (xterm.js's list) whose dropped lines are being counted */
    var trimmedRows = null;
    /** The buffer's rows the numbers were counted on, and its width: either changing counts again */
    var numberedRows = null;
    var numberedCols = 0;
    var numberedLength = 0;
    var numbersFrame = 0;
    /** The gutter's width as last set, in pixels */
    var gutterWidth = 0;
    /** How many digits the gutter last had room for (the highest number's) */
    var gutterDigits = 4;

    /** The gutter's width for numbers of so many digits in the font's cells: room for 4 at least, so the terminal doesn't narrow at every tenfold */
    function gutterWidthFor(digits, dimensions) {
        return Math.ceil(Math.max(4, digits) * dimensions.css.cell.width) + 10;
    }
    /**
     * Where commands began, oldest first: a line marker (which moves as the scrollback does) and the colour of the
     * button that sent it ("" for a command typed, or a button without one). A line's number takes the colour of
     * the last command started on or above it.
     */
    var commandMarks = [];
    /** Commands remembered, at most: older ones' lines have long left the scrollback */
    var MAX_COMMAND_MARKS = 2000;

    /** A command starts on the cursor's line: from there the numbers are its button's colour, or the usual one */
    function markCommand(color) {
        if (term.buffer.active.type !== 'normal') { return; }
        var good = /^#[0-9a-f]{3,8}$/i.test(color || '') ? color : '';
        var last = commandMarks[commandMarks.length - 1];
        // nothing coloured yet, and nothing to stop colouring
        if (!good && (!last || !last.color)) { return; }
        var marker = term.registerMarker(0);
        if (!marker) { return; }
        // a second command on the same line takes it over
        if (last && !last.marker.isDisposed && last.marker.line === marker.line) {
            last.marker.dispose();
            commandMarks.pop();
        }
        commandMarks.push({ marker: marker, color: good });
        if (commandMarks.length > MAX_COMMAND_MARKS) { commandMarks.shift().marker.dispose(); }
        scheduleNumbers();
    }

    /** The colour of a row's number: the last command's started on or above it; "" for the usual */
    function commandColour(row) {
        for (var i = commandMarks.length - 1; i >= 0; i--) {
            var marker = commandMarks[i].marker;
            // a marker whose line left the scrollback is above every row
            if (marker.isDisposed || marker.line <= row) { return commandMarks[i].color; }
        }
        return '';
    }

    /** Whether Line Numbers is on (off unless turned on) and the screen is the output's, not a full-screen program's */
    function numbering() {
        return loadPref('lineNumbers') === 'true' && term.buffer.active.type === 'normal';
    }

    /** The scrollback's rows (xterm.js's own list), to know when it's a new one and when it drops rows */
    function normalRows() {
        var core = term._core;
        return core && core._bufferService && core._bufferService.buffers ? core._bufferService.buffers.normal.lines : null;
    }

    /**
     * Count the lines a full scrollback drops (each new line drops the oldest), so the numbers don't shift. Called as
     * the terminal opens and at every scroll, so a new list (after a reset) is watched from its first line on, long
     * before it's full and drops any
     */
    function watchTrims() {
        var rows = normalRows();
        if (!rows || rows === trimmedRows) { return; }
        trimmedRows = rows;
        droppedLines = 0;
        if (rows.onTrim) {
            rows.onTrim(function (amount) {
                if (rows !== trimmedRows) { return; }
                droppedLines += amount;
                numberCounts = [0];
            });
        }
    }

    /** Start counting again: the rows were reflowed (a new width), replaced (a reset) or cleared */
    function recountNumbers() {
        numberCounts = [0];
        numberedRows = normalRows();
        watchTrims();
        numberedCols = term.cols;
    }

    /** Lines that start in rows [from, to) of the normal buffer: rows a long line wraps onto aren't lines */
    function linesStarting(buffer, from, to) {
        var n = 0;
        for (var y = from; y < to; y++) {
            var line = buffer.getLine(y);
            if (line && !line.isWrapped) { n++; }
        }
        return n;
    }

    /** Lines above a row, counted from the kept counts on */
    function linesAbove(buffer, row) {
        var k = Math.floor(row / NUMBER_STEP);
        // the counts are only kept for rows above the screen, which output doesn't change
        var keep = Math.floor(buffer.baseY / NUMBER_STEP);
        while (numberCounts.length <= Math.min(k, keep)) {
            var j = numberCounts.length;
            numberCounts.push(numberCounts[j - 1] + linesStarting(buffer, (j - 1) * NUMBER_STEP, j * NUMBER_STEP));
        }
        var start = Math.min(k, numberCounts.length - 1);
        return numberCounts[start] + linesStarting(buffer, start * NUMBER_STEP, row);
    }

    /** Draw the numbers soon: once a frame however much output comes */
    function scheduleNumbers() {
        // a tab behind another shows no numbers: they're drawn when it's in front again (setShown)
        if (numbersFrame || !shown) { return; }
        numbersFrame = window.requestAnimationFrame(function () {
            numbersFrame = 0;
            drawNumbers();
        });
    }

    /**
     * Number the rows that show: a line's first row gets its number, the rows it wraps onto and the empty ones under
     * the cursor none. A full-screen program (less, vim, top) draws a picture rather than lines, so the gutter goes
     * while it runs (less -N numbers its own), and comes back after it.
     */
    function drawNumbers() {
        var on = numbering();
        var gutter = document.getElementById('gutter');
        if (!on) {
            if (document.body.classList.contains('numbers')) {
                document.body.classList.remove('numbers');
                gutterWidth = 0;
                fit();
            }
            return;
        }
        var buffer = term.buffer.active;
        var dimensions = term._core && term._core._renderService && term._core._renderService.dimensions;
        if (!dimensions || !dimensions.css.cell.height) { return; }
        var rows = normalRows();
        // fewer rows than before: the scrollback was cleared (Clear Scrollback, the clear command), so start at 1 again
        if (rows && rows.length < numberedLength) { numberedRows = null; droppedLines = 0; }
        if (rows !== numberedRows || term.cols !== numberedCols) {
            recountNumbers();
        }
        numberedLength = rows ? rows.length : 0;
        var last = buffer.baseY + buffer.cursorY;
        var top = buffer.viewportY;
        var first = droppedLines + linesAbove(buffer, top);
        var text = [];
        var coloured = false;
        var n = first;
        var widest = droppedLines + linesAbove(buffer, last) + 1;
        for (var y = top; y < top + term.rows; y++) {
            var line = buffer.getLine(y);
            if (!line || y > last) { text.push(''); continue; }
            if (line.isWrapped) { text.push(''); continue; }
            n++;
            var colour = commandMarks.length ? commandColour(y) : '';
            // digits and a checked colour only
            text.push(colour ? '<span style="color:' + colour + '">' + n + '</span>' : String(n));
            coloured = coloured || !!colour;
        }
        gutterDigits = String(widest).length;
        var width = gutterWidthFor(gutterDigits, dimensions);
        var style = gutter.style;
        style.fontFamily = term.options.fontFamily;
        style.fontSize = term.options.fontSize + 'px';
        style.lineHeight = dimensions.css.cell.height + 'px';
        if (coloured) { gutter.innerHTML = text.join('\n'); } else { gutter.textContent = text.join('\n'); }
        if (!document.body.classList.contains('numbers') || width !== gutterWidth) {
            gutterWidth = width;
            document.documentElement.style.setProperty('--gutter', width + 'px');
            document.body.classList.add('numbers');
            fit();
        }
    }

    // ---- output, logging and highlighting -----------------------------------------------------------------------

    /** Base64 from TerminalControl to bytes; xterm.js decodes UTF-8 across chunks itself */
    function decode(base64) {
        var binary = window.atob(base64);
        var bytes = new Uint8Array(binary.length);
        for (var i = 0; i < binary.length; i++) { bytes[i] = binary.charCodeAt(i); }
        return bytes;
    }

    // ---- Windows' pseudo console's redraw ----------------------------------------------------------------------------

    /**
     * Windows' pseudo console (ConPTY) keeps its own copy of the screen, and when the tab's new size reaches it (a
     * theme or font change, a resized window) it sends that whole screen again: the cursor hidden (ESC[?25l), the rows
     * from the top left (ESC[H), then the cursor shown again (ESC[?25h); for a quick run of resizes, once each, the
     * first also naming a size on the way (ESC[8;rows;cols t). After a clear here its copy still holds what was cleared, so the redraw brought it
     * back. ConPTY can't be told to forget it (ClearPseudoConsole is only in Windows Terminal's own conpty.dll, not in
     * Windows), so that redraw is dropped instead: the page already has everything since the clear. Once a screenful
     * of output has followed the clear, ConPTY's copy holds nothing older, and its redraws are drawn again.
     */
    var conpty = {
        cleared: null,     // marker on the cursor's line at the last clear, while ConPTY's screen may still hold older text
        resizedAt: 0,      // when the terminal last changed size (the new size goes to the session)
        dropping: false,   // inside a redraw being dropped
        dropTimer: null    // ends a drop whose ESC[?25h never came
    };
    /** How soon after a resize ConPTY's redraw comes: it goes through the app to ConPTY and back */
    var REDRAW_WITHIN_MS = 2000;
    /** How long a dropped redraw can take, so a lost ESC[?25h can't hide the session's output */
    var REDRAW_TAKES_MS = 500;

    /** A clear: ConPTY's screen still holds what was on it, until a screenful of output has followed */
    function clearedForConpty() {
        if (conpty.cleared) { conpty.cleared.dispose(); }
        conpty.cleared = term.registerMarker(0);
    }

    /** Whether ConPTY's screen may hold text cleared here: on the usual screen, less than a screenful since a clear */
    function conptyHoldsCleared() {
        var marker = conpty.cleared;
        var buffer = term.buffer.active;
        if (!marker || marker.isDisposed || buffer.type !== 'normal') { return false; }
        if (buffer.baseY + buffer.cursorY - marker.line < term.rows) { return true; }
        marker.dispose();
        conpty.cleared = null;
        return false;
    }

    /** The bytes with ConPTY's redraw after a resize left out, if it would bring back text cleared here */
    function dropConptyRedraw(bytes) {
        if (!conpty.dropping && !conptyHoldsCleared()) { return bytes; }
        // the escape sequences are ASCII: one character per byte keeps the positions
        var text = String.fromCharCode.apply(null, bytes);
        // the cursor hidden, then from the top left; only the first of a quick run of resizes' redraws names a size
        // between them, and it's the size that redraw was for, not the terminal's now
        var redraw = /\x1b\[\?25l(?:\x1b\[8;\d+;\d+t)?\x1b\[H/g;
        var pieces = [];
        var at = 0;
        // a run of resizes sends a redraw for each, and they can come together
        while (at < text.length) {
            if (!conpty.dropping) {
                redraw.lastIndex = at;
                var found = redraw.exec(text);
                if (!found || Date.now() - conpty.resizedAt > REDRAW_WITHIN_MS) { break; }
                pieces.push(bytes.subarray(at, found.index));
                at = found.index;
                conpty.dropping = true;
                window.clearTimeout(conpty.dropTimer);
                conpty.dropTimer = window.setTimeout(function () {
                    if (conpty.dropping) { conpty.dropping = false; term.write('\x1b[?25h'); }
                }, REDRAW_TAKES_MS);
            }
            var end = text.indexOf('\x1b[?25h', at);
            if (end < 0) { return joinBytes(pieces); }
            // the cursor shown again is kept, as the redraw hid it
            conpty.dropping = false;
            window.clearTimeout(conpty.dropTimer);
            at = end;
            if (!conptyHoldsCleared()) { break; }
            pieces.push(bytes.subarray(at, at + 6));
            at += 6;
        }
        pieces.push(bytes.subarray(at));
        return joinBytes(pieces);
    }

    /** Byte arrays one after another, as one */
    function joinBytes(pieces) {
        var length = pieces.reduce(function (sum, piece) { return sum + piece.length; }, 0);
        var joined = new Uint8Array(length);
        var at = 0;
        pieces.forEach(function (piece) { joined.set(piece, at); at += piece.length; });
        return joined;
    }

    /** Write output and ask for the next chunk once it's drawn */
    function write(bytes) {
        term.write(dropConptyRedraw(bytes), function () {
            post({ type: 'ack' });
            scheduleSettle();
        });
    }

    /** Highlight and log shortly after output stops arriving */
    function scheduleSettle() {
        if (settleTimer === null) {
            settleTimer = window.setTimeout(function () {
                settleTimer = null;
                highlight(false);
                sendLog();
            }, SETTLE_MS);
        }
    }

    /** A marker on an absolute buffer line (registerMarker counts from the cursor's line) */
    function markLine(line) {
        var buffer = term.buffer.active;
        return term.registerMarker(line - (buffer.baseY + buffer.cursorY));
    }

    /** The buffer line where the logical line holding a buffer line starts (wrapped lines continue the one above) */
    function logicalStart(line) {
        var buffer = term.buffer.active;
        while (line > 0 && buffer.getLine(line) && buffer.getLine(line).isWrapped) {
            line--;
        }
        return line;
    }

    /** Text of buffer lines [from, to), wrapped lines joined back into one */
    function readLines(from, to) {
        var buffer = term.buffer.active;
        var lines = [];
        for (var i = from; i < to; i++) {
            var line = buffer.getLine(i);
            var text = line ? line.translateToString(true) : '';
            if (line && line.isWrapped && lines.length) {
                lines[lines.length - 1] += text;
            } else {
                lines.push(text);
            }
        }
        return lines;
    }

    /**
     * A line as matching sees it, from buffer row `start`: its text, the rows a long line wraps onto joined back on
     * (so a match isn't missed, or coloured from its second character, where the line wraps), and for each character
     * of the text the row and column of the cell it's drawn in, and that cell's width. A character's place in the
     * text isn't its column: one taking two columns (a Chinese character, many emoji) counts once, an emoji is two
     * characters of text in one cell, and an accent added to a letter (e + U+0301) is text with no column of its own;
     * using the place in the text as the column put every highlight and find match after such a character off by a
     * column or more. Rows of plain ASCII, nearly all of them, are read as text at once: their characters and columns
     * are one to one.
     * @returns {{text: string, row: number[], col: number[], width: number[], rows: number}} rows: how many it spans
     */
    function lineCells(start) {
        var buffer = term.buffer.active;
        var cells = { text: '', row: [], col: [], width: [], rows: 0 };
        var cell;
        var r = start;
        var continues;
        do {
            var line = buffer.getLine(r);
            if (!line) { break; }
            var next = buffer.getLine(r + 1);
            continues = !!(next && next.isWrapped);
            // a row the line goes on from is read whole: its last cells are the line's, not padding
            var text = line.translateToString(!continues);
            if (!/[^\x20-\x7e]/.test(text)) {
                for (var k = 0; k < text.length; k++) {
                    cells.row.push(r); cells.col.push(k); cells.width.push(1);
                }
                cells.text += text;
            } else {
                var used = '';
                for (var c = 0; c < line.length; c++) {
                    cell = line.getCell(c, cell);
                    if (!cell) { break; }
                    var width = cell.getWidth();
                    // the second half of a character two columns wide
                    if (width === 0) { continue; }
                    var chars = cell.getChars() || ' ';
                    for (var u = 0; u < chars.length; u++) {
                        cells.row.push(r); cells.col.push(c); cells.width.push(width);
                    }
                    used += chars;
                }
                if (!continues) {
                    // trailing blank cells aren't the line's, as translateToString(true) leaves them off
                    var kept = used.replace(/ +$/, '').length;
                    cells.row.length = cells.col.length = cells.width.length = cells.text.length + kept;
                    used = used.substring(0, kept);
                }
                cells.text += used;
            }
            r++;
        } while (r < buffer.length && continues);
        cells.rows = Math.max(1, r - start);
        return cells;
    }

    /**
     * Where part of a line's text [start, end) is drawn: a stretch of cells on each row it's on (one, or more where the
     * line wraps), for a decoration each
     * @returns {Array<{row: number, x: number, width: number}>}
     */
    function cellSpans(cells, start, end) {
        var spans = [];
        end = Math.min(end, cells.text.length);
        for (var i = start; i < end; i++) {
            var row = cells.row[i], col = cells.col[i], right = col + cells.width[i];
            var last = spans[spans.length - 1];
            if (last && last.row === row && col <= last.x + last.width) {
                last.width = Math.max(last.width, right - last.x);
            } else {
                spans.push({ row: row, x: col, width: right - col });
            }
        }
        return spans;
    }

    /**
     * Send the session log what changed: lines the cursor has left, and the cursor's own line. A line is sent once,
     * when the cursor first moves below it, so "--More--" (erased on the same line) never reaches the log, and a
     * screen repainted after a resize isn't logged twice.
     */
    function sendLog() {
        var buffer = term.buffer.active;
        var cursorStart = logicalStart(buffer.baseY + buffer.cursorY);
        var from = logged && !logged.isDisposed ? logged.line : 0;
        var lines = cursorStart > from ? readLines(from, cursorStart) : [];
        if (cursorStart > from || !logged || logged.isDisposed) {
            if (logged) { logged.dispose(); }
            logged = markLine(Math.max(cursorStart, from));
        }
        var current = readLines(cursorStart, buffer.baseY + buffer.cursorY + 1).join('');
        if (lines.length || current !== lastCurrent) {
            lastCurrent = current;
            post({ type: 'log', lines: lines, current: current });
        }
        if (!autoSet) {
            lines.concat([current]).some(function (text) {
                autoSet = SpRules.detect(ruleSets, text);
                return autoSet !== null;
            });
            if (autoSet) { highlight(true); }
        }
    }

    /** The rules this tab uses: its menu's choice, else the session's or its folder's, else Options > Terminal's */
    function chosenRules() {
        return tabRules || sessionRules || loadPref('ruleSet') || 'auto';
    }

    /** A rules choice's name for the menu: Auto (with its pick), None, or the rule set's name */
    function rulesName(id) {
        if (id === 'auto') { return 'Auto' + (autoSet ? ': ' + autoSet.name : ''); }
        if (id === 'none') { return 'None'; }
        for (var i = 0; i < ruleSets.length; i++) {
            if (ruleSets[i].id === id) { return ruleSets[i].name; }
        }
        return id;
    }

    /** The rule set in use: the chosen one, Auto's pick, or none */
    function currentSet() {
        var chosen = chosenRules();
        if (chosen === 'auto') { return autoSet; }
        for (var i = 0; i < ruleSets.length; i++) {
            if (ruleSets[i].id === chosen) { return ruleSets[i]; }
        }
        return null;
    }

    /** Remove highlighted lines' decorations and markers (see unmark for the order) */
    function disposeEntries(list) {
        list.forEach(function (entry) { entry.decorations.forEach(function (d) { d.dispose(); }); });
        flushDecorations();
        list.forEach(function (entry) {
            entry.marker.dispose();
            // a wrapped line's markers on its other rows
            (entry.markers || []).forEach(function (m) { m.dispose(); });
        });
    }

    /**
     * Colour lines by the rules: everything when <paramref>all</paramref> (rules or theme changed), otherwise the
     * lines that may have changed since last time (the screen, and lines that scrolled off it since)
     */
    function highlight(all) {
        // behind another tab nothing shows: the lines are coloured when the tab is in front again (setShown), from
        // where the last colouring stopped
        if (!shown) {
            owedHighlight = all || owedHighlight === 'all' ? 'all' : 'new';
            return;
        }
        var buffer = term.buffer.active;
        var set = currentSet();
        if (all) {
            disposeEntries(entries);
            entries = [];
        }
        // from the start of the line the first row is part of, so a line wrapped onto it is matched whole
        var from = logicalStart(all || !scanFrom || scanFrom.isDisposed ? 0 : scanFrom.line);
        var to = buffer.length;
        entries = entries.filter(function (e) { return !e.marker.isDisposed; });
        var byLine = {};
        entries.forEach(function (e) { if (e.marker.line >= from) { byLine[e.marker.line] = e; } });

        for (var i = from; i < to; ) {
            var cells = lineCells(i);
            var text = cells.text;
            var next = i + cells.rows;
            var entry = byLine[i];
            if (entry && entry.text === text && entry.set === set) { i = next; continue; }
            if (entry) {
                disposeEntries([entry]);
                entries.splice(entries.indexOf(entry), 1);
            }
            var found = set ? SpRules.matches(set, text, MAX_MATCH_LENGTH) : [];
            if (!found.length) { i = next; continue; }
            // a marker on each of the line's rows, for its decorations there; the first is the entry's place
            var markers = [];
            for (var r = i; r < next; r++) { markers.push(markLine(r)); }
            if (!markers[0]) { markers.forEach(function (m) { if (m) { m.dispose(); } }); i = next; continue; }
            var decorations = [];
            var first = i;
            found.forEach(function (m) {
                var style = styleFor(m.style);
                if (!style || (!style.color && !style.background)) { return; }
                // a whole-line rule colours every row of the line; a match, the cells its characters are drawn in
                var spans = m.line
                    ? markers.map(function (_, k) { return { row: first + k, x: 0, width: term.cols }; })
                    : cellSpans(cells, m.start, m.end);
                spans.forEach(function (span) {
                    var marker = markers[span.row - first];
                    if (!marker) { return; }
                    var options = {
                        marker: marker, x: span.x, width: Math.max(1, Math.min(term.cols - span.x, span.width)), layer: 'top'
                    };
                    if (style.color) { options.foregroundColor = style.color; }
                    if (style.background) { options.backgroundColor = style.background; }
                    var decoration = term.registerDecoration(options);
                    if (decoration) { decorations.push(decoration); }
                });
            });
            entries.push({ marker: markers[0], markers: markers.slice(1).filter(Boolean), decorations: decorations, text: text, set: set });
            i = next;
        }
        if (scanFrom) { scanFrom.dispose(); }
        // next time, start at the top of the screen: lines above it can no longer change
        scanFrom = markLine(buffer.baseY);
    }

    // ---- clipboard ----------------------------------------------------------------------------------------------

    /** Copy the selection to the clipboard (TerminalControl sets it, the page has no clipboard access) */
    function copy() {
        var text = selectedText();
        if (text) {
            post({ type: 'copy', text: text });
        }
    }

    /** The selection, or while the menu is open, the one there was when the right button went down (rememberSelection) */
    function selectedText() {
        if (term.hasSelection()) { return term.getSelection(); }
        return menuSelection ? menuSelection.text : '';
    }

    /**
     * Remember the selection as the right button goes down. In SuperPuTTY the click also moves the keyboard focus to
     * the tab, and that can clear the selection before the menu opens; the menu then puts it back and copies it.
     */
    function rememberSelection() {
        menuSelection = term.hasSelection()
            ? { text: term.getSelection(), position: term.getSelectionPosition() }
            : null;
    }

    /** Select again what was selected when the right button went down, if it has been cleared since */
    function restoreSelection() {
        if (!menuSelection || term.hasSelection() || !menuSelection.position) { return; }
        var p = menuSelection.position;
        var length = (p.end.y - p.start.y) * term.cols + (p.end.x - p.start.x);
        if (length > 0) { term.select(p.start.x, p.start.y, length); }
    }

    // ---- selection: copy on select, and marking the other matches -------------------------------------------------

    /** Whether an on/off preference is on; both default to on, as in PuTTY and Notepad++ */
    function prefOn(name) {
        return loadPref(name) !== 'false';
    }

    /**
     * Put a background on part of a buffer line (find and selection matches)
     * @returns {{marker, decoration}|null} to pass to unmark
     */
    function mark(row, x, width, colour, textColour) {
        var marker = markLine(row);
        if (!marker) { return null; }
        var options = {
            marker: marker, x: x, width: Math.max(1, Math.min(width, term.cols - x)), backgroundColor: colour, layer: 'top'
        };
        // a bright mark needs dark text, whatever colour the rules gave it
        if (textColour) { options.foregroundColor = textColour; }
        var decoration = term.registerDecoration(options);
        return { marker: marker, decoration: decoration };
    }

    /**
     * Mark a match where it's drawn (cellSpans): a mark on each row it's on, so one a long line wraps across is marked
     * on both rows
     * @returns {Array<{marker, decoration}>} to pass to unmark
     */
    function markSpans(spans, colour, textColour) {
        return spans.map(function (span) { return mark(span.row, span.x, span.width, colour, textColour); }).filter(Boolean);
    }

    /**
     * Where a match begins and how many cells it spans, counting on across the rows it wraps onto, as term.select
     * takes them (its first cell's column and row, and a length that carries on to the next row at the right edge)
     */
    function selectionOf(spans) {
        var first = spans[0], last = spans[spans.length - 1];
        return { col: first.x, row: first.row, length: (last.row - first.row) * term.cols + last.x + last.width - first.x };
    }

    /**
     * Every match of a regular expression in the scrollback and screen, by whole lines (wrapped rows joined), each
     * with where it's drawn: at most `limit`
     * @returns {Array<{spans: Array<{row, x, width}>, col: number, row: number, length: number}>}
     */
    function findInBuffer(regex, limit) {
        var buffer = term.buffer.active;
        var found = [];
        for (var row = 0; row < buffer.length && found.length < limit; ) {
            var cells = lineCells(row);
            var match;
            regex.lastIndex = 0;
            while ((match = regex.exec(cells.text)) !== null && found.length < limit) {
                if (!match[0].length) { regex.lastIndex++; continue; }
                var spans = cellSpans(cells, match.index, match.index + match[0].length);
                if (!spans.length) { continue; }
                var at = selectionOf(spans);
                found.push({ spans: spans, col: at.col, row: at.row, length: at.length });
            }
            row += cells.rows;
        }
        return found;
    }

    /**
     * Remove marks: every decoration, then flushDecorations, then the markers. xterm.js 6.0.0 keeps decorations in a
     * list sorted by their marker's line, and a disposed marker's line becomes -1. A decoration disposed before its
     * marker is only queued for removal and stays in the list until later; if its marker is disposed meanwhile, the
     * list holds a -1 among the real lines, the next removal's binary search misses its decoration, and that one stays
     * for good under the wrong line: lookups for other lines then fail and their colours disappear.
     */
    function unmark(marks) {
        marks = marks.filter(Boolean);
        marks.forEach(function (m) { if (m.decoration) { m.decoration.dispose(); } });
        flushDecorations();
        marks.forEach(function (m) { m.marker.dispose(); });
    }

    /**
     * Make xterm.js take queued decoration removals out of its list now. There's no API for it, but adding a
     * decoration does it first; the one added here is removed again at once, on a marker that's never disposed by us.
     */
    function flushDecorations() {
        if (!flushMarker || flushMarker.isDisposed) {
            flushMarker = markLine(term.buffer.active.baseY + term.buffer.active.cursorY);
        }
        var decoration = flushMarker && term.registerDecoration({ marker: flushMarker });
        if (decoration) { decoration.dispose(); }
    }

    /** Remove the marks on the selection's other matches */
    function clearSelectionMatches() {
        unmark(selectionMatches);
        selectionMatches = [];
    }

    /**
     * Mark everywhere else the selected text appears, as Notepad++ does: case-insensitive, and as a whole word when
     * a whole word is selected. Only for a selection within one line, of 2 to 200 characters.
     */
    function markSelectionMatches() {
        clearSelectionMatches();
        // the find bar marks its own matches
        if (!prefOn('markMatches') || !term.hasSelection() || !document.getElementById('find').hidden) { return; }
        var text = term.getSelection();
        if (text.indexOf('\n') >= 0 || text.trim().length < 2 || text.length > 200) { return; }
        var escaped = text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
        var word = /^\w+$/.test(text);
        var regex = new RegExp(word ? '\\b' + escaped + '\\b' : escaped, 'gi');
        var colour = (theme && theme.colors && theme.colors.selectionMatch)
            || (theme && theme.base === 'vs' ? '#c8e6a0' : '#4a5a28');
        var selected = term.getSelectionPosition();
        findInBuffer(regex, MAX_FIND_MATCHES).forEach(function (m) {
            // not the selected text itself: the mark would hide the selection
            if (selected && m.row === selected.start.y && m.col === selected.start.x) { return; }
            selectionMatches = selectionMatches.concat(markSpans(m.spans, colour));
        });
    }

    /** Ask TerminalControl for the clipboard; it answers with "paste" */
    function paste() {
        post({ type: 'pasteRequest' });
    }

    // ---- clearing -----------------------------------------------------------------------------------------------

    /**
     * After clearing, start the log again at the cursor's line: what a clear moves or redraws was the screen, not
     * output, and is already in the log
     */
    function skipLog() {
        var buffer = term.buffer.active;
        if (logged) { logged.dispose(); }
        logged = markLine(logicalStart(buffer.baseY + buffer.cursorY));
    }

    /** Clear Scrollback: drop the lines above the screen, keep the screen */
    function clearScrollback() {
        term.write('\x1b[3J', function () { skipLog(); highlight(true); clearedForConpty(); });
    }

    /**
     * Clear Screen: move what's on screen into the scrollback and leave the cursor's line (the prompt) at the top,
     * the cursor where it was in it
     */
    function clearScreen() {
        var buffer = term.buffer.active;
        var x = buffer.cursorX;
        var line = readLines(logicalStart(buffer.baseY + buffer.cursorY), buffer.baseY + buffer.cursorY + 1).join('');
        term.write('\x1b[2J\x1b[H\x1b[2K' + line + '\x1b[' + (x + 1) + 'G', function () { skipLog(); highlight(true); clearedForConpty(); });
        post({ type: 'cleared' });
    }

    /** Clear Screen and Scrollback: only the cursor's line stays */
    function clearAll() {
        term.clear();
        skipLog();
        highlight(true);
        clearedForConpty();
        post({ type: 'cleared' });
    }

    // ---- find -----------------------------------------------------------------------------------------------------

    /** The key the menu shows for an item: the app's (keyLabels, even "" for none), else PuTTY's own */
    function menuKey(name, putty) {
        return Object.prototype.hasOwnProperty.call(keyLabels, name) ? keyLabels[name] : putty;
    }

    /** Open the find bar with the selection (if any) as the text */
    function openFind() {
        var bar = document.getElementById('find');
        var input = document.getElementById('findText');
        bar.hidden = false;
        if (term.hasSelection() && term.getSelection().indexOf('\n') < 0) {
            input.value = term.getSelection();
        }
        input.focus();
        input.select();
        runFind();
    }

    /** Close the find bar, remove its marks and go back to typing in the terminal */
    function closeFind() {
        document.getElementById('find').hidden = true;
        clearFindMarks();
        term.clearSelection();
        term.focus();
    }

    /**
     * Whether the selection is the match Find selected to show it, rather than one the user made while the find bar
     * is open: Copy on Select copies the user's, and would otherwise put each match found on the clipboard
     */
    function selectionIsFinds() {
        var p = term.getSelectionPosition();
        var f = find.selected;
        if (document.getElementById('find').hidden || !p || !f) { return false; }
        return p.start.x === f.start.x && p.start.y === f.start.y && p.end.x === f.end.x && p.end.y === f.end.y;
    }

    /** Remove the marks on every match, and on the current one */
    function clearFindMarks() {
        var marks = [];
        find.decorations.forEach(function (list) { if (list) { marks = marks.concat(list); } });
        unmark(marks.concat(find.current && find.current.mark ? find.current.mark : []));
        find.decorations = [];
        find.current = null;
    }

    /** Find every match in the scrollback and screen, mark them, and select the one nearest the end */
    function runFind() {
        clearFindMarks();
        var input = document.getElementById('findText');
        var text = input.value;
        var caseOn = document.getElementById('findCase').getAttribute('aria-pressed') === 'true';
        var regexOn = document.getElementById('findRegex').getAttribute('aria-pressed') === 'true';
        find.matches = [];
        find.index = -1;
        input.classList.remove('nomatch');
        if (!text) {
            document.getElementById('findCount').textContent = '';
            return;
        }
        var regex;
        try {
            regex = new RegExp(regexOn ? text : text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), caseOn ? 'g' : 'gi');
        } catch (e) {
            input.classList.add('nomatch');
            document.getElementById('findCount').textContent = 'bad regex';
            return;
        }
        find.matches = findInBuffer(regex, MAX_FIND_MATCHES);
        if (!find.matches.length) {
            input.classList.add('nomatch');
            document.getElementById('findCount').textContent = 'No results';
            return;
        }
        // marks for each match, on each row it's on (find.decorations[i] are match i's; null while it's the current one)
        find.decorations = find.matches.map(function (m) { return markSpans(m.spans, FIND_MATCH_COLOUR); });
        showMatch(find.matches.length - 1);
    }

    /** Background of every find match, and of the current one */
    var FIND_MATCH_COLOUR = '#806000', FIND_CURRENT_COLOUR = '#ffb000';

    /** Select a match and scroll it into the middle of the view */
    function showMatch(index) {
        if (!find.matches.length) { return; }
        find.index = (index + find.matches.length) % find.matches.length;
        var m = find.matches[find.index];
        term.select(m.col, m.row, m.length);
        // Find's own selection, which Copy on Select leaves alone (selectionIsFinds)
        find.selected = term.getSelectionPosition() || null;
        // marks cover the selection, so the current match gets a brighter mark instead of its usual one (with both,
        // xterm.js draws the one registered first on top, which is the usual one)
        if (find.current) {
            unmark(find.current.mark);
            var previous = find.matches[find.current.index];
            find.decorations[find.current.index] = markSpans(previous.spans, FIND_MATCH_COLOUR);
        }
        if (find.decorations[find.index]) {
            unmark(find.decorations[find.index]);
            find.decorations[find.index] = null;
        }
        find.current = { index: find.index, mark: markSpans(m.spans, FIND_CURRENT_COLOUR, '#000000') };
        term.scrollToLine(Math.max(0, m.row - Math.floor(term.rows / 2)));
        if (wide()) { panTo(m.col * cellWidth(), m.length * cellWidth()); }
        document.getElementById('findCount').textContent = (find.index + 1) + ' of ' + find.matches.length;
    }

    /** Wire up the find bar's text box and buttons */
    function bindFind() {
        var input = document.getElementById('findText');
        input.addEventListener('input', runFind);
        input.addEventListener('keydown', function (e) {
            if (e.key === 'Enter') { showMatch(find.index + (e.shiftKey ? -1 : 1)); e.preventDefault(); }
            else if (e.key === 'Escape') { closeFind(); e.preventDefault(); }
        });
        ['findCase', 'findRegex'].forEach(function (id) {
            document.getElementById(id).addEventListener('click', function () {
                var button = document.getElementById(id);
                button.setAttribute('aria-pressed', button.getAttribute('aria-pressed') === 'true' ? 'false' : 'true');
                runFind();
                input.focus();
            });
        });
        document.getElementById('findPrev').addEventListener('click', function () { showMatch(find.index - 1); });
        document.getElementById('findNext').addEventListener('click', function () { showMatch(find.index + 1); });
        document.getElementById('findClose').addEventListener('click', closeFind);
        document.getElementById('findOpen').addEventListener('click', openFind);
        titleFindButton();
    }

    /** The magnifying glass's tooltip names Find's key, as the menu's Find... does (it changes in Settings > Shortcuts) */
    function titleFindButton() {
        var key = menuKey('find', 'Ctrl+Shift+F');
        document.getElementById('findOpen').title = key ? 'Find (' + key + ')' : 'Find';
    }

    // ---- right-click menu -----------------------------------------------------------------------------------------

    /** The menu's items: {label, key, action, enabled, checked, items (a submenu)} or '-' for a separator */
    function menuItems() {
        var selected = selectedText() !== '';
        // a choice here is for this tab only; the session's (Edit Session), folder's or Options' rules are the default
        var defaultRules = sessionRules || loadPref('ruleSet') || 'auto';
        var ruleItems = [{ label: 'Default (' + rulesName(defaultRules) + ')', checked: tabRules === null, action: function () { chooseRules(null); } }, '-']
            .concat([{ label: 'Auto' + (autoSet ? ' (' + autoSet.name + ')' : ''), checked: tabRules === 'auto', action: function () { chooseRules('auto'); } }])
            .concat(ruleSets.map(function (set) {
                return { label: set.name, checked: tabRules === set.id, action: function () { chooseRules(set.id); } };
            }))
            .concat([{ label: 'None', checked: tabRules === 'none', action: function () { chooseRules('none'); } }]);
        var following = !profileTheme && loadPref('theme') === FOLLOW_LOOK;
        var themeItems = [{ label: "Follow the App's Look", checked: following, action: function () { savePref('theme', FOLLOW_LOOK); applyTheme(); } }, '-']
            .concat(themes.map(function (t) {
                return { label: t.name, checked: !following && !profileTheme && theme && theme.name === t.name, action: function () { savePref('theme', t.name); applyTheme(); } };
            }));
        // the profile's Colours name a theme (SuperXTerminal): it wins over the one chosen here
        if (profileTheme) { themeItems.unshift({ label: 'From the profile: ' + profileTheme, checked: true, enabled: false }, '-'); }
        var chosenFamily = loadPref('fontFamily') || '';
        var setFamily = function (value) { return function () { savePref('fontFamily', value); applyFont(); }; };
        var fontItems = [
            { label: 'Larger', key: 'Ctrl+Wheel', action: function () { stepFontSize(1); } },
            { label: 'Smaller', key: 'Ctrl+Wheel', action: function () { stepFontSize(-1); } },
            { label: 'Default Size (' + chosenFontDefaultSize() + ' px)', checked: !(parseInt(loadPref('fontSize'), 10) > 0), action: function () { savePref('fontSize', '0'); applyFont(); } },
            '-',
            { label: 'From Theme (' + themeFont().family.split(',')[0].replace(/"/g, '') + ')', checked: chosenFamily === '', action: setFamily('') },
            { label: 'From the Profile (' + profileFontName() + ')', checked: chosenFamily === 'profile', action: setFamily('profile') },
            '-'
        ].concat(installedFonts.map(function (name) {
            return { label: name, checked: chosenFamily === name, action: setFamily(name) };
        }));
        var shapeNames = { block: 'Block', underline: 'Underline', bar: 'Vertical Line' };
        var chosenShape = loadPref('cursorShape') || 'profile';
        var chosenBlink = loadPref('cursorBlink') || 'profile';
        var setCursor = function (name, value) { return function () { savePref(name, value); applyCursor(); }; };
        var cursorItems = [['profile', 'From PuTTY Profile (' + shapeNames[cursorFromProfile.style] + ')'], ['block', 'Block'], ['underline', 'Underline'], ['bar', 'Vertical Line']]
            .map(function (c) { return { label: c[1], checked: chosenShape === c[0], action: setCursor('cursorShape', c[0]) }; })
            .concat(['-'])
            .concat([['profile', 'Blink: From PuTTY Profile (' + (cursorFromProfile.blink ? 'On' : 'Off') + ')'], ['on', 'Blink'], ['off', 'Don\'t Blink']]
                .map(function (c) { return { label: c[1], checked: chosenBlink === c[0], action: setCursor('cursorBlink', c[0]) }; }));
        var chosenBell = loadPref('bell') || 'profile';
        var bellNames = { visual: 'Flash', sound: 'Sound', none: 'None' };
        var bellItems = [['profile', 'From PuTTY Profile (' + bellNames[bellFromProfile] + ')'], ['visual', 'Flash'], ['sound', 'Sound'], ['none', 'None']]
            .map(function (choice) {
                return { label: choice[1], checked: chosenBell === choice[0], action: function () { savePref('bell', choice[0]); } };
            });
        return [
            { label: 'Copy', key: menuKey('copy', 'Ctrl+Insert'), enabled: selected, action: copy },
            { label: 'Paste', key: menuKey('paste', 'Shift+Insert'), action: paste },
            // through TerminalControl, like every paste, so several lines are asked about first
            { label: 'Copy and Paste', enabled: selected, action: function () { var text = selectedText(); copy(); post({ type: 'pasteRequest', text: text }); } },
            '-',
            { label: 'Find...', key: menuKey('find', 'Ctrl+Shift+F'), action: openFind },
            { label: 'Select All', action: function () { term.selectAll(); } },
            '-',
            { label: 'Clear Scrollback', action: clearScrollback },
            { label: 'Clear Screen', action: clearScreen },
            { label: 'Clear Screen and Scrollback', action: clearAll },
            '-',
            { label: 'Open as Document', action: function () { post({ type: 'document' }); } },
            { label: 'Highlighting', items: ruleItems },
            { label: 'Theme', items: themeItems },
            { label: 'Bell', items: bellItems },
            { label: 'Font', items: fontItems },
            { label: 'Cursor', items: cursorItems },
            '-',
            { label: 'Copy on Select', checked: prefOn('copyOnSelect'), action: function () { togglePref('copyOnSelect'); } },
            { label: 'Mark Matches of Selection', checked: prefOn('markMatches'), action: function () { togglePref('markMatches'); markSelectionMatches(); } },
            { label: 'Wrap Lines', checked: wrapping(), action: function () { togglePref('wrap'); applyWrap(); } },
            { label: 'Line Numbers', checked: loadPref('lineNumbers') === 'true', action: function () { savePref('lineNumbers', loadPref('lineNumbers') === 'true' ? 'false' : 'true'); drawNumbers(); } },
            '-',
            // typed through the tab (ctlPuttyPanel.SendKey), as SENDKEY types them; Other Key... asks there
            { label: 'Send Key', items: menuKeys.map(function (k) {
                return { label: k.label, key: k.hint, action: function () { post({ type: 'sendKey', key: k.key }); } };
            }).concat(
                // a Break is a signal on the line, not a key: plink can't send one, PuTTY's Special Command can
                // (SuperXTerminal has no PuTTY, so there it's left out where the tab can't send one, as the tab's menu does)
                specials.some(function (s) { return s.command === 'break'; })
                    ? [{ label: 'Break', key: 'signal', action: function () { post({ type: 'special', command: 'break' }); } }, '-']
                    : puttyToHand ? [{ label: 'Break', key: 'needs PuTTY', enabled: false }, '-'] : []
            ).concat([
                { label: 'Other Key...', action: function () { post({ type: 'sendKey' }); } }
            ]) },
            '-'
        ].concat(specials.length ? [
            { label: 'Special Command', items: specials.map(function (s) {
                return { label: s.label, action: function () { post({ type: 'special', command: s.command }); } };
            }) },
            '-'
        ] : localShell ? [] : !puttyToHand ? [
            // SuperXTerminal: ssh can't send them and there's no PuTTY to open, so the menu says so, as the tab's does
            { label: 'Special Command', items: [{ label: "Not in the built-in terminal: ssh can't send them", enabled: false }] },
            '-'
        ] : [
            // plink can't send PuTTY's Special Commands (Break, signals...); PuTTY's own window can
            { label: serialSession ? 'Open in PuTTY (Special Commands, replaces this tab)' : 'Open in PuTTY (Special Commands)',
                action: function () { post({ type: 'openInPutty' }); } },
            '-'
        ]).concat([
            { label: 'Close Tab', action: function () { post({ type: 'tab', command: 'close' }); } },
            { label: 'Close Other Tabs', action: function () { post({ type: 'tab', command: 'closeOthers' }); } },
            { label: 'Close Tabs to the Right', action: function () { post({ type: 'tab', command: 'closeRight' }); } },
            { label: 'Close All Tabs', action: function () { post({ type: 'tab', command: 'closeAll' }); } }
        ]);
    }

    /** Build a menu element from items; submenus open on hover. isSubmenu: the rows are a submenu's */
    function buildMenu(element, items, isSubmenu) {
        element.innerHTML = '';
        items.forEach(function (item) {
            if (item === '-') {
                var separator = document.createElement('div');
                separator.className = 'separator';
                element.appendChild(separator);
                return;
            }
            var row = document.createElement('div');
            row.className = 'item' + (item.enabled === false ? ' disabled' : '') + (item.checked ? ' checked' : '');
            var label = document.createElement('span');
            label.textContent = item.label;
            row.appendChild(label);
            var key = document.createElement('span');
            key.className = 'key';
            key.textContent = item.items ? '▸' : (item.key || '');
            row.appendChild(key);
            if (item.items) {
                row.addEventListener('mouseenter', function () { openSubmenu(row, item.items); });
            } else {
                // a row of the main menu closes an open submenu, after a moment: the way to the submenu often
                // crosses the row below. A submenu's own rows leave it open.
                if (!isSubmenu) {
                    row.addEventListener('mouseenter', scheduleCloseSubmenu);
                }
                row.addEventListener('mousedown', function (e) { e.preventDefault(); });
                row.addEventListener('click', function () {
                    if (item.enabled === false) { return; }
                    // before hiding: hiding forgets the selection remembered for Copy
                    item.action();
                    hideMenu();
                    if (document.getElementById('find').hidden) { term.focus(); }
                });
            }
            element.appendChild(row);
        });
    }

    /** Show a submenu beside its row, kept on screen */
    function openSubmenu(row, items) {
        closeSubmenu();
        row.classList.add('open');
        var sub = document.createElement('div');
        sub.className = 'menu submenu';
        sub.id = 'submenu';
        buildMenu(sub, items, true);
        sub.addEventListener('mouseenter', cancelCloseSubmenu);
        document.body.appendChild(sub);
        var r = row.getBoundingClientRect();
        // overlapping the main menu a little leaves no gap to cross
        var left = r.right - 4;
        if (left + sub.offsetWidth > window.innerWidth) { left = r.left - sub.offsetWidth + 4; }
        sub.style.left = Math.max(0, left) + 'px';
        sub.style.top = Math.max(0, Math.min(r.top - 5, window.innerHeight - sub.offsetHeight)) + 'px';
    }

    /** Pending close of the submenu, from scheduleCloseSubmenu */
    var submenuCloseTimer = null;

    /** Close the submenu shortly, unless the mouse reaches it first */
    function scheduleCloseSubmenu() {
        cancelCloseSubmenu();
        submenuCloseTimer = window.setTimeout(closeSubmenu, 250);
    }

    /** The mouse reached the submenu (or another submenu opened): keep it */
    function cancelCloseSubmenu() {
        window.clearTimeout(submenuCloseTimer);
        submenuCloseTimer = null;
    }

    /** Close the open submenu, if any */
    function closeSubmenu() {
        cancelCloseSubmenu();
        var sub = document.getElementById('submenu');
        if (sub) { sub.remove(); }
        Array.prototype.forEach.call(document.querySelectorAll('.menu .item.open'), function (r) { r.classList.remove('open'); });
    }

    /** Show the right-click menu at the mouse, kept on screen */
    function showMenu(x, y) {
        var menu = document.getElementById('menu');
        buildMenu(menu, menuItems());
        menu.hidden = false;
        menu.style.left = Math.max(0, Math.min(x, window.innerWidth - menu.offsetWidth - 2)) + 'px';
        menu.style.top = Math.max(0, Math.min(y, window.innerHeight - menu.offsetHeight - 2)) + 'px';
    }

    /**
     * Have the host show the right-click menu at the mouse (SuperXTerminal): the page is only as big as its tab, a
     * small tile in Grid View, and a menu drawn here is cut off at its edges; the host draws it over the whole window.
     * It gets the rows without their actions, and says which was chosen ("menuChoose", chooseFromHost)
     */
    function showHostMenu(x, y) {
        hostMenuItems = menuItems();
        var rows = function (items) {
            return items.map(function (item) {
                if (item === '-') { return { separator: true }; }
                return {
                    label: item.label,
                    key: item.items ? '' : (item.key || ''),
                    enabled: item.enabled !== false,
                    checked: item.checked === undefined ? undefined : !!item.checked,
                    items: item.items ? rows(item.items) : undefined
                };
            });
        };
        post({ type: 'menu', x: x, y: y, items: rows(hostMenuItems) });
    }

    /**
     * Run the item chosen in the host's menu, found by its place: path[0] in the menu, path[1] in that row's submenu
     * @param path Where the item is
     */
    function chooseFromHost(path) {
        var items = hostMenuItems;
        var item = null;
        (path || []).forEach(function (index) {
            item = items && items[index];
            items = item && item.items;
        });
        hostMenuItems = null;
        if (!item || item === '-' || item.items || item.enabled === false) { return; }
        item.action();
        menuSelection = null;
        if (document.getElementById('find').hidden) { term.focus(); }
    }

    /** Hide the right-click menu and its submenu, and forget the selection remembered for it */
    function hideMenu() {
        menuSelection = null;
        closeSubmenu();
        document.getElementById('menu').hidden = true;
    }

    /** Turn an on/off preference over, for every terminal tab */
    function togglePref(name) {
        savePref(name, prefOn(name) ? 'false' : 'true');
    }

    /** Choose this tab's highlighting rules; null goes back to the session's or default */
    function chooseRules(id) {
        tabRules = id;
        highlight(true);
    }

    // ---- theme ----------------------------------------------------------------------------------------------------

    /** The theme preference that follows the app's light or dark look (a theme file can't be named so) */
    var FOLLOW_LOOK = '@look';

    /** The profile's own colours (background, foreground, selection, cursor), over the preferences' */
    var profileColours = {};
    /** The theme the profile names (its Colours: Dark or Light), over the preference's; null for the preference's */
    var profileTheme = null;

    /** Apply the chosen theme (PuTTY's colours by default): terminal colours, font and the highlighting colours */
    function applyTheme() {
        var wanted = profileTheme || loadPref('theme');
        // Follow the App's Look (SuperXTerminal's "@look"): its Light or Dark theme as the app is light or dark
        if (wanted === FOLLOW_LOOK) { wanted = darkDefault ? 'Dark' : 'Light'; }
        theme = themes.filter(function (t) { return t.name === wanted; })[0]
            || themes.filter(function (t) { return t.name === 'PuTTY'; })[0]
            || themes.filter(function (t) { return (t.base === 'vs-dark') === darkDefault; })[0]
            || themes[0] || null;
        if (!theme) { return; }
        var colors = ownColours(theme.colors || {});
        var names = ['black', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white',
            'brightBlack', 'brightRed', 'brightGreen', 'brightYellow', 'brightBlue', 'brightMagenta', 'brightCyan', 'brightWhite'];
        var xtermTheme = {
            background: colors.background,
            foreground: colors.foreground,
            // the session's environment colour (red for production) when it has one, as on its tab
            cursor: cursorColor || colors.cursor || colors.foreground,
            cursorAccent: colors.background,
            selectionBackground: colors.selection
        };
        (colors.ansi || []).slice(0, 16).forEach(function (c, i) { xtermTheme[names[i]] = c; });
        term.options.theme = xtermTheme;
        applyFont();
        document.documentElement.style.setProperty('--bg', colors.background || '#000000');
        document.documentElement.style.setProperty('--line-numbers', colors.lineNumbers || (theme.base === 'vs' ? '#8a8a8a' : '#6e7681'));
        // the menu and the find bar follow the theme: light ones for a light theme (Monaco's base "vs")
        document.documentElement.classList.toggle('light-theme', ownBackground() ? !isDarkColour(colors.background) : theme.base === 'vs');
        window.setTimeout(fit, 0);
        highlight(true);
    }

    /**
     * The theme's colours with the own background, text, selection and cursor colours over them: the profile's (SuperXTerminal
     * sends them in config.colors), else the preferences' (Settings > Terminal)
     */
    function ownColours(colors) {
        var own = {};
        Object.keys(colors).forEach(function (k) { own[k] = colors[k]; });
        [['colorBackground', 'background'], ['colorForeground', 'foreground'], ['colorSelection', 'selection'], ['colorCursor', 'cursor']].forEach(function (pair) {
            var value = profileColours[pair[1]] || loadPref(pair[0]);
            if (value && /^#[0-9a-f]{6}$/i.test(value)) { own[pair[1]] = value; }
        });
        return own;
    }

    /** Whether the background isn't the theme's: then the menu and find bar go by how dark it is */
    function ownBackground() {
        return /^#[0-9a-f]{6}$/i.test(profileColours.background || loadPref('colorBackground') || '');
    }

    /** Whether a "#rrggbb" colour is dark, so the menu and find bar on it are drawn dark */
    function isDarkColour(hex) {
        var n = parseInt(String(hex || '#000000').slice(1), 16);
        var r = (n >> 16) & 255, g = (n >> 8) & 255, b = n & 255;
        return 0.299 * r + 0.587 * g + 0.114 * b < 140;
    }

    // ---- messages and start ---------------------------------------------------------------------------------------

    /** A highlighting style from the theme */
    function styleFor(name) {
        return theme && theme.styles ? theme.styles[name] : null;
    }

    // ---- bell -----------------------------------------------------------------------------------------------------

    /** Bell times within the over-use window, and when the bell went quiet for over-use */
    var bellTimes = [], bellMutedUntil = 0;

    /** The PuTTY profile's over-use settings (Terminal > Bell), from TerminalControl; PuTTY's defaults until then */
    var bellOverload = { on: true, count: 5, windowMs: 2000, quietMs: 5000 };

    /** What the bell does: the menu's choice, else the PuTTY profile's ("none", "visual" or "sound") */
    function bellStyle() {
        var chosen = loadPref('bell') || 'profile';
        return chosen === 'profile' ? bellFromProfile : chosen;
    }

    /**
     * The device rang the bell (e.g. Backspace at the start of a line): flash the terminal or play the system's
     * alert sound, as PuTTY does. As in PuTTY's profile (5 bells in 2 seconds by default), over-use silences it
     * until that many quiet seconds; the bell that reaches the count is the first one silenced, as in PuTTY.
     */
    function ring() {
        var now = Date.now();
        if (bellOverload.on) {
            if (now < bellMutedUntil) {
                bellMutedUntil = now + bellOverload.quietMs;
                return;
            }
            bellTimes = bellTimes.filter(function (t) { return now - t < bellOverload.windowMs; });
            bellTimes.push(now);
            if (bellTimes.length >= bellOverload.count) {
                bellMutedUntil = now + bellOverload.quietMs;
                bellTimes = [];
                return;
            }
        }
        var style = bellStyle();
        if (style === 'sound') {
            post({ type: 'bell' });
        } else if (style === 'visual') {
            var element = document.getElementById('terminal');
            element.classList.remove('bell');
            // restart the animation even when bells come quickly
            void element.offsetWidth;
            element.classList.add('bell');
        }
    }

    // ---- font -----------------------------------------------------------------------------------------------------

    /**
     * The profile's font (PuTTY's Window > Appearance; SuperXTerminal's Settings > Profiles > Font), from
     * TerminalControl: its family ("" for none) and size in points (0 for none). What it doesn't set is the theme's
     */
    var profileFont = { family: '', points: 0 };

    /** The installed monospaced fonts, from TerminalControl */
    var installedFonts = [];

    /** Smallest and largest font size, in pixels */
    var MIN_FONT = 6, MAX_FONT = 48;

    /** The theme's font, used when the preference doesn't name one */
    function themeFont() {
        var font = theme && theme.font ? theme.font : {};
        return { family: font.family || 'Consolas, "Courier New", monospace', size: font.size || 14 };
    }

    /**
     * The font to draw with: the preference's family (the theme's, the profile's, or a named font) and size (the
     * preference's, else the profile's points as pixels at 96 dpi, else the theme's). The profile's family or size,
     * when it has none, is the theme's
     */
    function chosenFont() {
        var family = loadPref('fontFamily');
        var size = parseInt(loadPref('fontSize'), 10) || 0;
        var font;
        if (family === 'profile') {
            font = themeFont();
            if (profileFont.family) { font.family = '"' + profileFont.family + '", monospace'; }
            if (profileFont.points > 0) { font.size = Math.round(profileFont.points * 96 / 72); }
        } else if (family) {
            font = { family: '"' + family + '", monospace', size: themeFont().size };
        } else {
            font = themeFont();
        }
        if (size > 0) { font.size = size; }
        return font;
    }

    /** The profile's font as the menu names it: "Cascadia Mono, 12 pt", or the theme's for what it doesn't set */
    function profileFontName() {
        var family = profileFont.family || themeFont().family.split(',')[0].replace(/"/g, '') + ' from the theme';
        return family + (profileFont.points > 0 ? ', ' + profileFont.points + ' pt' : '');
    }

    /** Draw with the chosen font, and refit the columns and rows to it */
    function applyFont() {
        var font = chosenFont();
        term.options.fontFamily = font.family;
        term.options.fontSize = Math.max(MIN_FONT, Math.min(MAX_FONT, font.size));
        window.setTimeout(fit, 0);
    }

    /** The size the font has when no size is chosen: the profile's or the theme's */
    function chosenFontDefaultSize() {
        var saved = prefs.fontSize;
        prefs.fontSize = '0';
        var size = chosenFont().size;
        prefs.fontSize = saved;
        return size;
    }

    /** Make the font a step larger or smaller (the menu, Ctrl+mouse wheel), for every terminal tab */
    function stepFontSize(step) {
        var size = Math.max(MIN_FONT, Math.min(MAX_FONT, term.options.fontSize + step));
        if (size !== term.options.fontSize) {
            savePref('fontSize', size);
            applyFont();
        }
    }

    // ---- cursor ---------------------------------------------------------------------------------------------------

    /** The cursor's shape: the menu's choice, else the PuTTY profile's ("block", "underline" or "bar") */
    function cursorShape() {
        var chosen = loadPref('cursorShape') || 'profile';
        return chosen === 'profile' ? cursorFromProfile.style : chosen;
    }

    /** Whether the cursor blinks: the menu's choice, else the PuTTY profile's */
    function cursorBlinks() {
        var chosen = loadPref('cursorBlink') || 'profile';
        return chosen === 'profile' ? !!cursorFromProfile.blink : chosen === 'on';
    }

    /**
     * Shape and blink the cursor as the PuTTY profile (Window > Appearance) or the menu says. Without the keyboard it
     * shows as an outline for a block, as PuTTY does, and as the same line otherwise.
     */
    function applyCursor() {
        var shape = cursorShape();
        term.options.cursorStyle = shape;
        term.options.cursorInactiveStyle = shape === 'block' ? 'outline' : shape;
        term.options.cursorBlink = cursorBlinks();
    }

    /**
     * The preferences changed (this tab's menu, another tab's, or Tools > Options > Terminal): apply them all. The
     * theme also applies the font and redoes the highlighting.
     */
    function applyPrefs(changed) {
        prefs = changed || {};
        applyScrollback();
        applyRenderer();
        applyTheme();
        applyCursor();
        applyWrap();
        drawNumbers();
        markSelectionMatches();
    }

    /**
     * Keep the profile's number of lines (SuperXTerminal's Settings > Profiles > Scrollback), else the preference's
     * (Options > Terminal); 0 or less: as many as unlimitedScrollback
     */
    function applyScrollback() {
        var lines = profileScrollback > 0 ? profileScrollback : parseInt(loadPref('scrollback'), 10);
        var wanted = lines > 0 ? lines : unlimitedScrollback;
        if (term.options.scrollback !== wanted) { term.options.scrollback = wanted; }
    }

    /**
     * Draw with xterm.js's WebGL renderer (the default; much faster with a lot of output) or its DOM renderer
     * (renderer "dom" in Options > Terminal). Without WebGL, or when the graphics driver drops its context, the DOM
     * renderer takes over. A tab behind another has no WebGL context (applyBehind), and one whose context the browser
     * took (more tabs showing at once, in a grid, than it keeps contexts for) doesn't ask again until it's next shown:
     * asking with every change of a preference took another tab's context in turn, the tab in front's among them.
     */
    function applyRenderer() {
        var wanted = loadPref('renderer') !== 'dom' && !behind() && !webglLost;
        if (wanted && !webgl && window.WebglAddon) {
            try {
                var addon = new window.WebglAddon.WebglAddon();
                addon.onContextLoss(lostWebgl);
                term.loadAddon(addon);
                webgl = addon;
            } catch (e) {
                webgl = null;
            }
            fit();
        } else if (!wanted && webgl) {
            dropWebgl();
        }
    }

    /**
     * The browser took this tab's WebGL context (it keeps about 16, the newest): the DOM renderer draws meanwhile. A
     * tab in front asks for one again a little later, by when the tabs behind have given theirs up (opening many at
     * once, each takes one as it opens in front of the last); a few times only, as tabs side by side in a grid, more
     * than the browser keeps contexts for, would take each other's in turn for ever.
     */
    function lostWebgl() {
        webglLost = true;
        dropWebgl();
        if (!shown || webglRetries >= MAX_WEBGL_RETRIES) { return; }
        webglRetries += 1;
        window.clearTimeout(webglTimer);
        webglTimer = window.setTimeout(function () {
            if (shown && !webgl) {
                webglLost = false;
                applyRenderer();
            }
        }, WEBGL_RETRY_MS);
    }

    /** Stop using the WebGL renderer; xterm.js goes back to the DOM one */
    function dropWebgl() {
        if (!webgl) { return; }
        var addon = webgl;
        webgl = null;
        try { addon.dispose(); } catch (e) { /* already gone with its context */ }
        fit();
    }

    /** Apply a config message: rule sets, themes and whether SuperPuTTY is dark; then fit and report the size */
    function configure(config) {
        darkDefault = !!config.dark;
        prefs = config.prefs || {};
        if (config.profileFont) { profileFont = config.profileFont; }
        installedFonts = config.fonts || [];
        bellFromProfile = config.bell || 'visual';
        if (config.bellOverload) { bellOverload = config.bellOverload; }
        if (config.cursor) { cursorFromProfile = config.cursor; }
        localShell = !!config.local;
        serialSession = !!config.serial;
        specials = config.specials || [];
        puttyToHand = config.putty !== false;
        menuKeys = config.menuKeys || [];
        hostMenu = !!config.hostMenu;
        keyLabels = config.keyLabels || {};
        titleFindButton();
        if (config.keys) { keysFromProfile = config.keys; }
        sessionRules = config.sessionRules || '';
        if (config.unlimitedScrollback) { unlimitedScrollback = config.unlimitedScrollback; }
        profileScrollback = config.profileScrollback > 0 ? config.profileScrollback : 0;
        applyCursor();
        cursorColor = config.cursorColor || null;
        profileColours = config.colors || {};
        profileTheme = config.profileTheme || null;
        themes = config.themes || [];
        var compiled = SpRules.compile(config.ruleSets || []);
        ruleSets = compiled.sets;
        autoSet = null;
        applyScrollback();
        applyRenderer();
        applyTheme();
        fit();
        if (compiled.problems.length) {
            notice('Some highlighting rules were skipped: ' + compiled.problems.join('; '), 'info');
        }
    }

    /** Show a message from SuperPuTTY in the terminal, on its own line, in red (errors), yellow (warnings) or cyan */
    function notice(text, kind) {
        var colour = kind === 'error' ? '1;31' : kind === 'warn' ? '33' : '36';
        var buffer = term.buffer.active;
        var lead = buffer.cursorX > 0 ? '\r\n' : '';
        term.write(lead + '\x1b[' + colour + 'm' + text + '\x1b[0m\r\n');
    }

    /** Where a decoration is drawn, for the tests (terminalDev): its row, column and width in cells */
    function placeOf(decoration) {
        return { row: decoration.marker.line, x: decoration.options.x || 0, width: decoration.options.width || 1 };
    }

    /** Handle a message from TerminalControl */
    function onMessage(message) {
        if (!message || !message.type) { return; }
        switch (message.type) {
            case 'config': configure(message); break;
            case 'data': write(decode(message.data)); break;
            case 'notice': notice(message.text, message.kind); break;
            case 'paste': if (message.text) { term.paste(message.text); } break;
            // behind another tab there's nothing to focus: it takes the keyboard when it's in front (setShown)
            case 'focus': if (shown) { term.focus(); } else { owedFocus = true; } break;
            // the tab came to the front or went behind another (SuperXTerminal)
            case 'shown': setShown(message.visible !== false); break;
            case 'menuChoose': chooseFromHost(message.path); break;
            case 'action': runAction(message.name); break;
            case 'cursorColor': cursorColor = message.color || null; applyTheme(); break;
            // the app went light or dark (SuperXTerminal): a theme that follows it changes with it
            case 'look': darkDefault = !!message.dark; applyTheme(); break;
            case 'prefs': applyPrefs(message.prefs); break;
            // Settings > Shortcuts changed: the menu shows the new keys (SuperXTerminal)
            case 'keyLabels': keyLabels = message.keyLabels || {}; titleFindButton(); break;
            // a button's command (SuperXTerminal): its lines' numbers in the button's colour
            case 'commandColor': markCommand(message.color); break;
            // the profile was saved with other colours (SuperXTerminal)
            case 'colors': profileColours = message.colors || {}; profileTheme = message.theme || null; applyTheme(); break;
            // the profile was saved with another font or size (SuperXTerminal)
            case 'profileFont': profileFont = message.font || { family: '', points: 0 }; applyFont(); break;
            // the profile was saved with other Backspace, Home/End or function keys (SuperXTerminal)
            case 'keys': if (message.keys) { keysFromProfile = message.keys; } break;
            // the profile was saved with another Scrollback (SuperXTerminal)
            case 'profileScrollback': profileScrollback = message.lines > 0 ? message.lines : 0; applyScrollback(); break;
        }
    }

    /**
     * The tab menu's PuTTY items (Clear Scrollback, Reset Terminal, Copy All to Clipboard), sent by TerminalControl, and
     * SuperXTerminal's Find… (its shortcut is caught before the page sees it, so it can't come as the page's own key)
     */
    function runAction(name) {
        switch (name) {
            case 'find': openFind(); break;
            case 'clearScrollback': clearScrollback(); break;
            case 'reset':
                term.reset();
                skipLog();
                highlight(true);
                clearedForConpty();
                post({ type: 'cleared' });
                break;
            case 'copyAll':
                term.selectAll();
                copy();
                term.clearSelection();
                break;
        }
    }

    /**
     * PuTTY's keys that the terminal itself doesn't know; anything else goes to the session as typed. Shift+Insert and
     * Ctrl+Shift+V stop the browser's own paste too: it reaches the paste listener below as a second paste request,
     * with the clipboard's text, and the line went in twice.
     */
    function onKey(e) {
        if (e.type !== 'keydown') { return true; }
        var key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
        if (e.shiftKey && !e.ctrlKey && key === 'Insert') { e.preventDefault(); paste(); return false; }
        if (e.ctrlKey && !e.shiftKey && key === 'Insert') { copy(); return false; }
        if (e.ctrlKey && e.shiftKey && key === 'c') { copy(); return false; }
        if (e.ctrlKey && e.shiftKey && key === 'v') { e.preventDefault(); paste(); return false; }
        if (e.ctrlKey && e.shiftKey && key === 'f') { openFind(); return false; }
        var sequence = profileKey(e);
        if (sequence !== null) {
            // also stops the browser's own F5 (reload) and F12
            e.preventDefault();
            post({ type: 'input', data: sequence });
            return false;
        }
        return true;
    }

    /** xterm's codes for F1-F12 in PuTTY's default "ESC[n~" mode (Terminal > Keyboard > Function keys) */
    var TILDE_CODES = [11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 23, 24];

    /**
     * What PuTTY would send for Backspace, Home, End or F1-F12 with this session's profile (Terminal > Keyboard), where
     * that differs from xterm.js: Backspace as Control-H, Home/End as ESC[1~ / ESC[4~ (or rxvt's), function keys in the
     * profile's style. Only for connections through plink, and not when Options > Terminal says to use xterm's keys.
     */
    function profileKey(e) {
        if (localShell || loadPref('keys') === 'xterm' || e.altKey || e.metaKey) { return null; }
        var keys = keysFromProfile;
        if (e.key === 'Backspace' && !e.shiftKey && keys.backspace === 'bs') { return e.ctrlKey ? '\x7f' : '\x08'; }
        if (e.ctrlKey || e.shiftKey) { return null; }
        if (e.key === 'Home') { return keys.homeEnd === 'rxvt' ? '\x1b[H' : '\x1b[1~'; }
        if (e.key === 'End') { return keys.homeEnd === 'rxvt' ? '\x1bOw' : '\x1b[4~'; }
        var f = /^F(\d+)$/.exec(e.key);
        var n = f ? parseInt(f[1], 10) : 0;
        if (n < 1 || n > 12) { return null; }
        switch (keys.functionKeys) {
            case 'linux': if (n <= 5) { return '\x1b[[' + 'ABCDE'.charAt(n - 1); } break;
            case 'xtermr6':
            case 'xterm216': if (n <= 4) { return '\x1bO' + 'PQRS'.charAt(n - 1); } break;
            case 'vt100': return '\x1bO' + 'PQRSTUVWXYZ['.charAt(n - 1);
            case 'sco': return '\x1b[' + 'MNOPQRSTUVWX'.charAt(n - 1);
        }
        return '\x1b[' + TILDE_CODES[n - 1] + '~';
    }

    /** Create the terminal, wire up input, mouse, menu and messages, then tell TerminalControl the page is ready */
    function start() {
        term = new Terminal({
            allowProposedApi: true,
            cursorBlink: true,
            cursorInactiveStyle: 'outline',
            scrollback: 10000,
            scrollOnEraseInDisplay: true,
            rightClickSelectsWord: false,
            fontFamily: 'Consolas, "Courier New", monospace',
            fontSize: 14,
            windowsPty: { backend: 'conpty' }
        });
        var element = document.getElementById('terminal');
        term.open(element);
        holdSelectionRedraws();
        term.attachCustomKeyEventHandler(onKey);
        term.onData(function (data) {
            typedAt = Date.now();
            // a command typed: the lines from here aren't a button's
            if (data.indexOf('\r') >= 0) { markCommand(''); }
            followCursor();
            post({ type: 'input', data: data });
        });
        // without wrapping, the view follows the cursor: always just after typing (the echo moves it), and with other
        // output only while the cursor was in view, so a line the user panned along to read stays put
        term.onCursorMove(function () {
            if (cursorShowed || Date.now() - typedAt < 1000) { followCursor(); }
            cursorShowed = cursorShows();
        });
        // a full-screen program (alternate screen) gets the tab's own width, and the wide lines come back after it
        term.buffer.onBufferChange(function () { fit(); followCursor(); scheduleNumbers(); });
        term.onRender(scheduleNumbers);
        term.onResize(function (size) {
            conpty.resizedAt = Date.now();
            if (sized) { post({ type: 'size', cols: size.cols, rows: size.rows }); }
            // a tab behind another that's still settling on its size keeps its layout a little longer
            if (!shown && !behind()) { applyBehind(); }
        });
        // the dropped lines are counted from the start, not from the gutter's first drawing
        watchTrims();
        term.onScroll(function () { watchTrims(); scheduleSettle(); scheduleNumbers(); });
        term.onBell(ring);
        // links in the output open with Ctrl+click, as in Windows Terminal, so selecting text never opens a browser
        if (window.WebLinksAddon) {
            term.loadAddon(new window.WebLinksAddon.WebLinksAddon(function (event, uri) {
                if (event.ctrlKey) { post({ type: 'openLink', uri: uri }); }
            }));
        }

        // PuTTY copies as soon as text is selected (an option in the menu), the find bar open or not, and the
        // selection's other matches are marked
        var selectionTimer = null;
        term.onSelectionChange(function () {
            window.clearTimeout(selectionTimer);
            selectionTimer = window.setTimeout(function () {
                markSelectionMatches();
                if (prefOn('copyOnSelect') && term.hasSelection() && !selectionIsFinds()) { copy(); }
            }, 150);
        });

        // the browser's own paste (Ctrl+V, Shift+Insert on the text box) goes through TerminalControl too, which
        // asks before several lines in a production session (capture: before xterm.js pastes it)
        element.addEventListener('paste', function (e) {
            e.preventDefault();
            e.stopPropagation();
            var text = e.clipboardData ? e.clipboardData.getData('text/plain') : '';
            if (text) { post({ type: 'pasteRequest', text: text }); }
        }, true);
        // capture phase: before xterm.js or SuperPuTTY's focus handling sees the click
        element.addEventListener('mousedown', function (e) {
            if (e.button === 2) { rememberSelection(); }
        }, true);
        // Ctrl+mouse wheel makes the font larger or smaller, as in Windows Terminal (capture: before xterm.js scrolls)
        element.addEventListener('wheel', function (e) {
            if (!e.ctrlKey) { return; }
            e.preventDefault();
            e.stopPropagation();
            stepFontSize(e.deltaY < 0 ? 1 : -1);
        }, { capture: true, passive: false });
        element.addEventListener('contextmenu', function (e) {
            e.preventDefault();
            restoreSelection();
            if (hostMenu) { showHostMenu(e.clientX, e.clientY); } else { showMenu(e.clientX, e.clientY); }
        });
        // middle-click pastes, as in PuTTY when the right button opens a menu
        element.addEventListener('auxclick', function (e) {
            if (e.button === 1) { e.preventDefault(); paste(); }
        });
        element.addEventListener('mousedown', function (e) { if (e.button === 1) { e.preventDefault(); } });
        document.addEventListener('mousedown', function (e) {
            // a right-click reopens the menu itself; closing it here would forget the selection just remembered
            if (e.button !== 2 && !(e.target.closest && e.target.closest('.menu'))) { hideMenu(); }
        });
        window.addEventListener('blur', hideMenu);
        document.addEventListener('keydown', function (e) {
            if (e.key === 'Escape' && !document.getElementById('menu').hidden) { hideMenu(); term.focus(); }
        });
        bindFind();

        // without wrapping: the side-to-side scroll bar, sideways wheel or trackpad, and Shift+wheel pan the view
        var bar = document.getElementById('hscroll');
        bar.addEventListener('scroll', function () { if (bar.scrollLeft !== pan) { applyPan(bar.scrollLeft); } });
        element.addEventListener('wheel', function (e) {
            if (!wide() || e.ctrlKey) { return; }
            var dx = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : (e.shiftKey ? e.deltaY : 0);
            if (!dx) { return; }
            e.preventDefault();
            e.stopPropagation();
            applyPan(pan + dx);
        }, { capture: true, passive: false });
        // the browser scrolls clipped boxes itself to show the focused text box, which follows the cursor; the view
        // pans only by applyPan (scroll doesn't bubble, so capture)
        element.addEventListener('scroll', function (e) {
            if (e.target.scrollLeft) { e.target.scrollLeft = 0; }
        }, true);

        new ResizeObserver(function () { fit(); }).observe(element);
        if (host) {
            host.addEventListener('message', function (event) { onMessage(event.data); });
        }
        // for testing the page in a plain browser: terminalDev.message({...}) and terminalDev.feedText('...')
        window.terminalDev = {
            message: onMessage,
            feedText: function (text) { write(new TextEncoder().encode(text)); },
            term: term,
            posted: [],
            matchCount: function (text) { return SpRules.matches(currentSet(), text, MAX_MATCH_LENGTH).length; },
            // where the highlights and find's marks are drawn: each decoration's row, column and width in cells
            highlights: function () {
                var out = [];
                entries.forEach(function (e) { e.decorations.forEach(function (d) { out.push(placeOf(d)); }); });
                return out;
            },
            findMarks: function () {
                var out = [];
                find.decorations.forEach(function (list) { (list || []).forEach(function (m) { out.push(placeOf(m.decoration)); }); });
                (find.current ? find.current.mark : []).forEach(function (m) { out.push(placeOf(m.decoration)); });
                return out;
            }
        };
        term.focus();
        post({ type: 'ready' });
    }

    start();
})();
