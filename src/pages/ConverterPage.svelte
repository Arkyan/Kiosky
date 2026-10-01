<script lang="ts">
  import ConverterBox from "../lib/ConverterBox.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import { store } from "../lib/settings.svelte";

  let box = $state<ReturnType<typeof ConverterBox>>();

  const groups = [
    { label: "Calcul", items: ["(12 + 8) * 1,2", "20% de 150", "sqrt(2)", "2^16"] },
    { label: "Unités", items: ["10 km en miles", "72 f en c", "5 go en mio", "90 km/h"] },
    { label: "Devises", items: ["50 eur usd", "$120", "1000 jpy en eur"] },
    { label: "Heure", items: ["tokyo", "14h en new york", "9h30 londres en paris"] },
    { label: "Dev", items: ["b64 bonjour", "url a b&c", "0xff", '{"ok":true,"n":[1,2]}'] },
  ];

  const keys = $derived(store.s?.palette_shortcut.split("+") ?? []);
</script>

<PageHeader title="Convertisseur" subtitle="Calculs, unités, devises, fuseaux horaires et encodages, au même endroit." />

<div class="card hero">
  <div class="shortcut">
    <span class="muted">Ouvre-le de n'importe où avec</span>
    {#each keys as k, i}
      {#if i > 0}<span class="plus">+</span>{/if}<kbd>{k}</kbd>
    {/each}
  </div>
  <ConverterBox bind:this={box} />
</div>

<h2>Exemples</h2>
<div class="examples">
  {#each groups as g}
    <div class="group">
      <span class="glabel">{g.label}</span>
      <div class="chips">
        {#each g.items as ex}
          <button class="chip" onclick={() => box?.setQuery(ex)}>{ex}</button>
        {/each}
      </div>
    </div>
  {/each}
</div>

<style>
  .hero {
    padding: 18px;
  }
  .shortcut {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 14px;
    font-size: 13px;
  }
  .shortcut .muted {
    margin-right: 4px;
  }
  .plus {
    color: var(--text-3);
    font-size: 12px;
  }
  h2 {
    margin: 28px 0 12px;
    font-size: 14px;
    font-weight: 600;
  }
  .examples {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .group {
    display: grid;
    grid-template-columns: 90px 1fr;
    align-items: start;
    gap: 12px;
  }
  .glabel {
    padding-top: 3px;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    font-family: var(--mono);
    font-size: 12px;
  }
</style>
