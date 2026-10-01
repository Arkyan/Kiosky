<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, copyText, type ConvResult, type ShellOutput } from "./api";
  import Icon from "./Icon.svelte";

  let { palette = false }: { palette?: boolean } = $props();

  let query = $state("");
  let results = $state<ConvResult[]>([]);
  let selected = $state(0);
  let copied = $state(-1);
  let loading = $state(false);
  let input = $state<HTMLInputElement>();
  let list = $state<HTMLDivElement>();

  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  /** Indice du résultat qui attend une seconde validation (redémarrer, éteindre…) */
  let confirming = $state(-1);
  /** Palette vide : on affiche les éléments les plus utilisés */
  let home = $state(false);
  /** Commande « > » : en cours, puis sa sortie (affichée à la place des résultats) */
  let running = $state("");
  let shellOut = $state<(ShellOutput & { cmd: string }) | null>(null);
  let outCopied = $state(false);

  async function runShell(cmd: string) {
    running = cmd;
    shellOut = null;
    try {
      shellOut = { ...(await api.runShell(cmd)), cmd };
    } catch (e) {
      shellOut = { output: String(e), code: -1, ms: 0, timed_out: false, cmd };
    }
    running = "";
  }

  function closeShell() {
    shellOut = null;
    running = "";
    input?.focus();
  }

  /** Vraies icônes (applications, éditeurs, dossiers), gardées pour toute la session */
  let icons = $state<Record<string, string>>({});

  async function loadIcons(list: ConvResult[]) {
    const wanted = list.map((r) => r.action).filter((a) => /^(app|run|project|openwith):/.test(a) && !(a in icons));
    if (!wanted.length) return;
    try {
      const got = await api.getIcons(wanted);
      icons = { ...icons, ...got };
    } catch {
      // Pas d'icône : on garde le pictogramme par type.
    }
  }

  async function loadHome() {
    if (!palette) return;
    const id = ++seq;
    const r = await api.paletteHome();
    if (id === seq && !query.trim()) {
      results = r;
      selected = 0;
      home = r.length > 0;
      loadIcons(r);
    }
  }

  export function setQuery(q: string) {
    query = q;
    run(0);
    input?.focus();
  }

  function run(delay = 90) {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      const id = ++seq;
      const q = query.trim();
      confirming = -1;
      if (!q) {
        results = [];
        loading = false;
        home = false;
        loadHome();
        return;
      }
      loading = true;
      try {
        const r = await api.convert(q);
        if (id === seq) {
          results = r;
          selected = 0;
          home = false;
          loadIcons(r);
        }
      } finally {
        if (id === seq) loading = false;
      }
    }, delay);
  }

  const verb = (r: ConvResult) => r.action.split(":")[0];

  /** Icône du bouton de droite : l'action du résultat, ou la copie. */
  function actionIcon(r: ConvResult): string {
    if (!r.action) return "copy";
    return { kill: "stop", open: "globe", web: "globe", system: "power", shell: "bolt", shellterm: "terminal" }[verb(r)] ?? "bolt";
  }

  /** Icône de gauche pour les résultats de recherche (applications, projets…) */
  function kindIcon(r: ConvResult): string | null {
    const icons: Record<string, string> = {
      app: "sparkle",
      project: "code",
      openwith: "folder",
      uri: "settings",
      run: "terminal",
      system: "power",
      page: "calc",
      pick: "pipette",
      web: "globe",
      shell: "terminal",
      shellterm: "terminal",
    };
    return icons[verb(r)] ?? null;
  }

  /** Actions irréversibles : Entrée deux fois. */
  const needsConfirm = (r: ConvResult) => ["system:restart", "system:shutdown", "system:logoff"].includes(r.action);

  async function copy(i: number) {
    const r = results[i];
    if (!r || r.error || (!r.copy && !r.action)) return;
    if (r.action) {
      if (needsConfirm(r) && confirming !== i) {
        confirming = i;
        return;
      }
      confirming = -1;
      // « >ipconfig » : la sortie s'affiche dans la palette, qui reste ouverte.
      if (verb(r) === "shell") {
        runShell(r.action.slice("shell:".length));
        return;
      }
      // Applications, projets, « kill 3000 »… : Entrée lance l'action au lieu de copier.
      try {
        await api.runAction(r.action);
      } catch (e) {
        results[i] = { ...r, error: true, value: String(e), action: "" };
        return;
      }
    } else {
      await copyText(r.copy);
    }
    copied = i;
    setTimeout(() => (copied = -1), 900);
    if (palette) setTimeout(() => api.hidePalette(), 220);
  }

  async function onkeydown(e: KeyboardEvent) {
    if (shellOut || running) {
      // Panneau de sortie : Échap revient aux résultats, Entrée relance.
      if (e.key === "Escape") {
        e.preventDefault();
        closeShell();
      } else if (e.key === "Enter" && shellOut) {
        e.preventDefault();
        runShell(shellOut.cmd);
      }
      return;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!results.length) return;
      const d = e.key === "ArrowDown" ? 1 : -1;
      selected = (selected + d + results.length) % results.length;
      confirming = -1;
      await tick();
      list?.querySelector(".sel")?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter") {
      e.preventDefault();
      copy(selected);
    } else if (e.key === "Escape") {
      e.preventDefault();
      if (palette) api.hidePalette();
      else {
        query = "";
        results = [];
      }
    }
  }

  onMount(() => {
    input?.focus();
    if (!palette) return;
    // Chaque ouverture de la palette repart d'un champ vide.
    const un = listen("palette-open", async () => {
      query = "";
      results = [];
      confirming = -1;
      await tick();
      input?.focus();
      loadHome();
    });
    loadHome();
    return () => {
      un.then((f) => f());
    };
  });
