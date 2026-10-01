<script lang="ts">
  import type { Snippet } from "svelte";

  let { title, subtitle = "", actions }: { title: string; subtitle?: string; actions?: Snippet } = $props();
</script>

<header>
  <div>
    <h1>{title}</h1>
    {#if subtitle}<p>{subtitle}</p>{/if}
  </div>
  {#if actions}<div class="actions">{@render actions()}</div>{/if}
</header>

<style>
  header {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 22px;
  }
  h1 {
    margin: 0;
    font-family: var(--font-display);
    font-size: 28px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  p {
    margin: 4px 0 0;
    color: var(--text-2);
  }
  .actions {
    display: flex;
    gap: 8px;
  }
</style>
