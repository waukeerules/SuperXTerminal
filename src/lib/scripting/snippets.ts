/**
 * Values asked for when a snippet is sent, as SuperXPuTTY's SnippetPrompts, so one button works for any port or VLAN:
 * "interface {prompt:Interface}" asks for Interface. {prompt:Name=default} offers a default; {secret:Name} is typed
 * masked and never remembered. Each name is asked once however often it appears, and the last value given is offered
 * next time (this run only). Filled after {key} placeholders, whose names have no colon, so the two never mix.
 */

/** One value to ask for */
export interface Field {
  name: string;
  /** The default from {prompt:Name=default}, or null */
  default: string | null;
  secret: boolean;
}

/** A {prompt:Name}, {prompt:Name=default} or {secret:Name} field: its kind, name and default */
const PATTERN = /\{(prompt|secret):([^{}=]+?)(?:=([^{}]*))?\}/gi;

/** The last value given for each name (not secrets), by lower-case name */
const lastValues = new Map<string, string>();

/** The values a text asks for, each name once, in the order they first appear */
export function findFields(text: string): Field[] {
  const fields: Field[] = [];
  for (const match of text.matchAll(PATTERN)) {
    const name = match[2].trim();
    const secret = match[1].toLowerCase() === "secret";
    const existing = fields.find((f) => f.name.toLowerCase() === name.toLowerCase());
    if (!existing) fields.push({ name, default: match[3] ?? null, secret });
    // masked if any use of the name is secret
    else if (secret) existing.secret = true;
  }
  return fields;
}

/** Put the values in place of every {prompt:...} and {secret:...}; a name without a value stays as written */
export function fillFields(text: string, values: Map<string, string>): string {
  return text.replace(PATTERN, (whole, _kind, name: string) => values.get(name.trim().toLowerCase()) ?? whole);
}

/** What each field's box starts with: the last value given, else the default */
export function startValue(field: Field): string {
  return (!field.secret && lastValues.get(field.name.toLowerCase())) || field.default || "";
}

/** Remember the values given (not secrets), to offer next time */
export function remember(fields: Field[], values: Map<string, string>): void {
  for (const field of fields) {
    if (!field.secret) lastValues.set(field.name.toLowerCase(), values.get(field.name.toLowerCase()) ?? "");
  }
}
