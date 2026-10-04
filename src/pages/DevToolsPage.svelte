<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import { api, copyText, type AppUpdate, type DevTool, type DevToolUpdate } from "../lib/api";

  let tools = $state<DevTool[]>([]);
  let loaded = $state(false);
  let loading = $state(false);
  let error = $state("");
  let info = $state("");
  /** Mises à jour connues de winget (page Mises à jour) */
  let wingetUpdates = $state<AppUpdate[]>([]);
  /** Celles que winget ne voit pas : Rust, via rustup */
  let otherUpdates = $state<DevToolUpdate[]>([]);

  async function load() {
    loading = true;
    try {
      tools = await api.getDevtools();
      error = "";
    } catch (e) {
      error = String(e);
    }
    loading = false;
    loaded = true;
    // Les mises à jour arrivent après : elles interrogent winget et le réseau.
    api.getAppUpdates(false).then((u) => (wingetUpdates = u.apps)).catch(() => {});
    api.getDevtoolUpdates().then((u) => (otherUpdates = u)).catch(() => {});
  }

  onMount(load);

  const installed = $derived(tools.filter((t) => t.installed));
  const missing = $derived(tools.filter((t) => !t.installed));
  const groups = $derived([...new Set(installed.map((t) => t.group))]);

  type Update = { available: string; hint: string; winget: string | null; command: string | null };

  function update(t: DevTool): Update | null {
    const other = otherUpdates.find((u) => u.id === t.id);
    if (other) return { available: other.available, hint: `À mettre à jour avec « ${other.command} »`, winget: null, command: other.command };
    const app = wingetUpdates.find((a) => t.winget.some((w) => a.id.toLowerCase().startsWith(w.toLowerCase())));
    if (app) return { available: app.available, hint: "Ouvrir la page Mises à jour", winget: app.id, command: null };
    return null;
  }

  const pending = $derived(installed.filter((t) => update(t)).length);

  let infoTimer: ReturnType<typeof setTimeout> | undefined;
  async function copy(text: string, what: string) {
    await copyText(text);
    info = `${what} copié.`;
    clearTimeout(infoTimer);
    infoTimer = setTimeout(() => (info = ""), 3000);
  }

  function onUpdate(u: Update) {
    if (u.command) copy(u.command, `« ${u.command} »`);
    else api.runAction("page:updates").catch((e) => (error = String(e)));
  }

  const dir = (p: string) => p.replace(/[\\/][^\\/]*$/, "");

  function openDir(p: string) {
    api.openWith("explorer", dir(p)).catch((e) => (error = String(e)));
  }
</script>

<PageHeader title="Outils dev" subtitle="Ce qui est installé, en quelle version, et où le PATH le trouve.">
  {#snippet actions()}
    <button class="btn" onclick={load} disabled={loading}>
      <span class:spin={loading} style="display:flex"><Icon name="refresh" size={16} /></span> Actualiser
    </button>
  {/snippet}
</PageHeader>

{#if error}
  <div class="banner error">{error}</div>
{:else if info}
  <div class="banner"><Icon name="check" size={16} /> {info}</div>
{/if}

{#if loaded}
  <div class="summary small muted">
    {installed.length} outil{installed.length > 1 ? "s" : ""} installé{installed.length > 1 ? "s" : ""}
    {#if pending}· <span class="accent">{pending} mise{pending > 1 ? "s" : ""} à jour disponible{pending > 1 ? "s" : ""}</span>{/if}
  </div>

  {#each groups as group}
    <h2>{group}</h2>
    <div class="card list">
      {#each installed.filter((t) => t.group === group) as t (t.id)}
        {@const u = update(t)}
        <div class="row">
          <span class="name strong">{t.name}</span>
          <button class="link mono version" title="Copier" onclick={() => copy(t.version, `${t.name} ${t.version}`)}>{t.version}</button>
          <div class="where">
            <button class="link small mono path" title={`Ouvrir ${dir(t.path)}`} onclick={() => openDir(t.path)}>{t.path}</button>
            {#if t.others.length}
              <span class="badge warn" title={`Aussi dans le PATH, jamais lancé${t.others.length > 1 ? "s" : ""} :\n${t.others.join("\n")}`}>
                +{t.others.length} dans le PATH
              </span>
            {/if}
          </div>
          {#if u}
            <button class="btn ghost update" title={u.hint} onclick={() => onUpdate(u)}>
              <Icon name={u.command ? "copy" : "download"} size={13} /> {u.available}
            </button>
          {/if}
        </div>
      {/each}
    </div>
  {/each}

  {#if !installed.length}
    <div class="card empty muted">Aucun outil de développement trouvé dans le PATH.</div>
  {/if}

  {#if missing.length}
    <h2>Non installés</h2>
    <div class="missing">
      {#each missing as t (t.id)}<span class="badge">{t.name}</span>{/each}
    </div>
  {/if}

  <div class="small muted note">
    Les versions sont celles du PATH de Kiosky : après une installation, relance Kiosky pour qu'il la voie. « +1 dans le PATH » signale
    un second exemplaire, plus loin, que la ligne de commande n'utilise jamais.
  </div>
{:else}
  <div class="card empty muted">Recherche des outils…</div>
{/if}

<style>
  .banner {
    margin-bottom: 12px;
  }
  h2 {
    margin: 22px 2px 10px;
    font-size: 15px;
    font-weight: 600;
  }
  .strong {
    font-weight: 600;
  }
  .summary {
    margin: -8px 2px 0;
  }
  .accent {
    color: var(--accent);
    font-weight: 600;
  }
  .list {
    overflow: hidden;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 14px;
    min-height: 44px;
    padding: 3px 8px 3px 18px;
    border-bottom: 1px solid var(--stroke);
  }
  .row:last-child {
    border-bottom: none;
  }
  .row:hover {
    background: var(--fill-hover);
  }
  .name {
    flex: none;
    width: 130px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .link {
    padding: 1px 5px;
    margin: 0 -5px;
    border: none;
    border-radius: 5px;
    background: transparent;
    cursor: pointer;
  }
  .link:hover {
    background: var(--fill-press);
    color: var(--accent);
  }
  .version {
    flex: none;
    width: 90px;
    text-align: left;
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }
  .where {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: 1;
    min-width: 0;
  }
  .path {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    /* Coupe au début : c'est la fin du chemin qui renseigne */
    direction: rtl;
    text-align: left;
    color: var(--text-3);
    font-size: 11.5px;
  }
  .badge {
    flex: none;
  }
  .update {
    flex: none;
    height: 28px;
    padding: 0 10px;
    gap: 6px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--accent);
    font-variant-numeric: tabular-nums;
  }
  .missing {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0 2px;
  }
  .empty {
    padding: 24px;
    text-align: center;
  }
  .note {
    margin: 16px 4px 0;
    line-height: 1.5;
  }
</style>
