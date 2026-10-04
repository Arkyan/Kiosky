<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import Toggle from "../lib/Toggle.svelte";
  import {
    api,
    copyText,
    type Adapter,
    type DnsAnswer,
    type HostsState,
    type PublicIp,
    type SpeedProgress,
    type SpeedResult,
  } from "../lib/api";

  let adapters = $state<Adapter[]>([]);
  let publicIp = $state<PublicIp | null>(null);
  let loaded = $state(false);
  let loading = $state(false);
  let error = $state("");
  let info = $state("");

  // Test de débit
  let testing = $state(false);
  let phase = $state<SpeedProgress["phase"] | null>(null);
  let progress = $state(0);
  let live = $state<{ down: number | null; up: number | null }>({ down: null, up: null });
  let result = $state<SpeedResult | null>(null);
  let speedError = $state("");

  // DNS
  let host = $state("");
  let answer = $state<DnsAnswer | null>(null);
  let dnsError = $state("");
  let resolving = $state(false);

  // Fichier hosts
  let hosts = $state<HostsState | null>(null);
  let newIp = $state("127.0.0.1");
  let newNames = $state("");
  let confirmLine = $state<number | null>(null);
  let confirmTimer: ReturnType<typeof setTimeout> | undefined;

  /** La carte qui mène à Internet : la première à avoir une passerelle. */
  const main = $derived(adapters.find((a) => a.gateway.length && a.ipv4.length) ?? adapters[0]);
  const readOnly = $derived(!hosts?.is_admin);
  const entries = $derived(hosts ? hosts.lines.filter((l) => l.entry).length : 0);

  async function loadNetwork() {
    loading = true;
    try {
      adapters = await api.getNetwork();
      loaded = true;
      // L'adresse publique demande un aller-retour sur Internet : elle arrive après le reste.
      publicIp = await api.getPublicIp();
    } catch (e) {
      error = String(e);
    }
    loading = false;
  }

  async function loadHosts() {
    try {
      hosts = await api.getHosts();
    } catch (e) {
      error = String(e);
    }
  }

  function refresh() {
    publicIp = null;
    error = "";
    loadNetwork();
    loadHosts();
  }

  onMount(() => {
    loadNetwork();
    loadHosts();
    const un = listen<SpeedProgress>("speedtest-progress", (e) => {
      if (!testing) return;
      const p = e.payload;
      phase = p.phase;
      progress = p.progress;
      if (p.phase === "down") live.down = p.mbps;
      if (p.phase === "up") live.up = p.mbps;
    });
    return () => {
      un.then((f) => f());
      if (testing) api.stopSpeedtest();
    };
  });

  let infoTimer: ReturnType<typeof setTimeout> | undefined;
  function flash(text: string) {
    info = text;
    error = "";
    clearTimeout(infoTimer);
    infoTimer = setTimeout(() => (info = ""), 3000);
  }

  async function copy(text: string) {
    await copyText(text);
    flash(`${text} copié.`);
  }

  const regions = new Intl.DisplayNames(["fr"], { type: "region" });
  function country(code: string) {
    try {
      return regions.of(code) ?? code;
    } catch {
      return code;
    }
  }

  const KINDS: Record<Adapter["kind"], string> = {
    wifi: "Wi-Fi",
    ethernet: "Ethernet",
    vpn: "VPN",
    virtual: "Virtuelle",
    other: "Autre",
  };

  /** 1 000 000 000 → « 1 Gbit/s » */
  function fmtLink(bps: number) {
    if (!bps) return "";
    const mbps = bps / 1e6;
    return mbps >= 1000 ? `${fmt(mbps / 1000, 1)} Gbit/s` : `${fmt(mbps, 0)} Mbit/s`;
  }

  const fmt = (v: number, digits: number) => v.toLocaleString("fr-FR", { maximumFractionDigits: digits });
  const fmtMbps = (v: number) => fmt(v, v >= 100 ? 0 : v >= 10 ? 1 : 2);

  // ─── Test de débit ───

  async function startTest() {
    testing = true;
    phase = "ping";
    progress = 0;
    live = { down: null, up: null };
    result = null;
    speedError = "";
    try {
      const r = await api.runSpeedtest();
      if (r) {
        result = r;
        live = { down: r.down_mbps, up: r.up_mbps };
      }
    } catch (e) {
      speedError = String(e);
    }
    testing = false;
    phase = null;
  }

  function stopTest() {
    api.stopSpeedtest();
  }

  /** La latence compte pour un dixième de la barre, chaque débit pour 45 %. */
  const overall = $derived(phase === "down" ? 10 + progress * 45 : phase === "up" ? 55 + progress * 45 : 4);

  // ─── DNS ───

  async function lookup() {
    if (!host.trim()) return;
    resolving = true;
    try {
      answer = await api.dnsLookup(host);
      dnsError = "";
    } catch (e) {
      answer = null;
      dnsError = String(e);
    }
    resolving = false;
  }

  const fromHosts = $derived(
    !!answer && !!hosts?.lines.some((l) => l.entry && l.enabled && l.names.toLowerCase().split(/\s+/).includes(answer!.host)),
  );

  async function flushDns() {
    try {
      await api.flushDns();
      flash("Cache DNS vidé.");
    } catch (e) {
      error = String(e);
    }
  }

  // ─── Fichier hosts ───

  /** Écrit le fichier, puis relit ce qu'il contient vraiment (y compris après un refus). */
  async function saveHosts(done: string) {
    if (!hosts) return;
    try {
      await api.saveHosts($state.snapshot(hosts.lines), hosts.stamp);
      flash(done);
    } catch (e) {
      error = String(e);
    }
    await loadHosts();
  }

  function setField(i: number, field: "ip" | "names" | "comment", value: string) {
    if (!hosts || hosts.lines[i][field] === value.trim()) return;
    hosts.lines[i][field] = value.trim();
    saveHosts("Fichier hosts enregistré.");
  }

  function setEnabled(i: number, enabled: boolean) {
    if (!hosts) return;
    hosts.lines[i].enabled = enabled;
    saveHosts(enabled ? `${hosts.lines[i].names} activé.` : `${hosts.lines[i].names} désactivé.`);
  }

  function removeLine(i: number) {
    if (!hosts) return;
    if (confirmLine !== i) {
      confirmLine = i;
      clearTimeout(confirmTimer);
      confirmTimer = setTimeout(() => (confirmLine = null), 3500);
      return;
    }
    confirmLine = null;
    const names = hosts.lines[i].names;
    hosts.lines.splice(i, 1);
    saveHosts(`${names} supprimé.`);
  }

  function addLine() {
    if (!hosts || !newIp.trim() || !newNames.trim()) return;
    hosts.lines.push({ raw: "", entry: true, ip: newIp.trim(), names: newNames.trim(), comment: "", enabled: true });
    const names = newNames.trim();
    newNames = "";
    saveHosts(`${names} ajouté.`);
  }

  async function undoHosts() {
    try {
      await api.undoHosts();
      flash("Dernière modification du fichier hosts annulée.");
    } catch (e) {
      error = String(e);
    }
    await loadHosts();
  }

  const blurOnEnter = (e: KeyboardEvent) => e.key === "Enter" && (e.currentTarget as HTMLInputElement).blur();
