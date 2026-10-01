<script lang="ts">
  let {
    checked = false,
    disabled = false,
    label = "",
    onchange,
  }: { checked?: boolean; disabled?: boolean; label?: string; onchange?: (v: boolean) => void } = $props();
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  class="toggle"
  class:on={checked}
  {disabled}
  onclick={() => onchange?.(!checked)}
>
  <span class="knob"></span>
</button>

<style>
  .toggle {
    position: relative;
    width: 40px;
    height: 20px;
    flex: none;
    border-radius: 999px;
    border: 1px solid var(--text-2);
    background: transparent;
    padding: 0;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }
  .toggle:hover:not(:disabled) {
    background: var(--fill-hover);
  }
  .knob {
    position: absolute;
    top: 50%;
    left: 4px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--text-2);
    transform: translateY(-50%);
    transition: left 0.18s cubic-bezier(0.3, 0.7, 0.4, 1.2), width 0.12s, background 0.15s;
  }
  .toggle:active:not(:disabled) .knob {
    width: 16px;
  }
  .on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .on:hover:not(:disabled) {
    background: var(--accent-hover);
  }
  .on .knob {
    left: calc(100% - 16px);
    background: var(--on-accent);
  }
  .on:active:not(:disabled) .knob {
    left: calc(100% - 20px);
  }
  .toggle:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
