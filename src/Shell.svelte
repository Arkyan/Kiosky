<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import Icon from "./lib/Icon.svelte";
  import { store, loadSettings } from "./lib/settings.svelte";
  import { MODULES } from "./lib/modules";
  import ConverterPage from "./pages/ConverterPage.svelte";
  import ExpanderPage from "./pages/ExpanderPage.svelte";
  import MixerPage from "./pages/MixerPage.svelte";
  import StartupPage from "./pages/StartupPage.svelte";
  import SettingsPage from "./pages/SettingsPage.svelte";
  import ColorPage from "./pages/ColorPage.svelte";
  import CleanerPage from "./pages/CleanerPage.svelte";
  import PortsPage from "./pages/PortsPage.svelte";
  import MonitorPage from "./pages/MonitorPage.svelte";
  import FoldersPage from "./pages/FoldersPage.svelte";
  import ProjectsPage from "./pages/ProjectsPage.svelte";
  import EnvPage from "./pages/EnvPage.svelte";
  import ContainersPage from "./pages/ContainersPage.svelte";

  type Page =
    | "converter"
    | "expander"
    | "color"
    | "mixer"
    | "monitor"
    | "startup"
    | "cleaner"
    | "ports"
    | "folders"
    | "projects"
    | "env"
    | "containers"
    | "settings";

  // Modules désactivés dans les réglages : retirés de la barre latérale.
  const nav = $derived(
    MODULES.filter((m) => m.id === "converter" || !store.s?.disabled_modules.includes(m.id)) as {
      id: Page;
      label: string;
      icon: string;
      group: string;
    }[],
  );

  let page = $state<Page>("converter");

  // La page affichée vient d'être désactivée : retour au convertisseur.
  $effect(() => {
    if (page !== "settings" && !nav.some((n) => n.id === page)) page = "converter";
  });
  let loadError = $state("");

  onMount(() => {
    loadSettings().catch((e) => (loadError = String(e)));
    // La palette peut ouvrir une page (« nettoyage », « ports »…).
    const unSettings = listen("settings-changed", () => loadSettings());
    const un = listen<string>("navigate", (e) => {
      const target = e.payload as Page;
      if (target === "settings" || nav.some((n) => n.id === target)) page = target;
    });
    return () => {
      un.then((f) => f());
      unSettings.then((f) => f());
    };
  });
</script>

<div class="shell">
  <aside class="sidebar">
    <div class="brand">
      <div class="logo">
        <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="white" stroke-width="2.4" stroke-linecap="round">
          <path d="M5 7h14M5 12h14M5 17h14" opacity=".55" />
          <circle cx="15" cy="7" r="1.6" fill="white" />
          <circle cx="9" cy="12" r="1.6" fill="white" />
          <circle cx="13" cy="17" r="1.6" fill="white" />
        </svg>
      </div>
      <span>Kiosky</span>
    </div>

    <nav>
      {#each nav as item, i (item.id)}
        {#if i === 0 || nav[i - 1].group !== item.group}<div class="group">{item.group}</div>{/if}
        <button class="nav-item" class:active={page === item.id} onclick={() => (page = item.id)}>
          <Icon name={item.icon} />
          <span>{item.label}</span>
        </button>
      {/each}
    </nav>

    <div class="spacer"></div>

    <button class="nav-item" class:active={page === "settings"} onclick={() => (page = "settings")}>
      <Icon name="settings" />
      <span>Réglages</span>
    </button>
  </aside>

  <main class="content">
    {#if loadError}
      <div class="banner error">Impossible de charger les réglages : {loadError}</div>
    {:else if !store.s}
      <div class="loading"><Icon name="refresh" size={22} /></div>
    {:else}
      {#key page}
        <div class="page">
          {#if page === "converter"}
            <ConverterPage />
          {:else if page === "expander"}
            <ExpanderPage />
          {:else if page === "mixer"}
            <MixerPage />
          {:else if page === "startup"}
            <StartupPage />
          {:else if page === "color"}
            <ColorPage />
          {:else if page === "cleaner"}
            <CleanerPage />
          {:else if page === "ports"}
            <PortsPage />
          {:else if page === "monitor"}
            <MonitorPage />
          {:else if page === "folders"}
            <FoldersPage />
          {:else if page === "projects"}
            <ProjectsPage />
          {:else if page === "env"}
            <EnvPage />
          {:else if page === "containers"}
            <ContainersPage />
          {:else}
            <SettingsPage />
          {/if}
        </div>
      {/key}
    {/if}

    {#if store.error}
      <div class="toast error">{store.error}</div>
    {:else if store.saved}
      <div class="toast"><Icon name="check" size={14} /> Enregistré</div>
    {/if}
  </main>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: 236px 1fr;
    height: 100%;
  }

  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 14px 8px 12px;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 12px 18px;
    font-family: var(--font-display);
    font-size: 15px;
    font-weight: 600;
  }
  .logo {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 7px;
    background: linear-gradient(160deg, #0078d4, #7c4dff);
    box-shadow: 0 2px 6px rgba(80, 60, 220, 0.35);
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .group {
    padding: 14px 14px 4px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-3);
  }
  .group:first-child {
    padding-top: 0;
  }

  .nav-item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 14px;
    height: 36px;
    padding: 0 14px;
    border: none;
    border-radius: 6px;
    background: transparent;
    text-align: left;
    cursor: pointer;
    transition: background 0.12s;
  }
  .nav-item:hover {
    background: var(--fill-hover);
  }
  .nav-item:active {
    background: var(--fill-press);
  }
  .nav-item.active {
    background: var(--fill-hover);
    font-weight: 600;
  }
  /* Indicateur de sélection Fluent */
  .nav-item.active::before {
    content: "";
    position: absolute;
    left: 0;
    top: 50%;
    width: 3px;
    height: 16px;
    border-radius: 3px;
    background: var(--accent);
    transform: translateY(-50%);
    animation: pill 0.22s cubic-bezier(0.3, 0.7, 0.4, 1);
  }
  @keyframes pill {
    from {
      height: 0;
    }
  }

  .spacer {
    flex: 1;
  }

  .content {
    position: relative;
    overflow-y: auto;
    padding: 28px 36px 40px;
    background: var(--card-2);
    border-top-left-radius: 10px;
    border-left: 1px solid var(--stroke);
    border-top: 1px solid var(--stroke);
  }

  .page {
    max-width: 920px;
    margin: 0 auto;
    animation: enter 0.25s cubic-bezier(0.2, 0.8, 0.2, 1);
  }

  .loading {
    display: grid;
    place-items: center;
    height: 60%;
    color: var(--text-3);
  }
  .loading :global(svg) {
    animation: spin 1s linear infinite;
  }

  .toast {
    position: fixed;
    right: 20px;
    bottom: 18px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    border-radius: 8px;
    background: var(--card);
    border: 1px solid var(--stroke);
    box-shadow: var(--shadow);
    font-size: 12.5px;
    backdrop-filter: blur(20px);
    animation: enter 0.2s ease-out;
  }
  .toast.error {
    color: var(--bad);
    max-width: 420px;
  }
</style>
