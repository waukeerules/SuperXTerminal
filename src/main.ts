/**
 * The main window's entry point: mounts App.svelte in index.html's #app, with dockview's styles and the app's own
 * (app.css), turns off the Mac's text helpers in every field, and stops a dropped file opening in the window.
 */
import { mount } from "svelte";
import "virtual:dockview.css";
import "./app.css";
import App from "./App.svelte";
import { stopFileDropsOpening } from "./lib/ui/fileDrops";

// a file dropped where nothing takes it would replace the whole window with it, every tab gone (lib/ui/fileDrops.ts)
stopFileDropsOpening(window);

// Usernames, hosts and commands must stay as typed: turn off macOS's automatic capitalisation, autocorrect, spell
// checking and Writing Tools ("Write with Siri", the badge by the cursor) in every text field, including ones added
// later. The terminal pages do the same in public/web/host/webview.js.
document.addEventListener(
  "focusin",
  (event) => {
    const field = event.target;
    if (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement) {
      field.autocapitalize = "off";
      field.setAttribute("autocorrect", "off");
      field.spellcheck = false;
      field.setAttribute("writingsuggestions", "false");
    }
  },
  true,
);

/** The main window's app */
const app = mount(App, { target: document.getElementById("app")! });

export default app;
