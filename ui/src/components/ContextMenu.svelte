<script lang="ts" module>
  export type MenuItem = { label: string; onselect: () => void; disabled?: boolean; danger?: boolean } | 'separator'
</script>

<script lang="ts">
  let { items, x, y, onclose }: { items: MenuItem[]; x: number; y: number; onclose: () => void } = $props()

  let menu: HTMLElement | undefined = $state()
  let width = $state(0)
  let height = $state(0)

  // Opens at the mouse, pushed back inside the window when it would overflow.
  const left = $derived(Math.max(4, Math.min(x, window.innerWidth - width - 4)))
  const top = $derived(Math.max(4, Math.min(y, window.innerHeight - height - 4)))

  function closeOnOutsidePress(event: PointerEvent) {
    if (menu && !menu.contains(event.target as Node)) onclose()
  }
</script>

<!-- Escape is handled by the app (it must not also clear the selection); the rest closes here. -->
<svelte:window onpointerdown={closeOnOutsidePress} onblur={onclose} onresize={onclose} />

<div class="context-menu" role="menu" bind:this={menu} bind:clientWidth={width} bind:clientHeight={height} style="left: {left}px; top: {top}px">
  {#each items as item, index (index)}
    {#if item === 'separator'}
      <hr />
    {:else}
      <button
        type="button"
        role="menuitem"
        class:danger={item.danger}
        disabled={item.disabled}
        onmousedown={(event) => event.preventDefault()}
        onclick={() => {
          onclose()
          item.onselect()
        }}
      >
        {item.label}
      </button>
    {/if}
  {/each}
</div>

<style>
  .context-menu {
    position: fixed;
    z-index: 10;
    display: flex;
    flex-direction: column;
    min-width: 190px;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    box-shadow: var(--shadow);
  }

  button {
    padding: 6px 10px;
    border: 0;
    border-radius: 5px;
    background: none;
    color: var(--ink);
    font: inherit;
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }

  button:hover:not(:disabled) {
    background: var(--surface-2);
  }

  button.danger {
    color: var(--danger);
  }

  button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  hr {
    width: 100%;
    margin: 4px 0;
    border: 0;
    border-top: 1px solid var(--border);
  }
</style>
