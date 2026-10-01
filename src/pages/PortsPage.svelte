<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import Toggle from "../lib/Toggle.svelte";
  import { api, type PortEntry } from "../lib/api";
  import { store, saveSettings } from "../lib/settings.svelte";

  type Row = PortEntry & { addrs: string[]; key: string };

  let entries = $state<PortEntry[]>([]);
  let loaded = $state(false);
  let loading = $state(false);
  let error = $state("");
  let info = $state("");
  let query = $state("");
  let filter = $state<"listen" | "all">("listen");
  let auto = $state(true);
  let confirmPid = $state<number | null>(null);
  let confirmTimer: ReturnType<typeof setTimeout> | undefined;

  async function load() {
    loading = true;
    try {
      entries = await api.getPorts();
      error = "";
    } catch (e) {
      error = String(e);
    }
    loading = false;
    loaded = true;
  }

  onMount(() => {
    load();
    const t = setInterval(() => {
      if (auto && !document.hidden && confirmPid === null) load();
    }, 3000);
    return () => clearInterval(t);
  });

  const hideSystem = $derived(store.s?.ports_hide_system ?? true);

  function setHideSystem(v: boolean) {
    if (!store.s) return;
    store.s.ports_hide_system = v;
    saveSettings(0);
  }

  const q = $derived(query.trim().toLowerCase());
  const numeric = $derived(/^\d+$/.test(q));

  /** Un même port écouté en IPv4 et IPv6 par le même processus = une seule ligne. */
  const rows = $derived.by(() => {
    const out: Row[] = [];
    const seen = new Map<string, Row>();
    for (const e of entries) {
      if (filter === "listen" && !e.listening) continue;
      if (hideSystem && e.system) continue;
      if (q) {
        const hit = numeric
          ? String(e.local_port).startsWith(q) || (!e.listening && String(e.remote_port) === q)
          : e.process.toLowerCase().includes(q) || String(e.pid) === q || e.local_addr.includes(q) || e.remote_addr.includes(q);
        if (!hit) continue;
      }
      const key = e.listening
        ? `${e.proto}|${e.local_port}|${e.pid}`
        : `${e.proto}|${e.local_addr}|${e.local_port}|${e.remote_addr}|${e.remote_port}`;
      const prev = seen.get(key);
      if (prev) {
        if (!prev.addrs.includes(e.local_addr)) prev.addrs.push(e.local_addr);
        continue;
      }
      const row = { ...e, addrs: [e.local_addr], key };
      seen.set(key, row);
      out.push(row);
    }
    // Recherche d'un port précis : correspondance exacte en premier.
    if (numeric) out.sort((a, b) => Number(b.local_port === +q) - Number(a.local_port === +q));
    return out;
  });

  const exactOwner = $derived(numeric ? entries.find((e) => e.listening && e.local_port === +q) : undefined);
  const hiddenCount = $derived(
    hideSystem ? new Set(entries.filter((e) => e.system && (filter === "all" || e.listening)).map((e) => `${e.proto}|${e.local_port}|${e.pid}`)).size : 0,
  );

  function addrLabel(a: string) {
    if (a === "0.0.0.0" || a === "::") return "Toutes";
    if (a === "127.0.0.1" || a === "::1") return "Local";
    return a;
  }

  const LOCAL = ["0.0.0.0", "::", "127.0.0.1", "::1"];

  /** Un serveur web local probable : TCP en écoute, joignable via localhost, pas un service Windows. */
  const openable = (r: Row) => r.proto === "TCP" && r.listening && !r.system && r.addrs.some((a) => LOCAL.includes(a));

  async function open(r: Row) {
    try {
      await api.openUrl(`http://localhost:${r.local_port}`);
    } catch (e) {
      error = String(e);
    }
  }

  async function kill(row: Row) {
    if (confirmPid !== row.pid) {
      confirmPid = row.pid;
      clearTimeout(confirmTimer);
      confirmTimer = setTimeout(() => (confirmPid = null), 3500);
      return;
    }
    confirmPid = null;
    try {
      await api.killProcess(row.pid);
      info = `${row.process} (PID ${row.pid}) arrêté.`;
      setTimeout(() => (info = ""), 3000);
      error = "";
      await load();
    } catch (e) {
      error = String(e);
    }
  }
