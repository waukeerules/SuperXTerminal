// Stands in for the app's tab area (src/lib/layout/TabArea.ts) and each tab's host (src/lib/terminal/PageHost.ts) in
// tests/browser/manyTabs.spec.ts: `tabs.open(n)` makes n terminal pages, each in its own iframe of this window, as the
// app's tabs are, so they share its one thread. One is in front; the others are hidden the way dockview hides a tab
// that's behind another (its "always" renderer: the page stays, laid out at full size, with `visibility: hidden`).
// Pages are told when they go behind and come to the front as PageHost tells them ({type: "shown"}), through the same
// envelope (public/web/host/webview.js), and what each page posts back is kept, so a test can count a tab's "size",
// "ack" and "log" messages. Plain JavaScript, as the terminal page is: nothing here is compiled.
(function () {
    'use strict';

    /** Each tab: its iframe, its panel (what's hidden), what its page posted, and how often its terminal drew */
    var all = [];
    /** The tab in front */
    var front = 0;

    window.addEventListener('message', function (event) {
        var envelope = event.data;
        if (!envelope || envelope.channel !== 'sxt-page') { return; }
        var tab = all[Number(String(envelope.tab).slice(1))];
        if (!tab) { return; }
        tab.posted.push(envelope.message);
        if (envelope.message.type === 'ack' && tab.acked) {
            var done = tab.acked;
            tab.acked = null;
            done();
        }
    });

    /** Send a tab's page a message, as its PageHost does */
    function post(index, message) {
        all[index].frame.contentWindow.postMessage({ channel: 'sxt-host', tab: 't' + index, message: message }, '/');
    }

    /** Wait until `ready()` is true, looking every few milliseconds */
    function until(ready) {
        return new Promise(function (resolve) {
            (function look() { if (ready()) { resolve(); } else { setTimeout(look, 10); } })();
        });
    }

    /** A tab's page's test hooks (terminalDev) */
    function dev(index) {
        return all[index].frame.contentWindow.terminalDev;
    }

    /** Put a tab's panel in front or behind, as dockview does, and tell its page, as PageHost does */
    function setShown(index, shown) {
        all[index].panel.style.visibility = shown ? 'visible' : 'hidden';
        post(index, { type: 'shown', visible: shown });
    }

    window.tabs = {
        dev: dev,
        post: post,
        /** What a tab's page posted, of one type */
        posted: function (index, type) { return all[index].posted.filter(function (m) { return m.type === type; }); },
        /** How many times a tab's terminal has drawn */
        renders: function (index) { return all[index].renders; },
        /** Whether a tab's page has stopped being laid out, as one behind another does once it has its size */
        behind: function (index) { return all[index].frame.contentDocument.body.classList.contains('behind'); },
        /** Whether a tab's terminal is drawing with WebGL (its canvases are in the page) rather than the DOM renderer */
        webgl: function (index) { return all[index].frame.contentDocument.querySelectorAll('#terminal canvas').length > 0; },

        /**
         * Open `count` tabs with these preferences and rule sets, the first in front, and wait until each has its size
         * (as a session waits for it before starting). With `grid`, they're side by side instead, all showing, as
         * Grid View has them.
         */
        open: async function (count, prefs, ruleSets, grid) {
            var area = document.getElementById('area');
            var across = Math.ceil(Math.sqrt(count)), down = Math.ceil(count / across);
            for (var i = 0; i < count; i++) {
                var panel = document.createElement('div');
                panel.style.cssText = grid
                    ? 'position: absolute; left: ' + (100 * (i % across) / across) + '%; top: ' + (100 * Math.floor(i / across) / down) +
                        '%; width: ' + (100 / across) + '%; height: ' + (100 / down) + '%'
                    : 'position: absolute; inset: 0; visibility: ' + (i === 0 ? 'visible' : 'hidden');
                var frame = document.createElement('iframe');
                frame.style.cssText = 'width: 100%; height: 100%; border: 0; display: block';
                frame.src = '/web/terminal/index.html?tab=t' + i;
                panel.append(frame);
                area.append(panel);
                all.push({ frame: frame, panel: panel, posted: [], renders: 0, acked: null });
            }
            await until(function () { return all.every(function (t) { return t.frame.contentWindow && t.frame.contentWindow.terminalDev; }); });
            all.forEach(function (tab, index) {
                dev(index).term.onRender(function () { tab.renders += 1; });
                // PageHost's order: whether it's in front as soon as the page is ready, then its settings
                post(index, { type: 'shown', visible: !!grid || index === 0 });
                post(index, { type: 'config', prefs: prefs || {}, themes: [{ name: 'PuTTY', colors: {}, styles: { error: { background: '#ff0000' } } }], ruleSets: ruleSets || [] });
            });
            await until(function () { return all.every(function (t) { return t.posted.some(function (m) { return m.type === 'size'; }); }); });
        },

        /** Bring a tab to the front; the one that was there goes behind */
        show: function (index) {
            if (index === front) { return; }
            setShown(front, false);
            setShown(index, true);
            front = index;
        },

        /** Write output to a tab as its session would, settling once its page has taken it (its "ack") */
        feed: function (index, text) {
            return new Promise(function (resolve) {
                all[index].acked = resolve;
                dev(index).feedText(text);
            });
        },

        /**
         * Sessions printing at once: each of `indexes` is fed `chunk` `rounds` times, the next once the page has taken
         * the one before, as PageHost sends a session's output. Returns how long it took, and how the window fared
         * meanwhile: how many frames it drew and the longest wait between two (a window that can't draw can't be used).
         */
        flood: async function (indexes, chunk, rounds) {
            var frames = 0, longest = 0, last = performance.now(), running = true;
            (function tick() {
                var now = performance.now();
                frames += 1;
                longest = Math.max(longest, now - last);
                last = now;
                if (running) { requestAnimationFrame(tick); }
            })();
            var started = performance.now();
            await Promise.all(indexes.map(async function (index) {
                for (var round = 0; round < rounds; round++) { await window.tabs.feed(index, chunk); }
            }));
            var took = performance.now() - started;
            running = false;
            return { ms: Math.round(took), frames: frames, longestFrameMs: Math.round(longest) };
        }
    };
})();
