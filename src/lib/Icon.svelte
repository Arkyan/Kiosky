<script lang="ts">
  let { name, size = 18 }: { name: string; size?: number } = $props();

  // Icônes au trait (24×24), dessinées à la main pour n'avoir aucune dépendance.
  const paths: Record<string, string> = {
    calc: '<rect x="5" y="2.5" width="14" height="19" rx="2.5"/><path d="M8.5 7h7"/><path d="M8.5 11.5h.01M12 11.5h.01M15.5 11.5h.01M8.5 15h.01M12 15h.01M15.5 15h.01M8.5 18.5h.01M12 18.5h.01M15.5 18.5h.01"/>',
    keyboard: '<rect x="2.5" y="5.5" width="19" height="13" rx="2.5"/><path d="M6.5 9.5h.01M10 9.5h.01M14 9.5h.01M17.5 9.5h.01M6.5 13h.01M17.5 13h.01M10 13h4M8 15.8h8"/>',
    volume: '<path d="M11 5 6.5 9H3v6h3.5L11 19z"/><path d="M15.5 9a4.5 4.5 0 0 1 0 6M18.5 6a8.5 8.5 0 0 1 0 12"/>',
    mute: '<path d="M11 5 6.5 9H3v6h3.5L11 19z"/><path d="m16 9.5 5 5M21 9.5l-5 5"/>',
    power: '<path d="M12 3v8.5"/><path d="M17.7 6.8a8 8 0 1 1-11.4 0"/>',
    settings: '<path d="M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h0"/><circle cx="16" cy="6" r="2"/><circle cx="10" cy="12" r="2"/><circle cx="18" cy="18" r="2"/>',
    plus: '<path d="M12 5v14M5 12h14"/>',
    trash: '<path d="M4 7h16M9.5 7V4.5h5V7M6.5 7l1 13h9l1-13"/>',
    search: '<circle cx="11" cy="11" r="6.5"/><path d="m20 20-4.2-4.2"/>',
    copy: '<rect x="8.5" y="8.5" width="12" height="12" rx="2"/><path d="M15.5 8.5V5.5a2 2 0 0 0-2-2h-8a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h3"/>',
    check: '<path d="m5 12.5 4.5 4.5L19 7.5"/>',
    shield: '<path d="M12 21s7.5-3.5 7.5-9.5V5.5L12 3 4.5 5.5v6C4.5 17.5 12 21 12 21z"/><path d="m9 12 2 2 4-4"/>',
    refresh: '<path d="M20 12a8 8 0 1 1-2.6-5.9L20 8.5"/><path d="M20 3.5v5h-5"/>',
    clock: '<circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/>',
    bolt: '<path d="M13 2.5 4.5 13.5H12l-1 8 8.5-11H12z"/>',
    x: '<path d="M6 6l12 12M18 6 6 18"/>',
    save: '<path d="M5 3.5h11l3.5 3.5v13.5H5z"/><path d="M8 3.5v5h7v-5M8 20.5v-6h8v6"/>',
    globe: '<circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17M12 3.5c2.5 2.5 3.5 5.5 3.5 8.5s-1 6-3.5 8.5c-2.5-2.5-3.5-5.5-3.5-8.5s1-6 3.5-8.5z"/>',
    sparkle: '<path d="M12 3.5 13.8 10.2 20.5 12l-6.7 1.8L12 20.5l-1.8-6.7L3.5 12l6.7-1.8z"/>',
    folder: '<path d="M3.5 6.5a2 2 0 0 1 2-2h4l2 2.5h7a2 2 0 0 1 2 2v8.5a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z"/>',
    pipette: '<path d="M16 5l3 3 1.5-1.5a2.1 2.1 0 0 0-3-3z"/><path d="m14 6.8 3.2 3.2"/><path d="M15.6 8.4 7 17l-3 .9.9-3 8.6-8.6"/>',
    broom: '<path d="M14 3.5 11 10"/><path d="M7 10.5h8l2.5 9.5h-13z"/><path d="M9.5 20l.4-3.5M13.5 20l-.3-3.5"/>',
    plug: '<path d="M9 3.5V8M15 3.5V8"/><path d="M6.5 8h11v3a5.5 5.5 0 0 1-11 0z"/><path d="M12 16.5v4"/>',
    activity: '<path d="M3 12h4l2.5-6.5 5 13L17 12h4"/>',
    down: '<path d="M12 4.5v15M6 13.5l6 6 6-6"/>',
    up: '<path d="M12 19.5v-15M6 10.5l6-6 6 6"/>',
    stop: '<rect x="6" y="6" width="12" height="12" rx="2.5"/>',
    disk: '<rect x="3.5" y="6.5" width="17" height="11" rx="2.5"/><path d="M16.5 12h.01M7 12h5"/>',
    terminal: '<rect x="3" y="4.5" width="18" height="15" rx="2.5"/><path d="m7 9.5 3 2.5-3 2.5M12.5 15H17"/>',
    code: '<path d="m8.5 7-5 5 5 5M15.5 7l5 5-5 5M13.5 4.5l-3 15"/>',
    star: '<path d="m12 3.8 2.5 5.1 5.6.8-4 4 .9 5.6-5-2.7-5 2.7.9-5.6-4-4 5.6-.8z"/>',
    branch: '<circle cx="7" cy="5.5" r="2"/><circle cx="7" cy="18.5" r="2"/><circle cx="17" cy="8.5" r="2"/><path d="M7 7.5v9M17 10.5c0 4-10 2.5-10 6"/>',
    variable: '<path d="M7.5 4.5c-2 2-3 4.5-3 7.5s1 5.5 3 7.5M16.5 4.5c2 2 3 4.5 3 7.5s-1 5.5-3 7.5"/><path d="m9.5 9 5 6M14.5 9l-5 6"/>',
    up_small: '<path d="m7 14 5-5 5 5"/>',
    down_small: '<path d="m7 10 5 5 5-5"/>',
    undo: '<path d="M9 14 4.5 9.5 9 5"/><path d="M4.5 9.5H14a5.5 5.5 0 0 1 0 11h-3"/>',
    box: '<path d="M12 3 20 7.5v9L12 21l-8-4.5v-9z"/><path d="m4 7.5 8 4.5 8-4.5M12 12v9"/>',
    play: '<path d="M8 5.5v13l10-6.5z"/>',
    log: '<path d="M6 3.5h9l3.5 3.5v13.5H6z"/><path d="M9 11h6M9 14.5h6M9 18h4"/>',
  };
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.6"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {@html paths[name] ?? ""}
</svg>