</script>

<PageHeader title="Réseau" subtitle="Tes adresses, le débit, le DNS et le fichier hosts.">
  {#snippet actions()}
    <button class="btn" onclick={refresh} disabled={loading}>
      <span class:spin={loading} style="display:flex"><Icon name="refresh" size={16} /></span> Actualiser
    </button>
  {/snippet}
</PageHeader>

{#if error}
  <div class="banner error">{error}</div>
{:else if info}
  <div class="banner ok"><Icon name="check" size={16} /> {info}</div>
{/if}

<!-- Adresses -->
<div class="addresses">
  <div class="card addr">
    <div class="small muted label">Adresse locale</div>
    {#if main?.ipv4.length}
      <button class="ip mono" title="Copier" onclick={() => copy(main.ipv4[0])}>{main.ipv4[0]}</button>
      <div class="small muted">
        {main.name}
        {#if main.gateway.length}
          · passerelle <button class="link mono gw" title="Copier" onclick={() => copy(main.gateway[0])}>{main.gateway[0]}</button>
        {/if}
      </div>
    {:else}
      <div class="ip off">{loaded ? "Non connecté" : "…"}</div>
      <div class="small muted">{loaded ? "Aucune carte réseau n'a d'adresse." : "Lecture des cartes réseau"}</div>
    {/if}
  </div>
  <div class="card addr">
    <div class="small muted label">Adresse publique</div>
    {#if !publicIp}
      <div class="ip off">…</div>
      <div class="small muted">Demandée à Cloudflare</div>
    {:else if publicIp.ipv4 || publicIp.ipv6}
      {@const first = publicIp.ipv4 ?? publicIp.ipv6 ?? ""}
      <button class="ip mono" class:long={!publicIp.ipv4} title="Copier" onclick={() => copy(first)}>{first}</button>
      <div class="small muted pub">
        {#if publicIp.country}<span>{country(publicIp.country)}</span>{/if}
        {#if publicIp.ipv4 && publicIp.ipv6}
          <button class="link mono v6" title="Copier l'adresse IPv6" onclick={() => copy(publicIp!.ipv6!)}>{publicIp.ipv6}</button>
        {:else if publicIp.ipv4}
          <span>pas d'IPv6</span>
        {/if}
      </div>
    {:else}
      <div class="ip off">Hors ligne</div>
      <div class="small muted">Internet est injoignable.</div>
    {/if}
  </div>
</div>

<!-- Test de débit -->
<h2>Débit</h2>
<div class="card speed">
  <div class="metrics">
    <div class="metric" class:active={phase === "ping"}>
      <div class="small muted">Latence</div>
      <div class="value">
        {#if result}{fmt(result.ping_ms, 0)}<span class="unit">ms</span>{:else}<span class="dash">–</span>{/if}
      </div>
      <div class="small muted sub">{result ? `gigue ${fmt(result.jitter_ms, 1)} ms` : " "}</div>
    </div>
    <div class="metric" class:active={phase === "down"}>
      <div class="small muted"><Icon name="down" size={12} /> Descendant</div>
      <div class="value">
        {#if live.down !== null}{fmtMbps(live.down)}<span class="unit">Mbit/s</span>{:else}<span class="dash">–</span>{/if}
      </div>
      <div class="small muted sub">{live.down !== null ? `${fmt(live.down / 8, 1)} Mo/s` : " "}</div>
    </div>
    <div class="metric" class:active={phase === "up"}>
      <div class="small muted"><Icon name="up" size={12} /> Montant</div>
      <div class="value">
        {#if live.up !== null}{fmtMbps(live.up)}<span class="unit">Mbit/s</span>{:else}<span class="dash">–</span>{/if}
      </div>
      <div class="small muted sub">{live.up !== null ? `${fmt(live.up / 8, 1)} Mo/s` : " "}</div>
    </div>
    <div class="go">
      {#if testing}
        <button class="btn" onclick={stopTest}><Icon name="stop" size={14} /> Arrêter</button>
      {:else}
        <button class="btn primary" onclick={startTest}><Icon name="bolt" size={15} /> {result ? "Relancer" : "Lancer le test"}</button>
      {/if}
    </div>
  </div>
  {#if testing}
    <div class="pbar"><div style:width={`${overall}%`}></div></div>
  {/if}
  <div class="foot small muted">
    {#if speedError}
      <span class="bad-text">{speedError}</span>
    {:else if testing}
      {phase === "ping" ? "Mesure de la latence…" : phase === "down" ? "Téléchargement en cours…" : "Envoi en cours…"}
    {:else}
      Environ 15 secondes, avec les serveurs de Cloudflare. Sur une connexion rapide, le test échange plusieurs centaines de Mo.
    {/if}
  </div>
</div>

<!-- Cartes réseau -->
<h2>Cartes réseau <span class="muted small">· {adapters.length}</span></h2>
<div class="card cards">
  {#each adapters as a (a.name)}
    <div class="adapter">
      <div class="ahead">
        <span class="strong">{a.name}</span>
        <span class="badge" class:accent={a === main}>{KINDS[a.kind]}</span>
        <span class="small muted desc" title={a.description}>{a.description}</span>
        {#if a.speed}<span class="small muted">{fmtLink(a.speed)}</span>{/if}
      </div>
      <dl>
        {#each a.ipv4 as ip, i}
          <dt>IPv4</dt>
          <dd>
            <span class="with-prefix">
              <button class="link mono" title="Copier" onclick={() => copy(ip)}>{ip}</button>{#if i === 0 && a.prefix}<span class="muted mono">/{a.prefix}</span>{/if}
            </span>
            {#if i === 0}<span class="muted small">{a.dhcp ? "automatique (DHCP)" : "fixe"}</span>{/if}
          </dd>
        {/each}
        {#each a.ipv6 as ip}
          <dt>IPv6</dt>
          <dd><button class="link mono" title="Copier" onclick={() => copy(ip)}>{ip}</button></dd>
        {/each}
        {#if a.gateway.length}
          <dt>Passerelle</dt>
          <dd>{#each a.gateway as g}<button class="link mono" title="Copier" onclick={() => copy(g)}>{g}</button>{/each}</dd>
        {/if}
        {#if a.dns.length}
          <dt>DNS</dt>
          <dd>{#each a.dns as d}<button class="link mono" title="Copier" onclick={() => copy(d)}>{d}</button>{/each}</dd>
        {/if}
        {#if a.mac}
          <dt>Adresse MAC</dt>
          <dd><button class="link mono" title="Copier" onclick={() => copy(a.mac)}>{a.mac}</button></dd>
        {/if}
      </dl>
    </div>
  {/each}
  {#if !adapters.length}
    <div class="empty muted">{loaded ? "Aucune carte réseau connectée." : "Chargement…"}</div>
  {/if}
</div>

<!-- DNS -->
<h2>DNS</h2>
<div class="card dns">
  <form
    class="row"
    onsubmit={(e) => {
      e.preventDefault();
      lookup();
    }}
  >
    <input class="field mono grow" bind:value={host} placeholder="exemple.fr" spellcheck="false" />
    <button class="btn" type="submit" disabled={resolving || !host.trim()}><Icon name="search" size={14} /> Résoudre</button>
    <button class="btn" type="button" onclick={flushDns} title="Oublie les réponses gardées en mémoire par Windows">
      <Icon name="broom" size={14} /> Vider le cache
    </button>
  </form>
  {#if dnsError}
    <div class="answer bad-text small">{dnsError}</div>
  {:else if answer}
    <div class="answer">
      <div class="small muted">
        <b>{answer.host}</b> · {answer.addresses.length} adresse{answer.addresses.length > 1 ? "s" : ""} en {answer.ms} ms
        {#if fromHosts}<span class="badge warn">fichier hosts</span>{/if}
      </div>
      <div class="ips">
        {#each answer.addresses as ip}
          <button class="chip mono" title="Copier" onclick={() => copy(ip)}>{ip}</button>
        {/each}
      </div>
    </div>
  {/if}
</div>

<!-- Fichier hosts -->
<div class="hhead">
  <h2>Fichier hosts <span class="muted small">· {entries}</span></h2>
  <span class="grow"></span>
  {#if hosts?.undo && !readOnly}
    <button class="btn ghost small-btn" onclick={undoHosts} title="Remet le fichier dans l'état d'avant la dernière modification faite ici">
      <Icon name="undo" size={14} /> Annuler la dernière modification
    </button>
  {/if}
</div>
{#if hosts}
  {#if readOnly}
    <div class="ro small muted">
      <Icon name="shield" size={13} /> Lecture seule : modifier le fichier hosts demande les droits admin.
      <button class="btn small-btn" onclick={() => api.restartAsAdmin().catch((e) => (error = String(e)))}>Relancer en admin</button>
    </div>
  {/if}
  <div class="card hosts">
    {#each hosts.lines as l, i (i)}
      {#if l.entry}
        <div class="hline" class:off={!l.enabled}>
          <Toggle checked={l.enabled} disabled={readOnly} label={`Activer ${l.names}`} onchange={(v) => setEnabled(i, v)} />
          <input
            class="mono hip"
            value={l.ip}
            disabled={readOnly}
            spellcheck="false"
            onkeydown={blurOnEnter}
            onchange={(e) => setField(i, "ip", e.currentTarget.value)}
          />
          <input
            class="mono hnames"
            value={l.names}
            disabled={readOnly}
            spellcheck="false"
            title={l.names}
            onkeydown={blurOnEnter}
            onchange={(e) => setField(i, "names", e.currentTarget.value)}
          />
          <input
            class="hcomment"
            value={l.comment}
            disabled={readOnly}
            placeholder={readOnly ? "" : "note"}
            title={l.comment}
            onkeydown={blurOnEnter}
            onchange={(e) => setField(i, "comment", e.currentTarget.value)}
          />
          {#if !readOnly}
            <button class="btn ghost del" class:confirm={confirmLine === i} onclick={() => removeLine(i)}>
              {#if confirmLine === i}Supprimer ?{:else}<Icon name="trash" size={14} />{/if}
            </button>
          {/if}
        </div>
      {/if}
    {/each}
    {#if !entries}
      <div class="empty muted">Aucune entrée : tous les noms sont résolus par le DNS.</div>
    {/if}
    {#if !readOnly}
      <form
        class="hline new"
        onsubmit={(e) => {
          e.preventDefault();
          addLine();
        }}
      >
        <input class="field mono hip" bind:value={newIp} placeholder="127.0.0.1" spellcheck="false" />
        <input class="field mono grow" bind:value={newNames} placeholder="monsite.test api.monsite.test" spellcheck="false" />
        <button class="btn" type="submit" disabled={!newIp.trim() || !newNames.trim()}><Icon name="plus" size={14} /> Ajouter</button>
      </form>
    {/if}
  </div>
  <div class="small muted path mono">{hosts.path}</div>
{/if}

<style>
  .banner {
    margin-bottom: 12px;
  }
  .banner.ok {
    border-color: color-mix(in srgb, var(--ok) 40%, transparent);
    background: color-mix(in srgb, var(--ok) 10%, var(--card));
  }
  h2 {
    margin: 24px 2px 10px;
    font-size: 15px;
    font-weight: 600;
  }
  .strong {
    font-weight: 600;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .bad-text {
    color: var(--bad);
  }
  .small-btn {
    height: 28px;
    font-size: 12.5px;
  }
  .empty {
    padding: 24px;
    text-align: center;
  }

  /* Boutons « texte à copier » */
  .link {
    padding: 1px 5px;
    margin: 0 -5px;
    border: none;
    border-radius: 5px;
    background: transparent;
    font-size: 12.5px;
    color: inherit;
    cursor: pointer;
  }
  .link:hover {
    background: var(--fill-hover);
    color: var(--accent);
  }

  .addresses {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .addr {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    min-width: 0;
    padding: 14px 18px 14px;
  }
  .label {
    font-weight: 600;
  }
  .ip {
    max-width: 100%;
    margin: 2px -8px;
    padding: 0 8px;
    border: none;
    border-radius: 7px;
    background: transparent;
    font-size: 26px;
    font-weight: 600;
    letter-spacing: -0.01em;
    line-height: 1.35;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  button.ip {
    cursor: pointer;
  }
  button.ip:hover {
    background: var(--fill-hover);
    color: var(--accent);
  }
  .ip.long {
    font-size: 15px;
    line-height: 2.3;
  }
  .ip.off {
    font-family: var(--font-display);
    color: var(--text-3);
  }
  .pub {
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: 100%;
    min-width: 0;
  }
  .pub span {
    white-space: nowrap;
  }
  .v6 {
    min-width: 0;
    margin: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 11.5px;
  }

  .speed {
    overflow: hidden;
  }
  .metrics {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 16px 18px 8px;
  }
  .metric {
    flex: 1;
    min-width: 0;
  }
  .metric > .small {
    display: flex;
    align-items: center;
    gap: 4px;
    font-weight: 600;
  }
  .value {
    font-family: var(--font-display);
    font-size: 28px;
    font-weight: 600;
    line-height: 1.3;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .metric.active .value {
    color: var(--accent);
  }
  .unit {
    margin-left: 5px;
    font-size: 12.5px;
    font-weight: 500;
    color: var(--text-2);
  }
  .dash {
    color: var(--text-3);
  }
  .sub {
    min-height: 17px;
    white-space: pre;
  }
  .go {
    flex: none;
  }
  .pbar {
    height: 3px;
    margin: 4px 18px 0;
    border-radius: 999px;
    background: var(--stroke-strong);
    overflow: hidden;
  }
  .pbar div {
    height: 100%;
    border-radius: 999px;
    background: var(--accent);
    transition: width 0.2s linear;
  }
  .foot {
    padding: 8px 18px 14px;
  }

  .cards {
    overflow: hidden;
  }
  .adapter {
    padding: 12px 18px 12px;
    border-bottom: 1px solid var(--stroke);
  }
  .adapter:last-child {
    border-bottom: none;
  }
  .ahead {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 6px;
  }
  .desc {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  dl {
    display: grid;
    grid-template-columns: 110px 1fr;
    gap: 2px 12px;
    margin: 0;
    font-size: 12.5px;
  }
  dt {
    color: var(--text-3);
  }
  dd {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 2px 16px;
    min-width: 0;
    margin: 0;
  }
  .with-prefix {
    display: flex;
    align-items: baseline;
    gap: 5px;
  }
  .gw {
    margin: 0;
  }

  .dns {
    padding: 12px;
  }
  .row {
    display: flex;
    gap: 6px;
  }
  .answer {
    padding: 12px 6px 2px;
  }
  .answer .badge {
    margin-left: 6px;
  }
  .ips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 8px;
  }

  .hhead {
    display: flex;
    align-items: flex-end;
    gap: 10px;
  }
  .hhead .btn {
    margin-bottom: 6px;
  }
  .ro {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 2px 10px;
  }
  .ro .btn {
    margin-left: 8px;
  }
  .hosts {
    overflow: hidden;
  }
  .hline {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 42px;
    padding: 3px 8px 3px 14px;
    border-bottom: 1px solid var(--stroke);
  }
  .hline:last-child {
    border-bottom: none;
  }
  .hline:not(.new):hover {
    background: var(--fill-hover);
  }
  .hline:not(.new) input {
    min-width: 0;
    padding: 4px 6px;
    border: 1px solid transparent;
    border-radius: 5px;
    background: transparent;
    font-size: 12.5px;
    outline: none;
    text-overflow: ellipsis;
  }
  .hline input:hover:not(:disabled):not(.field) {
    border-color: var(--stroke);
  }
  .hline input:focus:not(.field) {
    border-color: var(--accent);
    background: var(--input);
  }
  .hline.off input {
    color: var(--text-3);
  }
  .hip {
    flex: none;
    width: 150px;
    margin-left: 6px;
  }
  .hnames {
    flex: 2;
    font-weight: 600;
  }
  .hcomment {
    flex: 1;
    color: var(--text-2);
  }
  .del {
    flex: none;
    height: 28px;
    padding: 0 8px;
    font-size: 12.5px;
    color: var(--text-3);
  }
  .del:hover,
  .del.confirm {
    color: var(--bad);
  }
  .del.confirm {
    background: color-mix(in srgb, var(--bad) 12%, transparent);
    font-weight: 600;
  }
  .hline.new {
    padding: 10px 12px;
  }
  .hline.new .field {
    height: 32px;
    font-size: 12.5px;
  }
  .hline.new .hip {
    margin-left: 0;
  }
  .path {
    margin: 8px 4px 0;
    font-size: 11px;
    color: var(--text-3);
    user-select: text;
  }
</style>
