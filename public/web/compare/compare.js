/*
 * Compare Output: Monaco's diff editor over two outputs the main window sends ({ channel: "sxt-compare", left: { title,
 * text }, right: { title, text } }). Asks for them with { channel: "sxt-compare-page", type: "refresh" }.
 */
(function () {
    'use strict';

    var tab = new URLSearchParams(location.search).get('tab') || '';
    var editor = null;
    var left = null;
    var right = null;
    var followTimer = null;

    function $(id) { return document.getElementById(id); }

    function ask() {
        window.parent.postMessage({ channel: 'sxt-compare-page', tab: tab, type: 'refresh' }, location.origin);
    }

    /** The output as compared: the log's time stamp lines left out (they'd differ every time) */
    function clean(text) {
        return text.replace(/\r\n/g, '\n').split('\n')
            .filter(function (line) { return !/^--- \d{4}-\d\d-\d\d \d\d:\d\d:\d\d ---$/.test(line); })
            .join('\n');
    }

    function show(message) {
        $('titles').textContent = message.left.title + '  ↔  ' + message.right.title;
        var dark = window.matchMedia('(prefers-color-scheme: dark)').matches;
        monaco.editor.setTheme(dark ? 'vs-dark' : 'vs');
        var a = clean(message.left.text);
        var b = clean(message.right.text);
        if (!left) {
            left = monaco.editor.createModel(a, 'plaintext');
            right = monaco.editor.createModel(b, 'plaintext');
            editor.setModel({ original: left, modified: right });
        } else {
            if (left.getValue() !== a) left.setValue(a);
            if (right.getValue() !== b) right.setValue(b);
        }
        setTimeout(count, 300);
    }

    function count() {
        var changes = editor.getLineChanges() || [];
        $('status').textContent = changes.length === 0 ? 'The same' : changes.length + ' difference' + (changes.length === 1 ? '' : 's');
    }

    function start() {
        editor = monaco.editor.createDiffEditor($('editor'), {
            readOnly: true,
            originalEditable: false,
            automaticLayout: true,
            renderSideBySide: true,
            ignoreTrimWhitespace: true,
            minimap: { enabled: false },
            fontFamily: 'Menlo, monospace',
            fontSize: 12,
            scrollBeyondLastLine: false
        });
        editor.onDidUpdateDiff(count);
        $('inline').addEventListener('change', function () { editor.updateOptions({ renderSideBySide: !this.checked }); });
        $('spaces').addEventListener('change', function () { editor.updateOptions({ ignoreTrimWhitespace: this.checked }); });
        $('refresh').addEventListener('click', ask);
        $('next').addEventListener('click', function () { editor.goToDiff('next'); });
        $('previous').addEventListener('click', function () { editor.goToDiff('previous'); });
        $('follow').addEventListener('change', function () {
            if (followTimer) clearInterval(followTimer);
            followTimer = this.checked ? setInterval(ask, 2000) : null;
        });
        window.addEventListener('message', function (event) {
            var data = event.data;
            if (event.origin !== location.origin || !data || data.channel !== 'sxt-compare' || !data.left || !data.right) return;
            show(data);
        });
        ask();
    }

    window.require.config({ paths: { vs: '../lib/monaco/vs' } });
    window.require(['vs/editor/editor.main'], start);
})();
