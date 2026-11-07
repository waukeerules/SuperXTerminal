<!--
  The note under a login script field (Edit Session > Terminal, Settings > Profiles) when the script is a full path,
  which only this computer finds: a session or profile in a shared settings folder or database is opened from the Mac
  and from Windows. Copy to Scripts Folder puts the file in the settings folder's scripts folder and names it there
  ("scripts/jump-host.spsl"), asking first when the folder has a different file of that name. Nothing shows for a
  relative path or an http(s) address. Its button is styled here: the dialogs' button styles are scoped to them.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { fullPathNote } from "./scriptPath";

  /** What scripts::copy_in did: copied, or a different file of that name is there already */
  type CopiedIn = { kind: "copied" | "exists"; spec: string };

  let {
    spec,
    onCopied,
    disabled = false,
  }: {
    /** The login script, as typed or chosen */
    spec: string;
    /** Called with the copy's name, relative to the settings folder, for the field to take */
    onCopied: (spec: string) => void;
    /** The field can't be changed here (a local override locks it), so neither can this */
    disabled?: boolean;
  } = $props();

  /** The note's text, "" when there's nothing to say */
  const note = $derived(fullPathNote(spec));
  /** Why the copy didn't happen, shown in the note's place */
  let problem = $state("");
  $effect(() => {
    void spec;
    problem = "";
  });

  /** Copy the script in, asking before replacing a different one of the same name, and name the copy */
  async function copy(): Promise<void> {
    try {
      let done = await invoke<CopiedIn>("script_copy_in", { path: spec, replace: false });
      if (done.kind === "exists") {
        const replace = await ask(`The scripts folder already has ${done.spec}, and it's different. Replace it with this one?`, {
          title: "Copy to Scripts Folder", kind: "warning", okLabel: "Replace", cancelLabel: "Cancel",
        });
        if (!replace) return;
        done = await invoke<CopiedIn>("script_copy_in", { path: spec, replace: true });
      }
      onCopied(done.spec);
    } catch (e) {
      problem = String(e);
    }
  }
</script>

{#if note}
  <p role="note" aria-label="Login script path">
    {problem || note}
    <button onclick={copy} {disabled}>Copy to Scripts Folder</button>
  </p>
{/if}

<style>
  p {
    margin: 0;
    color: var(--text-muted);
  }

  /*
   * The dialogs' button look (Edit Session's, Settings'), which is each one's own and doesn't reach a button in
   * here: without it this was the browser's grey button, in the note's muted text, and looked disabled
   */
  button {
    margin-left: 0.4em;
    color: var(--text);
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 6px;
  }

  button:disabled {
    opacity: 0.45;
  }
</style>
