<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import { api, type AppUpdate, type UpdatesState } from "../lib/api";
  import { store, saveSettings } from "../lib/settings.svelte";

  /** `reinstall` : winget demande de désinstaller l'ancienne version avant d'installer la nouvelle */
  type Status = { state: "wait" | "run" | "fail"; message?: string; reinstall?: boolean };
  let confirmReinstall = $state<string | null>(null);
  let confirmTimer: ReturnType<typeof setTimeout> | undefined;

  let found = $state<UpdatesState | null>(null);
  let loading = $state(false);
  let error = $state("");
  let status = $state<Record<string, Status>>({});
  /** Une file de mises à jour est en cours */
  let running = $state(false);
  let stopping = $state(false);
  let done = $state(0);
  let showIgnored = $state(false);
  let now = $state(Date.now());

  const s = $derived(store.s!);
  const apps = $derived((found?.apps ?? []).filter((a) => !s.updates_ignored.includes(a.id)));
  const ignored = $derived((found?.apps ?? []).filter((a) => s.updates_ignored.includes(a.id)));

  async function load(refresh: boolean) {
    loading = true;
    error = "";
    try {
      found = await api.getAppUpdates(refresh);
      if (refresh) {
        status = {};
        done = 0;
      }
    } catch (e) {
      error = String(e);
    }
    loading = false;
  }

  onMount(() => {
    load(false);
    // Une application mise à jour disparaît de la liste ; la recherche en arrière-plan la rafraîchit.
    const un = listen<UpdatesState>("app-updates", (e) => (found = e.payload));
    const t = setInterval(() => (now = Date.now()), 30000);
    return () => {
      un.then((f) => f());
      clearInterval(t);
    };
  });

  /** Met à jour les applications une par une : winget n'en installe qu'une à la fois. */
  async function run(list: AppUpdate[], reinstall = false) {
    running = true;
    stopping = false;
    for (const a of list) status[a.id] = { state: "wait" };
    for (const a of list) {
      if (stopping) break;
      status[a.id] = { state: "run" };
      try {
        await api.upgradeApp(a.id, reinstall);
        delete status[a.id];
        done++;
      } catch (e) {
        status[a.id] =
          String(e) === "reinstall"
            ? {
                state: "fail",
                reinstall: true,
                message: "La nouvelle version s'installe autrement que celle en place : il faut désinstaller l'ancienne, puis installer la nouvelle.",
              }
            : { state: "fail", message: String(e) };
      }
    }
    // Arrêt demandé : celles qui attendaient encore retrouvent leur bouton.
    for (const a of list) if (status[a.id]?.state === "wait") delete status[a.id];
    running = false;
    stopping = false;
  }

  /** Désinstaller puis réinstaller : deux clics, les réglages de l'application peuvent y passer. */
  function reinstall(a: AppUpdate) {
    if (confirmReinstall !== a.id) {
      confirmReinstall = a.id;
      clearTimeout(confirmTimer);
      confirmTimer = setTimeout(() => (confirmReinstall = null), 4000);
      return;
    }
    confirmReinstall = null;
    run([a], true);
  }

  function setIgnored(id: string, on: boolean) {
    const cur = s.updates_ignored.filter((x) => x !== id);
    s.updates_ignored = on ? [...cur, id] : cur;
    saveSettings(0);
  }

  const ago = $derived.by(() => {
    if (!found?.checked_at) return "";
    const min = Math.max(0, Math.floor((now / 1000 - found.checked_at) / 60));
    if (min < 1) return "à l'instant";
    if (min < 60) return `il y a ${min} min`;
    const h = Math.floor(min / 60);
    return h < 24 ? `il y a ${h} h` : `il y a ${Math.floor(h / 24)} j`;
  });
  const plural = (n: number, one: string, many: string) => `${n} ${n > 1 ? many : one}`;
</script>

