/**
 * The one question window (Ask.svelte) and the ways to open it: a snippet's fields (resolveSnippet), and SPSL's
 * PROMPT and PWDPROMPT (askOne). Each returns null when cancelled.
 */
import { fillFields, findFields, remember, startValue } from "./snippets";

interface Question {
  title: string;
  okLabel: string;
  /** A field with choices is a list to pick from */
  fields: { label: string; value: string; secret: boolean; choices?: string[] }[];
  resolve: (values: string[] | null) => void;
}

/** The question showing, if any */
export const asking = $state<{ current: Question | null }>({ current: null });

/** Ask, waiting for any question already showing to be answered first */
function ask(question: Omit<Question, "resolve">): Promise<string[] | null> {
  return new Promise((resolve) => {
    const show = () => {
      if (asking.current) {
        setTimeout(show, 100);
        return;
      }
      asking.current = { ...question, resolve };
    };
    show();
  });
}

/**
 * A snippet's text with its {prompt:...} and {secret:...} values asked for and filled in, as SnippetPrompts.Resolve;
 * the text as it is when it asks for none; null if cancelled. `title` says what's being sent, e.g. 'Button "Shut port"'.
 */
export async function resolveSnippet(text: string, title: string): Promise<string | null> {
  const fields = findFields(text);
  if (fields.length === 0) return text;
  const answers = await ask({
    title,
    okLabel: "Send",
    fields: fields.map((f) => ({ label: `${f.name}:`, value: startValue(f), secret: f.secret })),
  });
  if (!answers) return null;
  const values = new Map(fields.map((f, i) => [f.name.toLowerCase(), answers[i]]));
  remember(fields, values);
  return fillFields(text, values);
}

/** One of a list, e.g. which serial device a COM name is; null if cancelled */
/** A login: the username (filled in with `username`) and the password; null if cancelled */
export async function askLogin(title: string, username = ""): Promise<{ username: string; password: string } | null> {
  const answers = await ask({
    title,
    okLabel: "Connect",
    fields: [{ label: "Username:", value: username, secret: false }, { label: "Password:", value: "", secret: true }],
  });
  return answers ? { username: answers[0].trim(), password: answers[1] } : null;
}

export async function askChoice(title: string, label: string, choices: string[]): Promise<string | null> {
  const answers = await ask({ title, okLabel: "OK", fields: [{ label, value: choices[0] ?? "", secret: false, choices }] });
  return answers ? answers[0] : null;
}

/** One answer, for SPSL's PROMPT (shown) and PWDPROMPT (masked); null if cancelled */
export async function askOne(message: string, secret: boolean): Promise<string | null> {
  const answers = await ask({ title: message || (secret ? "Password" : "Input"), okLabel: "OK", fields: [{ label: secret ? "Password:" : "", value: "", secret }] });
  return answers ? answers[0] : null;
}

/** A name, starting as `value` (New Folder, Rename); null if cancelled or left empty */
export async function askName(title: string, label: string, value = "", okLabel = "OK"): Promise<string | null> {
  const answers = await ask({ title, okLabel, fields: [{ label, value, secret: false }] });
  const name = answers?.[0].trim();
  return name ? name : null;
}
