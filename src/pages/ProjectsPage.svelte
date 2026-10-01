<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import { api, type GitStatus, type Opener, type Project, type ScanProgress } from "../lib/api";
  import { store, saveSettings } from "../lib/settings.svelte";

  const DAY = 24 * 3600;

  let projects = $state<Project[]>([]);
  let status = $state<Record<string, GitStatus>>({});
  let openers = $state<Opener[]>([]);
  let loaded = $state(false);
  let scanning = $state(false);
  let progress = $state<ScanProgress | null>(null);
  let error = $state("");
  let query = $state("");
  let sort = $state<"recent" | "modified" | "name">("recent");
  let tag = $state<string | null>(null);
  let showRoots = $state(false);
  let flash = $state("");

  const s = $derived(store.s!);
  const roots = $derived(s.project_roots);
  const editors = $derived(openers.filter((o) => o.kind === "editor"));

  async function loadGit(list: Project[]) {
    const paths = list.filter((p) => p.git).map((p) => p.path);
    if (!paths.length) return;
    const res = await api.gitStatus(paths);
    status = Object.fromEntries(res.map((r) => [r.path, r]));
  }

  async function scan() {
    scanning = true;
    progress = null;
    try {
      await saveSettings(0); // Rust lit les dossiers de recherche dans les réglages
      projects = await api.scanProjects();
      error = "";
      loadGit(projects);
    } catch (e) {
      error = String(e);
    }
    scanning = false;
  }

  onMount(() => {
    const un = listen<ScanProgress>("projects-progress", (e) => {
      if (scanning) progress = e.payload;
    });
    (async () => {
      openers = await api.getOpeners();
      try {
        projects = await api.getProjects();
      } catch (e) {
        error = String(e);
      }
      loaded = true;
      // Première visite : pas encore de cache, on cherche tout de suite.
      if (!projects.length) await scan();
      else loadGit(projects);
    })();
    return () => un.then((f) => f());
  });

  const tagCounts = $derived.by(() => {
    const m = new Map<string, number>();
    for (const p of projects) for (const t of p.tags) m.set(t, (m.get(t) ?? 0) + 1);
    return [...m.entries()].sort((a, b) => b[1] - a[1]);
  });

  /** Correspondance souple : toutes les lettres de la recherche, dans l'ordre (« tbx » → « toolbox »). */
  function fuzzy(text: string, q: string): boolean {
    let i = 0;
    for (const c of text) if (c === q[i]) i++;
    return i === q.length;
  }

  const visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const fav = new Set(s.project_favorites);
    const opened = s.project_opened;
    let list = projects.filter((p) => {
      if (tag && !p.tags.includes(tag)) return false;
      if (!q) return true;
      const name = p.name.toLowerCase();
      return name.includes(q) || p.path.toLowerCase().includes(q) || p.tags.some((t) => t.toLowerCase() === q) || fuzzy(name, q);
    });
    const score = (p: Project) => (sort === "recent" ? Math.max(opened[p.path] ?? 0, 0) * 10 + p.modified : p.modified);
    list = [...list].sort((a, b) => {
      const f = Number(fav.has(b.path)) - Number(fav.has(a.path));
      if (f) return f;
      if (q) {
        // Le nom qui commence par la recherche passe devant.
        const st = Number(b.name.toLowerCase().startsWith(q)) - Number(a.name.toLowerCase().startsWith(q));
        if (st) return st;
      }
      if (sort === "name") return a.name.localeCompare(b.name, "fr", { sensitivity: "base" });
      return score(b) - score(a);
    });
    return list;
  });

  function toggleFav(p: Project) {
    const fav = s.project_favorites;
    s.project_favorites = fav.includes(p.path) ? fav.filter((x) => x !== p.path) : [...fav, p.path];
    saveSettings(0);
  }

  async function open(p: Project, opener: string) {
    try {
      await api.openProject(p.path, opener);
      // Le Rust a noté l'ouverture : on reflète localement pour le tri.
      s.project_opened = { ...s.project_opened, [p.path]: Math.floor(Date.now() / 1000) };
      flash = `${p.path}|${opener}`;
      setTimeout(() => (flash = ""), 900);
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  const openerName = (id: string) => openers.find((o) => o.id === id)?.name ?? id;

  function ago(secs: number): string {
    if (!secs) return "";
    const d = (Date.now() / 1000 - secs) / DAY;
    if (d < 1 / 24) return "à l'instant";
    if (d < 1) return `il y a ${Math.floor(d * 24)} h`;
    if (d < 2) return "hier";
    if (d < 31) return `il y a ${Math.floor(d)} j`;
    if (d < 365) return `il y a ${Math.floor(d / 30.4)} mois`;
    const y = Math.floor(d / 365);
    return `il y a ${y} an${y > 1 ? "s" : ""}`;
  }

  async function addRoot() {
    const path = await api.pickFolder();
    if (!path || roots.includes(path)) return;
    s.project_roots = [...roots, path];
    saveSettings(0);
  }

  function removeRoot(path: string) {
    s.project_roots = roots.filter((r) => r !== path);
    saveSettings(0);
  }

  function onsearchkey(e: KeyboardEvent) {
    // Entrée : ouvre le premier résultat dans son éditeur conseillé.
    if (e.key === "Enter" && visible[0]) open(visible[0], visible[0].editor);
    if (e.key === "Escape") query = "";
  }
</script>

<PageHeader title="Projets" subtitle="Tous tes projets, ouverts en un clic dans le bon éditeur.">
  {#snippet actions()}
    <button class="btn" onclick={scan} disabled={scanning}>
      <span class:spin={scanning} style="display:flex"><Icon name="refresh" size={16} /></span>
      {scanning ? "Recherche…" : "Rechercher"}
    </button>
  {/snippet}
</PageHeader>

{#if error}
  <div class="banner error">{error}</div>
{/if}

<div class="toolbar">
  <div class="searchbox">
    <Icon name="search" size={15} />
    <!-- svelte-ignore a11y_autofocus -->
    <input bind:value={query} onkeydown={onsearchkey} placeholder="Nom, chemin ou techno… (Entrée pour ouvrir)" spellcheck="false" autofocus />
  </div>
  <div class="seg-ctrl">
    <button class:active={sort === "recent"} onclick={() => (sort = "recent")}>Récents</button>
    <button class:active={sort === "modified"} onclick={() => (sort = "modified")}>Modifiés</button>
    <button class:active={sort === "name"} onclick={() => (sort = "name")}>Nom</button>
  </div>
</div>

<div class="meta">
  {#if tagCounts.length}
    <div class="tags">
      <button class="tagf" class:active={!tag} onclick={() => (tag = null)}>Tous <span>{projects.length}</span></button>
      {#each tagCounts as [t, n] (t)}
        <button class="tagf" class:active={tag === t} onclick={() => (tag = tag === t ? null : t)}>{t} <span>{n}</span></button>
      {/each}
    </div>
  {/if}
  <button class="roots-toggle small muted" onclick={() => (showRoots = !showRoots)}>
    <Icon name="folder" size={13} />
    {roots.length ? `${roots.length} dossier${roots.length > 1 ? "s" : ""} analysé${roots.length > 1 ? "s" : ""}` : "Tous les disques"}
  </button>
</div>

{#if showRoots}
  <div class="card roots">
    {#each roots as r (r)}
      <div class="root">
        <Icon name="folder" size={14} />
        <span class="mono small rpath" title={r}>{r}</span>
        <button class="btn ghost icon tiny" title="Retirer" onclick={() => removeRoot(r)}><Icon name="x" size={12} /></button>
      </div>
    {:else}
      <div class="small muted">Tous les disques sont analysés (sauf Windows, Program Files, AppData et les dossiers cachés).</div>
    {/each}
    <button class="btn add-root" onclick={addRoot}><Icon name="plus" size={14} /> {roots.length ? "Ajouter un dossier" : "Limiter à un dossier"}</button>
  </div>
{/if}

{#if scanning}
  <div class="card progress">
    <span class="spin" style="display:flex"><Icon name="refresh" size={18} /></span>
    <div>
      <div class="strong">{(progress?.dirs ?? 0).toLocaleString("fr-FR")} dossiers parcourus</div>
      <div class="small muted">{progress?.found ?? 0} projets trouvés pour l'instant</div>
    </div>
  </div>
{/if}

<div class="card list">
  {#each visible as p (p.path)}
    {@const st = status[p.path]}
    {@const fav = s.project_favorites.includes(p.path)}
    <div class="item">
      <button class="star" class:on={fav} title={fav ? "Retirer des favoris" : "Favori"} onclick={() => toggleFav(p)}>
        <Icon name="star" size={16} />
      </button>
      <div class="info">
        <div class="line1">
          <span class="pname">{p.name}</span>
          {#each p.tags.slice(0, 4) as t}<span class="badge">{t}</span>{/each}
          {#if p.git}
            <span class="git small">
              <Icon name="branch" size={12} />
              {st?.branch ?? p.branch ?? "?"}
              {#if st?.changes}<span class="dirty" title="Fichiers modifiés non commités">● {st.changes}</span>{/if}
              {#if st?.ahead}<span class="sync" title="Commits à pousser">↑{st.ahead}</span>{/if}
              {#if st?.behind}<span class="sync" title="Commits à récupérer">↓{st.behind}</span>{/if}
            </span>
          {/if}
        </div>
        <div class="line2 small">
          <span class="ppath mono" title={p.path}>{p.path}</span>
          <span class="when">{ago(p.modified)}</span>
        </div>
      </div>
      <div class="actions">
        <button class="btn main-open" class:done={flash === `${p.path}|${p.editor}`} onclick={() => open(p, p.editor)}>
          <Icon name={flash === `${p.path}|${p.editor}` ? "check" : "code"} size={14} />
          {openerName(p.editor)}
        </button>
        <button class="btn ghost icon" title="Terminal" onclick={() => open(p, "terminal")}><Icon name="terminal" size={16} /></button>
        <button class="btn ghost icon" title="Explorateur" onclick={() => open(p, "explorer")}><Icon name="folder" size={16} /></button>
        <select
          class="other"
          title="Ouvrir avec…"
          value=""
          onchange={(e) => {
            const id = e.currentTarget.value;
            e.currentTarget.value = "";
            if (id) open(p, id);
          }}
        >
          <option value="" disabled>…</option>
          {#each editors as o (o.id)}<option value={o.id}>{o.name}</option>{/each}
        </select>
      </div>
    </div>
  {:else}
    <div class="empty muted">
      {#if !loaded || scanning}
        Recherche des projets…
      {:else if query || tag}
        Aucun projet ne correspond.
      {:else}
        Aucun projet trouvé. Clique sur Rechercher.
      {/if}
    </div>
  {/each}
</div>

<style>
  .banner {
    margin-bottom: 12px;
  }
  .toolbar {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .searchbox {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    height: 36px;
    padding: 0 12px;
    border-radius: 8px;
    border: 1px solid var(--stroke);
    background: var(--input);
    color: var(--text-2);
  }
  .searchbox:focus-within {
    border-bottom: 2px solid var(--accent);
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

  .meta {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    margin: 10px 0;
  }
  .tags {
    flex: 1;
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .tagf {
    height: 26px;
    padding: 0 10px;
    border-radius: 999px;
    border: 1px solid var(--stroke);
    background: var(--card-2);
    font-size: 12px;
    cursor: pointer;
  }
  .tagf span {
    margin-left: 3px;
    color: var(--text-3);
  }
  .tagf:hover {
    background: var(--fill-hover);
  }
  .tagf.active {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .roots-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 8px;
    border: none;
    border-radius: 6px;
    background: transparent;
    white-space: nowrap;
    cursor: pointer;
  }
  .roots-toggle:hover {
    background: var(--fill-hover);
    color: var(--text);
  }
  .roots {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    margin-bottom: 10px;
    padding: 12px 16px;
  }
  .root {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    color: var(--text-2);
  }
  .rpath {
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
  .progress {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 10px;
    padding: 14px 18px;
  }
  .progress > .spin {
    color: var(--accent);
  }
  .strong {
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .list {
    overflow: hidden;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px 8px 8px;
    border-bottom: 1px solid var(--stroke);
  }
  .item:last-child {
    border-bottom: none;
  }
  .item:hover {
    background: var(--fill-hover);
  }
  .star {
    display: grid;
    place-items: center;
    flex: none;
    width: 30px;
    height: 30px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-3);
    opacity: 0.4;
    cursor: pointer;
  }
  .item:hover .star {
    opacity: 1;
  }
  .star.on {
    opacity: 1;
    color: #f5b301;
  }
  .star.on :global(svg) {
    fill: currentColor;
  }
  .info {
    flex: 1;
    min-width: 0;
  }
  .line1 {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .pname {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .line1 .badge {
    flex: none;
    height: 18px;
    padding: 0 7px;
    font-size: 10.5px;
    font-weight: 500;
  }
  .git {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex: none;
    margin-left: 4px;
    color: var(--text-2);
  }
  .dirty {
    color: var(--warn);
    font-weight: 600;
  }
  .sync {
    color: var(--accent);
    font-weight: 600;
  }
  .line2 {
    display: flex;
    gap: 10px;
    min-width: 0;
    color: var(--text-3);
  }
  .ppath {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 11.5px;
  }
  .when {
    flex: none;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex: none;
  }
  .main-open {
    height: 30px;
    min-width: 120px;
    margin-right: 4px;
    padding: 0 12px;
    gap: 7px;
    font-size: 12.5px;
    font-weight: 600;
  }
  .main-open.done {
    color: var(--ok);
  }
  .other {
    width: 30px;
    height: 30px;
    padding: 0;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-2);
    text-align: center;
    cursor: pointer;
    appearance: none;
  }
  .other:hover {
    background: var(--fill-hover);
  }
  .empty {
    padding: 26px;
    text-align: center;
  }
</style>
