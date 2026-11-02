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

    var tab = new URLSearchParams(window.location.search).get('tab') || '';
    var listeners = [];

    window.addEventListener('message', function (event) {
        // only the main window (same origin) speaks for the host
        if (event.source !== window.parent || event.origin !== window.location.origin) { return; }
        var envelope = event.data;
        if (!envelope || envelope.channel !== 'sxt-host' || envelope.tab !== tab) { return; }
        listeners.forEach(function (listener) { listener({ data: envelope.message }); });
    });

    window.chrome = window.chrome || {};
    window.chrome.webview = {
        /** WebView2's postMessage: the page's message to the host */
        postMessage: function (message) {
            window.parent.postMessage({ channel: 'sxt-page', tab: tab, message: message }, window.location.origin);
        },
        /** WebView2's addEventListener: only "message" is used by the pages */
        addEventListener: function (type, listener) {
            if (type === 'message') { listeners.push(listener); }
        },
        removeEventListener: function (type, listener) {
            listeners = listeners.filter(function (l) { return l !== listener; });
        }
    };
})();
