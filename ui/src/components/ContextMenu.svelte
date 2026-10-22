<script lang="ts" module>
  /** What a submenu holds: choices, one of which may be the current one. */
  export type SubmenuItem = { label: string; onselect: () => void; checked?: boolean }

  export type MenuItem =
    | { label: string; onselect: () => void; disabled?: boolean; danger?: boolean }
    /** An entry that opens a list of choices beside it. */
    | { label: string; submenu: SubmenuItem[]; disabled?: boolean }
    | 'separator'
</script>

<script lang="ts">
  const SUBMENU_WIDTH = 220

  let { items, x, y, onclose }: { items: MenuItem[]; x: number; y: number; onclose: () => void } = $props()

  let menu: HTMLElement | undefined = $state()
  let width = $state(0)
  let height = $state(0)

  // Opens at the mouse, pushed back inside the window when it would overflow.
  const left = $derived(Math.max(4, Math.min(x, window.innerWidth - width - 4)))
  const top = $derived(Math.max(4, Math.min(y, window.innerHeight - height - 4)))

  /** Index of the entry whose submenu is open. */
  let openSubmenu = $state<number | null>(null)

  // A submenu opens to the right, or to the left when the window has no room on that side.
  const submenuOnLeft = $derived(left + width + SUBMENU_WIDTH > window.innerWidth)

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
    {:else if 'submenu' in item}
      <div class="has-submenu" role="none" onmouseenter={() => (openSubmenu = index)} onmouseleave={() => (openSubmenu = null)}>
        <button
          type="button"
          role="menuitem"
          aria-haspopup="menu"
          aria-expanded={openSubmenu === index}
          disabled={item.disabled}
          onmousedown={(event) => event.preventDefault()}
          onclick={() => (openSubmenu = openSubmenu === index ? null : index)}
        >
          <span>{item.label}</span>
          <span class="arrow" aria-hidden="true">▸</span>
        </button>
        {#if openSubmenu === index}
          <div class="context-menu submenu" class:left={submenuOnLeft} role="menu" aria-label={item.label}>
            {#each item.submenu as choice (choice.label)}
              <button
                type="button"
                role="menuitemradio"
                aria-checked={choice.checked === true}
                class:current={choice.checked}
                onmousedown={(event) => event.preventDefault()}
                onclick={() => {
                  onclose()
                  choice.onselect()
                }}
              >
                {choice.label}{choice.checked ? ' ✓' : ''}
              </button>
            {/each}
          </div>
        {/if}
      </div>
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

  .has-submenu {
    position: relative;
    display: flex;
    flex-direction: column;
  }

  .has-submenu > button {
    display: flex;
    justify-content: space-between;
    gap: 16px;
  }

  .arrow {
    color: var(--ink-muted);
  }

  .context-menu.submenu {
    position: absolute;
    top: -5px;
    left: 100%;
    z-index: 11;
    min-width: 160px;
  }

  .context-menu.submenu.left {
    right: 100%;
    left: auto;
  }

  button.current {
    color: var(--accent);
    font-weight: 600;
  }

  hr {
    width: 100%;
    margin: 4px 0;
    border: 0;
    border-top: 1px solid var(--border);
  }
</style>
