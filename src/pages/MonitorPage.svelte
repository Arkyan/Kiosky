<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import Toggle from "../lib/Toggle.svelte";
  import { api, fmtBytes, type Disk, type Gpu, type ProcGroup, type Sample, type TopProcs } from "../lib/api";
  import { explainProcess } from "../lib/processes";
  import { store, saveSettings } from "../lib/settings.svelte";

  const HISTORY = 120; // 2 minutes, une mesure par seconde

  let history = $state<Sample[]>([]);
  let disks = $state<Disk[]>([]);
  let gpu = $state<Gpu | null>(null);
  let top = $state<TopProcs>({ cpu: [], mem: [], gpu: [] });
  let confirmKey = $state<string | null>(null);
  let killError = $state("");
  let confirmTimer: ReturnType<typeof setTimeout> | undefined;

  onMount(() => {
    api.getMonitor().then((m) => {
      history = m.history;
      disks = m.disks;
      gpu = m.gpu;
      top = m.top;
    });
    const un = listen<Sample>("monitor-sample", (e) => {
      history = [...history.slice(-(HISTORY - 1)), e.payload];
    });
    const unTop = listen<TopProcs>("monitor-top", (e) => {
      // Pendant une confirmation, on fige la liste pour que la ligne ne bouge pas sous la souris.
      if (!confirmKey) top = e.payload;
    });
    // L'espace disque bouge lentement.
    const t = setInterval(() => api.getMonitor().then((m) => (disks = m.disks)), 30000);
    return () => {
      un.then((f) => f());
      unTop.then((f) => f());
      clearInterval(t);
    };
  });

  const last = $derived(history[history.length - 1]);
  const memPct = $derived(last && last.mem_total ? (last.mem_used / last.mem_total) * 100 : 0);
  const netMax = $derived(Math.max(64 * 1024, ...history.map((s) => Math.max(s.net_down, s.net_up))));

  const W = 300;
  const H = 64;

  /** Courbe remplie à droite du graphique, les valeurs récentes à droite. */
  function area(values: number[], max: number): { line: string; fill: string } {
    if (values.length < 2) return { line: "", fill: "" };
    const step = W / (HISTORY - 1);
    const x0 = W - (values.length - 1) * step;
    const pts = values.map((v, i) => `${(x0 + i * step).toFixed(1)},${(H - 2 - (Math.min(v, max) / max) * (H - 4)).toFixed(1)}`);
    const line = "M" + pts.join("L");
    return { line, fill: `${line}L${W},${H}L${x0.toFixed(1)},${H}Z` };
  }

  const cpuCurve = $derived(area(history.map((s) => s.cpu), 100));
  const gpuCurve = $derived(area(history.map((s) => s.gpu ?? 0), 100));
  const memCurve = $derived(area(history.map((s) => (s.mem_total ? (s.mem_used / s.mem_total) * 100 : 0)), 100));
  const downCurve = $derived(area(history.map((s) => s.net_down), netMax));
  const upCurve = $derived(area(history.map((s) => s.net_up), netMax));

  const pct = (v: number) => Math.round(v).toLocaleString("fr-FR") + " %";
  const degrees = (v: number) => Math.round(v).toLocaleString("fr-FR") + " °C";
  /** Température de la carte graphique : orange dès 80 °C, rouge dès 90 °C */
  const heat = (v: number) => (v >= 90 ? "bad" : v >= 80 ? "warn" : "");

  const columns = $derived([
    { id: "cpu", label: "Processeur", list: top.cpu, empty: "Mesure en cours…" },
    ...(gpu ? [{ id: "gpu", label: "Processeur graphique", list: top.gpu, empty: "Aucun processus ne l'utilise" }] : []),
    { id: "mem", label: "Mémoire", list: top.mem, empty: "Mesure en cours…" },
  ]);
  const level = (v: number) => (v >= 90 ? "bad" : v >= 70 ? "warn" : "");

  function setTooltip(v: boolean) {
    if (!store.s) return;
    store.s.monitor_tooltip = v;
    saveSettings(0);
  }

  function setTrayIcon(v: boolean) {
    if (!store.s) return;
    store.s.monitor_tray_icon = v;
    saveSettings(0);
  }

  async function kill(list: string, g: ProcGroup) {
    const key = `${list}|${g.name}`;
    if (confirmKey !== key) {
      confirmKey = key;
      clearTimeout(confirmTimer);
      confirmTimer = setTimeout(() => (confirmKey = null), 3500);
      return;
    }
    confirmKey = null;
    try {
      await api.killProcesses(g.pids);
      killError = "";
    } catch (e) {
      killError = String(e);
    }
  }

  const pretty = (name: string) => name.replace(/\.exe$/i, "");
</script>

<PageHeader title="Moniteur" subtitle="Processeur, carte graphique, mémoire et réseau en direct, sur les 2 dernières minutes." />

