<script lang="ts">
  import ConverterBox from "./lib/ConverterBox.svelte";
</script>

<div class="palette-root">
  <ConverterBox palette />
  <footer>
    <span><kbd>↑</kbd> <kbd>↓</kbd> naviguer</span>
    <span><kbd>Entrée</kbd> ouvrir ou copier</span>
    <span><kbd>Échap</kbd> fermer</span>
  </footer>
</div>

<style>
  .palette-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    /* Teinte légère par-dessus l'acrylique pour garder un bon contraste */
    background: color-mix(in srgb, var(--input) 40%, transparent);
  }
  .palette-root > :global(.box) {
    flex: 1;
  }
  footer {
    display: flex;
    gap: 18px;
    padding: 8px 18px;
    border-top: 1px solid var(--stroke);
    font-size: 12px;
    color: var(--text-2);
  }
</style>
