<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, copyText, type ChatMessage, type Settings } from "./api";
  import Icon from "./Icon.svelte";
  import { renderMarkdown } from "./markdown";

  // Conversation avec l'assistant, dans la palette. Le champ de saisie est celui de la palette :
  // ce composant affiche les échanges et expose send(), stop() et reset().

  type Turn = ChatMessage & {
    /** Ce qu'on montre à la place du contenu (action sur le presse-papiers : le texte envoyé est long) */
    label?: string;
    error?: string;
  };

  let { busy = $bindable(false), empty = $bindable(true) }: { busy?: boolean; empty?: boolean } = $props();

  let turns = $state<Turn[]>([]);
  let settings = $state<Settings | null>(null);
  let keys = $state<{ claude: boolean; gemini: boolean } | null>(null);
  let clipboard = $state("");
  let copied = $state(-1);
  let scroller = $state<HTMLDivElement>();
  let current = 0;

  $effect(() => {
    empty = turns.length === 0;
  });

  const provider = $derived(settings?.ai_provider ?? "claude");
  const providerName = $derived(provider === "gemini" ? "Gemini" : "Claude");
  const ready = $derived(!!keys && keys[provider]);

  /** Actions sur le texte copié : la consigne envoyée avec lui. */
  const ACTIONS = [
    { id: "fix", label: "Corriger", prompt: "Corrige l'orthographe, la grammaire et la ponctuation de ce texte, sans changer son sens ni son ton." },
    { id: "translate", label: "Traduire", prompt: "Traduis ce texte en français s'il est dans une autre langue, sinon en anglais." },
    { id: "summarize", label: "Résumer", prompt: "Résume ce texte en quelques phrases." },
    { id: "explain", label: "Expliquer", prompt: "Explique simplement ce texte (ou ce code, ou ce message d'erreur)." },
    { id: "rewrite", label: "Reformuler", prompt: "Reformule ce texte pour qu'il soit plus clair et plus fluide, dans la même langue." },
  ];

  /** À chaque ouverture : le fournisseur a pu changer dans les Réglages, et le presse-papiers aussi. */
  export async function refresh() {
    [settings, keys] = await Promise.all([api.getSettings(), api.aiKeyStatus()]);
    clipboard = ((await api.getClipboardText().catch(() => null)) ?? "").trim();
  }

  async function scrollDown() {
    await tick();
    scroller?.scrollTo({ top: scroller.scrollHeight });
  }

  export async function send(text: string, label?: string) {
    text = text.trim();
    if (!text || busy) return;
    if (!ready) await refresh();
    if (!ready) return;
    turns.push({ role: "user", content: text, label });
    const history = turns.map(({ role, content }) => ({ role, content }));
    turns.push({ role: "assistant", content: "" });
    const answer = turns.length - 1;
    const id = ++current;
    busy = true;
    scrollDown();
    try {
      await api.aiChat(id, history);
    } catch (e) {
      if (id === current) turns[answer].error = String(e);
    }
    if (id === current) {
      busy = false;
      scrollDown();
    }
  }

  export function stop() {
    if (!busy) return;
    current++; // les morceaux encore en route sont ignorés
    busy = false;
    api.aiStop();
  }

  export function reset() {
    stop();
    turns = [];
  }

  function act(action: (typeof ACTIONS)[number]) {
    send(`${action.prompt}\n\n"""\n${clipboard}\n"""`, `${action.label} le texte copié`);
  }

  async function copy(i: number) {
    await copyText(turns[i].content);
    copied = i;
    setTimeout(() => (copied = -1), 900);
  }

  /** Les liens d'une réponse s'ouvrent dans le navigateur, pas dans la palette. */
  function onclick(e: MouseEvent) {
    const link = (e.target as Element).closest<HTMLElement>("a[data-url]");
    if (!link) return;
    e.preventDefault();
    api.openUrl(link.dataset.url!);
  }

  onMount(() => {
    refresh();
    const un = listen<{ id: number; text: string }>("ai-delta", (e) => {
      if (e.payload.id !== current || !busy) return;
      const last = turns[turns.length - 1];
      if (last?.role !== "assistant") return;
      // Ne suit la réponse que si on est déjà en bas : remonter pour relire ne doit pas être contrarié.
      const atBottom = !scroller || scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 40;
      last.content += e.payload.text;
      if (atBottom) scrollDown();
    });
    return () => {
      un.then((f) => f());
    };
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="chat" bind:this={scroller} {onclick}>
  {#if keys && !ready}
    <div class="setup">
      <div class="big"><Icon name="chat" size={22} /></div>
      <div class="strong">Aucune clé API pour {providerName}</div>
      <div class="small muted">
        L'assistant utilise ta propre clé {providerName}. Ajoute-la dans les Réglages, section Assistant.
      </div>
      <button
        class="btn primary"
        onclick={() => {
          api.runAction("page:settings");
          api.hidePalette();
        }}
      >
        Ouvrir les Réglages
      </button>
    </div>
  {:else if !turns.length}
    <div class="welcome">
      <div class="small muted">
        Pose une question à {providerName}, ou agis sur le texte que tu viens de copier.
      </div>
      {#if clipboard}
        <div class="clip small" title={clipboard.slice(0, 600)}>
          <Icon name="copy" size={13} />
          <span>{clipboard.slice(0, 160)}</span>
        </div>
        <div class="chips">
          {#each ACTIONS as a (a.id)}
            <button class="chip-btn" onclick={() => act(a)}>{a.label}</button>
          {/each}
        </div>
      {:else}
        <div class="small muted hint">Le presse-papiers ne contient pas de texte : copie quelque chose pour le corriger, le traduire ou le résumer.</div>
      {/if}
    </div>
  {:else}
    {#each turns as t, i (i)}
      {#if t.role === "user"}
        <div class="turn user">
          <div class="bubble" class:action={!!t.label} title={t.label ? t.content.slice(0, 600) : undefined}>{t.label ?? t.content}</div>
        </div>
      {:else}
        <div class="turn assistant">
          {#if t.content}
            <div class="answer">{@html renderMarkdown(t.content)}</div>
          {:else if !t.error}
            <div class="wait small muted"><span class="spin" style="display:flex"><Icon name="refresh" size={13} /></span> {providerName} réfléchit…</div>
          {/if}
          {#if t.error}<div class="error small">{t.error}</div>{/if}
          {#if t.content && !(busy && i === turns.length - 1)}
            <button class="btn ghost mini-btn" onclick={() => copy(i)}>
              <Icon name={copied === i ? "check" : "copy"} size={13} />
              {copied === i ? "Copié" : "Copier"}
            </button>
          {/if}
        </div>
      {/if}
    {/each}
  {/if}
</div>

<style>
  .chat {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
    padding: 12px 16px;
    overflow-y: auto;
    user-select: text;
  }
  .setup,
  .welcome {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: auto 0;
    padding: 0 8px;
  }
  .setup {
    align-items: center;
    text-align: center;
  }
  .big {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: 12px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .strong {
    font-weight: 600;
  }
  .clip {
    display: flex;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 8px;
    border: 1px solid var(--stroke);
    background: var(--fill-hover);
    color: var(--text-2);
  }
  .clip :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .clip span {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip-btn {
    height: 30px;
    padding: 0 12px;
    border-radius: 999px;
    border: 1px solid var(--stroke-strong);
    background: var(--card);
    font-size: 12.5px;
    cursor: pointer;
  }
  .chip-btn:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .turn {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
  }
  .turn.user {
    align-items: flex-end;
  }
  .bubble {
    max-width: 85%;
    max-height: 120px;
    padding: 7px 12px;
    border-radius: 12px 12px 2px 12px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 13.5px;
    white-space: pre-wrap;
    word-break: break-word;
    overflow-y: auto;
  }
  .bubble.action {
    font-weight: 600;
  }
  .answer {
    max-width: 100%;
    font-size: 13.5px;
    line-height: 1.5;
    word-break: break-word;
  }
  .answer :global(p) {
    margin: 0 0 8px;
  }
  .answer :global(:last-child) {
    margin-bottom: 0;
  }
  .answer :global(h4) {
    margin: 10px 0 4px;
    font-size: 13.5px;
    font-weight: 600;
  }
  .answer :global(ul),
  .answer :global(ol) {
    margin: 0 0 8px;
    padding-left: 20px;
  }
  .answer :global(li) {
    margin: 2px 0;
  }
  .answer :global(code) {
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--fill-press);
    font-family: var(--mono);
    font-size: 12.5px;
  }
  .answer :global(pre) {
    margin: 0 0 8px;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid var(--stroke);
    background: color-mix(in srgb, var(--input) 70%, transparent);
    overflow-x: auto;
  }
  .answer :global(pre code) {
    padding: 0;
    background: none;
    font-size: 12px;
    line-height: 1.45;
    white-space: pre;
  }
  .answer :global(a) {
    color: var(--accent);
  }
  .wait {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .error {
    padding: 8px 10px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--bad) 10%, transparent);
    color: var(--bad);
  }
  .mini-btn {
    height: 24px;
    padding: 0 8px;
    margin-left: -8px;
    gap: 5px;
    font-size: 12px;
    color: var(--text-2);
    user-select: none;
  }
</style>