</script>

<div class="box" class:palette>
  <div class="search">
    <span class="icon" class:busy={loading}><Icon name={loading ? "refresh" : "search"} size={palette ? 20 : 18} /></span>
    <input
      bind:this={input}
      bind:value={query}
      oninput={() => {
        if (shellOut) closeShell();
        run();
      }}
      {onkeydown}
      placeholder={palette ? "Application, projet, dossier, paramètre… ou 10 km en miles, 2^10" : "10 km en miles · 50 eur usd · 14h tokyo · 2^10 · kill 3000"}
      spellcheck="false"
      autocomplete="off"
    />
    {#if query}
      <button class="btn ghost icon clear" title="Effacer" onclick={() => setQuery("")}><Icon name="x" size={14} /></button>
    {/if}
  </div>

  {#if running || shellOut}
    <div class="shell">
      <div class="shell-head">
        <span class="mono cmd">&gt; {shellOut?.cmd ?? running}</span>
        {#if running}
          <span class="shell-meta"><span class="spin" style="display:flex"><Icon name="refresh" size={13} /></span> En cours…</span>
        {:else if shellOut}
          <span class="shell-meta" class:bad={shellOut.code !== 0}>
            {shellOut.timed_out ? "arrêtée après 20 s" : shellOut.code === 0 ? "terminée" : `code ${shellOut.code}`}
            · {(shellOut.ms / 1000).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} s
          </span>
          <button
            class="btn ghost mini-btn"
            onclick={async () => {
              await copyText(shellOut!.output);
              outCopied = true;
              setTimeout(() => (outCopied = false), 900);
            }}
          >
            <Icon name={outCopied ? "check" : "copy"} size={13} /> Copier
          </button>
          <button class="btn ghost mini-btn" onclick={() => runShell(shellOut!.cmd)}><Icon name="refresh" size={13} /> Relancer</button>
          <button
            class="btn ghost mini-btn"
            onclick={() => {
              api.runAction(`shellterm:${shellOut!.cmd}`);
              if (palette) api.hidePalette();
            }}
          >
            <Icon name="terminal" size={13} /> Terminal
          </button>
        {/if}
      </div>
      <pre class="mono">{shellOut ? shellOut.output || "(aucune sortie)" : ""}</pre>
      <div class="shell-foot">Échap : retour aux résultats · Entrée : relancer</div>
    </div>
  {:else}
  <div class="results" bind:this={list}>
    {#if home}<div class="section">Les plus utilisés</div>{/if}
    {#each results as r, i (i)}
      {@const kind = kindIcon(r)}
      <button
        class="result"
        class:sel={i === selected}
        class:error={r.error}
        class:launch={!!kind}
        class:confirm={confirming === i}
        onmouseenter={() => (selected = i)}
        onclick={() => copy(i)}
      >
        {#if kind}
          {#if icons[r.action]}
            <img class="kind real" src={icons[r.action]} alt="" />
          {:else}
            <span class="kind"><Icon name={kind} size={17} /></span>
          {/if}
        {/if}
        <div class="text">
          {#if kind}
            <span class="value">{r.value}</span>
            <span class="hint">
              {#if confirming === i}
                Entrée encore une fois pour confirmer
              {:else}
                {r.title}{r.hint ? ` · ${r.hint}` : ""}
              {/if}
            </span>
          {:else}
            <span class="title">{r.title}</span>
            <span class="value" class:mono={r.title.startsWith("JSON") || r.title.startsWith("Base64")}>{r.value}</span>
            {#if r.hint}<span class="hint">{r.hint}</span>{/if}
          {/if}
        </div>
        {#if !r.error && (r.copy || r.action)}
          <span class="action" class:done={copied === i} class:act={!!r.action}>
            <Icon name={copied === i ? "check" : actionIcon(r)} size={15} />
          </span>
        {/if}
      </button>
    {:else}
      {#if query.trim() && !loading}
        <div class="empty">Rien trouvé. Essaie un nom d'application, de projet, « wifi », « 72 f en c » ou « port 3000 ».</div>
      {/if}
    {/each}
  </div>
  {/if}
</div>

<style>
  .box {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 46px;
    padding: 0 8px 0 14px;
    border-radius: 10px;
    border: 1px solid var(--stroke);
    border-bottom: 2px solid var(--accent);
    background: var(--input);
  }
  .palette .search {
    height: 58px;
    padding: 0 12px 0 18px;
    border: none;
    border-bottom: 1px solid var(--stroke);
    border-radius: 0;
    background: transparent;
  }
  .icon {
    display: flex;
    color: var(--text-2);
  }
  .icon.busy :global(svg) {
    animation: spin 0.9s linear infinite;
  }
  input {
    flex: 1;
    min-width: 0;
    border: none;
    outline: none;
    background: transparent;
    font-size: 15px;
  }
  .palette input {
    font-size: 18px;
    font-family: var(--font-display);
  }
  input::placeholder {
    color: var(--text-3);
  }
  .clear {
    width: 28px;
    height: 28px;
    color: var(--text-2);
  }

  .results {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: 10px;
    overflow-y: auto;
  }
  .palette .results {
    flex: 1;
    margin: 0;
    padding: 8px;
  }

  .result {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 10px 14px;
    border: none;
    border-radius: 8px;
    background: transparent;
    text-align: left;
    cursor: pointer;
    animation: enter 0.18s ease-out both;
  }
  .shell {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    margin: 0 8px 8px;
    border: 1px solid var(--stroke);
    border-radius: 10px;
    background: color-mix(in srgb, var(--input) 70%, transparent);
    overflow: hidden;
  }
  .shell-head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px 6px 12px;
    border-bottom: 1px solid var(--stroke);
  }
  .cmd {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 12.5px;
    font-weight: 600;
  }
  .shell-meta {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    color: var(--ok);
    white-space: nowrap;
  }
  .shell-meta.bad {
    color: var(--bad);
  }
  .mini-btn {
    height: 26px;
    padding: 0 8px;
    gap: 5px;
    font-size: 12px;
  }
  .shell pre {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 10px 12px;
    overflow: auto;
    font-size: 12px;
    line-height: 1.45;
    white-space: pre;
    user-select: text;
  }
  /* Dans la page Convertisseur (pas de hauteur fixe) : on borne la sortie. */
  .box:not(.palette) .shell {
    margin: 10px 0 0;
  }
  .box:not(.palette) .shell pre {
    max-height: 380px;
  }
  .shell-foot {
    padding: 4px 12px 6px;
    font-size: 11px;
    color: var(--text-3);
  }
  .section {
    padding: 6px 14px 4px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-3);
  }
  .result.launch {
    padding-top: 7px;
    padding-bottom: 7px;
  }
  .kind {
    display: grid;
    place-items: center;
    flex: none;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .kind.real {
    padding: 2px;
    background: none;
    object-fit: contain;
  }
  .launch .value {
    font-size: 14.5px;
  }
  .launch .hint {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .result.confirm .hint {
    color: var(--bad);
    font-weight: 600;
  }
  .result.confirm .kind {
    background: color-mix(in srgb, var(--bad) 15%, transparent);
    color: var(--bad);
  }
  .result.sel {
    background: var(--fill-hover);
  }
  .result.sel::before {
    content: "";
    width: 3px;
    height: 18px;
    margin-left: -10px;
    margin-right: -5px;
    border-radius: 3px;
    background: var(--accent);
  }
  .text {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .title {
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    color: var(--text-3);
  }
  .value {
    font-family: var(--font-display);
    font-size: 20px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .value.mono {
    font-family: var(--mono);
    font-size: 14px;
    font-weight: 400;
  }
  .error .value {
    font-size: 14px;
    font-weight: 400;
    color: var(--bad);
  }
  .hint {
    font-size: 12.5px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .action {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 6px;
    color: var(--text-3);
    opacity: 0;
    transition: opacity 0.12s;
  }
  .sel .action {
    opacity: 1;
  }
  .action.act {
    color: var(--accent);
  }
  .action.done {
    opacity: 1;
    color: var(--ok);
  }
  .empty {
    padding: 18px 14px;
    color: var(--text-2);
    font-size: 13px;
  }
</style>
