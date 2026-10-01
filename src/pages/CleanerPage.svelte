<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import { api, fmtBytes, type CleanReport, type CleanScan, type FoundFolder, type SearchProgress } from "../lib/api";
  import { store, loadSettings, saveSettings } from "../lib/settings.svelte";
  import { cleanCache } from "../lib/cache.svelte";

  // Repris du cache : revenir sur la page affiche aussitôt les derniers résultats.
  let scan = $state<CleanScan | null>(cleanCache.scan);
  let scanning = $state(false);
  let cleaning = $state(false);
  let error = $state("");
  let report = $state<CleanReport | null>(null);
  let checked = $state<Record<string, boolean>>(cleanCache.checked);
  let confirming = $state(false);

  const icons: Record<string, string> = {
    temp_user: "clock",
    temp_windows: "clock",
    browsers: "globe",
    crash: "bolt",
    windows_update: "refresh",
    dev: "keyboard",
    recycle: "trash",
  };

  const usable = (id: string) => {
    const c = scan?.categories.find((x) => x.id === id);
    return !!c && c.available && (!c.needs_admin || scan!.is_admin);
  };

  async function analyse() {
    scanning = true;
    try {
      const first = !scan;
      scan = await api.scanCleanup();
      if (first) {
        for (const c of scan.categories) checked[c.id] = c.recommended;
      }
      error = "";
    } catch (e) {
      error = String(e);
    }
    scanning = false;
  }

  onMount(() => {
    analyse();
    const un = listen<SearchProgress>("folders-progress", (e) => {
      if (searching) progress = e.payload;
    });
    return () => un.then((f) => f());
  });

  const selected = $derived(scan?.categories.filter((c) => checked[c.id] && usable(c.id)) ?? []);
  const selectedSize = $derived(selected.reduce((n, c) => n + c.size, 0));
  const totalSize = $derived(scan?.categories.filter((c) => usable(c.id)).reduce((n, c) => n + c.size, 0) ?? 0);
  const emptiesBin = $derived(selected.some((c) => c.id === "recycle"));

  // ─── Dossiers recherchés par nom (node_modules…) ───

  const SUGGESTIONS = ["node_modules", "target", ".venv", "venv", "__pycache__", ".next", ".nuxt", ".gradle", ".angular", ".turbo"];
  const DAY = 24 * 3600;

  let newName = $state("");
  let found = $state<FoundFolder[] | null>(cleanCache.found);
  let foundAt = $state(cleanCache.foundAt);
  let searching = $state(false);
  let progress = $state<SearchProgress | null>(null);
  let deleting = $state(false);
  let picked = $state<Record<string, boolean>>(cleanCache.picked);

  $effect(() => {
    Object.assign(cleanCache, { scan, checked, found, foundAt, picked });
  });
  let confirmDelete = $state(false);

  const names = $derived(store.s?.clean_folder_names ?? []);
  const roots = $derived(store.s?.clean_roots ?? []);
  const pickedFolders = $derived(found?.filter((f) => picked[f.path]) ?? []);
  const pickedSize = $derived(pickedFolders.reduce((n, f) => n + f.size, 0));
  const foundSize = $derived(found?.reduce((n, f) => n + f.size, 0) ?? 0);

  /** Résultat par nom recherché : combien de dossiers et quelle taille. */
  const byName = $derived(
    names.map((name) => {
      const list = found?.filter((f) => f.name.toLowerCase() === name.toLowerCase()) ?? [];
      return { name, count: list.length, size: list.reduce((n, f) => n + f.size, 0) };
    }),
  );
  let nameFilter = $state<string | null>(null);
  const visible = $derived(found?.filter((f) => !nameFilter || f.name === nameFilter) ?? []);

  /** Coche ou décoche les dossiers affichés, sans toucher aux autres. */
  function pickVisible(test: (f: FoundFolder) => boolean) {
    const next = { ...picked };
    for (const f of visible) next[f.path] = test(f);
    picked = next;
  }

  function addName(name: string) {
    const n = name.trim().replace(/[\\/]/g, "");
    if (!n || !store.s || names.some((x) => x.toLowerCase() === n.toLowerCase())) return;
    store.s.clean_folder_names = [...names, n];
    newName = "";
    found = null;
    saveSettings(0);
  }

  function removeName(name: string) {
    if (!store.s) return;
    store.s.clean_folder_names = names.filter((x) => x !== name);
    found = null;
    saveSettings(0);
  }

  async function addRoot() {
    try {
      const path = await api.pickFolder();
      if (!path || !store.s || roots.includes(path)) return;
      store.s.clean_roots = [...roots, path];
      found = null;
      saveSettings(0);
    } catch (e) {
      error = String(e);
    }
  }

  function removeRoot(path: string) {
    if (!store.s) return;
    store.s.clean_roots = roots.filter((r) => r !== path);
    found = null;
    saveSettings(0);
  }

  async function search() {
    if (!names.length) {
      error = "Ajoute au moins un nom de dossier à rechercher (node_modules…).";
      return;
    }
    searching = true;
    progress = null;
    confirmDelete = false;
    try {
      await saveSettings(0); // Rust lit les noms et dossiers dans les réglages
      found = await api.findFolders();
      foundAt = Date.now();
      picked = {};
      if (nameFilter && !names.includes(nameFilter)) nameFilter = null;
      error = "";
    } catch (e) {
      error = String(e);
    }
    searching = false;
  }

  function pickInactive(days: number) {
    const limit = Date.now() / 1000 - days * DAY;
    pickVisible((f) => f.modified > 0 && f.modified < limit);
  }

  async function deletePicked() {
    if (!confirmDelete) {
      confirmDelete = true;
      return;
    }
    confirmDelete = false;
    deleting = true;
    try {
      const paths = pickedFolders.map((f) => f.path);
      report = await api.deleteFolders(paths);
      // Pas besoin de tout relancer : on retire de la liste ce qui a disparu.
      const gone = new Set(paths.filter((p) => !report!.kept.includes(p)));
      found = (found ?? []).filter((f) => !gone.has(f.path));
      picked = {};
      await loadSettings();
    } catch (e) {
      error = String(e);
    }
    deleting = false;
  }

  function ago(secs: number): string {
    if (!secs) return "date inconnue";
    const d = (Date.now() / 1000 - secs) / DAY;
    if (d < 1) return "aujourd'hui";
    if (d < 2) return "hier";
    if (d < 31) return `il y a ${Math.floor(d)} j`;
    if (d < 365) return `il y a ${Math.floor(d / 30.4)} mois`;
    const y = Math.floor(d / 365);
    return `il y a ${y} an${y > 1 ? "s" : ""}`;
  }

  /** Il y a combien de temps la recherche a été faite */
  function since(ms: number): string {
    const min = Math.floor((Date.now() - ms) / 60000);
    if (min < 1) return "à l'instant";
    if (min < 60) return `il y a ${min} min`;
    return `il y a ${Math.floor(min / 60)} h`;
  }

  /** « C:\Users\moi\Dev\projet » → « projet », avec le chemin complet en infobulle */
  const base = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;

  async function clean() {
    if (emptiesBin && !confirming) {
      confirming = true;
      return;
    }
    confirming = false;
    cleaning = true;
    report = null;
    try {
      report = await api.runCleanup(selected.map((c) => c.id));
      await Promise.all([analyse(), loadSettings()]);
    } catch (e) {
      error = String(e);
    }
    cleaning = false;
  }
