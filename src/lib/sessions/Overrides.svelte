<!--
  Local overrides, as SuperXPuTTY's dlgSessionOverrides: the user's own host, port, username, profile, extra
  arguments or jump host for one session or several, used instead of the stored ones whenever it opens, and their own
  note for it; the session itself isn't changed, so it works for shared sessions and locked Database fields. Each field
  has a box to tick and shows the stored value beside it. For several sessions a field where their overrides differ
  starts half-ticked, leaving each as it is; ticked sets the value on all of them, unticked removes it from all.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { listProfiles } from "../settings/profiles";
  import { getSession, loadOverrides, saveOverrides, type Override, type SessionFields, type SessionView } from "./api";

  interface Props {
    sessions: Pick<SessionView, "id" | "store" | "name">[];
    onSaved: () => void;
    onCancel: () => void;
  }

  let { sessions, onSaved, onCancel }: Props = $props();

  type Key = "Host" | "Port" | "Username" | "PuttyProfile" | "ExtraArgs" | "JumpHost" | "Note";
  /** set: this value on all; remove: no override on any; keep: each as it is (only where they differ) */
  type Use = "set" | "remove" | "keep";

  const FIELDS: { key: Key; label: string; stored: (f: SessionFields) => string; hint: string }[] = [
    { key: "Host", label: "Host", stored: (f) => f.host, hint: "" },
    { key: "Port", label: "Port", stored: (f) => (f.port ? String(f.port) : "the protocol's"), hint: "" },
    { key: "Username", label: "Login username", stored: (f) => f.username, hint: "e.g. your own login, or {user}" },
    { key: "PuttyProfile", label: "Profile", stored: (f) => f.profile, hint: "" },
    { key: "ExtraArgs", label: "Extra arguments", stored: (f) => f.extraArgs, hint: "replaces the session's" },
    { key: "JumpHost", label: "Jump host", stored: (f) => f.jumpHost || "none", hint: "empty for none" },
    { key: "Note", label: "Notes", stored: (f) => f.note, hint: "your own note, shown instead of the session's" },
  ];

  const many = untrack(() => sessions.length > 1);
  let current = $state<(Override | undefined)[]>([]);
  let stored = $state<(SessionFields | null)[]>([]);
  let use = $state<Record<Key, Use>>({ Host: "remove", Port: "remove", Username: "remove", PuttyProfile: "remove", ExtraArgs: "remove", JumpHost: "remove", Note: "remove" });
  /** Whether a field's overrides differ between the sessions, so "keep" is offered */
  let mixed = $state<Record<Key, boolean>>({ Host: false, Port: false, Username: false, PuttyProfile: false, ExtraArgs: false, JumpHost: false, Note: false });
  let values = $state<Record<Key, string>>({ Host: "", Port: "", Username: "", PuttyProfile: "", ExtraArgs: "", JumpHost: "", Note: "" });
  let profileNames = $state<string[]>([]);
  let ready = $state(false);
  let error = $state("");

  /** A field's override value as text; undefined when not overridden */
  function valueOf(o: Override | undefined, key: Key): string | undefined {
    if (!o) return undefined;
    if (key === "Port") return o.Port > 0 ? String(o.Port) : undefined;
    return o[key] ?? undefined;
  }

  onMount(async () => {
    try {
      const all = await loadOverrides();
      current = sessions.map((s) => all[s.id]);
      stored = await Promise.all(sessions.map((s) => getSession(s.store, s.id).catch(() => null)));
      profileNames = (await listProfiles()).map((p) => p.name);
      for (const { key } of FIELDS) {
        const each = current.map((o) => valueOf(o, key));
        const first = each[0];
        mixed[key] = each.some((v) => v !== first);
        use[key] = mixed[key] ? "keep" : first !== undefined ? "set" : "remove";
        values[key] = each.find((v) => v !== undefined) ?? (stored[0] && !many ? startFrom(key, stored[0]) : "");
      }
      ready = true;
    } catch (e) {
      error = String(e);
    }
  });

  /** An unset field starts from the stored value, so it's easy to change a little */
  function startFrom(key: Key, f: SessionFields): string {
    switch (key) {
      case "Host": return f.host;
      case "Port": return f.port ? String(f.port) : "";
      case "Username": return f.username;
      case "PuttyProfile": return f.profile;
      case "ExtraArgs": return f.extraArgs;
      case "JumpHost": return "";
      case "Note": return f.note;
    }
  }

  /** The stored value beside a field: the session's, or "(differs)" for several that don't agree */
  function storedText(key: Key): string {
    const field = FIELDS.find((f) => f.key === key)!;
    const each = stored.map((f) => (f ? field.stored(f) : ""));
    if (each.length === 0) return "";
    return each.every((v) => v === each[0]) ? each[0] || "(empty)" : "(differs)";
  }

  /** The box's click: ticked, unticked, and half-ticked again where the sessions differ */
  function cycle(key: Key): void {
    const order: Use[] = mixed[key] ? ["keep", "set", "remove"] : ["set", "remove"];
    use[key] = order[(order.indexOf(use[key]) + 1) % order.length];
  }

  /** One session's new overrides from the choices */
  function merge(old: Override | undefined): Override {
    const out: Override = { Port: 0 };
    for (const { key } of FIELDS) {
      const choice = use[key];
      const value = choice === "keep" ? valueOf(old, key) : choice === "set" ? values[key] : undefined;
      if (value === undefined) continue;
      if (key === "Port") out.Port = Math.max(0, Math.min(65535, Number(value) || 0));
      else out[key] = key === "Note" ? value : value.trim();
    }
    return out;
  }

  async function save(): Promise<void> {
    if (use.Port === "set" && !(Number(values.Port) > 0)) {
      error = "Give a port from 1 to 65535, or untick Port.";
      return;
    }
    try {
      await saveOverrides(sessions.map((s, i) => [s.id, merge(current[i])]));
      onSaved();
    } catch (e) {
      error = String(e);
    }
  }

  const title = $derived(many ? `Local overrides for ${sessions.length} sessions` : `Local overrides: ${sessions[0]?.name ?? ""}`);
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onCancel()} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label={title}>
    <header>{title}</header>
    <p class="note">Your own values, used instead of the session's whenever it opens. Only you get them (SessionOverrides.json in your own folder); the session isn't changed.
      {#if many}A half-ticked box leaves each session's override as it is.{/if}</p>

    <div class="fields">
      {#each FIELDS as f (f.key)}
        <label class="use" title={use[f.key] === "keep" ? "Each session keeps its own" : ""}>
          <input type="checkbox" checked={use[f.key] === "set"} indeterminate={use[f.key] === "keep"} disabled={!ready}
            onchange={(e) => {
              // the box's own toggle can't show three states, so it shows the choice made
              cycle(f.key);
              e.currentTarget.checked = use[f.key] === "set";
              e.currentTarget.indeterminate = use[f.key] === "keep";
            }} />
          {f.label}:
        </label>
        {#if f.key === "Note"}
          <textarea rows="3" bind:value={values.Note} disabled={use.Note !== "set"} placeholder={f.hint}></textarea>
        {:else if f.key === "Port"}
          <input class="port" type="number" min="1" max="65535" bind:value={values.Port} disabled={use.Port !== "set"} />
        {:else if f.key === "PuttyProfile"}
          <span>
            <input list="override-profiles" bind:value={values.PuttyProfile} disabled={use.PuttyProfile !== "set"} />
            <datalist id="override-profiles">{#each profileNames as n}<option value={n}></option>{/each}</datalist>
          </span>
        {:else}
          <input bind:value={values[f.key]} disabled={use[f.key] !== "set"} placeholder={f.hint} />
        {/if}
        <span class="stored" title="The session's own value">{ready ? `Session: ${storedText(f.key)}` : ""}</span>
      {/each}
    </div>

    <footer>
      <span class="error">{error}</span>
      <button class="primary" disabled={!ready} onclick={save}>Save</button>
      <button onclick={onCancel}>Cancel</button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 130;
    background: rgb(0 0 0 / 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dialog {
    width: min(720px, 94vw);
    max-height: 94vh;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 16px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  header {
    font-weight: 600;
  }

  .note {
    margin: 0;
    color: var(--text-muted);
  }

  .fields {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) minmax(0, 14em);
    gap: 6px 10px;
    align-items: center;
  }

  .use {
    display: flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
  }

  .fields span:not(.stored) {
    display: flex;
  }

  .fields span input {
    flex: 1;
  }

  .stored {
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  input,
  textarea,
  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 6px;
    min-width: 0;
  }

  input[type="checkbox"] {
    padding: 0;
  }

  .port {
    width: 90px;
  }

  :disabled {
    opacity: 0.45;
  }

  footer {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: flex-end;
  }

  .error {
    flex: 1;
    color: #e5534b;
  }
</style>
