<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import Toggle from "../lib/Toggle.svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, fmtBytes, type UpdateInfo } from "../lib/api";
  import { store, saveSettings } from "../lib/settings.svelte";
  import { MODULES, PALETTE_SOURCES } from "../lib/modules";
  import { getVersion } from "@tauri-apps/api/app";

  let version = $state("");
  getVersion().then((v) => (version = v));

  const WIDGET_ITEMS: { id: string; label: string; icon: string }[] = [
    { id: "cpu", label: "Processeur", icon: "activity" },
    { id: "gpu", label: "Processeur graphique", icon: "activity" },
    { id: "ram", label: "Mémoire", icon: "activity" },
    { id: "net", label: "Réseau ↓↑", icon: "globe" },
    { id: "time", label: "Heure", icon: "clock" },
    { id: "date", label: "Date", icon: "clock" },
    { id: "battery", label: "Batterie", icon: "bolt" },
    { id: "ports", label: "Serveurs locaux", icon: "plug" },
    { id: "docker", label: "Conteneurs Docker", icon: "box" },
    { id: "media", label: "Musique en cours", icon: "volume" },
    { id: "volume", label: "Volume", icon: "volume" },
    { id: "mic", label: "Micro (voyant)", icon: "mic" },
  ];

  // Ordre stable de tous les éléments : cocher ou décocher ne déplace rien, seules les flèches le font.
  const widgetOrder = $derived(
    [...s.widget_order, ...WIDGET_ITEMS.map((w) => w.id).filter((id) => !s.widget_order.includes(id))]
      .map((id) => WIDGET_ITEMS.find((w) => w.id === id))
      .filter((w) => !!w) as { id: string; label: string; icon: string }[],
  );

  function toggleWidgetItem(id: string, on: boolean) {
    const cur = s.widget_items.filter((x) => x !== id);
    s.widget_items = on ? [...cur, id] : cur;
    saveSettings(0);
  }

  function moveWidgetItem(id: string, d: number) {
    const list = widgetOrder.map((w) => w.id);
    const i = list.indexOf(id);
    const j = i + d;
    if (j < 0 || j >= list.length) return;
    [list[i], list[j]] = [list[j], list[i]];
    s.widget_order = list;
    saveSettings(0);
  }

  function toggleIn(list: "disabled_modules" | "palette_disabled", id: string, enabled: boolean) {
    const cur = s[list].filter((x) => x !== id);
    s[list] = enabled ? cur : [...cur, id];
    saveSettings(0);
  }

  const s = $derived(store.s!);

  let autostart = $state(false);
  type ShortcutField = "palette_shortcut" | "picker_shortcut";
  let recording = $state<ShortcutField | null>(null);
  let preview = $state("");

  // ─── Mises à jour ───

  let update = $state<UpdateInfo | null>(null);
  /** "" (rien fait), "checking", "none" (à jour), "installing" */
  let updateStatus = $state<"" | "checking" | "none" | "installing">("");
  let updateError = $state("");
  let progress = $state<{ done: number; total: number | null } | null>(null);

  async function checkUpdate() {
    updateStatus = "checking";
    updateError = "";
    try {
      update = await api.checkUpdate();
      updateStatus = update ? "" : "none";
    } catch (e) {
      updateError = String(e);
      updateStatus = "";
    }
  }

  async function installUpdate() {
    updateStatus = "installing";
    updateError = "";
    progress = null;
    try {
      await api.installUpdate(); // l'installateur ferme puis relance Kiosky
    } catch (e) {
      updateError = String(e);
      updateStatus = "";
    }
  }

  onMount(() => {
    api.getAutostart().then((v) => (autostart = v));
    api.pendingUpdate().then((u) => (update = u));
    const unFound = listen<UpdateInfo | null>("update-available", (e) => (update = e.payload));
    const unProgress = listen<{ done: number; total: number | null }>("update-progress", (e) => (progress = e.payload));
    return () => {
      unFound.then((f) => f());
      unProgress.then((f) => f());
    };
  });

  async function setAutostart(v: boolean) {
    try {
      await api.setAutostart(v);
      autostart = v;
    } catch (e) {
      store.error = String(e);
    }
  }

  /** « KeyK » → « K », « Digit1 » → « 1 », « Space » → « Space » */
  function keyName(code: string): string | null {
    if (/^(Control|Shift|Alt|Meta|OS)/.test(code)) return null;
    if (code.startsWith("Key")) return code.slice(3);
    if (code.startsWith("Digit")) return code.slice(5);
    return code;
  }

  function onkeydown(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.code === "Escape") {
      recording = null;
      preview = "";
      return;
    }
    const mods: string[] = [];
    if (e.ctrlKey) mods.push("Ctrl");
    if (e.altKey) mods.push("Alt");
    if (e.shiftKey) mods.push("Shift");
    if (e.metaKey) mods.push("Super");
    const key = keyName(e.code);
    preview = [...mods, key ?? "…"].join(" + ");
    if (!key || mods.length === 0) return; // il faut au moins un modificateur
    s[recording] = [...mods, key].join("+");
    recording = null;
    preview = "";
    saveSettings(0);
  }
