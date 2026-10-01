<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Shell from "./Shell.svelte";
  import Palette from "./Palette.svelte";
  import Picker from "./Picker.svelte";

  // Une seule application Svelte pour toutes les fenêtres : on regarde laquelle nous héberge.
  const label = getCurrentWindow().label;
  const isPalette = location.hash === "#palette" || label === "palette";
  const isPicker = location.hash === "#picker" || label === "picker";
</script>

{#if isPicker}
  <Picker />
{:else if isPalette}
  <Palette />
{:else}
  <Shell />
{/if}
