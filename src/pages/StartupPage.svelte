<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import Toggle from "../lib/Toggle.svelte";
  import { api, fmtSeconds, type StartupItem, type StartupReport } from "../lib/api";

  let report = $state<StartupReport | null>(null);
  let loading = $state(true);
  let error = $state("");
  let query = $state("");
  let filter = $state<"all" | "on" | "off">("all");
  let busy = $state<string | null>(null);

  async function load() {
    loading = true;
    try {
      report = await api.getStartup();
      error = "";
    } catch (e) {
      error = String(e);
    }
    loading = false;
  }

  onMount(load);

  async function toggle(item: StartupItem) {
    busy = item.id;
    try {
      await api.setStartupEnabled(item.id, !item.enabled);
      item.enabled = !item.enabled;
      error = "";
    } catch (e) {
      error = String(e);
    }
    busy = null;
  }

  const items = $derived(
    (report?.items ?? []).filter((i) => {
      if (filter === "on" && !i.enabled) return false;
      if (filter === "off" && i.enabled) return false;
      const q = query.trim().toLowerCase();
      return !q || i.name.toLowerCase().includes(q) || i.command.toLowerCase().includes(q);
    }),
  );

  const enabledCount = $derived(report?.items.filter((i) => i.enabled).length ?? 0);
  const lastBoot = $derived(report?.boot_history[0]);
  const history = $derived([...(report?.boot_history ?? [])].reverse());
  const maxBoot = $derived(Math.max(1, ...history.map((b) => b.total_ms)));
  const slowest = $derived(
    [...(report?.items ?? [])].filter((i) => i.impact_ms).sort((a, b) => (b.impact_ms ?? 0) - (a.impact_ms ?? 0))[0],
  );

  function impactClass(ms: number) {
    return ms >= 5000 ? "bad" : ms >= 2000 ? "warn" : "ok";
  }
  function impactLabel(ms: number) {
    return ms >= 5000 ? "Impact élevé" : ms >= 2000 ? "Impact moyen" : "Impact faible";
  }
  function kindIcon(kind: string) {
    return kind === "task" ? "clock" : kind === "folder" ? "folder" : "bolt";
  }
</script>

