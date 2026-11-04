/*
 * Stands in for WebView2's window.chrome.webview, so SuperXPuTTY's pages (terminal, output) run unchanged inside
 * SuperXTerminal. Each page is in an iframe of the main window; this passes its messages up to the main window
 * (src/lib/terminal/PageHost.ts), which talks to the Rust core, and passes the main window's messages back down.
 * The page's tab is named by the iframe URL's "tab" query parameter.
 *
 * Load it before the page's own scripts: they look for window.chrome.webview when they start.
 */
(function () {
    'use strict';

    /** The tab's panel id, from the iframe's URL, which every message carries */
    var tab = new URLSearchParams(window.location.search).get('tab') || '';
    /** The page's "message" listeners */
    var listeners = [];

    window.addEventListener('message', function (event) {
        // only the main window (same origin) speaks for the host
        if (event.source !== window.parent || event.origin !== window.location.origin) { return; }
        var envelope = event.data;
        if (!envelope || envelope.channel !== 'sxt-host' || envelope.tab !== tab) { return; }
        listeners.forEach(function (listener) { listener({ data: envelope.message }); });
    });

    /** Turn off the system's text helpers on a field */
    // keystrokes go to the session as typed: no capitalisation, autocorrect, spell checking or Writing Tools ("Write
    // with Siri", whose badge sat by the cursor) on xterm's hidden input or Monaco's. Set as each field is created:
    // WebKit decides what to offer as the field gains focus, before a focus handler could change it.
    function plain(field) {
        field.setAttribute('autocapitalize', 'off');
        field.setAttribute('autocorrect', 'off');
        field.setAttribute('autocomplete', 'off');
        field.setAttribute('spellcheck', 'false');
        field.setAttribute('writingsuggestions', 'false');
    }
    new MutationObserver(function (changes) {
        changes.forEach(function (change) {
            change.addedNodes.forEach(function (node) {
                if (node.nodeType !== 1) { return; }
                if (node.tagName === 'TEXTAREA' || node.tagName === 'INPUT') { plain(node); }
                node.querySelectorAll && node.querySelectorAll('textarea, input').forEach(plain);
            });
        });
    }).observe(document, { childList: true, subtree: true });

    // on Windows, WebView2 puts its own chrome.webview in every frame, which talks to the native host rather than the
    // main window; pinned here (not just assigned) so it can't replace this one
    var shim = {
        /** WebView2's postMessage: the page's message to the host */
        postMessage: function (message) {
            window.parent.postMessage({ channel: 'sxt-page', tab: tab, message: message }, '/');
        },
        /** WebView2's addEventListener: only "message" is used by the pages */
        addEventListener: function (type, listener) {
            if (type === 'message') { listeners.push(listener); }
        },
        removeEventListener: function (type, listener) {
            listeners = listeners.filter(function (l) { return l !== listener; });
        }
    };
    window.chrome = window.chrome || {};
    Object.defineProperty(window.chrome, 'webview', { value: shim, writable: false, configurable: false });
})();
