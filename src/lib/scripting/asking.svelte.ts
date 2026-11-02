/**
 * The one question window (Ask.svelte) and the ways to open it: a snippet's fields (resolveSnippet), and SPSL's
 * PROMPT and PWDPROMPT (askOne). Each returns null when cancelled.
 */
import { fillFields, findFields, remember, startValue } from "./snippets";

interface Question {
  title: string;
  okLabel: string;
  fields: { label: string; value: string; secret: boolean }[];
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

/** One answer, for SPSL's PROMPT (shown) and PWDPROMPT (masked); null if cancelled */
export async function askOne(message: string, secret: boolean): Promise<string | null> {
  const answers = await ask({ title: message || (secret ? "Password" : "Input"), okLabel: "OK", fields: [{ label: secret ? "Password:" : "", value: "", secret }] });
  return answers ? answers[0] : null;
}
