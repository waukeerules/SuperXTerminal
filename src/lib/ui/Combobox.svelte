<!--
  A text field with a list to pick from, for a profile's name in Edit Session, Bulk Edit, Folder settings and Local
  overrides. Opening it (a click, the ▾ button, or the Down key) shows every option, the current one ticked; typing
  narrows the list to the options with what's typed in them. Up and Down move along it, Enter or a click picks one,
  Escape closes it (and only it: the window around stays open). Anything can still be typed, as a profile not set up
  on this computer can be named. The browser's own list (a datalist) showed only the current name once one was in.
-->
<script lang="ts">
  interface Props {
    /** The text, which a pick replaces */
    value: string;
    /** What can be picked, in the order shown */
    options: string[];
    placeholder?: string;
    disabled?: boolean;
    /** The field's tooltip */
    title?: string;
    /** The field's name for screen readers, when no label is around it */
    ariaLabel?: string;
    /** Called after the text changes, typed or picked */
    onChange?: (value: string) => void;
  }

  let { value = $bindable(), options, placeholder = "", disabled = false, title = "", ariaLabel, onChange = () => {} }: Props = $props();

  /** Gives each list its own id, for the field's aria-controls */
  const uid = `combobox-${Math.random().toString(36).slice(2, 9)}`;
  let open = $state(false);
  /** Whether the list is narrowed to what's typed: only after typing, so opening it shows every option */
  let narrowing = $state(false);
  /** The option the keys are on, as an index into `shown`; -1 for none */
  let active = $state(-1);
  let field: HTMLInputElement;
  /** Where the list goes: under the field, on top of everything (a dialog's scrolling body would cut it off) */
  let place = $state({ left: 0, top: 0, width: 0 });

  /** The options listed: all, or those with what's typed in them, ignoring case */
  const shown = $derived(
    narrowing && value.trim() ? options.filter((o) => o.toLowerCase().includes(value.trim().toLowerCase())) : options,
  );
  /** Whether an option is the one in the field */
  const current = (option: string) => option.toLowerCase() === value.trim().toLowerCase();

  /** Show the list under the field, every option, with the keys on the current one */
  function show(): void {
    if (disabled) return;
    const box = field.getBoundingClientRect();
    place = { left: box.left, top: box.bottom + 2, width: box.width };
    narrowing = false;
    active = options.findIndex(current);
    open = true;
  }

  /** Put an option in the field and close the list */
  function pick(option: string): void {
    value = option;
    onChange(option);
    open = false;
    narrowing = false;
  }

  /** Typing: the list narrows to what matches, with the keys on the first */
  function typed(event: Event): void {
    value = (event.currentTarget as HTMLInputElement).value;
    onChange(value);
    if (!open) show();
    narrowing = true;
    active = shown.length ? 0 : -1;
  }

  /** The keys: Down and Up along the list (Down opens it), Enter picks, Escape closes only the list */
  function key(event: KeyboardEvent): void {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) {
        show();
        return;
      }
      const by = event.key === "ArrowDown" ? 1 : -1;
      active = shown.length ? (active + by + shown.length) % shown.length : -1;
    } else if (event.key === "Enter" && open && active >= 0 && shown[active] !== undefined) {
      event.preventDefault();
      pick(shown[active]);
    } else if (event.key === "Escape" && open) {
      event.stopPropagation();
      open = false;
    } else if (event.key === "Tab") {
      open = false;
    }
  }
</script>

<span class="combobox">
  <input bind:this={field} {value} {placeholder} {disabled} {title} aria-label={ariaLabel} role="combobox" autocomplete="off"
    aria-expanded={open} aria-controls={uid} aria-autocomplete="list"
    aria-activedescendant={open && active >= 0 ? `${uid}-${active}` : undefined}
    oninput={typed} onkeydown={key} onclick={() => !open && show()} onblur={() => (open = false)} />
  <button class="toggle" type="button" tabindex="-1" {disabled} aria-label="Show the list" title="Show the list"
    onmousedown={(e) => { e.preventDefault(); if (open) open = false; else { field.focus(); show(); } }}>▾</button>
  {#if open}
    <ul id={uid} role="listbox" style:left="{place.left}px" style:top="{place.top}px" style:min-width="{place.width}px">
      {#each shown as option, i (option)}
        <!-- a mouse press picks it before the field loses the keyboard (which would close the list first) -->
        <li id="{uid}-{i}" role="option" aria-selected={current(option)} class:active={i === active}
          onmousedown={(e) => { e.preventDefault(); pick(option); }} onmouseenter={() => (active = i)}>
          <span class="tick">{current(option) ? "✓" : ""}</span>{option}
        </li>
      {:else}
        <li class="none" role="presentation">None match: the name is used as typed</li>
      {/each}
    </ul>
  {/if}
</span>

<style>
  .combobox {
    position: relative;
    display: flex;
    flex: 1;
    min-width: 0;
  }

  /* as the dialogs' own fields, whose styles don't reach inside this */
  input {
    flex: 1;
    min-width: 0;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 24px 4px 6px;
  }

  input:disabled,
  .toggle:disabled {
    opacity: 0.55;
  }

  /* inside the field's right end, as a select's arrow */
  .toggle {
    position: absolute;
    right: 1px;
    top: 1px;
    bottom: 1px;
    width: 22px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-muted);
    cursor: default;
  }

  ul {
    position: fixed;
    z-index: 300;
    max-height: 240px;
    overflow: auto;
    margin: 0;
    padding: 4px 0;
    list-style: none;
    background: var(--bg-pane);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.35);
  }

  li {
    padding: 4px 10px 4px 6px;
    white-space: nowrap;
    cursor: default;
  }

  li.active {
    background: var(--accent);
    color: #fff;
  }

  li[aria-selected="true"]:not(.active) {
    font-weight: 600;
  }

  .tick {
    display: inline-block;
    width: 1.3em;
  }

  li.none {
    color: var(--text-muted);
    font-style: italic;
  }
</style>
