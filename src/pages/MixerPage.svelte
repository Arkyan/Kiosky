<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import { api, type MixerState } from "../lib/api";
  import { store, saveSettings } from "../lib/settings.svelte";

  let mixer = $state<MixerState | null>(null);
  let error = $state("");
  let presetName = $state("");
  let naming = $state(false);
  let applied = $state("");

  // Clés en cours de glissement : on ne les écrase pas pendant le rafraîchissement.
  const dragging = new Set<string>();

  const presets = $derived(store.s?.presets ?? []);

  async function refresh() {
    try {
      const next = await api.getMixer();
      if (mixer) {
        for (const a of next.apps) {
          if (dragging.has(a.key)) a.volume = mixer.apps.find((x) => x.key === a.key)?.volume ?? a.volume;
        }
        if (dragging.has("__master")) next.master = mixer.master;
      }
      mixer = next;
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  onMount(() => {
    refresh();
    const t = setInterval(() => {
      if (!document.hidden) refresh();
    }, 1500);
    return () => clearInterval(t);
  });

  const pct = (v: number) => Math.round(v * 100);

  function hue(key: string) {
    let h = 0;
    for (const c of key) h = (h * 31 + c.charCodeAt(0)) % 360;
    return h;
  }

  function savePreset() {
    const name = presetName.trim();
    if (!name || !mixer || !store.s) return;
    const rules = mixer.apps.map((a) => ({ key: a.key, volume: a.volume, muted: a.muted }));
    const existing = store.s.presets.findIndex((p) => p.name === name);
    if (existing >= 0) store.s.presets[existing].rules = rules;
    else store.s.presets.push({ name, rules });
    saveSettings(0);
    presetName = "";
    naming = false;
  }

  async function apply(name: string) {
    try {
      await api.applyPreset(name);
      applied = name;
      setTimeout(() => (applied = ""), 1400);
      refresh();
    } catch (e) {
      error = String(e);
    }
  }

  function removePreset(name: string) {
    if (!store.s) return;
    store.s.presets = store.s.presets.filter((p) => p.name !== name);
    saveSettings(0);
  }
</script>

<PageHeader title="Volume" subtitle="Le volume de chaque application, et des préréglages en un clic.">
  {#snippet actions()}
    <button class="btn" onclick={refresh}><Icon name="refresh" size={16} /> Actualiser</button>
  {/snippet}
</PageHeader>

{#if error}
  <div class="banner error">{error}</div>
{/if}

<!-- Préréglages -->
<section class="presets">
  {#each presets as p (p.name)}
    <div class="preset" class:flash={applied === p.name}>
      <button class="apply" onclick={() => apply(p.name)} title="Appliquer">
        <Icon name={applied === p.name ? "check" : "bolt"} size={15} />
        {p.name}
        <span class="count">{p.rules.length}</span>
      </button>
      <button class="rm" title="Supprimer" onclick={() => removePreset(p.name)}><Icon name="x" size={12} /></button>
    </div>
  {/each}

  {#if naming}
    <form
      class="naming"
      onsubmit={(e) => {
        e.preventDefault();
        savePreset();
      }}
    >
      <!-- svelte-ignore a11y_autofocus -->
      <input class="field" bind:value={presetName} placeholder="Jeu, Réunion, Musique…" autofocus />
      <button class="btn primary" type="submit" disabled={!presetName.trim()}>Enregistrer</button>
      <button class="btn ghost" type="button" onclick={() => (naming = false)}>Annuler</button>
    </form>
  {:else}
    <button class="chip add" onclick={() => (naming = true)} disabled={!mixer?.apps.length}>
      <Icon name="save" size={14} /> Enregistrer l'état actuel
    </button>
  {/if}
</section>

{#if mixer}
  <!-- Volume général -->
  <div class="card master">
    <button
      class="btn ghost icon big"
      title={mixer.master_muted ? "Rétablir le son" : "Couper le son"}
      onclick={async () => {
        if (!mixer) return;
        mixer.master_muted = !mixer.master_muted;
        await api.setMasterMute(mixer.master_muted);
      }}
    >
      <Icon name={mixer.master_muted ? "mute" : "volume"} size={22} />
    </button>
    <div class="grow">
      <div class="label"><span class="strong">Volume général</span><span class="pct">{pct(mixer.master)}</span></div>
      <input
        type="range"
        min="0"
        max="100"
        value={pct(mixer.master)}
        style:--p={`${pct(mixer.master)}%`}
        class:dim={mixer.master_muted}
        onpointerdown={() => dragging.add("__master")}
        onpointerup={() => dragging.delete("__master")}
        onchange={() => dragging.delete("__master")}
        oninput={(e) => {
          if (!mixer) return;
          mixer.master = +e.currentTarget.value / 100;
          api.setMasterVolume(mixer.master);
        }}
      />
    </div>
  </div>

  <h2>Applications <span class="muted small">· {mixer.apps.length}</span></h2>

  <div class="apps">
    {#each mixer.apps as a (a.key)}
      <div class="card app" class:inactive={!a.active}>
        <div class="avatar" style:--h={hue(a.key)}>{a.name.charAt(0).toUpperCase()}</div>
        <div class="grow">
          <div class="label">
            <span class="strong name">{a.name}</span>
            {#if a.active}<span class="live" title="Joue du son"><i></i><i></i><i></i></span>{/if}
            <span class="pct">{a.muted ? "Muet" : pct(a.volume)}</span>
          </div>
          <input
            type="range"
            min="0"
            max="100"
            value={pct(a.volume)}
            style:--p={`${pct(a.volume)}%`}
            class:dim={a.muted}
            onpointerdown={() => dragging.add(a.key)}
            onpointerup={() => dragging.delete(a.key)}
            onchange={() => dragging.delete(a.key)}
            oninput={(e) => {
              a.volume = +e.currentTarget.value / 100;
              api.setAppVolume(a.key, a.volume);
            }}
          />
        </div>
        <button
          class="btn ghost icon"
          class:on={a.muted}
          title={a.muted ? "Rétablir" : "Couper"}
          onclick={async () => {
            a.muted = !a.muted;
            await api.setAppMute(a.key, a.muted);
          }}
        >
          <Icon name={a.muted ? "mute" : "volume"} size={18} />
        </button>
      </div>
    {:else}
      <div class="card empty">
        <Icon name="volume" size={28} />
        <p>Aucune application n'utilise le son pour l'instant.<br />Lance une musique ou une vidéo et elle apparaîtra ici.</p>
      </div>
    {/each}
  </div>
{:else if !error}
  <div class="card skeleton"></div>
{/if}

<style>
  .presets {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-bottom: 18px;
  }
  .preset {
    display: inline-flex;
    align-items: center;
    border-radius: 999px;
    border: 1px solid var(--stroke);
    background: var(--card);
    box-shadow: var(--shadow);
    overflow: hidden;
    transition: border-color 0.2s;
  }
  .preset.flash {
    border-color: var(--accent);
  }
  .apply {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 6px 0 14px;
    border: none;
    background: transparent;
    font-weight: 600;
    cursor: pointer;
  }
  .apply:hover,
  .rm:hover {
    background: var(--fill-hover);
  }
  .apply :global(svg) {
    color: var(--accent);
  }
  .count {
    font-size: 11px;
    font-weight: 500;
    color: var(--text-3);
  }
  .rm {
    display: grid;
    place-items: center;
    width: 28px;
    height: 32px;
    border: none;
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
  }
  .add {
    height: 32px;
  }
  .add:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .naming {
    display: flex;
    gap: 6px;
  }
  .naming .field {
    width: 200px;
  }

  .master {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 18px 20px;
  }
  .big {
    width: 44px;
    height: 44px;
    border-radius: 10px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .label {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 2px;
  }
  .strong {
    font-weight: 600;
  }
  .name {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .pct {
    margin-left: auto;
    min-width: 34px;
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
  }
  input.dim {
    opacity: 0.4;
  }

  h2 {
    margin: 26px 0 12px;
    font-size: 14px;
    font-weight: 600;
  }

  .apps {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(330px, 1fr));
    gap: 8px;
  }
  .app {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 12px 12px 14px;
    animation: enter 0.2s ease-out both;
    transition: opacity 0.2s;
  }
  .app.inactive {
    opacity: 0.75;
  }
  .avatar {
    display: grid;
    place-items: center;
    flex: none;
    width: 36px;
    height: 36px;
    border-radius: 9px;
    background: linear-gradient(150deg, hsl(var(--h) 70% 55%), hsl(calc(var(--h) + 40) 70% 42%));
    color: white;
    font-family: var(--font-display);
    font-weight: 700;
    font-size: 16px;
  }
  .btn.on {
    color: var(--bad);
  }

  /* Petit égaliseur animé quand l'app joue du son */
  .live {
    display: inline-flex;
    align-items: flex-end;
    gap: 2px;
    height: 10px;
  }
  .live i {
    width: 2px;
    height: 100%;
    border-radius: 1px;
    background: var(--accent);
    animation: eq 0.9s ease-in-out infinite;
  }
  .live i:nth-child(2) {
    animation-delay: -0.3s;
  }
  .live i:nth-child(3) {
    animation-delay: -0.6s;
  }
  @keyframes eq {
    0%,
    100% {
      transform: scaleY(0.3);
    }
    50% {
      transform: scaleY(1);
    }
  }
  .live i {
    transform-origin: bottom;
  }

  .empty {
    grid-column: 1 / -1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 36px;
    text-align: center;
    color: var(--text-2);
  }
  .skeleton {
    height: 84px;
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.5;
    }
  }
</style>
