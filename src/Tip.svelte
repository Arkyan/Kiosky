<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api } from "./lib/api";

  // Infobulle de la barre flottante : Rust la place au-dessus de l'élément survolé.
  let lines = $state<string[]>([]);
  let box: HTMLDivElement;

  onMount(() => {
    const un = listen<{ text: string; seq: number }>("tip-content", async (e) => {
      lines = e.payload.text.split("\n");
      await tick();
      const r = box.getBoundingClientRect();
      await api.placeTip(r.width, r.height, e.payload.seq);
    });
    return () => un.then((f) => f());
  });
</script>

<div class="tip" bind:this={box}>
  {#each lines as line, i}
    {#if line === ""}
      <div class="gap"></div>
    {:else if line.includes("\t")}
      {@const [name, value] = line.split("\t")}
      <div class="row"><span class="name">{name}</span><span class="value">{value}</span></div>
    {:else}
      <div class:first={i === 0} class:hint={line.startsWith("Clic") || line.startsWith("Molette")}>{line}</div>
    {/if}
  {/each}
</div>

<style>
  /* L'infobulle remplit la fenêtre ; ses coins arrondis sont dessinés par Windows. */
  :global(html),
  :global(body) {
    background: rgb(40, 40, 40);
    overflow: hidden;
  }
  @media (prefers-color-scheme: light) {
    :global(html),
    :global(body) {
      background: rgb(252, 252, 252);
    }
  }
  .tip {
    display: inline-block;
    max-width: 340px;
    padding: 8px 11px;
    background: rgb(40, 40, 40);
    color: #f3f3f3;
    font-size: 12px;
    line-height: 1.45;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  @media (prefers-color-scheme: light) {
    .tip {
      background: rgb(252, 252, 252);
      color: #1a1a1a;
    }
  }
  .tip div {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Deux colonnes : nom à gauche, valeur alignée à droite */
  .row {
    display: flex;
    justify-content: space-between;
    gap: 24px;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    opacity: 0.75;
  }
  .value {
    flex: none;
    font-weight: 600;
  }
  .first {
    font-weight: 600;
  }
  .gap {
    height: 5px;
  }
  .hint {
    font-size: 11px;
    opacity: 0.6;
  }
</style>