</script>

<svelte:window {onkeydown} />

{#snippet shortcutButton(field: ShortcutField)}
  <button
    class="shortcut"
    class:recording={recording === field}
    onclick={() => {
      recording = recording === field ? null : field;
      preview = "";
    }}
  >
    {#if recording === field}
      {preview || "En attente…"}
    {:else}
      {#each s[field].split("+") as k, i}
        {#if i > 0}<span class="plus">+</span>{/if}<kbd>{k === "Super" ? "Win" : k}</kbd>
      {/each}
    {/if}
  </button>
{/snippet}

<PageHeader title="Réglages" />

<div class="card group">
  <div class="row">
    <div class="ico"><Icon name="power" size={18} /></div>
    <div class="grow">
      <div class="strong">Lancer au démarrage de Windows</div>
      <div class="small muted">Kiosky démarre discrètement dans la zone de notification.</div>
    </div>
    <Toggle checked={autostart} label="Lancer au démarrage" onchange={setAutostart} />
  </div>

  <div class="row">
    <div class="ico"><Icon name="keyboard" size={18} /></div>
    <div class="grow">
      <div class="strong">Raccourci de la palette</div>
      <div class="small muted">
        {recording === "palette_shortcut"
          ? "Appuie sur la combinaison voulue (Échap pour annuler)…"
          : "Recherche d'applications, projets, dossiers et calculs. Astuce : Alt+Space, comme PowerToys Run."}
      </div>
    </div>
    {@render shortcutButton("palette_shortcut")}
  </div>

  <div class="row">
    <div class="ico"><Icon name="pipette" size={18} /></div>
    <div class="grow">
      <div class="strong">Raccourci de la pipette</div>
      <div class="small muted">
        {recording === "picker_shortcut"
          ? "Appuie sur la combinaison voulue (Échap pour annuler)…"
          : "Prend la couleur d'un pixel de l'écran et la copie."}
      </div>
    </div>
    {@render shortcutButton("picker_shortcut")}
  </div>
</div>

<h2>Barre flottante</h2>
<p class="small muted intro">Une petite barre toujours visible, posée sur la barre des tâches ou où tu veux. Double-clic dessus : ouvre le Moniteur.</p>
<div class="card group">
  <div class="row">
    <div class="ico"><Icon name="activity" size={18} /></div>
    <div class="grow">
      <div class="strong">Afficher la barre flottante</div>
      <div class="small muted">Elle se cache toute seule quand une application est en plein écran.</div>
    </div>
    <Toggle checked={s.widget_enabled} label="Afficher la barre flottante" onchange={(v) => { s.widget_enabled = v; saveSettings(0); }} />
  </div>
  <div class="wconf" class:disabled={!s.widget_enabled}>
    <div class="row">
      <div class="ico"><Icon name="folder" size={18} /></div>
      <div class="grow">
        <div class="strong">Position</div>
        <div class="small muted">Sur la barre des tâches, ou libre : déplaçable à la souris, place retenue.</div>
      </div>
      <div class="seg-ctrl">
        <button class:active={s.widget_mode === "taskbar-left"} onclick={() => { s.widget_mode = "taskbar-left"; saveSettings(0); }}>Barre des tâches ◧</button>
        <button class:active={s.widget_mode === "taskbar-right"} onclick={() => { s.widget_mode = "taskbar-right"; saveSettings(0); }}>Barre des tâches ◨</button>
        <button class:active={s.widget_mode === "free"} onclick={() => { s.widget_mode = "free"; saveSettings(0); }}>Libre</button>
      </div>
    </div>
    <div class="row" class:dim={s.widget_mode !== "free"}>
      <div class="ico"><Icon name="settings" size={18} /></div>
      <div class="grow">
        <div class="strong">Disposition verticale</div>
        <div class="small muted">En position libre seulement.</div>
      </div>
      <Toggle
        checked={s.widget_vertical}
        disabled={s.widget_mode !== "free"}
        label="Disposition verticale"
        onchange={(v) => { s.widget_vertical = v; saveSettings(0); }}
      />
    </div>
    <div class="witems">
      <div class="small muted whead">Éléments affichés, de gauche à droite</div>
      {#each widgetOrder as w, i (w.id)}
        {@const on = s.widget_items.includes(w.id)}
        <div class="witem" class:off={!on}>
          <Toggle checked={on} label={w.label} onchange={(v) => toggleWidgetItem(w.id, v)} />
          <Icon name={w.icon} size={15} />
          <span class="grow">
            {w.label}
            {#if w.id === "battery"}<span class="small muted"> · masquée sur un PC fixe</span>{/if}
            {#if w.id === "media"}<span class="small muted"> · Spotify, YouTube, VLC… avec pochette et ⏮ ⏯ ⏭</span>{/if}
            {#if w.id === "volume"}<span class="small muted"> · molette pour régler</span>{/if}
            {#if w.id === "mic"}<span class="small muted"> · rouge quand il est coupé, clic pour basculer</span>{/if}
          </span>
          <button class="mini" title="Plus à gauche" disabled={i === 0} onclick={() => moveWidgetItem(w.id, -1)}><Icon name="up_small" size={14} /></button>
          <button class="mini" title="Plus à droite" disabled={i === widgetOrder.length - 1} onclick={() => moveWidgetItem(w.id, 1)}><Icon name="down_small" size={14} /></button>
        </div>
      {/each}
    </div>
  </div>
</div>

<h2>Modules</h2>
<p class="small muted intro">Un module désactivé disparaît de la barre latérale et de la palette, et arrête ce qu'il fait en arrière-plan.</p>
<div class="card toggles">
  {#each MODULES.filter((m) => m.id !== "converter") as m (m.id)}
    {@const on = !s.disabled_modules.includes(m.id)}
    <div class="trow" class:off={!on}>
      <div class="tico"><Icon name={m.icon} size={16} /></div>
      <div class="grow">
        <div class="strong">{m.label}</div>
        <div class="small muted">{m.description}</div>
      </div>
      <Toggle checked={on} label={`Activer ${m.label}`} onchange={(v) => toggleIn("disabled_modules", m.id, v)} />
    </div>
  {/each}
</div>

<h2>Palette</h2>
<p class="small muted intro">Ce que la palette cherche quand tu tapes quelque chose.</p>
<div class="card toggles">
  {#each PALETTE_SOURCES as src (src.id)}
    {@const on = !s.palette_disabled.includes(src.id)}
    <div class="trow" class:off={!on}>
      <div class="tico"><Icon name={src.icon} size={16} /></div>
      <div class="grow">
        <div class="strong">{src.label}</div>
        <div class="small muted">{src.description}</div>
      </div>
      <Toggle checked={on} label={`Chercher : ${src.label}`} onchange={(v) => toggleIn("palette_disabled", src.id, v)} />
    </div>
  {/each}
</div>

<h2>Mises à jour</h2>
<div class="card group">
  <div class="row">
    <div class="ico"><Icon name="refresh" size={18} /></div>
    <div class="grow">
      {#if update}
        <div class="strong">Kiosky {update.version} est disponible</div>
        <div class="small muted">
          {#if updateStatus === "installing"}
            {progress
              ? `Téléchargement : ${fmtBytes(progress.done)}${progress.total ? ` sur ${fmtBytes(progress.total)}` : ""}`
              : "Téléchargement…"}
          {:else}
            Version installée : {version}. Kiosky se ferme, s'installe puis se relance.
          {/if}
        </div>
      {:else}
        <div class="strong">Kiosky {version}</div>
        <div class="small muted">
          {updateStatus === "checking" ? "Recherche en cours…" : updateStatus === "none" ? "Kiosky est à jour." : "Aucune recherche depuis l'ouverture de cette page."}
        </div>
      {/if}
      {#if updateError}<div class="small uerror">{updateError}</div>{/if}
    </div>
    {#if update}
      <button class="btn primary" disabled={updateStatus === "installing"} onclick={installUpdate}>
        {updateStatus === "installing" ? "Installation…" : "Installer"}
      </button>
    {:else}
      <button class="btn" disabled={updateStatus === "checking"} onclick={checkUpdate}>Rechercher</button>
    {/if}
  </div>
  {#if update?.notes}
    <div class="row notes small">{update.notes}</div>
  {/if}
  <div class="row">
    <div class="ico"><Icon name="clock" size={18} /></div>
    <div class="grow">
      <div class="strong">Rechercher automatiquement</div>
      <div class="small muted">Au lancement puis toutes les six heures. Rien n'est installé sans ton accord.</div>
    </div>
    <Toggle checked={s.update_check} label="Rechercher automatiquement les mises à jour" onchange={(v) => { s.update_check = v; saveSettings(0); }} />
  </div>
</div>

<h2>À propos</h2>
<div class="card group">
  <div class="row">
    <div class="ico logo"></div>
    <div class="grow">
      <div class="strong">Kiosky {version}</div>
      <div class="small muted">Rust + Tauri 2 + Svelte 5 · Réglages dans <span class="mono">%APPDATA%\com.kiosky.desktop</span></div>
    </div>
  </div>
  <div class="row">
    <div class="ico"><Icon name="sparkle" size={18} /></div>
    <div class="grow small muted">
      Fermer la fenêtre ne quitte pas l'application : elle reste dans la zone de notification. Clic droit sur l'icône → Quitter.
    </div>
  </div>
</div>

<style>
  .notes {
    white-space: pre-wrap;
    color: var(--text-2);
  }
  .uerror {
    margin-top: 4px;
    color: var(--bad);
  }
  .intro {
    margin: -6px 0 10px;
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
    padding: 0 10px;
    border: none;
    border-radius: 6px;
    background: transparent;
    font-size: 12.5px;
    color: var(--text-2);
    cursor: pointer;
    white-space: nowrap;
  }
  .seg-ctrl button.active {
    background: var(--card);
    color: var(--text);
    font-weight: 600;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12);
  }
  .wconf {
    transition: opacity 0.15s;
  }
  .wconf.disabled {
    opacity: 0.45;
    pointer-events: none;
  }
  .row.dim .grow,
  .row.dim .ico {
    opacity: 0.5;
  }
  .witems {
    display: flex;
    flex-direction: column;
    padding: 10px 18px 12px;
  }
  .whead {
    margin-bottom: 4px;
  }
  .witem {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 36px;
    color: var(--text-2);
  }
  .witem:not(.off) {
    color: var(--text);
  }
  .witem .grow {
    font-size: 13px;
  }
  .mini {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
  }
  .mini:hover:not(:disabled) {
    background: var(--fill-hover);
    color: var(--text);
  }
  .mini:disabled {
    opacity: 0.25;
    cursor: default;
  }
  .toggles {
    display: grid;
    grid-template-columns: 1fr 1fr;
    overflow: hidden;
  }
  .trow {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--stroke);
    transition: opacity 0.15s;
  }
  .trow:nth-child(odd) {
    border-right: 1px solid var(--stroke);
  }
  .trow.off .tico,
  .trow.off .grow {
    opacity: 0.5;
  }
  .tico {
    display: grid;
    place-items: center;
    flex: none;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    background: var(--fill-hover);
    color: var(--text-2);
  }
  .group {
    overflow: hidden;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 16px 18px;
    border-bottom: 1px solid var(--stroke);
  }
  .row:last-child {
    border-bottom: none;
  }
  .ico {
    display: grid;
    place-items: center;
    flex: none;
    width: 36px;
    height: 36px;
    border-radius: 9px;
    background: var(--fill-hover);
    color: var(--text-2);
  }
  .ico.logo {
    background: linear-gradient(160deg, #0078d4, #7c4dff);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .strong {
    font-weight: 600;
  }
  h2 {
    margin: 26px 0 12px;
    font-size: 14px;
    font-weight: 600;
  }
  .shortcut {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-width: 150px;
    height: 34px;
    justify-content: center;
    padding: 0 12px;
    border-radius: 8px;
    border: 1px solid var(--stroke-strong);
    background: var(--card);
    cursor: pointer;
    font-size: 13px;
  }
  .shortcut:hover {
    background: var(--fill-hover);
  }
  .shortcut.recording {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
    color: var(--accent);
    font-weight: 600;
  }
  .plus {
    color: var(--text-3);
    font-size: 11px;
  }
</style>