</script>

<PageHeader title="Ports" subtitle="Qui utilise quel port, et un bouton pour l'arrêter.">
  {#snippet actions()}
    <label class="auto small muted">Auto <Toggle checked={auto} label="Actualisation automatique" onchange={(v) => (auto = v)} /></label>
    <button class="btn" onclick={load} disabled={loading}>
      <span class:spin={loading} style="display:flex"><Icon name="refresh" size={16} /></span> Actualiser
    </button>
  {/snippet}
</PageHeader>

{#if error}
  <div class="banner error">{error}</div>
{:else if info}
  <div class="banner"><Icon name="check" size={16} /> {info}</div>
{/if}

<div class="toolbar">
  <div class="searchbox">
    <Icon name="search" size={15} />
    <!-- svelte-ignore a11y_autofocus -->
    <input bind:value={query} placeholder="Port (3000, 5173…), processus ou PID" spellcheck="false" autofocus />
    {#if query}
      <button class="clear" onclick={() => (query = "")} title="Effacer"><Icon name="x" size={12} /></button>
    {/if}
  </div>
  <div class="seg-ctrl">
    <button class:active={filter === "listen"} onclick={() => (filter = "listen")}>En écoute</button>
    <button class:active={filter === "all"} onclick={() => (filter = "all")}>Toutes les connexions</button>
  </div>
</div>

<div class="sysbar small">
  <label class="sys">
    <Toggle checked={hideSystem} label="Masquer les ports de Windows" onchange={setHideSystem} />
    Masquer les ports de Windows
  </label>
  <span class="muted">
    {#if hideSystem && hiddenCount}
      {hiddenCount} masqués (System, svchost, lsass…)
    {:else if !hideSystem}
      Les services de Windows sont affichés avec le badge « Windows »
    {/if}
  </span>
</div>

{#if numeric && loaded}
  <div class="verdict" class:busy={!!exactOwner}>
    <span class="dot"></span>
    {#if exactOwner}
      Le port <b>{q}</b> est utilisé par <b>{exactOwner.process}</b>
      {#if exactOwner.system && hideSystem}<span class="muted">(service Windows, masqué)</span>{/if}.
    {:else}
      Le port <b>{q}</b> est libre.
    {/if}
  </div>
{/if}

<div class="card table">
  <table>
    <thead>
      <tr class="small">
        <th>Port</th>
        <th class="w-proc">Processus</th>
        <th>{filter === "all" ? "Adresse distante" : "Adresse"}</th>
        {#if filter === "all"}<th>État</th>{/if}
        <th></th>
      </tr>
    </thead>
    <tbody>
      {#each rows.slice(0, 300) as r (r.key)}
        <tr class:exact={numeric && r.local_port === +q}>
          <td class="port">
            <b class="mono">{r.local_port}</b><span class="proto">{r.proto}</span>
          </td>
          <td class="w-proc">
            <div class="proc" title={r.path || r.process}>
              <span class="pname">{r.process.replace(/\.exe$/i, "")}</span>
              {#if r.system}<span class="badge win">Windows</span>{/if}
              <span class="pid small">{r.pid}</span>
            </div>
          </td>
          <td class="addr small" title={r.addrs.join(", ")}>
            {#if r.listening}
              {[...new Set(r.addrs.map(addrLabel))].join(" · ")}
            {:else}
              <span class="mono">{r.remote_addr}:{r.remote_port}</span>
            {/if}
          </td>
          {#if filter === "all"}
            <td>
              {#if r.proto === "UDP"}
                <span class="badge">UDP</span>
              {:else}
                <span class="badge" class:accent={r.listening} class:ok={r.state === "Établie"}>{r.state}</span>
              {/if}
            </td>
          {/if}
          <td class="act">
            <div>
              {#if openable(r)}
                <button class="btn ghost open" title={`Ouvrir http://localhost:${r.local_port}`} onclick={() => open(r)}>
                  <Icon name="globe" size={13} /> Ouvrir
                </button>
              {/if}
              {#if r.pid > 4}
                <button class="btn ghost kill" class:confirm={confirmPid === r.pid} onclick={() => kill(r)}>
                  <Icon name="stop" size={13} />
                  {confirmPid === r.pid ? "Confirmer" : "Arrêter"}
                </button>
              {/if}
            </div>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
  {#if !rows.length}
    <div class="empty muted">
      {loaded ? (q ? "Aucun résultat." : "Aucun port ouvert.") : "Chargement…"}
    </div>
  {/if}
  {#if rows.length > 300}
    <div class="empty small muted">{rows.length - 300} lignes de plus : affine la recherche.</div>
  {/if}
</div>

<style>
  .banner {
    margin-bottom: 12px;
  }
  .auto {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-right: 6px;
  }
  .toolbar {
    display: flex;
    gap: 10px;
    align-items: center;
    margin-bottom: 10px;
  }
  .searchbox {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    height: 34px;
    padding: 0 8px 0 12px;
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
  .clear {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
  }
  .clear:hover {
    background: var(--fill-hover);
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

  .sysbar {
    display: flex;
    align-items: center;
    gap: 14px;
    margin: 0 2px 12px;
  }
  .sys {
    display: flex;
    align-items: center;
    gap: 10px;
    cursor: pointer;
  }
  .badge.win {
    flex: none;
    height: 18px;
    font-size: 10.5px;
    font-weight: 500;
  }

  .verdict {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 2px 10px;
    font-size: 13px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--ok);
  }
  .verdict.busy .dot {
    background: var(--warn);
  }

  .table {
    overflow: hidden;
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th {
    height: 32px;
    padding: 0 10px;
    text-align: left;
    font-weight: 600;
    color: var(--text-3);
    white-space: nowrap;
    border-bottom: 1px solid var(--stroke);
  }
  td {
    height: 40px;
    padding: 0 10px;
    white-space: nowrap;
    border-bottom: 1px solid var(--stroke);
  }
  th:first-child,
  td:first-child {
    padding-left: 16px;
  }
  th:last-child,
  td:last-child {
    padding-right: 8px;
  }
  tbody tr:last-child td {
    border-bottom: none;
  }
  tbody tr {
    transition: background 0.12s;
  }
  tbody tr:hover {
    background: var(--fill-hover);
  }
  tr.exact {
    background: var(--accent-soft);
  }
  /* La colonne Processus prend toute la place restante, les autres se calent sur leur contenu */
  .w-proc {
    width: 100%;
    max-width: 0;
  }
  .port b {
    font-size: 14px;
  }
  .proto {
    margin-left: 6px;
    font-size: 10.5px;
    color: var(--text-3);
  }
  .addr {
    color: var(--text-2);
  }
  .proc {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }
  .pname {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-weight: 600;
  }
  .pid {
    flex: none;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .act div {
    display: flex;
    justify-content: flex-end;
    gap: 2px;
  }
  .open {
    height: 28px;
    padding: 0 10px;
    gap: 6px;
    font-size: 12.5px;
    color: var(--accent);
  }
  .kill {
    height: 28px;
    padding: 0 10px;
    gap: 6px;
    font-size: 12.5px;
    color: var(--text-2);
    opacity: 0;
    transition: opacity 0.12s;
  }
  tbody tr:hover .kill,
  .kill.confirm {
    opacity: 1;
  }
  .kill:hover,
  .kill.confirm {
    color: var(--bad);
  }
  .kill.confirm {
    background: color-mix(in srgb, var(--bad) 12%, transparent);
    font-weight: 600;
  }
  .empty {
    padding: 24px;
    text-align: center;
  }
</style>
