<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import Toggle from "../lib/Toggle.svelte";
  import { api } from "../lib/api";
  import { store, saveSettings } from "../lib/settings.svelte";

  const s = $derived(store.s!);

  let autostart = $state(false);
  type ShortcutField = "palette_shortcut" | "picker_shortcut";
  let recording = $state<ShortcutField | null>(null);
  let preview = $state("");

  onMount(async () => {
    autostart = await api.getAutostart();
  });

  async function setAutostart(v: boolean) {
    try {
      await api.setAutostart(v);
      autostart = v;
    } catch (e) {
      store.error = String(e);
    }
  }

  /** « KeyK » → « K », « Digit1 » → « 1 », « Space » → « Space » */
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
      recording = null;
      preview = "";
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
    s[recording] = [...mods, key].join("+");
    recording = null;
    preview = "";
    saveSettings(0);
  }
</script>

<svelte:window {onkeydown} />

{#snippet shortcutButton(field: ShortcutField)}
  <button
    class="shortcut"
    class:recording={recording === field}
    onclick={() => {
      recording = recording === field ? null : field;
      preview = "";
    }}
  >
    {#if recording === field}
      {preview || "En attente…"}
    {:else}
      {#each s[field].split("+") as k, i}
        {#if i > 0}<span class="plus">+</span>{/if}<kbd>{k === "Super" ? "Win" : k}</kbd>
      {/each}
    {/if}
  </button>
{/snippet}

<PageHeader title="Réglages" />

<div class="card group">
  <div class="row">
    <div class="ico"><Icon name="power" size={18} /></div>
    <div class="grow">
      <div class="strong">Lancer au démarrage de Windows</div>
      <div class="small muted">Toolbox démarre discrètement dans la zone de notification.</div>
    </div>
    <Toggle checked={autostart} label="Lancer au démarrage" onchange={setAutostart} />
  </div>

  <div class="row">
    <div class="ico"><Icon name="keyboard" size={18} /></div>
    <div class="grow">
      <div class="strong">Raccourci du convertisseur rapide</div>
      <div class="small muted">
        {recording === "palette_shortcut"
          ? "Appuie sur la combinaison voulue (Échap pour annuler)…"
          : "Ouvre la palette de conversion depuis n'importe quelle application."}
      </div>
    </div>
    {@render shortcutButton("palette_shortcut")}
  </div>

  <div class="row">
    <div class="ico"><Icon name="pipette" size={18} /></div>
    <div class="grow">
      <div class="strong">Raccourci de la pipette</div>
      <div class="small muted">
        {recording === "picker_shortcut"
          ? "Appuie sur la combinaison voulue (Échap pour annuler)…"
          : "Prend la couleur d'un pixel de l'écran et la copie."}
      </div>
    </div>
    {@render shortcutButton("picker_shortcut")}
  </div>
</div>

<h2>À propos</h2>
<div class="card group">
  <div class="row">
    <div class="ico logo"></div>
    <div class="grow">
      <div class="strong">Toolbox 0.1.0</div>
      <div class="small muted">Rust + Tauri 2 + Svelte 5 · Réglages dans <span class="mono">%APPDATA%\com.bebou.toolbox</span></div>
    </div>
  </div>
  <div class="row">
    <div class="ico"><Icon name="sparkle" size={18} /></div>
    <div class="grow small muted">
      Fermer la fenêtre ne quitte pas l'application : elle reste dans la zone de notification. Clic droit sur l'icône → Quitter.
    </div>
  </div>
</div>

<style>
  .group {
    overflow: hidden;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 16px 18px;
    border-bottom: 1px solid var(--stroke);
  }
  .row:last-child {
    border-bottom: none;
  }
  .ico {
    display: grid;
    place-items: center;
    flex: none;
    width: 36px;
    height: 36px;
    border-radius: 9px;
    background: var(--fill-hover);
    color: var(--text-2);
  }
  .ico.logo {
    background: linear-gradient(160deg, #0078d4, #7c4dff);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .strong {
    font-weight: 600;
  }
  h2 {
    margin: 26px 0 12px;
    font-size: 14px;
    font-weight: 600;
  }
  .shortcut {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-width: 150px;
    height: 34px;
    justify-content: center;
    padding: 0 12px;
    border-radius: 8px;
    border: 1px solid var(--stroke-strong);
    background: var(--card);
    cursor: pointer;
    font-size: 13px;
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
</style>
