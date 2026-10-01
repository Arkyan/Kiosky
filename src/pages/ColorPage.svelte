<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import { api, copyText, contrastOn, formatColor, type ColorFormat } from "../lib/api";
  import { store, loadSettings, saveSettings } from "../lib/settings.svelte";

  const s = $derived(store.s!);
  const formats: { id: ColorFormat; label: string }[] = [
    { id: "hex", label: "HEX" },
    { id: "rgb", label: "RGB" },
    { id: "hsl", label: "HSL" },
  ];

  let selected = $state<string | null>(null);
  let copied = $state("");
  const current = $derived(selected && s.colors.includes(selected) ? selected : s.colors[0]);

  onMount(() => {
    // La pipette enregistre la couleur côté Rust : on recharge l'historique.
    const un = listen<{ hex: string }>("color-picked", async (e) => {
      await loadSettings();
      selected = e.payload.hex;
    });
    return () => un.then((f) => f());
  });

  async function copy(text: string) {
    await copyText(text);
    copied = text;
    setTimeout(() => copied === text && (copied = ""), 1200);
  }

  function setFormat(f: ColorFormat) {
    s.color_format = f;
    saveSettings(0);
  }

  function remove(hex: string) {
    s.colors = s.colors.filter((c) => c !== hex);
    saveSettings(0);
  }

  function clear() {
    s.colors = [];
    saveSettings(0);
  }
</script>

<PageHeader title="Pipette" subtitle="Prends la couleur de n'importe quel pixel de l'écran.">
  {#snippet actions()}
    <button class="btn primary" onclick={() => api.pickColor()}>
      <Icon name="pipette" size={16} /> Prendre une couleur
    </button>
  {/snippet}
</PageHeader>

<div class="card intro">
  <div class="how">
    <div class="step">
      <span class="kbds">
        {#each s.picker_shortcut.split("+") as k, i}
          {#if i > 0}<span class="plus">+</span>{/if}<kbd>{k === "Super" ? "Win" : k}</kbd>
        {/each}
      </span>
      <span class="small muted">depuis n'importe où</span>
    </div>
    <div class="step"><kbd>Clic</kbd><span class="small muted">copie la couleur</span></div>
    <div class="step"><kbd>Molette</kbd><span class="small muted">zoom de la loupe</span></div>
    <div class="step"><kbd>Échap</kbd><span class="small muted">ou clic droit pour annuler</span></div>
  </div>
  <div class="fmt">
    <span class="small muted">Format copié</span>
    <div class="seg-ctrl">
      {#each formats as f}
        <button class:active={s.color_format === f.id} onclick={() => setFormat(f.id)}>{f.label}</button>
      {/each}
    </div>
  </div>
</div>

{#if current}
  <div class="detail card">
    <div class="big-swatch" style:background={current} style:color={contrastOn(current)}>
      <span class="mono">{current}</span>
    </div>
    <div class="values">
      {#each formats as f}
        {@const v = formatColor(current, f.id)}
        <button class="value" onclick={() => copy(v)} title="Copier">
          <span class="vlabel">{f.label}</span>
          <span class="mono">{v}</span>
          <span class="vicon"><Icon name={copied === v ? "check" : "copy"} size={14} /></span>
        </button>
      {/each}
    </div>
  </div>

  <div class="head">
    <h2>Historique <span class="muted small">· {s.colors.length}</span></h2>
    <button class="btn ghost" onclick={clear}><Icon name="trash" size={15} /> Tout effacer</button>
  </div>
  <div class="swatches">
    {#each s.colors as c (c)}
      <div class="sw" class:sel={c === current}>
        <button
          class="chip-color"
          style:background={c}
          title={`${formatColor(c, s.color_format)} (clic : copier)`}
          onclick={() => {
            selected = c;
            copy(formatColor(c, s.color_format));
          }}
        >
          {#if copied === formatColor(c, s.color_format)}
            <span style:color={contrastOn(c)}><Icon name="check" size={16} /></span>
          {/if}
        </button>
        <span class="mono small">{c}</span>
        <button class="rm" title="Retirer" onclick={() => remove(c)}><Icon name="x" size={11} /></button>
      </div>
    {/each}
  </div>
{:else}
  <div class="card empty">
    <Icon name="pipette" size={28} />
    <p>Aucune couleur pour l'instant.<br />Clique sur « Prendre une couleur » ou utilise le raccourci.</p>
  </div>
{/if}

<style>
  .intro {
    display: flex;
    align-items: center;
    gap: 20px;
    padding: 16px 18px;
  }
  .how {
    flex: 1;
    display: flex;
    flex-wrap: wrap;
    gap: 10px 22px;
  }
  .step {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .kbds {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .plus {
    color: var(--text-3);
    font-size: 11px;
  }
  .fmt {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 6px;
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

  .detail {
    display: flex;
    gap: 16px;
    margin-top: 8px;
    padding: 12px;
  }
  .big-swatch {
    display: flex;
    align-items: flex-end;
    width: 180px;
    min-height: 118px;
    padding: 10px 12px;
    border-radius: 9px;
    box-shadow: inset 0 0 0 1px rgba(128, 128, 128, 0.25);
    font-weight: 600;
    transition: background 0.2s;
  }
  .values {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    justify-content: center;
  }
  .value {
    display: flex;
    align-items: center;
    gap: 14px;
    height: 34px;
    padding: 0 12px;
    border: none;
    border-radius: 7px;
    background: transparent;
    text-align: left;
    cursor: pointer;
  }
  .value:hover {
    background: var(--fill-hover);
  }
  .vlabel {
    width: 34px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-3);
  }
  .vicon {
    margin-left: auto;
    display: flex;
    color: var(--text-3);
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 22px 0 10px;
  }
  h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  .swatches {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(92px, 1fr));
    gap: 10px;
  }
  .sw {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    animation: enter 0.2s ease-out both;
  }
  .chip-color {
    display: grid;
    place-items: center;
    width: 100%;
    height: 56px;
    border: none;
    border-radius: 9px;
    box-shadow: inset 0 0 0 1px rgba(128, 128, 128, 0.25);
    cursor: pointer;
    transition: transform 0.1s;
  }
  .chip-color:hover {
    transform: translateY(-1px);
  }
  .sw.sel .chip-color {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .rm {
    position: absolute;
    top: -6px;
    right: -6px;
    display: none;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 1px solid var(--stroke-strong);
    background: var(--card);
    backdrop-filter: blur(10px);
    color: var(--text-2);
    cursor: pointer;
  }
  .sw:hover .rm {
    display: grid;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
    padding: 36px;
    text-align: center;
    color: var(--text-2);
  }
</style>
