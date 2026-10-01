<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Shell from "./Shell.svelte";
  import Palette from "./Palette.svelte";
  import Picker from "./Picker.svelte";
  import Widget from "./Widget.svelte";
  import Tip from "./Tip.svelte";

  // Une seule application Svelte pour toutes les fenêtres : on regarde laquelle nous héberge.
  const label = getCurrentWindow().label;
  const isPalette = location.hash === "#palette" || label === "palette";
  const isPicker = location.hash === "#picker" || label === "picker";
  const isWidget = location.hash === "#widget" || label === "widget";
  const isTip = location.hash === "#tip" || label === "tip";
</script>

{#if isTip}
  <Tip />
{:else if isWidget}
  <Widget />
{:else if isPicker}
  <Picker />
{:else if isPalette}
  <Palette />
{:else}
  <Shell />
{/if}