<PageHeader title="Mises à jour" subtitle="Les applications installées qui ont une nouvelle version.">
  {#snippet actions()}
    <button class="btn" onclick={() => load(true)} disabled={loading || running}>
      <span class:spin={loading} style="display:flex"><Icon name="refresh" size={16} /></span> Rechercher
    </button>
    {#if running}
      <button class="btn" onclick={() => (stopping = true)} disabled={stopping}>
        <Icon name="stop" size={14} />
        {stopping ? "Arrêt après celle en cours…" : "Arrêter"}
      </button>
    {:else}
      <button class="btn primary" onclick={() => run(apps)} disabled={!apps.length || loading}>
        <Icon name="download" size={16} /> Tout mettre à jour
      </button>
    {/if}
  {/snippet}
</PageHeader>

{#if error}
  <div class="banner error">{error}</div>
{:else if found?.error}
  <div class="banner error">La recherche a échoué : {found.error}</div>
{:else if found && !found.installed}
  <div class="banner error">
    winget est introuvable. Il est fourni avec Windows 11 : installe « Programme d'installation d'application » depuis le Microsoft Store.
  </div>
{/if}

{#if found?.installed}
  <div class="summary small">
    <span>
      {#if loading}
        Recherche en cours…
      {:else if apps.length}
        <b>{plural(apps.length, "mise à jour disponible", "mises à jour disponibles")}</b>
      {:else}
        <b>Tout est à jour.</b>
      {/if}
      {#if done}<span class="ok"> · {plural(done, "application mise à jour", "applications mises à jour")}</span>{/if}
    </span>
    <span class="muted">{ago ? `Dernière recherche ${ago}` : ""}</span>
  </div>
{:else if !found && loading}
  <div class="card empty muted"><span class="spin" style="display:flex"><Icon name="refresh" size={18} /></span> Recherche des mises à jour, quelques secondes…</div>
{/if}

{#if apps.length}
  <div class="card list">
    {#each apps as a (a.id)}
      {@const st = status[a.id]}
      <div class="app" class:fail={st?.state === "fail"}>
        <div class="grow">
          <div class="name" title={a.name}>{a.name}</div>
          <div class="small muted mono id" title={a.id}>{a.id}</div>
          {#if st?.state === "fail"}<div class="small err">{st.message}</div>{/if}
        </div>
        <div class="versions small">
          <span class="muted">{a.version === "Unknown" ? "inconnue" : a.version}</span>
          <span class="arrow">→</span>
          <b>{a.available}</b>
        </div>
        <div class="acts">
          {#if st?.state === "run"}
            <span class="state"><span class="spin" style="display:flex"><Icon name="refresh" size={14} /></span> Installation…</span>
          {:else if st?.state === "wait"}
            <span class="state muted">En attente</span>
          {:else}
            <button class="btn ghost quiet" disabled={running} title="Ne plus proposer cette application" onclick={() => setIgnored(a.id, true)}>
              Ignorer
            </button>
            {#if st?.reinstall}
              <button
                class="btn"
                class:danger={confirmReinstall === a.id}
                disabled={running}
                title="Désinstalle la version actuelle, puis installe la nouvelle. Certaines applications y perdent leurs réglages."
                onclick={() => reinstall(a)}
              >
                {confirmReinstall === a.id ? "Désinstaller et réinstaller ?" : "Réinstaller"}
              </button>
            {:else}
              <button class="btn" disabled={running} onclick={() => run([a])}>
                {st?.state === "fail" ? "Réessayer" : "Mettre à jour"}
              </button>
            {/if}
          {/if}
        </div>
      </div>
    {/each}
  </div>
  <p class="small muted note">
    Une application ouverte est fermée pendant sa mise à jour, et Windows peut demander une autorisation pour certaines.
  </p>
{:else if found?.installed && !loading && !found.error}
  <div class="card empty muted"><Icon name="check" size={18} /> Aucune mise à jour à installer.</div>
{/if}

{#if ignored.length}
  <button class="btn ghost toggle small" onclick={() => (showIgnored = !showIgnored)}>
    <Icon name={showIgnored ? "up_small" : "down_small"} size={14} />
    {plural(ignored.length, "application ignorée", "applications ignorées")}
  </button>
  {#if showIgnored}
    <div class="card list">
      {#each ignored as a (a.id)}
        <div class="app off">
          <div class="grow">
            <div class="name" title={a.name}>{a.name}</div>
            <div class="small muted mono id">{a.id}</div>
          </div>
          <div class="versions small">
            <span class="muted">{a.version === "Unknown" ? "inconnue" : a.version}</span>
            <span class="arrow">→</span>
            <span>{a.available}</span>
          </div>
          <div class="acts">
            <button class="btn ghost" onclick={() => setIgnored(a.id, false)}>Ne plus ignorer</button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
{/if}

<style>
  .banner {
    margin-bottom: 12px;
  }
  .summary {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 10px;
  }
  .ok {
    color: var(--ok);
  }
  .list {
    overflow: hidden;
  }
  .app {
    display: flex;
    align-items: center;
    gap: 16px;
    min-height: 56px;
    padding: 8px 10px 8px 18px;
    border-bottom: 1px solid var(--stroke);
  }
  .app:last-child {
    border-bottom: none;
  }
  .app:hover {
    background: var(--fill-hover);
  }
  .app.off .name {
    color: var(--text-2);
    font-weight: 400;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .name {
    font-weight: 600;
  }
  .name,
  .id {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .id {
    font-size: 11.5px;
  }
  .err {
    margin-top: 2px;
    color: var(--bad);
  }
  .versions {
    display: flex;
    align-items: baseline;
    gap: 6px;
    flex: none;
    max-width: 260px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .arrow {
    color: var(--text-3);
  }
  .acts {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 4px;
    flex: none;
    min-width: 196px;
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding-right: 10px;
    font-size: 12.5px;
  }
  .danger {
    border-color: color-mix(in srgb, var(--bad) 45%, transparent);
    background: color-mix(in srgb, var(--bad) 12%, transparent);
    color: var(--bad);
    font-weight: 600;
  }
  /* Après un échec, « Ignorer » reste visible : c'est souvent la bonne réponse. */
  .app.fail .quiet {
    opacity: 1;
  }
  .quiet {
    color: var(--text-2);
    opacity: 0;
    transition: opacity 0.12s;
  }
  .app:hover .quiet {
    opacity: 1;
  }
  .note {
    margin: 10px 2px 0;
  }
  .empty {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 18px;
  }
  .toggle {
    margin: 18px 0 8px;
    padding: 0 10px 0 6px;
    gap: 4px;
    color: var(--text-2);
  }
</style>