<div class="grid">
  <div class="card metric {level(last?.cpu ?? 0)}">
    <div class="head">
      <span class="label">Processeur</span>
      <span class="value">{last ? pct(last.cpu) : "…"}</span>
    </div>
    <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none">
      <path class="fill" d={cpuCurve.fill} />
      <path class="line" d={cpuCurve.line} />
    </svg>
  </div>

  {#if gpu}
    <div class="card metric {level(last?.gpu ?? 0)}">
      <div class="head">
        <span class="label">Processeur graphique</span>
        <span class="value">
          {#if last?.gpu_temp != null}<span class="temp {heat(last.gpu_temp)}">{degrees(last.gpu_temp)}</span>{/if}
          {last?.gpu != null ? pct(last.gpu) : "…"}
        </span>
      </div>
      <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none">
        <path class="fill" d={gpuCurve.fill} />
        <path class="line" d={gpuCurve.line} />
      </svg>
      <span class="sub small muted gname" title={gpu.name}>
        {gpu.name}{last && gpu.mem_total ? ` · ${fmtBytes(last.gpu_mem)} sur ${fmtBytes(gpu.mem_total)}` : ""}
      </span>
    </div>
  {/if}

  <div class="card metric {level(memPct)}">
    <div class="head">
      <span class="label">Mémoire</span>
      <span class="value">{last ? pct(memPct) : "…"}</span>
    </div>
    <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none">
      <path class="fill" d={memCurve.fill} />
      <path class="line" d={memCurve.line} />
    </svg>
    <span class="sub small muted">{last ? `${fmtBytes(last.mem_used)} sur ${fmtBytes(last.mem_total)}` : ""}</span>
  </div>

  <div class="card metric net" class:wide={!gpu}>
    <div class="head">
      <span class="label">Réseau</span>
      <span class="legend">
        <span class="down"><Icon name="down" size={13} /> {last ? `${fmtBytes(last.net_down)}/s` : "…"}</span>
        <span class="up"><Icon name="up" size={13} /> {last ? `${fmtBytes(last.net_up)}/s` : "…"}</span>
      </span>
    </div>
    <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none">
      <path class="fill down" d={downCurve.fill} />
      <path class="line down" d={downCurve.line} />
      <path class="line up" d={upCurve.line} />
    </svg>
    <span class="sub small muted">Échelle : {fmtBytes(netMax)}/s</span>
  </div>
</div>

<h2>Processus les plus gourmands</h2>
{#if killError}
  <div class="banner error kill-error">{killError}</div>
{/if}
<div class="procs">
  {#each columns as col (col.id)}
    <div class="card plist">
      <div class="phead small">{col.label}</div>
      {#each col.list as g (g.name)}
        {@const key = `${col.id}|${g.name}`}
        {@const about = explainProcess(g.name)}
        <div class="prow">
          <div class="pinfo">
            <span class="pname" title={g.name}>{pretty(g.name)}</span>
            {#if g.pids.length > 1}<span class="pcount small">×{g.pids.length}</span>{/if}
            {#if about}<span class="pabout" title={about}><Icon name="info" size={14} /></span>{/if}
          </div>
          <span class="pval">{col.id === "mem" ? fmtBytes(g.mem) : pct(col.id === "gpu" ? g.gpu : g.cpu)}</span>
          {#if g.system}
            <span class="pwin" title="Processus de Windows : il ne peut pas être arrêté d'ici"><Icon name="shield" size={13} /></span>
          {:else}
            <button
              class="btn ghost pkill"
              class:confirm={confirmKey === key}
              title={g.pids.length > 1 ? `Arrêter les ${g.pids.length} processus` : "Arrêter"}
              onclick={() => kill(col.id, g)}
            >
              <Icon name="stop" size={12} />
              {#if confirmKey === key}Confirmer{/if}
            </button>
          {/if}
        </div>
      {:else}
        <div class="pempty small muted">{col.empty}</div>
      {/each}
    </div>
  {/each}
</div>

{#if disks.length}
  <h2>Disques</h2>
  <div class="card disks">
    {#each disks as d (d.letter)}
      {@const p = (d.used / d.total) * 100}
      <div class="disk">
        <div class="dico"><Icon name="disk" size={18} /></div>
        <div class="grow">
          <div class="dline">
            <span class="strong">{d.letter}</span>
            <span class="small muted">{fmtBytes(d.total - d.used)} libres sur {fmtBytes(d.total)}</span>
          </div>
          <div class="bar"><div class="used {level(p)}" style:width={`${p}%`}></div></div>
        </div>
      </div>
    {/each}
  </div>
{/if}

<h2>Zone de notification</h2>
<div class="card row">
  <div class="grow">
    <div class="strong">Résumé dans l'infobulle de l'icône</div>
    <div class="small muted">Survole l'icône Kiosky pour voir CPU, mémoire et débit sans ouvrir la fenêtre.</div>
  </div>
  <Toggle checked={store.s?.monitor_tooltip ?? true} label="Résumé dans l'infobulle" onchange={setTooltip} />
</div>
<div class="card row">
  <div class="grow">
    <div class="strong">Jauge du processeur comme icône</div>
    <div class="small muted">L'icône Kiosky devient une jauge qui se remplit selon l'utilisation du CPU (bleu, orange au-delà de 70 %, rouge au-delà de 90 %).</div>
  </div>
  <Toggle checked={store.s?.monitor_tray_icon ?? true} label="Jauge du processeur comme icône" onchange={setTrayIcon} />
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .metric {
    --c: var(--accent);
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 16px 18px 14px;
    min-width: 0;
  }
  .metric.warn {
    --c: var(--warn);
  }
  .metric.bad {
    --c: var(--bad);
  }
  .wide {
    grid-column: 1 / -1;
  }
  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
  .label {
    font-size: 12px;
    color: var(--text-2);
  }
  .value {
    font-family: var(--font-display);
    font-size: 26px;
    font-weight: 600;
    line-height: 1.1;
    font-variant-numeric: tabular-nums;
  }
  .temp {
    margin-right: 8px;
    font-size: 14px;
    color: var(--text-2);
  }
  .temp.warn {
    color: var(--warn);
  }
  .temp.bad {
    color: var(--bad);
  }
  .metric svg {
    width: 100%;
    height: 72px;
    display: block;
    border-radius: 6px;
    background: linear-gradient(var(--stroke) 1px, transparent 1px) 0 0 / 100% 25%;
  }
  .fill {
    fill: color-mix(in srgb, var(--c) 16%, transparent);
  }
  .line {
    fill: none;
    stroke: var(--c);
    stroke-width: 1.6;
    vector-effect: non-scaling-stroke;
    stroke-linejoin: round;
  }
  .net .down {
    --c: var(--accent);
  }
  .net .up {
    --c: #b46bff;
  }
  .net .line.up {
    stroke-dasharray: 4 3;
  }
  .legend {
    display: flex;
    gap: 18px;
    font-family: var(--font-display);
    font-size: 18px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .legend .down :global(svg),
  .legend .up :global(svg) {
    width: 13px;
    height: 13px;
    background: none;
    color: var(--c);
  }
  .sub {
    margin-top: -2px;
  }
  .gname {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  h2 {
    margin: 26px 0 12px;
    font-size: 14px;
    font-weight: 600;
  }
  .disks {
    overflow: hidden;
  }
  .disk {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px 18px;
    border-bottom: 1px solid var(--stroke);
  }
  .disk:last-child {
    border-bottom: none;
  }
  .dico {
    display: grid;
    place-items: center;
    flex: none;
    width: 36px;
    height: 36px;
    border-radius: 9px;
    background: var(--fill-hover);
    color: var(--text-2);
  }
  .dico :global(svg) {
    width: 18px;
    height: 18px;
    background: none;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .strong {
    font-weight: 600;
  }
  .dline {
    display: flex;
    justify-content: space-between;
    margin-bottom: 6px;
  }
  .bar {
    height: 6px;
    border-radius: 999px;
    background: var(--stroke-strong);
    overflow: hidden;
  }
  .used {
    height: 100%;
    border-radius: 999px;
    background: var(--accent);
    transition: width 0.4s;
  }
  .used.warn {
    background: var(--warn);
  }
  .used.bad {
    background: var(--bad);
  }
  .row + .row {
    margin-top: 8px;
  }
  .procs {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
    gap: 8px;
  }
  .kill-error {
    margin-bottom: 8px;
  }
  .plist {
    overflow: hidden;
  }
  .phead {
    padding: 10px 16px 6px;
    color: var(--text-2);
    font-weight: 600;
  }
  .prow {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 40px;
    padding: 0 8px 0 16px;
    border-top: 1px solid var(--stroke);
  }
  .prow:hover {
    background: var(--fill-hover);
  }
  .pinfo {
    flex: 1;
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
  }
  .pname {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-weight: 600;
  }
  .pcount {
    flex: none;
    color: var(--text-3);
  }
  .pabout {
    flex: none;
    align-self: center;
    display: grid;
    color: var(--text-3);
    cursor: help;
  }
  .pabout:hover {
    color: var(--accent);
  }
  .pval {
    flex: none;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .pwin {
    flex: none;
    display: grid;
    place-items: center;
    width: 28px;
    color: var(--text-3);
  }
  .pkill {
    flex: none;
    min-width: 28px;
    height: 28px;
    padding: 0 7px;
    gap: 5px;
    font-size: 12px;
    color: var(--text-2);
    opacity: 0;
    transition: opacity 0.12s;
  }
  .prow:hover .pkill,
  .pkill.confirm {
    opacity: 1;
  }
  .pkill:hover,
  .pkill.confirm {
    color: var(--bad);
  }
  .pkill.confirm {
    background: color-mix(in srgb, var(--bad) 12%, transparent);
    font-weight: 600;
  }
  .pempty {
    padding: 14px 16px;
    border-top: 1px solid var(--stroke);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 16px 18px;
  }
</style>
