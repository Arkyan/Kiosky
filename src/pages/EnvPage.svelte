<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import { api, type EnvState, type EnvVar, type PathCheck } from "../lib/api";

  let env = $state<EnvState | null>(null);
  let machine = $state(false);
  let error = $state("");
  let info = $state("");

  // Brouillon du PATH : rien n'est écrit avant « Enregistrer ».
  let draft = $state<string[]>([]);
  let checks = $state<Record<string, PathCheck>>({});
  /** Chemins (résolus) du PATH de l'autre portée, pour signaler les doublons entre les deux */
  let otherPaths = $state<Set<string>>(new Set());
  let saving = $state(false);
  let newEntry = $state("");

  let newName = $state("");
  let newValue = $state("");
  let confirmDelete = $state<string | null>(null);

  const vars = $derived(env ? (machine ? env.machine : env.user) : []);
  const pathVar = $derived(vars.find((v) => v.name.toLowerCase() === "path"));
  const others = $derived(vars.filter((v) => v.name.toLowerCase() !== "path"));
  const readOnly = $derived(machine && !env?.is_admin);

  const split = (v: string | undefined) => (v ?? "").split(";").map((e) => e.trim()).filter(Boolean);
  const original = $derived(split(pathVar?.value));
  const dirty = $derived(draft.join(";") !== original.join(";"));

  const norm = (p: string) => p.toLowerCase().replace(/[\\/]+$/, "");

  async function load() {
    try {
      env = await api.getEnv();
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  // Changement de portée ou rechargement : on repart de la valeur enregistrée.
  $effect(() => {
    draft = [...original];
  });

  // Existence et chemin réel de chaque entrée, recalculés quand le brouillon change.
  let checkTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const entries = [...draft];
    clearTimeout(checkTimer);
    checkTimer = setTimeout(async () => {
      const res = await api.checkPaths(entries);
      checks = Object.fromEntries(res.map((c) => [c.entry, c]));
    }, 200);
  });

  $effect(() => {
    if (!env) return;
    const other = split((machine ? env.user : env.machine).find((v) => v.name.toLowerCase() === "path")?.value);
    api.checkPaths(other).then((res) => (otherPaths = new Set(res.map((c) => norm(c.expanded)))));
  });

  onMount(load);

  function status(entry: string, i: number): { cls: string; label: string } {
    const c = checks[entry];
    if (!c) return { cls: "", label: "" };
    if (!c.exists) return { cls: "bad", label: "Introuvable" };
    const n = norm(c.expanded);
    if (draft.findIndex((e) => checks[e] && norm(checks[e].expanded) === n) !== i) return { cls: "warn", label: "Doublon" };
    if (otherPaths.has(n)) return { cls: "info", label: machine ? "Aussi dans l'utilisateur" : "Aussi dans le système" };
    return { cls: "ok", label: "" };
  }

  const deadCount = $derived(draft.filter((e) => checks[e] && !checks[e].exists).length);
  const dupCount = $derived(draft.filter((e, i) => status(e, i).cls === "warn").length);

  function move(i: number, d: number) {
    const j = i + d;
    if (j < 0 || j >= draft.length) return;
    [draft[i], draft[j]] = [draft[j], draft[i]];
  }

  function addEntry(path: string) {
    const p = path.trim().replace(/;/g, "");
    if (!p) return;
    draft = [...draft, p];
    newEntry = "";
  }

  async function addPicked() {
    const p = await api.pickFolder();
    if (p) addEntry(p);
  }

  function removeDead() {
    draft = draft.filter((e) => !checks[e] || checks[e].exists);
  }

  function removeDuplicates() {
    draft = draft.filter((e, i) => status(e, i).cls !== "warn");
  }

  async function savePath() {
    saving = true;
    try {
      await api.setEnv(machine, pathVar?.name ?? "Path", draft.join(";"));
      flashInfo("PATH enregistré. Relance tes terminaux et éditeurs pour qu'ils le voient.");
      await load();
    } catch (e) {
      error = String(e);
    }
    saving = false;
  }

  async function setVar(v: EnvVar, value: string) {
    if (value === v.value) return;
    try {
      await api.setEnv(machine, v.name, value);
      flashInfo(`${v.name} modifiée.`);
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  async function removeVar(name: string) {
    if (confirmDelete !== name) {
      confirmDelete = name;
      setTimeout(() => confirmDelete === name && (confirmDelete = null), 3500);
      return;
    }
    confirmDelete = null;
    try {
      await api.deleteEnv(machine, name);
      flashInfo(`${name} supprimée.`);
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  async function addVar() {
    const name = newName.trim();
    if (!name) return;
    if (vars.some((v) => v.name.toLowerCase() === name.toLowerCase())) {
      error = `${name} existe déjà : modifie-la dans la liste.`;
      return;
    }
    try {
      await api.setEnv(machine, name, newValue);
      newName = "";
      newValue = "";
      flashInfo(`${name} ajoutée.`);
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  async function undo() {
    try {
      const what = await api.undoEnv();
      flashInfo(`Modification annulée : ${what}.`);
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  let infoTimer: ReturnType<typeof setTimeout> | undefined;
  function flashInfo(text: string) {
    info = text;
    error = "";
    clearTimeout(infoTimer);
    infoTimer = setTimeout(() => (info = ""), 5000);
  }
</script>

<PageHeader title="Variables d'environnement" subtitle="Le PATH et tes variables, sans la vieille fenêtre de Windows.">
  {#snippet actions()}
    {#if env?.undo}
      <button class="btn" onclick={undo} title="Restaure la valeur d'avant la dernière modification faite ici">
        <Icon name="undo" size={15} /> Annuler : {env.undo}
      </button>
    {/if}
  {/snippet}
</PageHeader>

{#if error}
  <div class="banner error">{error}</div>
{:else if info}
  <div class="banner ok"><Icon name="check" size={16} /> {info}</div>
{/if}

<div class="toolbar">
  <div class="seg-ctrl">
    <button class:active={!machine} onclick={() => (machine = false)}>Utilisateur</button>
    <button class:active={machine} onclick={() => (machine = true)}>Système</button>
  </div>
  {#if readOnly}
    <span class="small muted ro"><Icon name="shield" size={13} /> Lecture seule : les variables système demandent les droits admin</span>
  {:else}
    <span class="small muted">Les applications déjà ouvertes gardent l'ancienne valeur jusqu'à leur redémarrage.</span>
  {/if}
</div>

{#if env}
  <!-- PATH -->
  <div class="card path">
    <div class="phead">
      <span class="strong">PATH</span>
      <span class="small muted">{draft.length} dossiers · l'ordre compte : le premier trouvé gagne</span>
      <span class="grow"></span>
      {#if deadCount && !readOnly}
        <button class="btn ghost small-btn bad-text" onclick={removeDead}>Retirer les introuvables ({deadCount})</button>
      {/if}
      {#if dupCount && !readOnly}
        <button class="btn ghost small-btn" onclick={removeDuplicates}>Retirer les doublons ({dupCount})</button>
      {/if}
    </div>
    {#each draft as entry, i (i)}
      {@const st = status(entry, i)}
      {@const c = checks[entry]}
      <div class="entry {st.cls}">
        <div class="order">
          <button class="mini" disabled={readOnly || i === 0} onclick={() => move(i, -1)} title="Monter"><Icon name="up_small" size={13} /></button>
          <button class="mini" disabled={readOnly || i === draft.length - 1} onclick={() => move(i, 1)} title="Descendre"><Icon name="down_small" size={13} /></button>
        </div>
        <span class="dot" title={st.label || "Trouvé"}></span>
        <div class="evalue">
          <input class="mono" bind:value={draft[i]} disabled={readOnly} spellcheck="false" />
          {#if c && c.expanded !== entry}<span class="expanded mono" title={c.expanded}>→ {c.expanded}</span>{/if}
        </div>
        {#if st.label}<span class="badge {st.cls}">{st.label}</span>{/if}
        {#if c?.exists}
          <button class="btn ghost icon" title="Ouvrir dans l'Explorateur" onclick={() => api.openWith("explorer", c.expanded)}><Icon name="folder" size={15} /></button>
        {/if}
        {#if !readOnly}
          <button class="btn ghost icon rm" title="Retirer" onclick={() => (draft = draft.filter((_, j) => j !== i))}><Icon name="x" size={14} /></button>
        {/if}
      </div>
    {/each}
    {#if !readOnly}
      <form
        class="add"
        onsubmit={(e) => {
          e.preventDefault();
          addEntry(newEntry);
        }}
      >
        <input class="field mono" bind:value={newEntry} placeholder="Ajouter un dossier (C:\outils\bin ou %USERPROFILE%\bin)" spellcheck="false" />
        <button class="btn" type="submit" disabled={!newEntry.trim()}>Ajouter</button>
        <button class="btn" type="button" onclick={addPicked}><Icon name="folder" size={14} /> Parcourir</button>
      </form>
    {/if}
    {#if dirty}
      <div class="savebar">
        <span class="small">Modifications non enregistrées</span>
        <button class="btn ghost" onclick={() => (draft = [...original])}>Annuler</button>
        <button class="btn primary" onclick={savePath} disabled={saving}>Enregistrer le PATH</button>
      </div>
    {/if}
  </div>

  <!-- Autres variables -->
  <h2>Variables <span class="muted small">· {others.length}</span></h2>
  <div class="card vars">
    {#each others as v (v.name)}
      <div class="var">
        <span class="vname mono" title={v.name}>{v.name}</span>
        <input
          class="vvalue mono"
          value={v.value}
          disabled={readOnly}
          spellcheck="false"
          title={v.value}
          onkeydown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          onchange={(e) => setVar(v, e.currentTarget.value)}
        />
        {#if !readOnly}
          <button class="btn ghost del" class:confirm={confirmDelete === v.name} onclick={() => removeVar(v.name)}>
            {#if confirmDelete === v.name}Supprimer ?{:else}<Icon name="trash" size={14} />{/if}
          </button>
        {/if}
      </div>
    {/each}
    {#if !readOnly}
      <form
        class="var new"
        onsubmit={(e) => {
          e.preventDefault();
          addVar();
        }}
      >
        <input class="field mono vname-in" bind:value={newName} placeholder="NOM" spellcheck="false" />
        <input class="field mono grow" bind:value={newValue} placeholder="valeur" spellcheck="false" />
        <button class="btn" type="submit" disabled={!newName.trim()}><Icon name="plus" size={14} /> Ajouter</button>
      </form>
    {/if}
  </div>
{/if}

<style>
  .banner {
    margin-bottom: 12px;
  }
  .banner.ok {
    border-color: color-mix(in srgb, var(--ok) 40%, transparent);
    background: color-mix(in srgb, var(--ok) 10%, var(--card));
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 12px;
  }
  .ro {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .seg-ctrl {
    display: flex;
    padding: 3px;
    border-radius: 8px;
    border: 1px solid var(--stroke);
    background: var(--card-2);
  }
  .seg-ctrl button {
    height: 26px;
    padding: 0 14px;
    border: none;
    border-radius: 6px;
    background: transparent;
    font-size: 13px;
    color: var(--text-2);
    cursor: pointer;
  }
  .seg-ctrl button.active {
    background: var(--card);
    color: var(--text);
    font-weight: 600;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12);
  }
  .strong {
    font-weight: 600;
  }
  .grow {
    flex: 1;
  }

  .path {
    overflow: hidden;
  }
  .phead {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 6px 10px 6px 16px;
    border-bottom: 1px solid var(--stroke);
  }
  .small-btn {
    height: 28px;
    font-size: 12.5px;
  }
  .bad-text {
    color: var(--bad);
  }
  .entry {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 40px;
    padding: 3px 8px 3px 6px;
    border-bottom: 1px solid var(--stroke);
  }
  .entry:hover {
    background: var(--fill-hover);
  }
  .order {
    display: flex;
    flex-direction: column;
  }
  .mini {
    display: grid;
    place-items: center;
    width: 20px;
    height: 15px;
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
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--stroke-strong);
  }
  .entry.ok .dot,
  .entry.info .dot {
    background: var(--ok);
  }
  .entry.bad .dot {
    background: var(--bad);
  }
  .entry.warn .dot {
    background: var(--warn);
  }
  .evalue {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .evalue input {
    width: 100%;
    padding: 2px 6px;
    border: 1px solid transparent;
    border-radius: 5px;
    background: transparent;
    font-size: 12.5px;
    outline: none;
  }
  .evalue input:hover:not(:disabled) {
    border-color: var(--stroke);
  }
  .evalue input:focus {
    border-color: var(--accent);
    background: var(--input);
  }
  .entry.bad .evalue input {
    color: var(--bad);
    text-decoration: line-through;
    text-decoration-color: color-mix(in srgb, var(--bad) 50%, transparent);
  }
  .expanded {
    padding: 0 7px;
    font-size: 11px;
    color: var(--text-3);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .badge.info {
    color: var(--text-2);
  }
  .rm:hover {
    color: var(--bad);
  }
  .add {
    display: flex;
    gap: 6px;
    padding: 10px 12px;
  }
  .add .field {
    flex: 1;
    height: 32px;
    font-size: 12.5px;
  }
  .savebar {
    position: sticky;
    bottom: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px 10px 16px;
    border-top: 1px solid var(--stroke);
    background: color-mix(in srgb, var(--accent) 10%, var(--card));
    backdrop-filter: blur(20px);
  }
  .savebar span {
    flex: 1;
    font-weight: 600;
  }

  h2 {
    margin: 26px 0 12px;
    font-size: 14px;
    font-weight: 600;
  }
  .vars {
    overflow: hidden;
  }
  .var {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 40px;
    padding: 4px 8px 4px 16px;
    border-bottom: 1px solid var(--stroke);
  }
  .var:last-child {
    border-bottom: none;
  }
  .var:hover {
    background: var(--fill-hover);
  }
  .vname {
    flex: none;
    width: 200px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 12.5px;
    font-weight: 600;
  }
  .vvalue {
    flex: 1;
    min-width: 0;
    padding: 4px 6px;
    border: 1px solid transparent;
    border-radius: 5px;
    background: transparent;
    font-size: 12.5px;
    color: var(--text-2);
    outline: none;
  }
  .vvalue:hover:not(:disabled) {
    border-color: var(--stroke);
  }
  .vvalue:focus {
    border-color: var(--accent);
    background: var(--input);
    color: var(--text);
  }
  .del {
    height: 30px;
    min-width: 32px;
    padding: 0 8px;
    color: var(--text-3);
  }
  .del:hover,
  .del.confirm {
    color: var(--bad);
  }
  .del.confirm {
    background: color-mix(in srgb, var(--bad) 12%, transparent);
    font-weight: 600;
    font-size: 12.5px;
  }
  .new {
    padding: 8px 12px;
  }
  .new .field {
    height: 32px;
    font-size: 12.5px;
  }
  .vname-in {
    width: 200px;
  }
</style>