</script>

<PageHeader title="Nettoyage" subtitle="Fichiers temporaires, caches et corbeille : récupère de la place.">
  {#snippet actions()}
    <button class="btn" onclick={analyse} disabled={scanning || cleaning}>
      <span class:spin={scanning} style="display:flex"><Icon name="refresh" size={16} /></span> Analyser
    </button>
  {/snippet}
</PageHeader>

{#if error}
  <div class="banner error">{error}</div>
{/if}

{#if report}
  <div class="banner ok">
    <Icon name="check" size={20} />
    <div class="grow">
      <div class="strong">{fmtBytes(report.freed)} libérés</div>
      <div class="small muted">
        {report.deleted.toLocaleString("fr-FR")} fichiers supprimés{#if report.skipped}
          · {report.skipped.toLocaleString("fr-FR")} en cours d'utilisation, laissés en place{/if}
      </div>
    </div>
    <button class="btn ghost icon" title="Fermer" onclick={() => (report = null)}><Icon name="x" size={14} /></button>
  </div>
{/if}

<div class="card hero">
  <div class="grow">
    <span class="slabel">Sélectionné</span>
    <div class="big">{scan ? fmtBytes(selectedSize) : "…"}</div>
    <span class="small muted">
      {#if scan}
        sur {fmtBytes(totalSize)} récupérables · {fmtBytes(store.s?.cleaned_total ?? 0)} libérés depuis l'installation
      {:else}
        Analyse en cours…
      {/if}
    </span>
  </div>
  {#if confirming}
    <div class="confirm">
      <span class="small">La corbeille sera vidée définitivement.</span>
      <button class="btn ghost" onclick={() => (confirming = false)}>Annuler</button>
      <button class="btn danger" onclick={clean}>Confirmer</button>
    </div>
  {:else}
    <button class="btn primary large" onclick={clean} disabled={!selected.length || cleaning || scanning}>
      {#if cleaning}
        <span class="spin" style="display:flex"><Icon name="refresh" size={16} /></span> Nettoyage…
      {:else}
        <Icon name="broom" size={16} /> Nettoyer
      {/if}
    </button>
  {/if}
</div>

{#if scan && !scan.is_admin}
  <p class="small muted note">
    <Icon name="shield" size={13} /> Les éléments de Windows demandent les droits admin (bouton « Relancer en admin » dans la page Démarrage).
  </p>
{/if}

<div class="card list">
  {#if scan}
    {#each scan.categories as c (c.id)}
      {@const ok = usable(c.id)}
      <label class="item" class:off={!ok}>
        <input type="checkbox" bind:checked={checked[c.id]} disabled={!ok} />
        <div class="kind"><Icon name={icons[c.id] ?? "folder"} size={16} /></div>
        <div class="info">
          <div class="strong">{c.name}</div>
          <div class="small muted desc">{c.description}</div>
        </div>
        <div class="size">
          {#if !c.available}
            <span class="small muted">Absent</span>
          {:else if c.needs_admin && !scan.is_admin}
            <span class="badge"><Icon name="shield" size={11} />&nbsp;Admin</span>
          {:else}
            <span class="strong">{fmtBytes(c.size)}</span>
            <span class="small muted">{c.files.toLocaleString("fr-FR")} {c.id === "recycle" ? "éléments" : "fichiers"}</span>
          {/if}
        </div>
      </label>
    {/each}
  {:else}
    {#each Array(6) as _}
      <div class="item"><div class="skeleton"></div></div>
    {/each}
  {/if}
</div>

<!-- Dossiers recherchés par nom -->
<div class="section-head">
  <div>
    <h2>Dossiers à rechercher</h2>
    <p class="small muted">Dépendances et fichiers de build des projets : ils se régénèrent avec <span class="mono">npm install</span>, <span class="mono">cargo build</span>…</p>
  </div>
  <button
    class="btn"
    onclick={search}
    disabled={searching}
  >
    <span class:spin={searching} style="display:flex"><Icon name="search" size={16} /></span>
    {searching ? (roots.length ? "Recherche…" : "Recherche sur tous les disques…") : "Rechercher"}
  </button>
</div>

<div class="card config">
  <div class="cfg-row">
    <span class="cfg-label small muted">Noms</span>
    <div class="chips">
      {#each names as n (n)}
        <span class="tag mono">{n}<button title="Retirer" onclick={() => removeName(n)}><Icon name="x" size={10} /></button></span>
      {/each}
      <form
        onsubmit={(e) => {
          e.preventDefault();
          addName(newName);
        }}
      >
        <input class="field small-field" bind:value={newName} placeholder="Ajouter un nom…" spellcheck="false" />
      </form>
    </div>
  </div>
  {#if SUGGESTIONS.some((x) => !names.includes(x))}
    <div class="cfg-row">
      <span class="cfg-label small muted">Suggestions</span>
      <div class="chips">
        {#each SUGGESTIONS.filter((x) => !names.includes(x)) as sug}
          <button class="chip mono" onclick={() => addName(sug)}><Icon name="plus" size={12} /> {sug}</button>
        {/each}
      </div>
    </div>
  {/if}
  <div class="cfg-row">
    <span class="cfg-label small muted">Chercher dans</span>
    <div class="roots">
      {#each roots as r (r)}
        <div class="root">
          <Icon name="folder" size={15} />
          <span class="mono small path" title={r}>{r}</span>
          <button class="btn ghost icon tiny" title="Retirer" onclick={() => removeRoot(r)}><Icon name="x" size={12} /></button>
        </div>
      {/each}
      {#if !roots.length}
        <div class="root">
          <Icon name="disk" size={15} />
          <span class="path strong">Tous les disques</span>
        </div>
      {/if}
      <button class="btn add-root" onclick={addRoot}>
        <Icon name="plus" size={14} />
        {roots.length ? "Ajouter un dossier" : "Limiter à un dossier"}
      </button>
      {#if !roots.length}
        <span class="small muted">Plus rapide si tu choisis le dossier qui contient tes projets (ton dossier Dev, par exemple).</span>
      {/if}
    </div>
  </div>
  <p class="small muted safe">
    <Icon name="shield" size={12} /> Jamais parcourus : Windows, Program Files, AppData et les outils installés (.vscode, .cargo, scoop…), pour ne rien casser.
  </p>
</div>

{#if searching}
  <div class="card progress">
    <span class="spin" style="display:flex"><Icon name="refresh" size={18} /></span>
    <div class="grow">
      {#if progress?.phase === "measure"}
        <div class="strong">Calcul des tailles… {progress.measured} / {progress.found}</div>
        <div class="pbar"><div style:width={`${progress.found ? (progress.measured / progress.found) * 100 : 0}%`}></div></div>
      {:else}
        <div class="strong">
          {roots.length ? "Parcours des dossiers…" : "Parcours des disques…"}
          {(progress?.dirs ?? 0).toLocaleString("fr-FR")} dossiers
        </div>
        <div class="small muted">
          {progress?.found ?? 0} trouvé{(progress?.found ?? 0) > 1 ? "s" : ""} pour l'instant
        </div>
      {/if}
    </div>
  </div>
{:else if found}
  <div class="card found">
    <div class="found-head">
      <div class="grow">
        <span class="strong">{found.length} dossier{found.length > 1 ? "s" : ""}</span>
        <span class="small muted">· {fmtBytes(foundSize)} au total · recherche faite {since(foundAt)}</span>
      </div>
      {#if found.length}
        <button class="btn ghost" onclick={() => pickVisible(() => true)}>Tout</button>
        <button class="btn ghost" onclick={() => pickInactive(30)} title="Aucun fichier modifié depuis 30 jours">Inactifs depuis 30 j</button>
        <button class="btn ghost" onclick={() => pickVisible(() => false)}>Aucun</button>
        {#if confirmDelete}
          <button class="btn ghost" onclick={() => (confirmDelete = false)}>Annuler</button>
          <button class="btn danger" onclick={deletePicked}>Supprimer définitivement</button>
        {:else}
          <button class="btn primary" onclick={deletePicked} disabled={!pickedFolders.length || deleting}>
            {#if deleting}
              <span class="spin" style="display:flex"><Icon name="refresh" size={15} /></span> Suppression…
            {:else}
              <Icon name="trash" size={15} /> Supprimer {pickedFolders.length ? fmtBytes(pickedSize) : ""}
            {/if}
          </button>
        {/if}
      {/if}
    </div>
    {#if byName.length > 1 && found.length}
      <div class="name-filter">
        <button class="nf" class:active={!nameFilter} onclick={() => (nameFilter = null)}>
          Tous <span class="nf-sub">{found.length} · {fmtBytes(foundSize)}</span>
        </button>
        {#each byName as b (b.name)}
          <button class="nf" class:active={nameFilter === b.name} disabled={!b.count} onclick={() => (nameFilter = b.name)}>
            <span class="mono">{b.name}</span>
            <span class="nf-sub">{b.count} · {fmtBytes(b.size)}</span>
          </button>
        {/each}
      </div>
    {/if}
    {#each visible as f (f.path)}
      <label class="item folder">
        <input type="checkbox" bind:checked={picked[f.path]} />
        <div class="info">
          <div class="line1">
            <span class="strong">{base(f.parent)}</span>
            <span class="badge mono">{f.name}</span>
          </div>
          <div class="small muted desc mono" title={f.path}>{f.parent}</div>
        </div>
        <div class="size">
          <span class="strong">{fmtBytes(f.size)}</span>
          <span class="small muted" title="Dernière modification d'un fichier à l'intérieur">modifié {ago(f.modified)}</span>
        </div>
      </label>
    {:else}
      <div class="empty small muted">Aucun dossier « {names.join(" », « ")} » trouvé.</div>
    {/each}
  </div>
{/if}

<style>
  .section-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    margin: 30px 0 12px;
  }
  h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  .section-head p {
    margin: 2px 0 0;
  }
  .config {
    padding: 6px 18px 12px;
  }
  .cfg-row {
    display: flex;
    align-items: flex-start;
    gap: 14px;
    padding: 10px 0;
    border-bottom: 1px solid var(--stroke);
  }
  .cfg-label {
    flex: none;
    width: 96px;
    padding-top: 5px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .tag {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 26px;
    padding: 0 4px 0 10px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 12.5px;
    font-weight: 600;
  }
  .tag button {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  .tag button:hover {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .small-field {
    height: 28px;
    width: 160px;
    font-size: 12.5px;
  }
  .chip.mono {
    font-family: var(--mono);
    font-size: 12px;
  }
  .roots {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    min-width: 0;
  }
  .root {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    color: var(--text-2);
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    color: var(--text);
  }
  .tiny {
    width: 26px;
    height: 26px;
  }
  .add-root {
    height: 28px;
    font-size: 12.5px;
  }
  .safe {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 10px 0 0;
  }
  .name-filter {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--stroke);
  }
  .nf {
    display: inline-flex;
    align-items: baseline;
    gap: 8px;
    height: 30px;
    padding: 0 12px;
    border-radius: 8px;
    border: 1px solid var(--stroke);
    background: var(--card-2);
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
    line-height: 28px;
  }
  .nf:hover {
    background: var(--fill-hover);
  }
  .nf.active {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .nf:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .nf-sub {
    font-weight: 400;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-top: 8px;
    padding: 16px 18px;
    color: var(--text);
  }
  .progress > .spin {
    color: var(--accent);
  }
  .progress .strong {
    font-variant-numeric: tabular-nums;
  }
  .pbar {
    height: 4px;
    margin-top: 8px;
    border-radius: 999px;
    background: var(--stroke-strong);
    overflow: hidden;
  }
  .pbar div {
    height: 100%;
    border-radius: 999px;
    background: var(--accent);
    transition: width 0.2s;
  }
  .found {
    margin-top: 8px;
    overflow: hidden;
  }
  .found-head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 12px 10px 16px;
    border-bottom: 1px solid var(--stroke);
  }
  .line1 {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .folder {
    min-height: 56px;
  }
  .empty {
    padding: 22px;
    text-align: center;
  }

  .banner {
    margin-bottom: 12px;
  }
  .banner.ok {
    border-color: color-mix(in srgb, var(--ok) 40%, transparent);
    background: color-mix(in srgb, var(--ok) 10%, var(--card));
  }
  .banner.ok > :global(svg) {
    color: var(--ok);
    flex: none;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .strong {
    font-weight: 600;
  }
  .hero {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 18px 20px;
  }
  .slabel {
    font-size: 12px;
    color: var(--text-2);
  }
  .big {
    font-family: var(--font-display);
    font-size: 30px;
    font-weight: 600;
    line-height: 1.25;
    font-variant-numeric: tabular-nums;
  }
  .btn.large {
    height: 38px;
    padding: 0 20px;
  }
  .btn.danger {
    background: var(--bad);
    border-color: transparent;
    color: #fff;
    font-weight: 600;
  }
  .confirm {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .note {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 10px 2px 0;
  }
  .list {
    margin-top: 12px;
    overflow: hidden;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 14px;
    min-height: 62px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--stroke);
    cursor: pointer;
    transition: background 0.12s;
  }
  .item:last-child {
    border-bottom: none;
  }
  .item:hover {
    background: var(--fill-hover);
  }
  .item.off {
    cursor: default;
  }
  .item.off .info,
  .item.off .kind {
    opacity: 0.5;
  }
  input[type="checkbox"] {
    width: 18px;
    height: 18px;
    margin: 0;
    accent-color: var(--accent);
    cursor: inherit;
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
  .desc {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .size {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    min-width: 90px;
    font-variant-numeric: tabular-nums;
  }
  .skeleton {
    width: 100%;
    height: 30px;
    border-radius: 6px;
    background: var(--fill-hover);
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.5;
    }
  }
</style>
