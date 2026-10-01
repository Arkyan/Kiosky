<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import { api, copyText, type Container, type DockerState, type WslState } from "../lib/api";

  let wsl = $state<WslState | null>(null);
  let docker = $state<DockerState | null>(null);
  let busy = $state<string | null>(null);
  let error = $state("");
  let starting = $state(false);
  let confirmRemove = $state<string | null>(null);
  let logs = $state<{ name: string; text: string } | null>(null);
  let logsCopied = $state(false);

  async function load() {
    const [w, d] = await Promise.all([api.getWsl(), api.getDocker()]);
    wsl = w;
    docker = d;
    if (d.running) starting = false;
  }

  onMount(() => {
    load();
    // Rafraîchissement : plus fréquent pendant le démarrage de Docker Desktop.
    const t = setInterval(() => {
      if (!document.hidden && !busy) load();
    }, 4000);
    return () => clearInterval(t);
  });

  async function act(key: string, f: () => Promise<void>) {
    busy = key;
    try {
      await f();
      error = "";
      await load();
    } catch (e) {
      error = String(e);
    }
    busy = null;
  }

  const wslDo = (name: string, action: string) => act(`wsl|${name}|${action}`, () => api.wslAction(name, action));
  const dockerDo = (ids: string[], action: string) => act(`docker|${ids.join(",")}|${action}`, () => api.dockerAction(ids, action));

  async function remove(c: Container) {
    if (confirmRemove !== c.id) {
      confirmRemove = c.id;
      setTimeout(() => confirmRemove === c.id && (confirmRemove = null), 3500);
      return;
    }
    confirmRemove = null;
    await dockerDo([c.id], "remove");
  }

  async function showLogs(c: Container) {
    try {
      logs = { name: c.name, text: "Chargement…" };
      logs = { name: c.name, text: (await api.dockerLogs(c.id)) || "(aucun journal)" };
    } catch (e) {
      logs = { name: c.name, text: String(e) };
    }
  }

  async function startDesktop() {
    starting = true;
    try {
      await api.startDockerDesktop();
    } catch (e) {
      error = String(e);
      starting = false;
    }
  }

  /** Conteneurs regroupés par projet docker compose */
  const groups = $derived.by(() => {
    const m = new Map<string, Container[]>();
    for (const c of docker?.containers ?? []) {
      const k = c.project ?? "";
      m.set(k, [...(m.get(k) ?? []), c]);
    }
    return [...m.entries()].sort((a, b) => (a[0] === "" ? 1 : b[0] === "" ? -1 : a[0].localeCompare(b[0])));
  });

  const isBusy = (prefix: string) => busy?.startsWith(prefix) ?? false;
  const internal = (name: string) => name.startsWith("docker-desktop");
</script>

