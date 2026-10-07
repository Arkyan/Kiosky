<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import Toggle from "../lib/Toggle.svelte";
  import { api, copyText, type SshBlock, type SshLine, type SshProject, type SshState, type SshTunnel } from "../lib/api";
  import { store, saveSettings } from "../lib/settings.svelte";

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

  onMount(() => {
    load();
    refreshTunnels();
    const timer = setInterval(refreshTunnels, 3000);
    // Une clé créée dans le terminal apparaît au retour dans Kiosky.
    window.addEventListener("focus", load);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", load);
    };
  });

  let infoTimer: ReturnType<typeof setTimeout> | undefined;
  function flash(text: string, ms = 3000) {
    info = text;
    error = "";
    clearTimeout(infoTimer);
    infoTimer = setTimeout(() => (info = ""), ms);
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

  async function forget(alias: string) {
    try {
      flash(await api.sshForget(alias), 6000);
    } catch (e) {
      error = String(e);
    }
  }

  // ─── Clés ───

  /** Serveurs auxquels on peut se connecter par leur nom. */
  const aliases = $derived(servers.filter((s) => s.block.kind === "host").flatMap((s) => s.block.patterns.split(/\s+/).filter(plain)));

  let keyForm = $state<{ name: string; comment: string; passphrase: boolean } | null>(null);
  let sending = $state<string | null>(null);
  let sendTo = $state("");

  function newKey() {
    const taken = ssh?.keys.some((k) => k.name === "id_ed25519");
    keyForm = { name: taken ? "" : "id_ed25519", comment: "", passphrase: false };
  }

  async function createKey() {
    if (!keyForm) return;
    const { name, comment, passphrase } = keyForm;
    try {
      await api.sshKeygen(name, comment, passphrase);
      flash(passphrase ? "Choisis la phrase secrète dans le terminal : la clé apparaîtra ici ensuite." : `Clé ${name.trim()} créée.`, 6000);
      keyForm = null;
    } catch (e) {
      error = String(e);
    }
    await load();
  }

  async function send(key: string) {
    try {
      await api.sshSendKey(key, sendTo);
      flash(`Un terminal s'est ouvert : ${sendTo} va demander son mot de passe une dernière fois.`, 6000);
      sending = null;
    } catch (e) {
      error = String(e);
    }
  }

  // ─── Projets ───

  const projects = $derived(store.s?.ssh_projects ?? []);
  const pid = (p: SshProject) => `${p.host}:${p.path}`;
  let draftProject = $state({ name: "", host: "", path: "" });

  $effect(() => {
    if (aliases.length && !aliases.includes(draftProject.host)) draftProject.host = aliases[0];
  });

  async function openProject(p: SshProject, vscode = false) {
    try {
      await api.sshOpenProject(p, vscode);
    } catch (e) {
      error = String(e);
    }
  }

  function addProject() {
    if (!store.s) return;
    const host = draftProject.host || aliases[0];
    const path = draftProject.path.trim().replace(/(.)\/+$/, "$1");
    if (!host || !path.startsWith("/")) {
      error = "Projet incomplet : il faut un serveur et un chemin absolu sur le serveur (« /root/projet »).";
      return;
    }
    // Sans nom, celui du dossier : « /root/citesco » → « citesco ».
    const name = draftProject.name.trim() || path.split("/").filter(Boolean).pop() || host;
    const p = { name, host, path };
    if (projects.some((x) => pid(x) === pid(p))) {
      error = "Ce projet existe déjà.";
      return;
    }
    store.s.ssh_projects = [...projects, p];
    saveSettings(0);
    draftProject = { name: "", host, path: "" };
    error = "";
  }

  function removeProject(p: SshProject) {
    if (!store.s) return;
    store.s.ssh_projects = projects.filter((x) => pid(x) !== pid(p));
    saveSettings(0);
  }

  // ─── Tunnels ───

  const tunnels = $derived(store.s?.ssh_tunnels ?? []);
  const tid = (t: SshTunnel) => `${t.host}:${t.local_port}:${t.remote_host}:${t.remote_port}`;
  let running = $state<string[]>([]);
  let pending = $state<string | null>(null);
  let draftTunnel = $state({ host: "", remote: "", remoteHost: "", local: "" });

  // Le serveur proposé par défaut : le premier, tant que rien d'autre n'est choisi.
  $effect(() => {
    if (aliases.length && !aliases.includes(draftTunnel.host)) draftTunnel.host = aliases[0];
  });

  async function refreshTunnels() {
    running = await api.sshTunnelsRunning().catch(() => []);
  }

  async function toggleTunnel(t: SshTunnel, on: boolean) {
    const id = tid(t);
    error = "";
    if (on) {
      pending = id;
      try {
        await api.sshTunnelStart(t);
      } catch (e) {
        error = `Tunnel vers ${t.host} : ${e}`;
      }
      pending = null;
    } else {
      await api.sshTunnelStop(id).catch(() => {});
    }
    await refreshTunnels();
  }

  const validPort = (n: number) => Number.isInteger(n) && n > 0 && n < 65536;

  function addTunnel() {
    if (!store.s) return;
    const remote_port = Number(draftTunnel.remote);
    const local_port = draftTunnel.local.trim() ? Number(draftTunnel.local) : remote_port;
    const host = draftTunnel.host || aliases[0];
    if (!host || !validPort(remote_port) || !validPort(local_port)) {
      error = "Tunnel incomplet : il faut un serveur et un port entre 1 et 65535.";
      return;
    }
    const t = { host, local_port, remote_host: draftTunnel.remoteHost.trim() || "localhost", remote_port };
    if (tunnels.some((x) => tid(x) === tid(t))) {
      error = "Ce tunnel existe déjà.";
      return;
    }
    store.s.ssh_tunnels = [...tunnels, t];
    saveSettings(0);
    draftTunnel = { host, remote: "", remoteHost: "", local: "" };
    error = "";
  }

  async function removeTunnel(t: SshTunnel) {
    if (!store.s) return;
    await api.sshTunnelStop(tid(t)).catch(() => {});
    store.s.ssh_tunnels = tunnels.filter((x) => tid(x) !== tid(t));
    saveSettings(0);
    refreshTunnels();
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
      {#if d.index !== null && ssh?.blocks[d.index].kind === "host"}
        {@const alias = ssh.blocks[d.index].patterns.split(/\s+/).find(plain)}
        {#if alias}
          <button
            class="btn ghost small-btn"
            type="button"
            onclick={() => forget(alias)}
            title="Serveur réinstallé ? SSH refuse de s'y connecter tant que l'ancienne empreinte est dans known_hosts"
          >
            <Icon name="shield" size={13} /> Oublier l'empreinte
          </button>
        {/if}
      {/if}
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

  <h2>Projets <span class="muted small">· {projects.length}</span></h2>
  <div class="card list">
    {#each projects as p (pid(p))}
      {@const known = aliases.includes(p.host)}
      <div class="srow">
        <div class="sname">
          <span class="strong">{p.name}</span>
          {#if !known}<span class="badge">serveur introuvable</span>{/if}
        </div>
        <div class="starget small muted mono" title={`${p.host}:${p.path}`}>{p.host}:{p.path}</div>
        <div class="sact">
          {#if known}
            <button class="btn ghost small-btn connect" onclick={() => openProject(p)} title={`Ouvre un terminal sur ${p.host}, dans ${p.path}`}>
              <Icon name="terminal" size={14} /> Terminal
            </button>
            {#if ssh.vscode}
              <button class="btn ghost small-btn connect" onclick={() => openProject(p, true)} title={`Ouvre ${p.path} dans VS Code, connecté à ${p.host}`}>
                <Icon name="code" size={14} /> VS Code
              </button>
            {/if}
          {/if}
          <button class="btn ghost del" title="Retirer" onclick={() => removeProject(p)}><Icon name="trash" size={14} /></button>
        </div>
      </div>
    {/each}
    {#if aliases.length}
      <form
        class="tform"
        onsubmit={(e) => {
          e.preventDefault();
          addProject();
        }}
      >
        <select class="field pick" bind:value={draftProject.host}>
          {#each aliases as a}<option value={a}>{a}</option>{/each}
        </select>
        <input class="field mono grow" bind:value={draftProject.path} placeholder="Dossier sur le serveur (/root/projet)" spellcheck="false" />
        <input class="field tname" bind:value={draftProject.name} placeholder="Nom (celui du dossier)" spellcheck="false" />
        <button class="btn" type="submit" disabled={!draftProject.path.trim()}><Icon name="plus" size={14} /> Ajouter</button>
      </form>
    {:else}
      <div class="empty muted">Ajoute d'abord un serveur : un projet est un dossier sur lui.</div>
    {/if}
  </div>
  <div class="small muted note">
    Un projet s'ouvre directement dans son dossier, depuis ici ou la palette : une seule connexion, donc une seule phrase secrète.
  </div>

  <div class="khead">
    <h2>Clés <span class="muted small">· {ssh.keys.length}</span></h2>
    <span class="grow"></span>
    <button class="btn ghost small-btn" onclick={newKey}><Icon name="plus" size={14} /> Nouvelle clé</button>
    <button class="btn ghost small-btn" onclick={() => api.openWith("explorer", ssh!.dir).catch((e) => (error = String(e)))}>
      <Icon name="folder" size={14} /> Ouvrir le dossier
    </button>
  </div>
  {#if keyForm}
    <form
      class="card new editor"
      onsubmit={(e) => {
        e.preventDefault();
        createKey();
      }}
    >
      <div class="etitle strong">Nouvelle clé (ED25519)</div>
      <div class="grid">
        <label>
          <span class="small muted">Nom du fichier</span>
          <!-- svelte-ignore a11y_autofocus -->
          <input class="field mono" bind:value={keyForm.name} placeholder="id_ed25519" spellcheck="false" autofocus />
        </label>
        <label>
          <span class="small muted">Commentaire (pour la reconnaître sur les serveurs)</span>
          <input class="field mono" bind:value={keyForm.comment} placeholder="moi@mon-pc" spellcheck="false" />
        </label>
      </div>
      <div class="ebar">
        <Toggle checked={keyForm.passphrase} label="Protéger par une phrase secrète" onchange={(v) => keyForm && (keyForm.passphrase = v)} />
        <span class="small muted grow">
          {keyForm.passphrase
            ? "Phrase secrète : tu la choisis dans un terminal, Kiosky ne la voit pas."
            : "Sans phrase secrète : utilisable sans saisie (tunnels, scripts)."}
        </span>
        <button class="btn ghost" type="button" onclick={() => (keyForm = null)}>Annuler</button>
        <button class="btn primary" type="submit" disabled={!keyForm.name.trim()}>Créer</button>
      </div>
    </form>
  {/if}
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
          {#if k.public && sending === k.name}
            <select class="field pick" bind:value={sendTo}>
              {#each aliases as a}<option value={a}>{a}</option>{/each}
            </select>
            <button class="btn primary small-btn" onclick={() => send(k.name)}>Envoyer</button>
            <button class="btn ghost icon" title="Annuler" onclick={() => (sending = null)}><Icon name="x" size={14} /></button>
          {:else if k.public}
            {#if aliases.length}
              <button
                class="btn ghost small-btn"
                title="Ajoute cette clé aux clés autorisées d'un serveur : plus de mot de passe ensuite"
                onclick={() => {
                  sending = k.name;
                  sendTo = aliases.includes(sendTo) ? sendTo : aliases[0];
                }}
              >
                <Icon name="send" size={14} /> Envoyer sur un serveur
              </button>
            {/if}
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

  <h2>Tunnels <span class="muted small">· {tunnels.length}</span></h2>
  <div class="card list">
    {#each tunnels as t (tid(t))}
      {@const id = tid(t)}
      {@const on = running.includes(id)}
      <div class="srow">
        <Toggle checked={on || pending === id} disabled={pending !== null} label={`Tunnel vers ${t.host}`} onchange={(v) => toggleTunnel(t, v)} />
        <div class="sname">
          <button class="link strong mono" title="Copier" onclick={() => copy(`localhost:${t.local_port}`, `localhost:${t.local_port}`)}>
            localhost:{t.local_port}
          </button>
          {#if pending === id}<span class="badge">connexion…</span>{:else if on}<span class="badge ok">ouvert</span>{/if}
        </div>
        <div class="starget small muted mono" title={`${t.remote_host}:${t.remote_port} vu depuis ${t.host}`}>
          → {t.host} → {t.remote_host}:{t.remote_port}
        </div>
        <div class="sact">
          {#if on}
            <button class="btn ghost small-btn connect" onclick={() => api.openUrl(`http://localhost:${t.local_port}`).catch((e) => (error = String(e)))}>
              <Icon name="globe" size={13} /> Ouvrir
            </button>
          {/if}
          <button class="btn ghost del" title="Supprimer" onclick={() => removeTunnel(t)}><Icon name="trash" size={14} /></button>
        </div>
      </div>
    {/each}
    {#if aliases.length}
      <form
        class="tform"
        onsubmit={(e) => {
          e.preventDefault();
          addTunnel();
        }}
      >
        <select class="field pick" bind:value={draftTunnel.host}>
          {#each aliases as a}<option value={a}>{a}</option>{/each}
        </select>
        <input class="field mono tport" bind:value={draftTunnel.remote} placeholder="Port distant" inputmode="numeric" spellcheck="false" />
        <input class="field mono grow" bind:value={draftTunnel.remoteHost} placeholder="Adresse vue du serveur (localhost)" spellcheck="false" />
        <input class="field mono tport" bind:value={draftTunnel.local} placeholder="Port local" inputmode="numeric" spellcheck="false" />
        <button class="btn" type="submit" disabled={!draftTunnel.remote.trim()}><Icon name="plus" size={14} /> Ajouter</button>
      </form>
    {:else}
      <div class="empty muted">Ajoute d'abord un serveur : un tunnel passe par lui.</div>
    {/if}
  </div>
  <div class="small muted note">
    Un tunnel amène un port du serveur sur ce PC : une base de données ou un site qui n'écoute que là-bas devient joignable sur
    <span class="mono">localhost</span>. Il tourne en arrière-plan, donc avec une clé utilisable sans saisie, et se ferme avec Kiosky.
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

  .pick {
    height: 28px;
    padding: 0 8px;
    font-size: 12.5px;
  }
  .link {
    padding: 1px 5px;
    margin: 0 -5px;
    border: none;
    border-radius: 5px;
    background: transparent;
    font-size: 13px;
    cursor: pointer;
  }
  .link:hover {
    background: var(--fill-press);
    color: var(--accent);
  }
  .tform {
    display: flex;
    gap: 6px;
    padding: 10px 12px;
  }
  .tform .field {
    height: 32px;
    min-width: 0;
    font-size: 12.5px;
  }
  .tform .pick {
    flex: none;
    max-width: 180px;
  }
  .tport {
    flex: none;
    width: 110px;
  }
  .tname {
    flex: none;
    width: 190px;
  }
  .note {
    margin: 8px 4px 0;
    line-height: 1.5;
  }
  form.new {
    margin-bottom: 12px;
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
