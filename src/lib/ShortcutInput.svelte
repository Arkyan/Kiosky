<script lang="ts">
  /** Bouton qui enregistre une combinaison de touches (« Ctrl+Shift+1 »). Échap annule, Retour arrière efface. */
  let {
    value,
    onchange,
    placeholder = "Aucun",
    clearable = false,
  }: { value: string; onchange: (v: string) => void; placeholder?: string; clearable?: boolean } = $props();

  let recording = $state(false);
  let preview = $state("");

  /** « KeyK » → « K », « Digit1 » → « 1 » */
  function keyName(code: string): string | null {
    if (/^(Control|Shift|Alt|Meta|OS)/.test(code)) return null;
    if (code.startsWith("Key")) return code.slice(3);
    if (code.startsWith("Digit")) return code.slice(5);
    return code;
  }

  function onkeydown(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.code === "Escape") {
      recording = false;
      preview = "";
      return;
    }
    if (clearable && (e.code === "Backspace" || e.code === "Delete") && !e.ctrlKey && !e.altKey && !e.shiftKey) {
      recording = false;
      preview = "";
      onchange("");
      return;
    }
    const mods: string[] = [];
    if (e.ctrlKey) mods.push("Ctrl");
    if (e.altKey) mods.push("Alt");
    if (e.shiftKey) mods.push("Shift");
    if (e.metaKey) mods.push("Super");
    const key = keyName(e.code);
    preview = [...mods, key ?? "…"].join(" + ");
    if (!key || mods.length === 0) return; // il faut au moins un modificateur
    recording = false;
    preview = "";
    onchange([...mods, key].join("+"));
  }
</script>

<svelte:window {onkeydown} />

<button
  class="shortcut"
  class:recording
  title={recording ? (clearable ? "Échap : annuler · Retour arrière : aucun raccourci" : "Échap : annuler") : "Cliquer pour changer"}
  onclick={() => {
    recording = !recording;
    preview = "";
  }}
  onblur={() => (recording = false)}
>
  {#if recording}
    {preview || "Appuie sur les touches…"}
  {:else if value}
    {#each value.split("+") as k, i}
      {#if i > 0}<span class="plus">+</span>{/if}<kbd>{k === "Super" ? "Win" : k}</kbd>
    {/each}
  {:else}
    <span class="none">{placeholder}</span>
  {/if}
</button>

<style>
  .shortcut {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    min-width: 150px;
    height: 32px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--stroke-strong);
    background: var(--card);
    cursor: pointer;
    font-size: 12.5px;
    white-space: nowrap;
  }
  .shortcut:hover {
    background: var(--fill-hover);
  }
  .shortcut.recording {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
    color: var(--accent);
    font-weight: 600;
  }
  .plus {
    color: var(--text-3);
    font-size: 11px;
  }
  .none {
    color: var(--text-3);
  }
</style>
