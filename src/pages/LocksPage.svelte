<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import { api, type Locker, type LockReport } from "../lib/api";

  let path = $state("");
  let report = $state<LockReport | null>(null);
  let checking = $state(false);
  let hovering = $state(false);
  let error = $state("");
  let info = $state("");
  let confirmPid = $state<number | null>(null);
  let confirmTimer: ReturnType<typeof setTimeout> | undefined;

  async function check(target = path) {
    target = target.trim();
    if (!target) return;
    path = target;
    checking = true;
    try {
      report = await api.checkLocks(target);
      error = "";
    } catch (e) {
      report = null;
      error = String(e);
    }
    checking = false;
  }

  async function pick(folder: boolean) {
    try {
      const chosen = await api.pickLockTarget(folder);
      if (chosen) await check(chosen);
    } catch (e) {
      error = String(e);
    }
  }

  onMount(() => {
    // Un fichier déposé depuis l'Explorateur arrive avec son chemin complet.
    const un = getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "enter" || p.type === "over") hovering = true;
      else hovering = false;
      if (p.type === "drop" && p.paths.length) check(p.paths[0]);
    });
    return () => un.then((f) => f());
  });

  const KINDS: Record<Locker["kind"], string> = {
    app: "",
    service: "Service",
    explorer: "Explorateur",
    console: "Console",
    critical: "Critique",
  };

  const name = $derived(report ? (report.path.split(/[\\/]/).filter(Boolean).pop() ?? report.path) : "");

  async function kill(l: Locker) {
    if (confirmPid !== l.pid) {
      confirmPid = l.pid;
      clearTimeout(confirmTimer);
      confirmTimer = setTimeout(() => (confirmPid = null), 3500);
      return;
    }
    confirmPid = null;
    try {
      await api.killProcess(l.pid);
      info = `${l.name} (PID ${l.pid}) arrêté.`;
      setTimeout(() => (info = ""), 3000);
      error = "";
      // Windows met un instant à relâcher les fichiers d'un processus arrêté.
      await new Promise((r) => setTimeout(r, 300));
      await check();
    } catch (e) {
      error = String(e);
    }
  }
</script>

<PageHeader title="Fichiers bloqués" subtitle="« Ce fichier est utilisé par un autre programme » : lequel, et un bouton pour l'arrêter." />

{#if error}
  <div class="banner error">{error}</div>
{:else if info}
  <div class="banner"><Icon name="check" size={16} /> {info}</div>
{/if}

<div class="card drop" class:hovering>
  <div class="hint">
    <Icon name="lock" size={22} />
    <div>
      <div class="strong">Dépose ici un fichier ou un dossier</div>
      <div class="small muted">ou choisis-le, ou colle son chemin</div>
    </div>
    <span class="grow"></span>
    <button class="btn" onclick={() => pick(false)}>Choisir un fichier</button>
    <button class="btn" onclick={() => pick(true)}><Icon name="folder" size={14} /> Choisir un dossier</button>
  </div>
  <form
    class="row"
    onsubmit={(e) => {
      e.preventDefault();
      check();
    }}
  >
    <input class="field mono grow" bind:value={path} placeholder="C:\projets\app\data.db" spellcheck="false" />
    <button class="btn primary" type="submit" disabled={checking || !path.trim()}>
      <span class:spin={checking} style="display:flex"><Icon name={checking ? "refresh" : "search"} size={14} /></span>
      {report ? "Revérifier" : "Vérifier"}
    </button>
  </form>
</div>

{#if report}
  <div class="verdict" class:busy={report.lockers.length > 0}>
    <span class="dot"></span>
    {#if report.lockers.length}
      <span><b>{name}</b> est utilisé par {report.lockers.length} programme{report.lockers.length > 1 ? "s" : ""}.</span>
    {:else}
      <span><b>{name}</b> est libre : aucun programme ne {report.is_dir ? "tient ses fichiers" : "l'utilise"}.</span>
    {/if}
    {#if report.is_dir}
      <span class="muted small">
        {report.files.toLocaleString("fr-FR")} fichier{report.files > 1 ? "s" : ""} examiné{report.files > 1 ? "s" : ""}{report.truncated
          ? " (les premiers seulement : le dossier en contient davantage)"
          : ""}
      </span>
    {/if}
  </div>

  {#if report.lockers.length}
    <div class="card table">
      {#each report.lockers as l (l.pid)}
        <div class="locker">
          <div class="lrow">
            <div class="proc" title={l.path || l.name}>
              <span class="pname">{l.app && l.app !== l.name ? l.app : l.name.replace(/\.exe$/i, "")}</span>
              {#if l.system}<span class="badge win">Windows</span>{/if}
              {#if KINDS[l.kind]}<span class="badge">{KINDS[l.kind]}</span>{/if}
              <span class="pid small mono">{l.name} · {l.pid}</span>
            </div>
            <button class="btn ghost kill" class:confirm={confirmPid === l.pid} onclick={() => kill(l)}>
              <Icon name="stop" size={13} />
              {confirmPid === l.pid ? (l.system ? "Arrêter ce processus Windows ?" : "Confirmer") : "Arrêter"}
            </button>
          </div>
          {#if l.files.length}
            <div class="files small mono muted">
              {#each l.files.slice(0, 6) as f}<div title={f}>{f}</div>{/each}
              {#if l.files.length > 6}<div>… et {l.files.length - 6} autre{l.files.length > 7 ? "s" : ""}</div>{/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
    <div class="small muted note">
      Arrêter un programme lui fait perdre ce qui n'était pas enregistré : ferme-le normalement si tu le peux.
    </div>
  {:else if report.is_dir}
    <div class="small muted note">
      Le dossier résiste quand même ? Un terminal ou une fenêtre de l'Explorateur simplement ouverts dedans le retiennent
      sans tenir de fichier : ils n'apparaissent pas ici.
    </div>
  {/if}
{/if}

<style>
  .banner {
    margin-bottom: 12px;
  }
  .strong {
    font-weight: 600;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .drop {
    padding: 14px;
    border-style: dashed;
    border-color: var(--stroke-strong);
    transition: border-color 0.12s, background 0.12s;
  }
  .drop.hovering {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .hint {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 4px 4px 14px 6px;
    color: var(--text-2);
  }
  .hint .strong {
    color: var(--text);
  }
  .row {
    display: flex;
    gap: 6px;
  }

  .verdict {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin: 18px 2px 10px;
    font-size: 13.5px;
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
  .locker {
    border-bottom: 1px solid var(--stroke);
  }
  .locker:last-child {
    border-bottom: none;
  }
  .lrow {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 46px;
    padding: 4px 8px 4px 18px;
  }
  .proc {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex: 1;
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
  }
  .badge {
    flex: none;
    align-self: center;
    height: 18px;
    font-size: 10.5px;
    font-weight: 500;
  }
  .kill {
    flex: none;
    height: 28px;
    padding: 0 10px;
    gap: 6px;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .kill:hover,
  .kill.confirm {
    color: var(--bad);
  }
  .kill.confirm {
    background: color-mix(in srgb, var(--bad) 12%, transparent);
    font-weight: 600;
  }
  .files {
    padding: 0 18px 10px;
    font-size: 11.5px;
    line-height: 1.6;
  }
  .files div {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .note {
    margin: 8px 4px 0;
    line-height: 1.5;
  }
</style>
