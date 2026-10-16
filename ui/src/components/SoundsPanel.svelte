<script lang="ts">
  import type { SoundPacks } from '../lib/document-api'
  import type { Track } from '../lib/score-types'
  import { soundsPanel } from '../lib/sound-packs'

  let {
    load,
    track,
    onloaded,
  }: {
    load: () => Promise<SoundPacks>
    track: Pick<Track, 'instrument' | 'sound' | 'fallback_program'> | null
    /** Told the packs each time they are asked again, so the Sound menu lists the same. */
    onloaded?: (packs: SoundPacks) => void
  } = $props()

  let packs = $state<SoundPacks | null>(null)
  let error = $state<string | null>(null)
  let details = $state<HTMLDetailsElement>()

  const panel = $derived(packs ? soundsPanel({ packs, track }) : null)

  // Asked again at each opening: a pack file that fails is only found out when a score uses it.
  async function refresh() {
    if (!details?.open) return
    try {
      packs = await load()
      onloaded?.(packs)
      error = null
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason)
    }
  }

  function closeOnOutsidePress(event: PointerEvent) {
    if (details?.open && !details.contains(event.target as Node)) details.open = false
  }
</script>

<svelte:window onpointerdown={closeOnOutsidePress} />

<details class="sounds-menu" bind:this={details} ontoggle={refresh}>
  <summary class="file-button" title="Sound packs and where the sound of the track comes from">Sounds</summary>
  <div class="sounds-panel" role="region" aria-label="Sounds">
    {#if error}
      <p class="error">{error}</p>
    {:else if !panel}
      <p class="muted">Loading…</p>
    {:else}
      {#if panel.current}
        <section class="current">
          <h2>This track</h2>
          <p><strong>{panel.current.label}</strong>: <span class="from">{panel.current.from}</span></p>
          <p class="muted">{panel.current.detail}</p>
        </section>
      {/if}

      <section>
        <h2>Sound packs</h2>
        {#if panel.packs.length === 0}
          <p class="muted">No pack: every sound plays with General MIDI.</p>
        {/if}
        <ul>
          {#each panel.packs as pack (pack.id)}
            <li class="pack">
              <div class="name">{pack.name} <span class="licence">{pack.licence}</span></div>
              <div class="muted">
                by {pack.author}{#if pack.source}, from <span class="source">{pack.source}</span>{/if}
              </div>
              <ul class="pack-sounds">
                {#each pack.sounds as packSound (packSound.label)}
                  <li class="pack-sound">
                    {packSound.label}
                    {#if packSound.isNew}<span class="new" title="A sound this pack adds to the built-in ones">new sound</span>{/if}
                    <span class="muted">({packSound.keys})</span>
                    {#if packSound.replacedBy}<span class="muted"> replaced by {packSound.replacedBy}</span>{/if}
                  </li>
                {/each}
              </ul>
            </li>
          {/each}
        </ul>
      </section>

      {#if panel.refused.length > 0}
        <section>
          <h2>Not used</h2>
          <ul>
            {#each panel.refused as refused, index (index)}
              <li class="refused"><span class="path">{refused.folder}</span>: {refused.reason}</li>
            {/each}
          </ul>
        </section>
      {/if}

      <section>
        <h2>Your packs</h2>
        {#if panel.userFolder}
          <p class="muted">Add a pack folder here and restart the app:</p>
          <p class="path user-folder">{panel.userFolder}</p>
        {:else}
          <p class="muted">Your packs folder is only known in the desktop app.</p>
        {/if}
      </section>
    {/if}
  </div>
</details>

<style>
  .sounds-menu {
    position: relative;
  }

  .sounds-menu summary {
    list-style: none;
    cursor: pointer;
  }

  .sounds-panel {
    position: absolute;
    top: 100%;
    right: 0;
    z-index: 3;
    width: min(380px, calc(100vw - 32px));
    max-height: 70vh;
    overflow: auto;
    margin-top: 4px;
    padding: 4px 14px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    box-shadow: var(--shadow);
    font-size: 13px;
  }

  h2 {
    margin: 12px 0 6px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }

  p {
    margin: 4px 0;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .pack {
    padding: 6px 0;
    border-top: 1px solid var(--border);
  }

  .pack:first-child {
    border-top: 0;
  }

  .name {
    font-weight: 600;
  }

  .licence {
    margin-left: 4px;
    padding: 1px 5px;
    border: 1px solid var(--border);
    border-radius: 4px;
    font-size: 11px;
    font-weight: 500;
  }

  .pack-sounds {
    margin-top: 3px;
  }

  .new {
    margin-left: 2px;
    padding: 0 4px;
    border-radius: 4px;
    background: var(--surface-2);
    font-size: 11px;
    font-weight: 600;
  }

  .muted {
    color: var(--ink-muted);
  }

  .error {
    color: var(--danger);
  }

  .path,
  .source {
    overflow-wrap: anywhere;
    font-family: ui-monospace, monospace;
    font-size: 12px;
  }

  .user-folder {
    padding: 4px 6px;
    border-radius: 5px;
    background: var(--surface-2);
    user-select: all;
  }
</style>
