<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Icon from "./lib/Icon.svelte";
  import { api, fmtBytes, type Project, type Sample, type Settings, type TopProcs } from "./lib/api";

  // Barre flottante : fenêtre transparente qui s'ajuste à son contenu (Rust la place et l'affiche).
  // Clic sur un élément : son action. Survol : le détail en infobulle. Molette sur le volume : réglage.

  type Media = { title: string; artist: string; app: string; playing: boolean; cover: string | null };

  let settings = $state<Settings | null>(null);
  let sample = $state<Sample | null>(null);
  let top = $state<TopProcs | null>(null);
  let now = $state(new Date());
  let battery = $state<{ present: boolean; percent: number; charging: boolean } | null>(null);
  let devPorts = $state<{ port: number; process: string }[]>([]);
  let docker = $state<{ running: number; total: number; up: boolean; names: string[] } | null>(null);
  let git = $state<{ name: string; path: string; branch: string; changes: number; ahead: number; behind: number } | null>(null);
  let projects: Project[] = [];
  let media = $state<Media | null>(null);
  let volume = $state<{ volume: number; muted: boolean } | null>(null);
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
    refreshFast();
    refreshDocker();
    refreshGit();
    await fit(true);
  }

  // ─── Données, seulement pour les éléments affichés ───

  async function refreshFast() {
    if (has("media")) media = await api.getMedia().catch(() => null);
    if (has("volume")) volume = await api.getVolume().catch(() => null);
  }

  async function refreshSlow() {
    if (!settings) return;
    if (has("battery")) battery = await api.getBattery();
    if (has("ports")) {
      const list = await api.getPorts().catch(() => []);
      const seen = new Map<number, string>();
      for (const p of list) if (p.proto === "TCP" && p.listening && !p.system && !seen.has(p.local_port)) seen.set(p.local_port, p.process);
      devPorts = [...seen.entries()].sort((a, b) => a[0] - b[0]).map(([port, process]) => ({ port, process }));
    }
  }

  async function refreshDocker() {
    if (!has("docker")) return;
    const d = await api.getDocker().catch(() => null);
    docker = d
      ? {
          running: d.containers.filter((c) => c.running).length,
          total: d.containers.length,
          up: d.running,
          names: d.containers.filter((c) => c.running).map((c) => c.name),
        }
      : null;
  }

  async function refreshGit() {
    if (!has("git") || !settings) return;
    const fav = settings.project_favorites[0];
    if (!fav) {
      git = null;
      return;
    }
    if (!projects.length) projects = await api.getProjects().catch(() => []);
    const [st] = await api.gitStatus([fav]).catch(() => []);
    const name = fav.split(/[\\/]/).filter(Boolean).pop() ?? fav;
    git = { name, path: fav, branch: st?.branch ?? "inconnue", changes: st?.changes ?? 0, ahead: st?.ahead ?? 0, behind: st?.behind ?? 0 };
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

  $effect(() => {
    void [sample, battery, devPorts, docker, git, media, volume, items, vertical, onTaskbar];
    fit();
  });

  onMount(() => {
    loadSettings();
    const timers = [
      setInterval(() => (now = new Date()), 1000),
      setInterval(refreshFast, 1500),
      setInterval(refreshSlow, 5000),
      setInterval(refreshDocker, 15000),
      setInterval(refreshGit, 30000),
    ];
    const uns = [
      listen<Sample>("monitor-sample", (e) => (sample = e.payload)),
      listen<TopProcs>("monitor-top", (e) => (top = e.payload)),
      listen("widget-config", loadSettings),
      listen("settings-changed", loadSettings),
    ];
    return () => {
      timers.forEach(clearInterval);
      uns.forEach((u) => u.then((f) => f()));
    };
  });

  // ─── Souris : clic = action, glisser = déplacer (position libre) ───

  let down: { x: number; y: number } | null = null;
  let dragged = false;

  function onpointerdown(e: PointerEvent) {
    if (e.button !== 0) return;
    clearTimeout(tipTimer);
    if (tipShown) {
      tipShown = "";
      api.hideTip();
    }
    down = { x: e.screenX, y: e.screenY };
    dragged = false;
  }

  function onpointermove(e: PointerEvent) {
    if (!down || dragged || onTaskbar || !(e.buttons & 1)) return;
    if (Math.abs(e.screenX - down.x) + Math.abs(e.screenY - down.y) > 4) {
      dragged = true;
      getCurrentWindow().startDragging();
    }
  }

  function onpointerup(e: PointerEvent) {
    if (e.button !== 0 || !down) return;
    down = null;
    if (dragged) return;
    const el = (e.target as Element).closest<HTMLElement>("[data-act]");
    if (el) act(el.dataset.act!);
  }

  async function act(action: string) {
    switch (action) {
      case "monitor":
        return api.runAction("page:monitor");
      case "ports":
        return api.runAction("page:ports");
      case "docker":
        return api.runAction("page:containers");
      case "git": {
        if (!git) return api.runAction("page:projects");
        const editor = projects.find((p) => p.path === git!.path)?.editor ?? "vscode";
        return api.openProject(git.path, editor);
      }
      case "battery":
        return api.runAction("uri:ms-settings:powersleep");
      case "media-toggle":
      case "media-next":
      case "media-prev":
        await api.mediaControl(action.slice(6) as "toggle" | "next" | "prev").catch(() => {});
        setTimeout(refreshFast, 250);
        return;
      case "volume":
        if (!volume) return;
        volume = { ...volume, muted: !volume.muted };
        return api.setMasterMute(volume.muted);
      case "media-open":
        return api.mediaFocus().catch(() => {});
    }
  }

  function onwheel(e: WheelEvent) {
    if (!volume || !(e.target as Element).closest("[data-act='volume']")) return;
    e.preventDefault();
    const v = Math.min(1, Math.max(0, volume.volume + (e.deltaY < 0 ? 0.02 : -0.02)));
    volume = { volume: v, muted: false };
    api.setMasterVolume(v);
    if (v > 0) api.setMasterMute(false);
  }

  // ─── Infobulle : fenêtre à part, toujours au-dessus de la barre ───

  let tipKey: string | null = null;
  let tipEl: HTMLElement | null = null;
  let tipTimer: ReturnType<typeof setTimeout> | undefined;
  let tipShown = "";

  function sendTip() {
    if (!tipKey || !tipEl) return;
    const text = tips[tipKey];
    if (!text) return;
    tipShown = text;
    const r = tipEl.getBoundingClientRect();
    api.showTip(text, r.left, r.width);
  }

  function onpointerover(e: PointerEvent) {
    const el = (e.target as Element).closest<HTMLElement>("[data-tip]");
    const key = el?.dataset.tip ?? null;
    if (key === tipKey) return;
    clearTimeout(tipTimer);
    tipKey = key;
    tipEl = el ?? null;
    if (!key) {
      tipShown = "";
      api.hideTip();
      return;
    }
    // Déjà une infobulle ouverte : on passe directement à la suivante.
    tipTimer = setTimeout(sendTip, tipShown ? 0 : 450);
  }

  function onpointerleave() {
    clearTimeout(tipTimer);
    tipKey = null;
    tipEl = null;
    tipShown = "";
    api.hideTip();
  }

  // Les valeurs changent pendant le survol : l'infobulle suit.
  $effect(() => {
    const key = tipKey;
    const text = key ? tips[key] : "";
    if (key && tipShown && text && text !== tipShown) sendTip();
  });

  // ─── Affichage ───

  const pct = (v: number) => `${Math.round(v)} %`;
  const memPct = $derived(sample && sample.mem_total ? (sample.mem_used / sample.mem_total) * 100 : 0);
  const level = (v: number) => (v >= 90 ? "bad" : v >= 70 ? "warn" : "");
  const time = $derived(now.toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" }));
  const date = $derived(now.toLocaleDateString("fr-FR", { weekday: "short", day: "numeric", month: "short" }));
  const longDate = $derived(now.toLocaleDateString("fr-FR", { weekday: "long", day: "numeric", month: "long", year: "numeric" }));
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
  const strip = (n: string) => n.replace(/\.exe$/i, "");

  // Infobulles : le détail au survol (infobulle native, elle peut dépasser de la barre).
  const tips = $derived({
    cpu:
      `Processeur : ${sample ? pct(sample.cpu) : "…"}` +
      (top?.cpu.length ? "\n\n" + top.cpu.slice(0, 3).map((g) => `${strip(g.name)}\t${pct(g.cpu)}`).join("\n") : "") +
      "\n\nClic : ouvrir le Moniteur",
    ram:
      `Mémoire : ${sample ? `${fmtBytes(sample.mem_used)} sur ${fmtBytes(sample.mem_total)} (${pct(memPct)})` : "…"}` +
      (top?.mem.length ? "\n\n" + top.mem.slice(0, 3).map((g) => `${strip(g.name)}\t${fmtBytes(g.mem)}`).join("\n") : "") +
      "\n\nClic : ouvrir le Moniteur",
    net: sample ? `Réseau\n\nRéception\t${fmtBytes(sample.net_down)}/s\nEnvoi\t${fmtBytes(sample.net_up)}/s\n\nClic : ouvrir le Moniteur` : "Réseau",
    time: longDate,
    date: longDate,
    battery: battery ? `Batterie : ${battery.percent} %\n${battery.charging ? "Branchée sur secteur" : "Sur batterie"}\n\nClic : options d'alimentation` : "",
    ports: devPorts.length
      ? `Serveurs locaux :\n${devPorts.map((p) => `${p.port}\t${strip(p.process)}`).join("\n")}\n\nClic : ouvrir la page Ports`
      : "Aucun serveur local en écoute\n\nClic : ouvrir la page Ports",
    docker: !docker
      ? "Docker"
      : !docker.up
        ? "Docker n'est pas lancé\n\nClic : ouvrir la page Conteneurs"
        : `Conteneurs en cours : ${docker.running} sur ${docker.total}` +
          (docker.names.length ? "\n" + docker.names.slice(0, 8).join("\n") : "") +
          "\n\nClic : ouvrir la page Conteneurs",
    git: git
      ? `${git.name}\n\nBranche\t${git.branch}\nModifications\t${git.changes || "aucune"}` +
        (git.ahead ? `\nÀ pousser\t${git.ahead} commit(s)` : "") +
        (git.behind ? `\nÀ récupérer\t${git.behind} commit(s)` : "") +
        "\n\nClic : ouvrir le projet"
      : "",
    media: media
      ? `${media.title}${media.artist ? `\n${media.artist}` : ""}\n\n${media.app}\t${media.playing ? "en lecture" : "en pause"}\n\nClic sur le titre : ouvrir ${media.app}`
      : "Aucune lecture en cours\n\nLance Spotify, YouTube, VLC…",
    volume: volume ? `Volume : ${volume.muted ? "coupé" : pct(volume.volume * 100)}\n\nMolette : régler · Clic : couper / rétablir` : "Volume",
  } as Record<string, string>);
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
  {onpointerdown}
  {onpointermove}
  {onpointerup}
  {onwheel}
  {onpointerover}
  {onpointerleave}
>
  {#each items as id (id)}
    {#if id === "cpu"}
      <span class="item click {level(sample?.cpu ?? 0)}" data-act="monitor" data-tip="cpu">
        <span class="lbl">CPU</span>
        <span class="gauge"><span style:height={`${Math.min(100, sample?.cpu ?? 0)}%`}></span></span>
        <span class="val w-pct">{sample ? pct(sample.cpu) : "…"}</span>
      </span>
    {:else if id === "ram"}
      <span class="item click {level(memPct)}" data-act="monitor" data-tip="ram">
        <span class="lbl">RAM</span>
        <span class="gauge"><span style:height={`${memPct}%`}></span></span>
        <span class="val w-pct">{sample ? pct(memPct) : "…"}</span>
      </span>
    {:else if id === "net"}
      <span class="item click net" data-act="monitor" data-tip="net">
        <span class="down"><Icon name="down" size={11} /><span class="w-rate">{sample ? rate(sample.net_down) : "…"}</span></span>
        <span class="up"><Icon name="up" size={11} /><span class="w-rate">{sample ? rate(sample.net_up) : "…"}</span></span>
      </span>
    {:else if id === "time"}
      <span class="item" data-tip="time"><span class="val big w-time">{time}</span></span>
    {:else if id === "date"}
      <span class="item" data-tip="date"><span class="val w-date">{date}</span></span>
    {:else if id === "battery" && battery?.present}
      <span class="item click {battery.percent <= 15 && !battery.charging ? 'bad' : ''}" data-act="battery" data-tip="battery">
        <Icon name={battery.charging ? "bolt" : "power"} size={12} />
        <span class="val w-pct">{battery.percent} %</span>
      </span>
    {:else if id === "ports"}
      <span class="item click" data-act="ports" data-tip="ports">
        <Icon name="plug" size={12} />
        <span class="val w-ports">{devPorts.length ? devPorts.map((p) => p.port).join(" · ") : "aucun"}</span>
      </span>
    {:else if id === "docker"}
      <span class="item click" data-act="docker" data-tip="docker">
        <Icon name="box" size={12} />
        <span class="val w-docker">{!docker ? "…" : docker.up ? `${docker.running}/${docker.total}` : "arrêté"}</span>
      </span>
    {:else if id === "git" && git}
      <span class="item click" data-act="git" data-tip="git">
        <Icon name="branch" size={12} />
        <span class="val w-git">{git.name} · {git.branch}</span>
        <span class="w-gitstate">
          {#if git.changes}<span class="dirty">●{git.changes}</span>{/if}
          {#if git.ahead}<span class="ahead">↑{git.ahead}</span>{/if}
          {#if !git.changes && !git.ahead}<span class="clean">✓</span>{/if}
        </span>
      </span>
    {:else if id === "media"}
      <span class="item media" class:paused={!media?.playing} class:empty={!media}>
        <span class="mopen" data-act="media-open" data-tip="media">
          {#if media?.cover}
            <img class="cover" src={media.cover} alt="" />
          {:else}
            <span class="cover none"><Icon name="volume" size={13} /></span>
          {/if}
          <span class="mlines">
            <span class="mtitle">{media ? media.title : "Aucune lecture"}</span>
            {#if !onTaskbar || (taskbarH ?? 40) >= 40}
              <span class="martist">{media ? media.artist || media.app : "Spotify, YouTube…"}</span>
            {/if}
          </span>
        </span>
        <span class="mctl">
          <button class="mbtn" data-act="media-prev" disabled={!media} aria-label="Précédent"><Icon name="prev" size={14} /></button>
          <button class="mbtn play" data-act="media-toggle" disabled={!media} aria-label="Lecture ou pause">
            <Icon name={media?.playing ? "pause" : "play"} size={15} />
          </button>
          <button class="mbtn" data-act="media-next" disabled={!media} aria-label="Suivant"><Icon name="next" size={14} /></button>
        </span>
      </span>
    {:else if id === "volume"}
      <span class="item click" class:muted={volume?.muted} data-act="volume" data-tip="volume">
        <Icon name={volume?.muted ? "mute" : "volume"} size={13} />
        <span class="val w-pct">{!volume ? "…" : volume.muted ? "muet" : pct(volume.volume * 100)}</span>
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
  /* Éléments cliquables : léger survol, sans rien déplacer. */
  .click {
    border-radius: 5px;
    cursor: pointer;
  }
  .draggable .click {
    cursor: pointer;
  }
  .click:hover,
  .mtext:hover {
    background: rgba(128, 128, 128, 0.18);
  }
  .muted .val,
  .muted :global(svg) {
    color: var(--dim);
  }
  /* Lecteur : pochette, titre et artiste sur deux lignes, ⏮ ⏯ ⏭ */
  .media {
    gap: 4px;
    padding: 0 4px 0 3px;
  }
  .mopen {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 100%;
    padding: 0 6px 0 3px;
    border-radius: 5px;
    cursor: pointer;
  }
  .mopen:hover {
    background: rgba(128, 128, 128, 0.18);
  }
  .empty .mopen {
    cursor: default;
  }
  .cover {
    flex: none;
    width: 24px;
    height: 24px;
    border-radius: 4px;
    object-fit: cover;
  }
  .cover.none {
    display: grid;
    place-items: center;
    background: rgba(128, 128, 128, 0.22);
    color: var(--dim);
  }
  .mlines {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 2px;
    width: 19ch; /* largeur fixe : rien ne bouge d'un morceau à l'autre */
  }
  .mtitle,
  .martist {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .mtitle {
    font-size: 11.5px;
    font-weight: 600;
  }
  .martist {
    font-size: 10px;
    color: var(--dim);
  }
  .paused .mtitle,
  .empty .mtitle {
    color: var(--dim);
  }
  .mctl {
    display: inline-flex;
    align-items: center;
  }
  .mbtn {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--fg);
    cursor: pointer;
  }
  .mbtn.play {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: rgba(128, 128, 128, 0.2);
  }
  .mbtn:hover:not(:disabled) {
    background: rgba(128, 128, 128, 0.3);
  }
  .mbtn:disabled {
    opacity: 0.3;
    cursor: default;
  }
  .mbtn :global(svg) {
    stroke-width: 2.2;
  }
  .mbtn.play :global(svg) {
    fill: currentColor;
  }
</style>
