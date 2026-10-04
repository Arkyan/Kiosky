<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import { api, copyText, type SshBlock, type SshLine, type SshState } from "../lib/api";

  /** Options qui ont leur propre champ ; les autres sont listées en dessous. */
  const KNOWN = ["HostName", "User", "Port", "IdentityFile"] as const;
  type Known = (typeof KNOWN)[number];
  type Extra = { key: string; value: string; line: number | null };
  type Draft = { index: number | null; patterns: string; fields: Record<Known, string>; extras: Extra[] };

  let ssh = $state<SshState | null>(null);
  let error = $state("");
  let info = $state("");
  let draft = $state<Draft | null>(null);
  let saving = $state(false);
  let confirmBlock = $state<number | null>(null);
  let confirmTimer: ReturnType<typeof setTimeout> | undefined;

  const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();
  const option = (b: SshBlock, key: string) => b.lines.find((l) => same(l.key, key))?.value ?? "";
  /** Un alias qu'on peut passer tel quel à « ssh » : pas un motif comme « *.interne ». */
  const plain = (alias: string) => /^[A-Za-z0-9._][A-Za-z0-9._-]*$/.test(alias);

  const servers = $derived(ssh ? ssh.blocks.map((block, index) => ({ block, index })).filter((s) => s.block.kind !== "global") : []);
  const general = $derived(ssh?.blocks.findIndex((b) => b.kind === "global" && b.lines.some((l) => l.key)) ?? -1);

  async function load() {
    try {
      ssh = await api.getSsh();
    } catch (e) {
      error = String(e);
    }
  }

  onMount(load);

  let infoTimer: ReturnType<typeof setTimeout> | undefined;
  function flash(text: string) {
    info = text;
    error = "";
    clearTimeout(infoTimer);
    infoTimer = setTimeout(() => (info = ""), 3000);
  }

  async function copy(text: string, what: string) {
    await copyText(text);
    flash(`${what} copié.`);
  }

  function summary(b: SshBlock) {
    const host = option(b, "HostName");
    const user = option(b, "User");
    const port = option(b, "Port");
    if (!host) return user ? `utilisateur ${user}` : "";
    return `${user ? user + "@" : ""}${host}${port ? ":" + port : ""}`;
  }

  const keyName = (b: SshBlock) => option(b, "IdentityFile").replace(/"/g, "").split(/[\\/]/).pop() ?? "";

  async function connect(alias: string, vscode = false) {
    try {
      await api.sshConnect(alias, vscode);
    } catch (e) {
      error = String(e);
    }
  }

  // ─── Édition ───

  function edit(index: number | null) {
    const b = index === null ? null : ssh!.blocks[index];
    const fields = Object.fromEntries(KNOWN.map((k) => [k, b ? option(b, k) : ""])) as Record<Known, string>;
    // La première ligne de chaque option connue est dans son champ ; tout le reste est une « autre option ».
    const taken = new Set<string>();
    const extras: Extra[] = [];
    b?.lines.forEach((l, line) => {
      if (!l.key) return;
      const known = KNOWN.find((k) => same(k, l.key));
      if (known && !taken.has(known)) taken.add(known);
      else extras.push({ key: l.key, value: l.value, line });
    });
    draft = { index, patterns: b?.patterns ?? "", fields, extras };
    error = "";
  }

  /** Les lignes du bloc d'après le brouillon : commentaires et ordre d'origine conservés. */
  function buildLines(d: Draft, original: SshLine[]): SshLine[] {
    const taken = new Set<string>();
    const lines: SshLine[] = [];
    original.forEach((l, line) => {
      if (!l.key) return lines.push(l);
      const known = KNOWN.find((k) => same(k, l.key));
      if (known && !taken.has(known)) {
        taken.add(known);
        const value = d.fields[known].trim();
        if (value) lines.push({ ...l, value });
        return;
      }
      const extra = d.extras.find((x) => x.line === line);
      if (extra && extra.key.trim() && extra.value.trim()) lines.push({ ...l, key: extra.key.trim(), value: extra.value.trim() });
    });
    const added: SshLine[] = [];
    for (const k of KNOWN) {
      if (!taken.has(k) && d.fields[k].trim()) added.push({ raw: "", key: k, value: d.fields[k].trim() });
    }
    for (const x of d.extras) {
      if (x.line === null && x.key.trim() && x.value.trim()) added.push({ raw: "", key: x.key.trim(), value: x.value.trim() });
    }
    // Les nouvelles options vont avant les lignes vides qui séparent du bloc suivant.
    let end = lines.length;
    while (end > 0 && !lines[end - 1].key && !lines[end - 1].raw.trim()) end--;
    lines.splice(end, 0, ...added);
    return lines;
  }

  async function write(blocks: SshBlock[], done: string) {
    if (!ssh) return false;
    saving = true;
    let ok = false;
    try {
      await api.saveSsh(blocks, ssh.stamp);
      flash(done);
      ok = true;
    } catch (e) {
      error = String(e);
    }
    saving = false;
    await load();
    return ok;
  }

  async function saveDraft() {
    if (!ssh || !draft) return;
    const d = $state.snapshot(draft) as Draft;
    const blocks = $state.snapshot(ssh.blocks) as SshBlock[];
    const patterns = d.patterns.trim();
    if (d.index === null) {
      const taken = blocks.some((b) => b.kind === "host" && b.patterns.split(/\s+/).some((a) => patterns.split(/\s+/).includes(a)));
      if (taken) {
        error = `Le serveur « ${patterns} » existe déjà.`;
        return;
      }
      blocks.push({ before: [], header: "", kind: "host", patterns, lines: buildLines(d, []) });
    } else {
      const b = blocks[d.index];
      blocks[d.index] = { ...b, patterns: b.kind === "global" ? "" : patterns, lines: buildLines(d, b.lines) };
    }
    if (await write(blocks, d.index === null ? `${patterns} ajouté.` : "Configuration SSH enregistrée.")) draft = null;
  }

  function remove(index: number) {
    if (!ssh) return;
    if (confirmBlock !== index) {
      confirmBlock = index;
      clearTimeout(confirmTimer);
      confirmTimer = setTimeout(() => (confirmBlock = null), 3500);
      return;
    }
    confirmBlock = null;
    const blocks = $state.snapshot(ssh.blocks) as SshBlock[];
    const [gone] = blocks.splice(index, 1);
    if (draft?.index === index) draft = null;
    write(blocks, `${gone.patterns} supprimé.`);
  }

  async function undo() {
    try {
      await api.undoSsh();
      flash("Dernière modification annulée.");
    } catch (e) {
      error = String(e);
    }
    draft = null;
    await load();
  }
</script>

{#snippet editor(d: Draft, title: string)}
  {@const global = d.index !== null && ssh?.blocks[d.index].kind === "global"}
  <form
    class="editor"
    onsubmit={(e) => {
      e.preventDefault();
      saveDraft();
    }}
  >
    <div class="etitle strong">{title}</div>
    {#if !global}
      <div class="grid">
        <label>
          <span class="small muted">Nom (ce que tu tapes après « ssh »)</span>
          <!-- svelte-ignore a11y_autofocus -->
          <input class="field mono" bind:value={d.patterns} placeholder="monserveur" spellcheck="false" autofocus={d.index === null} />
        </label>
        <label>
          <span class="small muted">Adresse</span>
          <input class="field mono" bind:value={d.fields.HostName} placeholder="exemple.fr ou 203.0.113.10" spellcheck="false" />
        </label>
        <label>
          <span class="small muted">Utilisateur</span>
          <input class="field mono" bind:value={d.fields.User} placeholder="root" spellcheck="false" />
        </label>
        <label>
          <span class="small muted">Port</span>
          <input class="field mono" bind:value={d.fields.Port} placeholder="22" inputmode="numeric" spellcheck="false" />
        </label>
        <label class="wide">
          <span class="small muted">Clé (IdentityFile)</span>
          <input class="field mono" bind:value={d.fields.IdentityFile} placeholder="~/.ssh/id_ed25519" spellcheck="false" list="ssh-keys" />
        </label>
      </div>
    {/if}
    {#if d.extras.length}
      <div class="small muted olabel">{global ? "Options appliquées à tous les serveurs" : "Autres options"}</div>
    {/if}
    {#each d.extras as x, i (i)}
      <div class="orow">
        <input class="field mono okey" bind:value={x.key} placeholder="Option" spellcheck="false" />
        <input class="field mono grow" bind:value={x.value} placeholder="valeur" spellcheck="false" />
        <button class="btn ghost icon" type="button" title="Retirer" onclick={() => d.extras.splice(i, 1)}><Icon name="x" size={14} /></button>
      </div>
    {/each}
    <div class="ebar">
      <button class="btn ghost small-btn" type="button" onclick={() => d.extras.push({ key: "", value: "", line: null })}>
        <Icon name="plus" size={13} /> Option (ForwardAgent, ProxyJump…)
      </button>
      <span class="grow"></span>
      <button class="btn ghost" type="button" onclick={() => (draft = null)}>Annuler</button>
      <button class="btn primary" type="submit" disabled={saving || (!global && !d.patterns.trim())}>Enregistrer</button>
    </div>
  </form>
{/snippet}

<PageHeader title="SSH" subtitle="Les serveurs de ton fichier ~/.ssh/config, et tes clés.">
  {#snippet actions()}
    {#if ssh?.undo}
      <button class="btn" onclick={undo} title="Remet le fichier dans l'état d'avant la dernière modification faite ici">
        <Icon name="undo" size={15} /> Annuler la dernière modification
      </button>
    {/if}
    <button class="btn primary" onclick={() => edit(null)} disabled={!ssh}><Icon name="plus" size={15} /> Ajouter un serveur</button>
  {/snippet}
</PageHeader>

{#if error}
  <div class="banner error">{error}</div>
{:else if info}
  <div class="banner ok"><Icon name="check" size={16} /> {info}</div>
{/if}

<datalist id="ssh-keys">
  {#each ssh?.keys.filter((k) => k.has_private) ?? [] as k}
    <option value={k.path}></option>
  {/each}
</datalist>

{#if ssh}
  {#if draft && draft.index === null}
    <div class="card new">{@render editor(draft, "Nouveau serveur")}</div>
  {/if}

  <div class="card list">
    {#each servers as { block: b, index } (index)}
      {@const aliases = b.patterns.split(/\s+/).filter(Boolean)}
      {@const first = aliases.find(plain)}
      <div class="server" class:open={draft?.index === index}>
        <div class="srow">
          <div class="sname">
            <span class="strong mono">{b.patterns}</span>
            {#if b.kind === "match"}<span class="badge">Match</span>{:else if !first}<span class="badge">Motif</span>{/if}
          </div>
          <div class="starget small muted mono" title={summary(b)}>{summary(b)}</div>
          {#if keyName(b)}<span class="small muted skey" title={option(b, "IdentityFile")}><Icon name="key" size={12} /> {keyName(b)}</span>{/if}
          <div class="sact">
            {#if first}
              <button class="btn ghost small-btn connect" onclick={() => connect(first)} title={`Ouvre un terminal sur « ssh ${first} »`}>
                <Icon name="terminal" size={14} /> Connecter
              </button>
              {#if ssh.vscode}
                <button class="btn ghost small-btn connect" onclick={() => connect(first, true)} title={`Ouvre VS Code connecté à ${first} (extension Remote - SSH)`}>
                  <Icon name="code" size={14} /> VS Code
                </button>
              {/if}
              <button class="btn ghost icon" title={`Copier « ssh ${first} »`} onclick={() => copy(`ssh ${first}`, `ssh ${first}`)}><Icon name="copy" size={14} /></button>
            {/if}
            <button class="btn ghost small-btn" onclick={() => (draft?.index === index ? (draft = null) : edit(index))}>Modifier</button>
            <button class="btn ghost del" class:confirm={confirmBlock === index} onclick={() => remove(index)}>
              {#if confirmBlock === index}Supprimer ?{:else}<Icon name="trash" size={14} />{/if}
            </button>
          </div>
        </div>
        {#if draft && draft.index === index}
          {@render editor(draft, b.kind === "match" ? "Bloc Match" : "Modifier le serveur")}
        {/if}
      </div>
    {/each}
    {#if !servers.length}
      <div class="empty muted">Aucun serveur pour l'instant. Ajoute-en un : il suffira ensuite de taper « ssh son-nom ».</div>
    {/if}
  </div>

  {#if general >= 0}
    <div class="card list general">
      <div class="server" class:open={draft?.index === general}>
        <div class="srow">
          <div class="sname"><span class="strong">Options générales</span></div>
          <div class="starget small muted mono">
            {ssh.blocks[general].lines.filter((l) => l.key).map((l) => `${l.key} ${l.value}`).join(" · ")}
          </div>
          <div class="sact">
            <button class="btn ghost small-btn" onclick={() => (draft?.index === general ? (draft = null) : edit(general))}>Modifier</button>
          </div>
        </div>
        {#if draft && draft.index === general}
          {@render editor(draft, "Options générales")}
        {/if}
      </div>
    </div>
  {/if}

  <div class="khead">
    <h2>Clés <span class="muted small">· {ssh.keys.length}</span></h2>
    <span class="grow"></span>
    <button class="btn ghost small-btn" onclick={() => api.openWith("explorer", ssh!.dir).catch((e) => (error = String(e)))}>
      <Icon name="folder" size={14} /> Ouvrir le dossier
    </button>
  </div>
  <div class="card list">
    {#each ssh.keys as k (k.name)}
      <div class="srow">
        <div class="sname">
          <span class="strong mono">{k.name}</span>
          {#if k.kind}<span class="badge accent">{k.kind}</span>{/if}
        </div>
        <div class="starget small muted" title={k.comment}>
          {#if !k.public}Clé privée sans fichier .pub{:else if !k.has_private}Clé publique seule : la clé privée n'est pas dans le dossier{:else}{k.comment}{/if}
        </div>
        <div class="sact">
          {#if k.public}
            <button class="btn ghost small-btn" onclick={() => copy(k.public!, "Clé publique")} title="À coller dans authorized_keys sur le serveur, ou sur GitHub">
              <Icon name="copy" size={14} /> Copier la clé publique
            </button>
          {/if}
        </div>
      </div>
    {/each}
    {#if !ssh.keys.length}
      <div class="empty muted">Aucune clé dans {ssh.dir}.</div>
    {/if}
  </div>
  <div class="small muted path mono">{ssh.dir}\config</div>
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
  .small-btn {
    height: 28px;
    padding: 0 10px;
    font-size: 12.5px;
  }
  .empty {
    padding: 24px;
    text-align: center;
  }

  .list {
    overflow: hidden;
  }
  .new,
  .general {
    margin-bottom: 12px;
  }
  .general {
    margin-top: 12px;
    margin-bottom: 0;
  }
  .server {
    border-bottom: 1px solid var(--stroke);
  }
  .server:last-child {
    border-bottom: none;
  }
  .server.open {
    background: var(--card-2);
  }
  .srow {
    display: flex;
    align-items: center;
    gap: 14px;
    min-height: 48px;
    padding: 4px 8px 4px 18px;
  }
  .list > .srow {
    border-bottom: 1px solid var(--stroke);
  }
  .list > .srow:last-child {
    border-bottom: none;
  }
  .srow:hover {
    background: var(--fill-hover);
  }
  .sname {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
    max-width: 40%;
    min-width: 120px;
    font-size: 13.5px;
  }
  .sname .strong {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .starget {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .skey {
    display: flex;
    align-items: center;
    gap: 5px;
    flex: none;
    max-width: 160px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .sact {
    display: flex;
    align-items: center;
    gap: 2px;
    flex: none;
  }
  .connect {
    color: var(--accent);
  }
  .del {
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

  .editor {
    padding: 14px 18px 14px;
  }
  .server .editor {
    padding-top: 6px;
    border-top: 1px solid var(--stroke);
  }
  .etitle {
    margin-bottom: 10px;
  }
  .server .etitle {
    display: none;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px 12px;
    margin-top: 8px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .wide {
    grid-column: 1 / -1;
  }
  .editor .field {
    width: 100%;
    height: 32px;
    font-size: 12.5px;
  }
  .olabel {
    margin: 14px 0 6px;
  }
  .orow {
    display: flex;
    gap: 6px;
    margin-bottom: 6px;
  }
  .editor .okey {
    flex: none;
    width: 200px;
  }
  .ebar {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 12px;
  }

  .khead {
    display: flex;
    align-items: flex-end;
    gap: 10px;
  }
  .khead .btn {
    margin-bottom: 6px;
  }
  .path {
    margin: 8px 4px 0;
    font-size: 11px;
    color: var(--text-3);
    user-select: text;
  }
</style>
