<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, copyText, type ConvResult } from "./api";
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
      if (!q) {
        results = [];
        loading = false;
        return;
      }
      loading = true;
      try {
        const r = await api.convert(q);
        if (id === seq) {
          results = r;
          selected = 0;
        }
      } finally {
        if (id === seq) loading = false;
      }
    }, delay);
  }

  /** Icône du bouton de droite : l'action du résultat, ou la copie. */
  const actionIcon = (r: ConvResult) =>
    r.action.startsWith("kill:") ? "stop" : r.action.startsWith("open:") ? "globe" : "copy";

  async function copy(i: number) {
    const r = results[i];
    if (!r || r.error || (!r.copy && !r.action)) return;
    if (r.action) {
      // « kill 3000 », « port 5173 » : Entrée lance l'action au lieu de copier.
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
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!results.length) return;
      const d = e.key === "ArrowDown" ? 1 : -1;
      selected = (selected + d + results.length) % results.length;
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
      await tick();
      input?.focus();
    });
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
      oninput={() => run()}
      {onkeydown}
      placeholder="10 km en miles · 50 eur usd · 14h tokyo · 2^10 · kill 3000"
      spellcheck="false"
      autocomplete="off"
    />
    {#if query}
      <button class="btn ghost icon clear" title="Effacer" onclick={() => setQuery("")}><Icon name="x" size={14} /></button>
    {/if}
  </div>

  <div class="results" bind:this={list}>
    {#each results as r, i (i)}
      <button
        class="result"
        class:sel={i === selected}
        class:error={r.error}
        onmouseenter={() => (selected = i)}
        onclick={() => copy(i)}
      >
        <div class="text">
          <span class="title">{r.title}</span>
          <span class="value" class:mono={r.title.startsWith("JSON") || r.title.startsWith("Base64")}>{r.value}</span>
          {#if r.hint}<span class="hint">{r.hint}</span>{/if}
        </div>
        {#if !r.error && (r.copy || r.action)}
          <span class="action" class:done={copied === i} class:act={!!r.action}>
            <Icon name={copied === i ? "check" : actionIcon(r)} size={15} />
          </span>
        {/if}
      </button>
    {:else}
      {#if query.trim() && !loading}
        <div class="empty">Rien de reconnu. Essaie « 72 f en c », « 20% de 150 », « paris en new york » ou « port 3000 ».</div>
      {/if}
    {/each}
  </div>
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