<PageHeader title="Démarrage" subtitle="Ce qui se lance avec Windows, et combien de temps ça coûte.">
  {#snippet actions()}
    <button class="btn" onclick={load} disabled={loading}>
      <span class:spin={loading} style="display:flex"><Icon name="refresh" size={16} /></span> Actualiser
    </button>
  {/snippet}
</PageHeader>

{#if report && !report.is_admin}
  <div class="banner admin">
    <Icon name="shield" size={20} />
    <div class="grow">
      <div class="strong">Mode limité</div>
      <div class="small muted">
        Sans droits administrateur, les temps de démarrage ne sont pas lisibles et les éléments « machine » ne peuvent pas être modifiés.
      </div>
    </div>
    <button class="btn primary" onclick={() => api.restartAsAdmin()}>Relancer en admin</button>
  </div>
{/if}

{#if error}
  <div class="banner error">{error}</div>
{/if}

{#if report}
  <!-- Résumé -->
  <div class="stats">
    <div class="card stat">
      <span class="slabel">Dernier démarrage</span>
      <span class="big">{lastBoot ? fmtSeconds(lastBoot.total_ms) : "Inconnu"}</span>
      <span class="small muted">
        {#if lastBoot}
          {lastBoot.date} · bureau prêt en {fmtSeconds(lastBoot.main_ms)}
        {:else if report.perf_available}
          Aucune mesure récente
        {:else}
          Nécessite les droits admin
        {/if}
      </span>
    </div>
    <div class="card stat">
      <span class="slabel">Programmes au démarrage</span>
      <span class="big">{enabledCount}<span class="of"> / {report.items.length}</span></span>
      <span class="small muted">activés</span>
    </div>
    <div class="card stat">
      <span class="slabel">Le plus lent</span>
      <span class="big name">{slowest ? slowest.name : "Aucun"}</span>
      <span class="small muted">{slowest?.impact_ms ? `+${fmtSeconds(slowest.impact_ms)} au démarrage` : "Aucun ralentissement signalé"}</span>
    </div>
  </div>

  <!-- Historique -->
  {#if history.length > 1}
    <div class="card chart">
      <div class="chart-head">
        <span class="strong">Historique des démarrages</span>
        <span class="legend"><i class="main"></i> jusqu'au bureau <i class="post"></i> après le bureau</span>
      </div>
      <div class="bars">
        {#each history as b}
          <div class="bar-col" title={`${b.date} : ${fmtSeconds(b.total_ms)}`}>
            <div class="bar" style:height={`${(b.total_ms / maxBoot) * 100}%`}>
              <div class="seg post" style:flex={b.post_ms || 0}></div>
              <div class="seg main" style:flex={b.main_ms || 1}></div>
            </div>
            <span class="blabel">{b.date.split(" ")[0]}</span>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Filtres -->
  <div class="toolbar">
    <div class="searchbox">
      <Icon name="search" size={15} />
      <input bind:value={query} placeholder="Rechercher" spellcheck="false" />
    </div>
    <div class="seg-ctrl">
      <button class:active={filter === "all"} onclick={() => (filter = "all")}>Tous</button>
      <button class:active={filter === "on"} onclick={() => (filter = "on")}>Activés</button>
      <button class:active={filter === "off"} onclick={() => (filter = "off")}>Désactivés</button>
    </div>
  </div>

  {#if report.tasks_error}
    <p class="small muted">Tâches planifiées non lues : {report.tasks_error}</p>
  {/if}

  <div class="card list">
    {#each items as item (item.id)}
      <div class="item" class:off={!item.enabled}>
        <div class="kind"><Icon name={kindIcon(item.kind)} size={16} /></div>
        <div class="info">
          <div class="line1">
            <span class="strong">{item.name}</span>
            {#if item.impact_ms}
              <span class="badge {impactClass(item.impact_ms)}" title="Mesuré par Windows lors d'un démarrage récent">
                {impactLabel(item.impact_ms)} · {fmtSeconds(item.impact_ms)}
              </span>
            {/if}
          </div>
          <div class="line2 small">
            <span class="src">{item.source}</span>
            <span class="cmd mono" title={item.command}>{item.command}</span>
          </div>
        </div>
        {#if item.needs_admin && !report.is_admin}
          <span class="lock" title="Droits administrateur requis"><Icon name="shield" size={14} /></span>
        {/if}
        <Toggle
          checked={item.enabled}
          disabled={busy === item.id || (item.needs_admin && !report.is_admin)}
          label={`Activer ${item.name}`}
          onchange={() => toggle(item)}
        />
      </div>
    {:else}
      <div class="empty muted">Aucun élément ne correspond.</div>
    {/each}
  </div>
{:else if loading}
  <div class="stats">
    <div class="card stat skeleton"></div>
    <div class="card stat skeleton"></div>
    <div class="card stat skeleton"></div>
  </div>
{/if}

<style>
  .banner {
    margin-bottom: 16px;
  }
  .banner.admin :global(svg) {
    color: var(--accent);
    flex: none;
  }
  .grow {
    flex: 1;
  }
  .strong {
    font-weight: 600;
  }

  .stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .stat {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    padding: 16px 18px;
  }
  .slabel {
    font-size: 12px;
    color: var(--text-2);
  }
  .big {
    font-family: var(--font-display);
    font-size: 26px;
    font-weight: 600;
    line-height: 1.25;
    font-variant-numeric: tabular-nums;
  }
  .big.name {
    font-size: 20px;
    padding: 3px 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .of {
    font-size: 16px;
    color: var(--text-3);
  }
  .skeleton {
    height: 96px;
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.5;
    }
  }

  .chart {
    margin-top: 8px;
    padding: 16px 18px 12px;
  }
  .chart-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
  }
  .legend {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-2);
  }
  .legend i {
    width: 10px;
    height: 10px;
    border-radius: 3px;
    margin-left: 8px;
  }
  .bars {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    height: 120px;
  }
  .bar-col {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    height: 100%;
  }
  .bar {
    display: flex;
    flex-direction: column;
    width: 100%;
    max-width: 34px;
    min-height: 4px;
    border-radius: 5px;
    overflow: hidden;
    transform-origin: bottom;
    animation: grow 0.5s cubic-bezier(0.2, 0.8, 0.2, 1) both;
  }
  @keyframes grow {
    from {
      transform: scaleY(0);
    }
  }
  .seg.main,
  .legend .main {
    background: var(--accent);
  }
  .seg.post,
  .legend .post {
    background: color-mix(in srgb, var(--accent) 40%, transparent);
  }
  .blabel {
    font-size: 11px;
    color: var(--text-3);
  }

  .toolbar {
    display: flex;
    gap: 10px;
    align-items: center;
    margin: 22px 0 10px;
  }
  .searchbox {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    height: 34px;
    padding: 0 12px;
    border-radius: 8px;
    border: 1px solid var(--stroke);
    background: var(--input);
    color: var(--text-2);
  }
  .searchbox input {
    flex: 1;
    border: none;
    outline: none;
    background: transparent;
    color: var(--text);
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
    padding: 0 12px;
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

  .list {
    overflow: hidden;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--stroke);
    transition: background 0.12s, opacity 0.2s;
  }
  .item:last-child {
    border-bottom: none;
  }
  .item:hover {
    background: var(--fill-hover);
  }
  .item.off .info {
    opacity: 0.55;
  }
  .kind {
    display: grid;
    place-items: center;
    flex: none;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    background: var(--fill-hover);
    color: var(--text-2);
  }
  .info {
    flex: 1;
    min-width: 0;
  }
  .line1 {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .line2 {
    display: flex;
    gap: 10px;
    color: var(--text-2);
    min-width: 0;
  }
  .src {
    flex: none;
  }
  .cmd {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    color: var(--text-3);
    font-size: 11.5px;
  }
  .lock {
    display: flex;
    color: var(--text-3);
  }
  .empty {
    padding: 24px;
    text-align: center;
  }
</style>