<PageHeader title="Conteneurs" subtitle="Distributions WSL et conteneurs Docker.">
  {#snippet actions()}
    <button class="btn" onclick={load}><Icon name="refresh" size={16} /> Actualiser</button>
  {/snippet}
</PageHeader>

{#if error}
  <div class="banner error">{error}</div>
{/if}

<!-- WSL -->
<div class="section-head">
  <h2>WSL</h2>
  {#if wsl?.distros.some((d) => d.running)}
    <button class="btn ghost small-btn" disabled={isBusy("wsl||shutdown")} onclick={() => wslDo("", "shutdown")}>
      <Icon name="power" size={14} /> Tout arrêter
    </button>
  {/if}
</div>
{#if wsl && !wsl.installed}
  <div class="card empty muted">WSL n'est pas installé (<span class="mono">wsl --install</span> dans un terminal admin).</div>
{:else if wsl}
  <div class="distros">
    {#each wsl.distros as d (d.name)}
      <div class="card distro" class:on={d.running}>
        <div class="dhead">
          <span class="dot"></span>
          <span class="strong">{d.name}</span>
          {#if d.default}<span class="badge accent">Par défaut</span>{/if}
          {#if internal(d.name)}<span class="badge">Docker</span>{/if}
          <span class="grow"></span>
          <span class="small muted">WSL {d.version} · {d.running ? "Démarrée" : "Arrêtée"}</span>
        </div>
        {#if !internal(d.name)}
          <div class="dactions">
            <button class="btn small-btn primary" onclick={() => wslDo(d.name, "terminal")}><Icon name="terminal" size={14} /> Terminal</button>
            <button class="btn ghost small-btn" onclick={() => wslDo(d.name, "explorer")}><Icon name="folder" size={14} /> Fichiers</button>
            {#if d.running}
              <button class="btn ghost small-btn" disabled={isBusy(`wsl|${d.name}`)} onclick={() => wslDo(d.name, "stop")}><Icon name="stop" size={13} /> Arrêter</button>
            {:else}
              <button class="btn ghost small-btn" disabled={isBusy(`wsl|${d.name}`)} onclick={() => wslDo(d.name, "start")}><Icon name="play" size={13} /> Démarrer</button>
            {/if}
            {#if !d.default}
              <button class="btn ghost small-btn" onclick={() => wslDo(d.name, "default")}>Par défaut</button>
            {/if}
          </div>
        {/if}
      </div>
    {/each}
  </div>
{:else}
  <div class="card skeleton"></div>
{/if}

<!-- Docker -->
<div class="section-head">
  <h2>Docker</h2>
  {#if docker?.running}<span class="small muted">{docker.containers.filter((c) => c.running).length} en cours sur {docker.containers.length}</span>{/if}
</div>
{#if docker && !docker.installed}
  <div class="card empty muted">Docker n'est pas installé.</div>
{:else if docker && !docker.running}
  <div class="card stopped">
    <Icon name="box" size={22} />
    <div class="grow">
      <div class="strong">{starting ? "Docker Desktop démarre…" : "Docker n'est pas lancé"}</div>
      <div class="small muted">
        {docker.error ?? (starting ? "Ça prend en général 20 à 60 secondes, la page se met à jour toute seule." : "Lance Docker Desktop pour voir et gérer tes conteneurs.")}
      </div>
    </div>
    {#if docker.desktop_path}
      <button class="btn primary" onclick={startDesktop} disabled={starting}>
        {#if starting}<span class="spin" style="display:flex"><Icon name="refresh" size={15} /></span>{:else}<Icon name="play" size={14} />{/if}
        Lancer Docker Desktop
      </button>
    {/if}
  </div>
{:else if docker}
  {#each groups as [project, list] (project)}
    <div class="card group">
      {#if project}
        {@const ids = list.map((c) => c.id)}
        <div class="ghead">
          <Icon name="box" size={15} />
          <span class="strong">{project}</span>
          <span class="small muted">compose · {list.filter((c) => c.running).length}/{list.length}</span>
          <span class="grow"></span>
          {#if list.some((c) => !c.running)}
            <button class="btn ghost small-btn" disabled={isBusy(`docker|${ids.join(",")}`)} onclick={() => dockerDo(ids, "start")}><Icon name="play" size={13} /> Tout démarrer</button>
          {/if}
          {#if list.some((c) => c.running)}
            <button class="btn ghost small-btn" disabled={isBusy(`docker|${ids.join(",")}`)} onclick={() => dockerDo(ids, "stop")}><Icon name="stop" size={12} /> Tout arrêter</button>
          {/if}
        </div>
      {/if}
      {#each list as c (c.id)}
        <div class="ctr" class:off={!c.running}>
          <span class="dot" class:live={c.running}></span>
          <div class="cinfo">
            <div class="cline1">
              <span class="strong cname">{c.name}</span>
              <span class="mono small cimg" title={c.image}>{c.image}</span>
            </div>
            <div class="small muted">{c.status}</div>
          </div>
          {#each c.ports as p}
            <button class="btn ghost port" title={`Ouvrir http://localhost:${p}`} onclick={() => api.openUrl(`http://localhost:${p}`)}>
              <Icon name="globe" size={12} /> {p}
            </button>
          {/each}
          <div class="cactions">
            {#if c.running}
              <button class="btn ghost icon" title="Terminal dans le conteneur" onclick={() => dockerDo([c.id], "terminal")}><Icon name="terminal" size={15} /></button>
              <button class="btn ghost icon" title="Journaux" onclick={() => showLogs(c)}><Icon name="log" size={15} /></button>
              <button class="btn ghost icon" title="Redémarrer" disabled={isBusy(`docker|${c.id}`)} onclick={() => dockerDo([c.id], "restart")}><Icon name="refresh" size={15} /></button>
              <button class="btn ghost icon" title="Arrêter" disabled={isBusy(`docker|${c.id}`)} onclick={() => dockerDo([c.id], "stop")}><Icon name="stop" size={14} /></button>
            {:else}
              <button class="btn ghost icon" title="Journaux" onclick={() => showLogs(c)}><Icon name="log" size={15} /></button>
              <button class="btn ghost icon" title="Démarrer" disabled={isBusy(`docker|${c.id}`)} onclick={() => dockerDo([c.id], "start")}><Icon name="play" size={14} /></button>
              <button class="btn ghost del" class:confirm={confirmRemove === c.id} title="Supprimer le conteneur" onclick={() => remove(c)}>
                {#if confirmRemove === c.id}Supprimer ?{:else}<Icon name="trash" size={14} />{/if}
              </button>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="card empty muted">Aucun conteneur.</div>
  {/each}
{:else}
  <div class="card skeleton"></div>
{/if}

{#if logs}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="overlay" onclick={() => (logs = null)}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="card logs" onclick={(e) => e.stopPropagation()}>
      <div class="lhead">
        <Icon name="log" size={16} />
        <span class="strong">Journaux · {logs.name}</span>
        <span class="small muted">300 dernières lignes</span>
        <span class="grow"></span>
        <button
          class="btn ghost small-btn"
          onclick={async () => {
            await copyText(logs!.text);
            logsCopied = true;
            setTimeout(() => (logsCopied = false), 900);
          }}
        >
          <Icon name={logsCopied ? "check" : "copy"} size={13} /> Copier
        </button>
        <button class="btn ghost icon" title="Fermer" onclick={() => (logs = null)}><Icon name="x" size={15} /></button>
      </div>
      <pre class="mono">{logs.text}</pre>
    </div>
  </div>
{/if}

<svelte:window onkeydown={(e) => e.key === "Escape" && (logs = null)} />

<style>
  .banner {
    margin-bottom: 12px;
  }
  .section-head {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 22px 0 10px;
  }
  .section-head:first-of-type {
    margin-top: 0;
  }
  h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  .strong {
    font-weight: 600;
  }
  .grow {
    flex: 1;
  }
  .small-btn {
    height: 28px;
    padding: 0 10px;
    gap: 6px;
    font-size: 12.5px;
  }
  .empty {
    padding: 20px;
    text-align: center;
  }
  .skeleton {
    height: 70px;
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.5;
    }
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--stroke-strong);
  }
  .on .dot,
  .dot.live {
    background: var(--ok);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 20%, transparent);
  }

  .distros {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(360px, 1fr));
    gap: 8px;
  }
  .distro {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px;
  }
  .dhead {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .dactions {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .stopped {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 16px 18px;
    color: var(--text);
  }
  .stopped > :global(svg) {
    color: var(--accent);
    flex: none;
  }
  .group {
    margin-bottom: 8px;
    overflow: hidden;
  }
  .ghead {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px 8px 16px;
    border-bottom: 1px solid var(--stroke);
    background: var(--card-2);
  }
  .ctr {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 50px;
    padding: 6px 8px 6px 16px;
    border-bottom: 1px solid var(--stroke);
  }
  .ctr:last-child {
    border-bottom: none;
  }
  .ctr:hover {
    background: var(--fill-hover);
  }
  .ctr.off .cinfo {
    opacity: 0.6;
  }
  .cinfo {
    flex: 1;
    min-width: 0;
  }
  .cline1 {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }
  .cname {
    white-space: nowrap;
  }
  .cimg {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    color: var(--text-3);
  }
  .port {
    height: 26px;
    padding: 0 8px;
    gap: 4px;
    font-size: 12px;
    color: var(--accent);
  }
  .cactions {
    display: flex;
    gap: 2px;
  }
  .del {
    height: 32px;
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

  .overlay {
    position: fixed;
    inset: 0;
    z-index: 10;
    display: grid;
    place-items: center;
    padding: 30px;
    background: rgba(0, 0, 0, 0.35);
    animation: enter 0.15s ease-out;
  }
  .logs {
    display: flex;
    flex-direction: column;
    width: min(900px, 100%);
    height: min(600px, 100%);
    background: var(--card);
    backdrop-filter: blur(30px);
    overflow: hidden;
  }
  .lhead {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 8px 16px;
    border-bottom: 1px solid var(--stroke);
  }
  .logs pre {
    flex: 1;
    margin: 0;
    padding: 12px 16px;
    overflow: auto;
    font-size: 11.5px;
    line-height: 1.5;
    user-select: text;
  }
</style>
