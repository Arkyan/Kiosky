<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Icon from "./lib/Icon.svelte";
  import { api, fmtBytes, type Sample, type Settings } from "./lib/api";

  // Barre flottante : fenêtre transparente qui s'ajuste à son contenu (Rust la place et l'affiche).

  let settings = $state<Settings | null>(null);
  let sample = $state<Sample | null>(null);
  let now = $state(new Date());
  let battery = $state<{ present: boolean; percent: number; charging: boolean } | null>(null);
  let devPorts = $state<number[]>([]);
  let docker = $state<{ running: number; total: number; up: boolean } | null>(null);
  let git = $state<{ name: string; branch: string; changes: number; ahead: number } | null>(null);
  let taskbarH = $state<number | null>(null);
  let bar: HTMLDivElement;

  const items = $derived(settings?.widget_items ?? []);
  const has = (id: string) => items.includes(id);
  const onTaskbar = $derived(settings?.widget_mode !== "free");
  const vertical = $derived(!onTaskbar && !!settings?.widget_vertical);

  async function loadSettings() {
    settings = await api.getSettings();
    taskbarH = await api.taskbarHeight();
    refreshSlow();
    await fit(true);
  }

  // ─── Données lentes, seulement pour les éléments affichés ───

  async function refreshSlow() {
    if (!settings) return;
    if (has("battery")) battery = await api.getBattery();
    if (has("ports")) {
      const list = await api.getPorts().catch(() => []);
      devPorts = [...new Set(list.filter((p) => p.proto === "TCP" && p.listening && !p.system).map((p) => p.local_port))].sort((a, b) => a - b);
    }
  }

  async function refreshDocker() {
    if (!has("docker")) return;
    const d = await api.getDocker().catch(() => null);
    docker = d ? { running: d.containers.filter((c) => c.running).length, total: d.containers.length, up: d.running } : null;
  }

  async function refreshGit() {
    if (!has("git") || !settings) return;
    const fav = settings.project_favorites[0];
    if (!fav) {
      git = null;
      return;
    }
    const [st] = await api.gitStatus([fav]).catch(() => []);
    const name = fav.split(/[\\/]/).filter(Boolean).pop() ?? fav;
    git = st ? { name, branch: st.branch ?? "?", changes: st.changes, ahead: st.ahead } : { name, branch: "—", changes: 0, ahead: 0 };
  }

  // ─── Taille : la fenêtre épouse la barre ───

  let lastSize = "";
  async function fit(force = false) {
    await tick();
    if (!bar || !settings?.widget_enabled) return;
    const r = bar.getBoundingClientRect();
    const key = `${Math.ceil(r.width)}x${Math.ceil(r.height)}`;
    if (!force && key === lastSize) return;
    lastSize = key;
    await api.fitWidget(r.width, r.height);
  }

  // Le contenu change de largeur (« 9 % » → « 12 % ») : on réajuste.
  $effect(() => {
    void [sample, now, battery, devPorts, docker, git, items, vertical, onTaskbar];
    fit();
  });

  onMount(() => {
    loadSettings();
    const timers = [
      setInterval(() => (now = new Date()), 1000),
      setInterval(refreshSlow, 5000),
      setInterval(refreshDocker, 15000),
      setInterval(refreshGit, 30000),
    ];
    refreshDocker();
    refreshGit();
    const uns = [
      listen<Sample>("monitor-sample", (e) => (sample = e.payload)),
      listen("widget-config", loadSettings),
      listen("settings-changed", loadSettings),
    ];
    return () => {
      timers.forEach(clearInterval);
      uns.forEach((u) => u.then((f) => f()));
    };
  });

  function onmousedown(e: MouseEvent) {
    if (e.button === 0 && !onTaskbar) getCurrentWindow().startDragging();
  }

  const pct = (v: number) => `${Math.round(v)} %`;
  const memPct = $derived(sample && sample.mem_total ? (sample.mem_used / sample.mem_total) * 100 : 0);
  const level = (v: number) => (v >= 90 ? "bad" : v >= 70 ? "warn" : "");
  const time = $derived(now.toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" }));
  const date = $derived(now.toLocaleDateString("fr-FR", { weekday: "short", day: "numeric", month: "short" }));
  /** Débit compact : « 1,2 Mo » plutôt que « 1,2 Mo/s » pour gagner de la place */
  const rate = (b: number) => fmtBytes(b).replace(" ", " ");
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  bind:this={bar}
  class="bar"
  class:vertical
  class:taskbar={onTaskbar}
  class:draggable={!onTaskbar}
  style:--opacity={settings?.widget_opacity ?? 0.85}
  style:height={onTaskbar && taskbarH ? `${Math.max(28, taskbarH - 10)}px` : undefined}
  {onmousedown}
  ondblclick={() => api.runAction("page:monitor")}
  title="Double-clic : ouvrir le Moniteur"
>
  {#each items as id (id)}
    {#if id === "cpu"}
      <span class="item {level(sample?.cpu ?? 0)}">
        <span class="lbl">CPU</span>
        <span class="gauge"><span style:height={`${Math.min(100, sample?.cpu ?? 0)}%`}></span></span>
        <span class="val">{sample ? pct(sample.cpu) : "…"}</span>
      </span>
    {:else if id === "ram"}
      <span class="item {level(memPct)}">
        <span class="lbl">RAM</span>
        <span class="gauge"><span style:height={`${memPct}%`}></span></span>
        <span class="val">{sample ? pct(memPct) : "…"}</span>
      </span>
    {:else if id === "net"}
      <span class="item net">
        <span class="down"><Icon name="down" size={11} />{sample ? rate(sample.net_down) : "…"}</span>
        <span class="up"><Icon name="up" size={11} />{sample ? rate(sample.net_up) : "…"}</span>
      </span>
    {:else if id === "time"}
      <span class="item"><span class="val big">{time}</span></span>
    {:else if id === "date"}
      <span class="item"><span class="val">{date}</span></span>
    {:else if id === "battery" && battery?.present}
      <span class="item {battery.percent <= 15 && !battery.charging ? 'bad' : ''}">
        <Icon name={battery.charging ? "bolt" : "power"} size={12} />
        <span class="val">{battery.percent} %</span>
      </span>
    {:else if id === "ports"}
      <span class="item" title="Serveurs locaux en écoute">
        <Icon name="plug" size={12} />
        <span class="val">{devPorts.length ? devPorts.slice(0, 4).join(" · ") + (devPorts.length > 4 ? "…" : "") : "aucun"}</span>
      </span>
    {:else if id === "docker" && docker}
      <span class="item" title="Conteneurs Docker en cours">
        <Icon name="box" size={12} />
        <span class="val">{docker.up ? `${docker.running}/${docker.total}` : "arrêté"}</span>
      </span>
    {:else if id === "git" && git}
      <span class="item" title="Projet favori">
        <Icon name="branch" size={12} />
        <span class="val">{git.name} · {git.branch}</span>
        {#if git.changes}<span class="dirty">●{git.changes}</span>{/if}
        {#if git.ahead}<span class="ahead">↑{git.ahead}</span>{/if}
      </span>
    {/if}
  {/each}
  {#if !items.length}
    <span class="item"><span class="val">Choisis des éléments dans Réglages</span></span>
  {/if}
</div>

<style>
  :global(html),
  :global(body) {
    background: transparent;
    overflow: hidden;
  }
  .bar {
    --bg: 32, 32, 32;
    --fg: #f3f3f3;
    --dim: rgba(255, 255, 255, 0.55);
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 0 6px;
    height: 34px;
    border-radius: 9px;
    background: rgba(var(--bg), var(--opacity));
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: var(--fg);
    font-size: 12px;
    line-height: 1;
    white-space: nowrap;
    user-select: none;
    cursor: default;
  }
  @media (prefers-color-scheme: light) {
    .bar {
      --bg: 249, 249, 249;
      --fg: #1a1a1a;
      --dim: rgba(0, 0, 0, 0.5);
      border-color: rgba(0, 0, 0, 0.08);
    }
  }
  .bar.draggable {
    cursor: grab;
  }
  .bar.taskbar {
    border-radius: 6px;
  }
  .bar.vertical {
    flex-direction: column;
    align-items: stretch;
    height: auto;
    padding: 6px;
    gap: 4px;
  }
  .item {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 0 7px;
    height: 100%;
    font-variant-numeric: tabular-nums;
  }
  .vertical .item {
    height: 22px;
  }
  .item + .item {
    border-left: 1px solid rgba(128, 128, 128, 0.25);
  }
  .vertical .item + .item {
    border-left: none;
  }
  .lbl {
    font-size: 10px;
    font-weight: 600;
    color: var(--dim);
  }
  .val {
    font-weight: 600;
  }
  .val.big {
    font-size: 13px;
  }
  .gauge {
    position: relative;
    width: 4px;
    height: 14px;
    border-radius: 2px;
    background: rgba(128, 128, 128, 0.3);
    overflow: hidden;
  }
  .gauge span {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    background: #3a9bf0;
    transition: height 0.4s;
  }
  .warn .gauge span {
    background: #f5a623;
  }
  .bad .gauge span {
    background: #e8483c;
  }
  .bad .val {
    color: #ff6b5e;
  }
  .net {
    gap: 8px;
  }
  .net span {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    font-weight: 600;
  }
  .net .down :global(svg) {
    color: #3a9bf0;
  }
  .net .up :global(svg) {
    color: #b46bff;
  }
  .dirty {
    color: #f5a623;
    font-weight: 700;
  }
  .ahead {
    color: #3a9bf0;
    font-weight: 700;
  }
</style>
