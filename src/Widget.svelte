<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Icon from "./lib/Icon.svelte";
  import { api, type Sample, type Settings } from "./lib/api";

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

  /** Éléments cochés, dans l'ordre réglé */
  const items = $derived.by(() => {
    if (!settings) return [];
    const on = settings.widget_items;
    return [...settings.widget_order.filter((id) => on.includes(id)), ...on.filter((id) => !settings!.widget_order.includes(id))];
  });
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
  /** Débit compact et de longueur bornée : « 980 Ko », « 9,8 Mo », « 98 Mo » (jamais « 1 023 Ko ») */
  function rate(b: number): string {
    const units = ["o", "Ko", "Mo", "Go"];
    let v = b;
    let i = 0;
    while (v >= 1000 && i < units.length - 1) {
      v /= 1024;
      i++;
    }
    const txt = v.toLocaleString("fr-FR", { maximumFractionDigits: v < 10 && i > 0 ? 1 : 0, useGrouping: false });
    return `${txt} ${units[i]}`;
  }
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
        <span class="val w-pct">{sample ? pct(sample.cpu) : "…"}</span>
      </span>
    {:else if id === "ram"}
      <span class="item {level(memPct)}">
        <span class="lbl">RAM</span>
        <span class="gauge"><span style:height={`${memPct}%`}></span></span>
        <span class="val w-pct">{sample ? pct(memPct) : "…"}</span>
      </span>
    {:else if id === "net"}
      <span class="item net">
        <span class="down"><Icon name="down" size={11} /><span class="w-rate">{sample ? rate(sample.net_down) : "…"}</span></span>
        <span class="up"><Icon name="up" size={11} /><span class="w-rate">{sample ? rate(sample.net_up) : "…"}</span></span>
      </span>
    {:else if id === "time"}
      <span class="item"><span class="val big w-time">{time}</span></span>
    {:else if id === "date"}
      <span class="item"><span class="val w-date">{date}</span></span>
    {:else if id === "battery" && battery?.present}
      <span class="item {battery.percent <= 15 && !battery.charging ? 'bad' : ''}">
        <Icon name={battery.charging ? "bolt" : "power"} size={12} />
        <span class="val w-pct">{battery.percent} %</span>
      </span>
    {:else if id === "ports"}
      <span class="item" title="Serveurs locaux en écoute">
        <Icon name="plug" size={12} />
        <span class="val w-ports" title={devPorts.join(", ")}>{devPorts.length ? devPorts.join(" · ") : "aucun"}</span>
      </span>
    {:else if id === "docker"}
      <span class="item" title="Conteneurs Docker en cours">
        <Icon name="box" size={12} />
        <span class="val w-docker">{!docker ? "…" : docker.up ? `${docker.running}/${docker.total}` : "arrêté"}</span>
      </span>
    {:else if id === "git" && git}
      <span class="item" title="Projet favori">
        <Icon name="branch" size={12} />
        <span class="val w-git" title={`${git.name} · ${git.branch}`}>{git.name} · {git.branch}</span>
        <span class="w-gitstate">
          {#if git.changes}<span class="dirty">●{git.changes}</span>{/if}
          {#if git.ahead}<span class="ahead">↑{git.ahead}</span>{/if}
          {#if !git.changes && !git.ahead}<span class="clean">✓</span>{/if}
        </span>
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
  /* Largeurs réservées pour la valeur la plus longue possible : rien ne bouge. */
  .w-pct,
  .w-rate,
  .w-time,
  .w-date,
  .w-ports,
  .w-docker,
  .w-git,
  .w-gitstate {
    display: inline-block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .w-pct {
    width: 4.6ch; /* « 100 % » */
    text-align: right;
  }
  .w-rate {
    width: 7.2ch; /* « 999,9 Ko » */
    text-align: right;
  }
  .w-time {
    width: 5.2ch;
    text-align: center;
  }
  .w-date {
    width: 13ch; /* « mer. 30 sept. » */
    text-align: center;
  }
  .w-ports {
    width: 15ch;
  }
  .w-docker {
    width: 6ch;
  }
  .w-git {
    width: 18ch;
  }
  .w-gitstate {
    width: 7ch;
  }
  .vertical .w-ports,
  .vertical .w-git {
    width: auto;
    max-width: 22ch;
  }
  .clean {
    color: #4caf50;
    font-weight: 700;
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
