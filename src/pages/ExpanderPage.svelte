<script lang="ts">
  import Icon from "../lib/Icon.svelte";
  import PageHeader from "../lib/PageHeader.svelte";
  import Toggle from "../lib/Toggle.svelte";
  import { store, saveSettings } from "../lib/settings.svelte";

  const s = $derived(store.s!);

  const variables = [
    { v: "{date}", d: "01/10/2026" },
    { v: "{heure}", d: "14:32" },
    { v: "{jour}", d: "jeudi" },
    { v: "{date_longue}", d: "jeudi 1 octobre 2026" },
    { v: "{annee}", d: "2026" },
  ];

  let lastFocused: HTMLTextAreaElement | null = null;

  function add() {
    s.snippets.push({ trigger: ";", text: "" });
    saveSettings();
    // Focus sur le nouveau déclencheur
    setTimeout(() => {
      const inputs = document.querySelectorAll<HTMLInputElement>(".trigger");
      const last = inputs[inputs.length - 1];
      last?.focus();
      last?.setSelectionRange(1, 1);
    });
  }

  function remove(i: number) {
    s.snippets.splice(i, 1);
    saveSettings(0);
  }

  function insertVariable(v: string) {
    const ta = lastFocused;
    if (!ta) return;
    const i = Number(ta.dataset.index);
    const sn = s.snippets[i];
    if (!sn) return;
    const start = ta.selectionStart ?? sn.text.length;
    const end = ta.selectionEnd ?? start;
    sn.text = sn.text.slice(0, start) + v + sn.text.slice(end);
    saveSettings();
    setTimeout(() => {
      ta.focus();
      ta.setSelectionRange(start + v.length, start + v.length);
    });
  }

  function problem(trigger: string, i: number): string {
    if (trigger.length < 2) return "Au moins 2 caractères";
    if (/\s/.test(trigger)) return "Pas d'espace";
    if (s.snippets.some((o, j) => j !== i && o.trigger === trigger)) return "Déjà utilisé";
    return "";
  }
</script>

<PageHeader title="Expanseur de texte" subtitle="Tape un déclencheur n'importe où : il est remplacé par ton texte.">
  {#snippet actions()}
    <button class="btn primary" onclick={add}><Icon name="plus" size={16} /> Ajouter</button>
  {/snippet}
</PageHeader>

<div class="card row-card">
  <div class="row-icon"><Icon name="keyboard" size={20} /></div>
  <div class="grow">
    <div class="strong">Activer l'expanseur</div>
    <div class="muted small">Fonctionne dans toutes les applications. Rien n'est enregistré : seuls les derniers caractères tapés sont gardés en mémoire.</div>
  </div>
  <Toggle
    checked={s.expander_enabled}
    label="Activer l'expanseur"
    onchange={(v) => {
      s.expander_enabled = v;
      saveSettings(0);
    }}
  />
</div>

<div class="vars">
  <span class="muted small">Variables (clique pour insérer dans le texte sélectionné) :</span>
  {#each variables as v}
    <button class="chip mono" title={`Exemple : ${v.d}`} onmousedown={(e) => e.preventDefault()} onclick={() => insertVariable(v.v)}>{v.v}</button>
  {/each}
</div>

<div class="list" class:disabled={!s.expander_enabled}>
  {#each s.snippets as sn, i (i)}
    {@const err = problem(sn.trigger, i)}
    <div class="card snippet">
      <div class="left">
        <label class="small muted" for={`t${i}`}>Déclencheur</label>
        <input
          id={`t${i}`}
          class="field mono trigger"
          class:invalid={err}
          bind:value={sn.trigger}
          oninput={() => saveSettings()}
          spellcheck="false"
        />
        {#if err}<span class="err small">{err}</span>{/if}
      </div>
      <div class="arrow">→</div>
      <div class="right">
        <label class="small muted" for={`x${i}`}>Remplacé par</label>
        <textarea
          id={`x${i}`}
          class="field"
          rows={Math.min(6, Math.max(1, sn.text.split("\n").length))}
          data-index={i}
          bind:value={sn.text}
          onfocus={(e) => (lastFocused = e.currentTarget)}
          oninput={() => saveSettings()}
          placeholder="Ton texte… (Entrée pour aller à la ligne)"
        ></textarea>
      </div>
      <button class="btn ghost icon del" title="Supprimer" onclick={() => remove(i)}><Icon name="trash" size={16} /></button>
    </div>
  {:else}
    <div class="empty card">
      <Icon name="sparkle" size={28} />
      <p>Aucun raccourci pour l'instant.</p>
      <button class="btn primary" onclick={add}><Icon name="plus" size={16} /> Créer le premier</button>
    </div>
  {/each}
</div>

<p class="tip muted small">
  Astuce : commence tes déclencheurs par un caractère rare comme <kbd>;</kbd> pour éviter les remplacements accidentels.
</p>

<style>
  .row-card {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 16px 18px;
  }
  .row-icon {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: 10px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .grow {
    flex: 1;
  }
  .strong {
    font-weight: 600;
  }

  .vars {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 22px 0 12px;
  }
  .vars .chip {
    font-size: 12px;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    transition: opacity 0.2s;
  }
  .list.disabled {
    opacity: 0.55;
  }

  .snippet {
    display: grid;
    grid-template-columns: 190px 24px 1fr 32px;
    align-items: start;
    gap: 12px;
    padding: 12px 14px;
    animation: enter 0.2s ease-out;
  }
  .left,
  .right {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .trigger {
    width: 100%;
    font-weight: 600;
  }
  .trigger.invalid {
    border-bottom-color: var(--bad);
  }
  .err {
    color: var(--bad);
  }
  textarea {
    width: 100%;
    min-height: 34px;
    line-height: 1.4;
  }
  .arrow {
    padding-top: 28px;
    color: var(--text-3);
    text-align: center;
  }
  .del {
    margin-top: 22px;
    color: var(--text-2);
  }
  .del:hover {
    color: var(--bad);
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 36px;
    color: var(--text-2);
  }
  .tip {
    margin-top: 18px;
  }
</style>
