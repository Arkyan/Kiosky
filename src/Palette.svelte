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
    background: #f9f9f9;
  }
  .palette-root > :global(.box) {
    flex: 1;
  }
  @media (prefers-color-scheme: dark) {
    .palette-root {
      background: #2b2b2b;
    }
  }
  :global(html),
  :global(body) {
    background: transparent;
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
