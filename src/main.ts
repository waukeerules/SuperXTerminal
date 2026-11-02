import { mount } from "svelte";
import "virtual:dockview.css";
import "./app.css";
import App from "./App.svelte";

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

const app = mount(App, { target: document.getElementById("app")! });

export default app;
