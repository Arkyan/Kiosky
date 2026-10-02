<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import Icon from "./lib/Icon.svelte";
  import { contrastOn, type PickerFrame } from "./lib/api";

  // Loupe de la pipette : fenêtre sans bordure qui suit la souris (déplacée par Rust).
  const SIZE = 152;

  let canvas: HTMLCanvasElement;
  let hex = $state("");
  let pos = $state("");
  let copied = $state("");

  const toHex = (rgb: number) => "#" + rgb.toString(16).padStart(6, "0").toUpperCase();

  function draw(f: PickerFrame) {
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const cell = SIZE / f.grid;
    for (let i = 0; i < f.pixels.length; i++) {
      ctx.fillStyle = toHex(f.pixels[i]);
      // +1 : évite les fines lignes entre cellules quand la taille ne tombe pas juste
      ctx.fillRect(Math.floor((i % f.grid) * cell), Math.floor(Math.floor(i / f.grid) * cell), Math.ceil(cell) + 1, Math.ceil(cell) + 1);
    }
    // Pixel visé : double contour noir et blanc, visible sur tous les fonds
    const c = Math.floor(f.grid / 2) * cell;
    ctx.lineWidth = 1;
    ctx.strokeStyle = "#000";
    ctx.strokeRect(Math.round(c) - 0.5, Math.round(c) - 0.5, Math.round(cell) + 1, Math.round(cell) + 1);
    ctx.strokeStyle = "#fff";
    ctx.strokeRect(Math.round(c) + 0.5, Math.round(c) + 0.5, Math.round(cell) - 1, Math.round(cell) - 1);
  }

  onMount(() => {
    const unlisten = [
      listen<PickerFrame>("picker-frame", (e) => {
        copied = "";
        hex = toHex(e.payload.pixels[Math.floor(e.payload.pixels.length / 2)]);
        pos = `${e.payload.x}, ${e.payload.y}`;
        draw(e.payload);
      }),
      listen<{ hex: string; text: string }>("color-picked", (e) => (copied = e.payload.text)),
      listen("picker-hidden", () => {
        copied = "";
        hex = "";
        canvas.getContext("2d")?.clearRect(0, 0, SIZE, SIZE);
      }),
    ];
    return () => unlisten.forEach((p) => p.then((f) => f()));
  });
</script>

<div class="loupe" class:hidden={!hex}>
  <canvas bind:this={canvas} width={SIZE} height={SIZE}></canvas>
  {#if copied}
    <div class="info done" style:background={hex} style:color={contrastOn(hex)}>
      <Icon name="check" size={14} /> Copié
    </div>
  {:else}
    <div class="info">
      <span class="swatch" style:background={hex}></span>
      <span class="hex">{hex}</span>
      <span class="pos">{pos}</span>
    </div>
    <div class="hint">Clic : copier · Molette : zoom · Échap</div>
  {/if}
</div>

<style>
  /* La loupe remplit la fenêtre ; ses coins arrondis sont dessinés par Windows. */
  :global(html),
  :global(body) {
    background: #f9f9f9;
  }
  .loupe {
    --bg: #f9f9f9;
    height: 100%;
    padding: 8px;
    background: var(--bg);
    transition: opacity 0.1s;
  }
  @media (prefers-color-scheme: dark) {
    :global(html),
    :global(body) {
      background: #2c2c2c;
    }
    .loupe {
      --bg: #2c2c2c;
    }
  }
  .hidden {
    opacity: 0;
  }
  canvas {
    display: block;
    width: 152px;
    height: 152px;
    border-radius: 7px;
    image-rendering: pixelated;
  }
  .info {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    margin-top: 6px;
    padding: 0 4px;
    border-radius: 6px;
  }
  .info.done {
    justify-content: center;
    font-weight: 600;
    font-size: 12.5px;
  }
  .swatch {
    width: 14px;
    height: 14px;
    border-radius: 4px;
    box-shadow: inset 0 0 0 1px rgba(128, 128, 128, 0.4);
  }
  .hex {
    font-family: var(--mono);
    font-size: 12.5px;
    font-weight: 600;
  }
  .pos {
    margin-left: auto;
    font-size: 10.5px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .hint {
    padding: 0 4px 2px;
    font-size: 10px;
    color: var(--text-3);
    white-space: nowrap;
  }
</style>
