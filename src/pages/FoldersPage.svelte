<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import ShortcutInput from "../lib/ShortcutInput.svelte";
  import { api, type KnownFolder, type Opener } from "../lib/api";
  import { store, saveSettings } from "../lib/settings.svelte";

  const s = $derived(store.s!);
  let openers = $state<Opener[]>([]);
  let known = $state<KnownFolder[]>([]);
  let error = $state("");

  onMount(async () => {
    [openers, known] = await Promise.all([api.getOpeners(), api.getKnownFolders()]);
  });

  const base = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;

  /** Premier Ctrl+Shift+chiffre libre, ou rien si les 9 sont pris. */
  function nextShortcut(): string {
    const used = new Set([s.palette_shortcut, s.picker_shortcut, ...s.folder_shortcuts.map((f) => f.shortcut)]);
    for (let n = 1; n <= 9; n++) {
      if (!used.has(`Ctrl+Shift+${n}`)) return `Ctrl+Shift+${n}`;
    }
    return "";
  }

  function add(name: string, path: string) {
    if (s.folder_shortcuts.some((f) => f.path.toLowerCase() === path.toLowerCase())) {
      error = `« ${name} » est déjà dans la liste.`;
      return;
    }
    error = "";
    s.folder_shortcuts.push({ name, path, shortcut: nextShortcut(), open_with: "explorer" });
    saveSettings(0);
  }

  async function addPicked() {
    try {
      const path = await api.pickFolder();
      if (path) add(base(path), path);
    } catch (e) {
      error = String(e);
    }
  }

  async function changePath(i: number) {
    const path = await api.pickFolder();
    if (!path) return;
    s.folder_shortcuts[i].path = path;
    saveSettings(0);
  }

  function remove(i: number) {
    s.folder_shortcuts.splice(i, 1);
    saveSettings(0);
  }

  function move(i: number, d: number) {
    const list = s.folder_shortcuts;
    const j = i + d;
    if (j < 0 || j >= list.length) return;
    [list[i], list[j]] = [list[j], list[i]];
    saveSettings(0);
  }

  async function open(i: number) {
    const f = s.folder_shortcuts[i];
    try {
      await api.openWith(f.open_with, f.path);
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  const suggestions = $derived(known.filter((k) => !s.folder_shortcuts.some((f) => f.path.toLowerCase() === k.path.toLowerCase())));
  const openerIcon = (id: string) => {
    const kind = openers.find((o) => o.id === id)?.kind;
    return kind === "terminal" ? "terminal" : kind === "editor" ? "code" : "folder";
  };
</script>

<PageHeader title="Dossiers" subtitle="Un raccourci clavier pour ouvrir tes dossiers favoris, depuis n'importe où.">
  {#snippet actions()}
    <button class="btn primary" onclick={addPicked}><Icon name="plus" size={16} /> Ajouter un dossier</button>
  {/snippet}
</PageHeader>

{#if error}
  <div class="banner error">{error}</div>
{/if}

<div class="card list">
  {#each s.folder_shortcuts as f, i (f.path)}
    <div class="item">
      <div class="order">
        <button class="mini" title="Monter" disabled={i === 0} onclick={() => move(i, -1)}><Icon name="up_small" size={14} /></button>
        <button class="mini" title="Descendre" disabled={i === s.folder_shortcuts.length - 1} onclick={() => move(i, 1)}><Icon name="down_small" size={14} /></button>
      </div>
      <div class="kind"><Icon name={openerIcon(f.open_with)} size={17} /></div>
      <div class="info">
        <input class="name" bind:value={f.name} onchange={() => saveSettings(0)} spellcheck="false" aria-label="Nom" />
        <button class="path mono small" title="Changer de dossier" onclick={() => changePath(i)}>{f.path}</button>
      </div>
      <select class="field with" bind:value={f.open_with} onchange={() => saveSettings(0)} aria-label="Ouvrir avec">
        {#each openers as o (o.id)}
          <option value={o.id}>{o.name}</option>
        {/each}
      </select>
      <ShortcutInput
        value={f.shortcut}
        clearable
        onchange={(v) => {
          f.shortcut = v;
          saveSettings(0);
        }}
      />
      <button class="btn ghost icon" title="Ouvrir maintenant" onclick={() => open(i)}><Icon name="bolt" size={16} /></button>
      <button class="btn ghost icon rm" title="Retirer" onclick={() => remove(i)}><Icon name="trash" size={16} /></button>
    </div>
  {:else}
    <div class="empty">
      <Icon name="folder" size={28} />
      <p>Aucun dossier pour l'instant.<br />Ajoute ton dossier Dev, tes Téléchargements…</p>
    </div>
  {/each}
</div>

{#if suggestions.length}
  <div class="suggest">
    <span class="small muted">Ajouter :</span>
    {#each suggestions as k (k.path)}
      <button class="chip" title={k.path} onclick={() => add(k.name, k.path)}><Icon name="plus" size={12} /> {k.name}</button>
    {/each}
  </div>
{/if}

<p class="small muted note">
  <Icon name="keyboard" size={13} />
  Clique sur un raccourci puis appuie sur la combinaison voulue (Retour arrière pour n'en mettre aucun).
  <kbd>Win</kbd> + chiffre est réservé par Windows à la barre des tâches.
</p>

<style>
  .banner {
    margin-bottom: 12px;
  }
  .list {
    overflow: hidden;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px 10px 6px;
    border-bottom: 1px solid var(--stroke);
    animation: enter 0.2s ease-out both;
  }
  .item:last-child {
    border-bottom: none;
  }
  .item:hover {
    background: var(--fill-hover);
  }
  .order {
    display: flex;
    flex-direction: column;
  }
  .mini {
    display: grid;
    place-items: center;
    width: 22px;
    height: 16px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
  }
  .mini:hover:not(:disabled) {
    background: var(--fill-press);
    color: var(--text);
  }
  .mini:disabled {
    opacity: 0.25;
    cursor: default;
  }
  .kind {
    display: grid;
    place-items: center;
    flex: none;
    width: 34px;
    height: 34px;
    border-radius: 8px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .name {
    width: 100%;
    padding: 1px 4px;
    margin-left: -4px;
    border: 1px solid transparent;
    border-radius: 5px;
    background: transparent;
    font-weight: 600;
    outline: none;
  }
  .name:hover {
    border-color: var(--stroke);
  }
  .name:focus {
    border-color: var(--accent);
    background: var(--input);
  }
  .path {
    padding: 0;
    border: none;
    background: transparent;
    color: var(--text-3);
    text-align: left;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    cursor: pointer;
  }
  .path:hover {
    color: var(--accent);
    text-decoration: underline;
  }
  .with {
    width: 140px;
    height: 32px;
    padding: 0 8px;
  }
  .rm:hover {
    color: var(--bad);
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 36px;
    text-align: center;
    color: var(--text-2);
  }
  .suggest {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-top: 12px;
  }
  .note {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 18px;
  }
</style>
