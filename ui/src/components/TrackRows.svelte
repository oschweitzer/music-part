<script lang="ts">
  import TabView from './TabView.svelte'
  import type { Editor } from '../lib/editor.svelte'
  import { instrumentLabel } from '../lib/instruments'
  import type { Playback } from '../lib/playback.svelte'
  import type { Score } from '../lib/score-types'
  import type { Selection } from '../lib/selection'
  import { layoutRows, locateTick, visibleRows } from '../lib/tab-layout'
  import type { ShownTrack } from '../lib/tab-layout'

  let {
    score,
    shown,
    editor,
    playback,
    ontogglecollapse,
    onremove,
    onmenu,
  }: {
    score: Score
    shown: ShownTrack[]
    editor: Editor
    playback: Playback
    ontogglecollapse: (track: number) => void
    onremove: (track: number) => void
    onmenu?: (target: Selection, at: { x: number; y: number }, track: number) => void
  } = $props()

  /** Width of the track headers on the left; the tablature starts after it. */
  const HEADER_WIDTH = 188
  /** Rows and measures this far off screen are drawn too, so a quick scroll shows no gap. */
  const OVERSCAN = 400
  /** The drawn band moves by this step, so scrolling redraws only now and then, not on every pixel. */
  const SCROLL_STEP = 200
  const EDGE = 48

  let scroller = $state<HTMLElement>()
  let scrollTop = $state(0)
  let scrollLeft = $state(0)
  let viewWidth = $state(0)
  let viewHeight = $state(0)

  const layout = $derived(layoutRows({ tracks: score.tracks, shown, width: Math.max(viewWidth - HEADER_WIDTH, 0) }))
  const rows = $derived(visibleRows({ layout, top: scrollTop - OVERSCAN, bottom: scrollTop + SCROLL_STEP + viewHeight + OVERSCAN }))
  const visibleX = $derived({ left: scrollLeft - OVERSCAN, right: scrollLeft + SCROLL_STEP + viewWidth - HEADER_WIDTH + OVERSCAN })

  const hasStarted = $derived(playback.isPlaying || playback.tick > 0)
  const cursor = $derived(hasStarted ? locateTick({ layout: layout.timeline, tick: playback.tick }) : null)

  function handleScroll() {
    if (!scroller) return
    scrollTop = Math.floor(scroller.scrollTop / SCROLL_STEP) * SCROLL_STEP
    scrollLeft = Math.floor(scroller.scrollLeft / SCROLL_STEP) * SCROLL_STEP
  }

  /** Scrolls the least needed so that the band of tablature x from `left` to `right` is in view. */
  function revealX(left: number, right: number) {
    if (!scroller) return
    const width = scroller.clientWidth - HEADER_WIDTH
    if (left < scroller.scrollLeft) scroller.scrollTo({ left: Math.max(left - EDGE, 0) })
    else if (right > scroller.scrollLeft + width) scroller.scrollTo({ left: Math.max(left - EDGE, 0) })
  }

  // Follow the playback cursor: when it leaves the screen, the next page of measures comes in.
  $effect(() => {
    if (cursor && playback.isPlaying) revealX(cursor.x, cursor.x + 1)
  })

  // Keep the selection in view when the keyboard moves it, to another track too (Alt+Up/Down).
  $effect(() => {
    const selection = editor.selection
    const row = layout.rows.find((r) => r.track === editor.track)
    if (!selection || !row || !scroller) return
    if (row.top < scroller.scrollTop) scroller.scrollTo({ top: row.top })
    else if (row.top + row.height > scroller.scrollTop + scroller.clientHeight) {
      scroller.scrollTo({ top: Math.max(row.top + row.height - scroller.clientHeight, 0) })
    }
    const beat = row.layout?.systems[0]?.measures[selection.measure]?.beats[selection.beat]
    if (beat) revealX(beat.x - 12, beat.x + 12)
  })

  // Keep focus where it was, so that typing a fret still works after clicking a header button.
  function keepFocus(event: MouseEvent) {
    event.preventDefault()
  }
</script>

<div
  class="scroller"
  bind:this={scroller}
  bind:clientWidth={viewWidth}
  bind:clientHeight={viewHeight}
  onscroll={handleScroll}
>
  <div class="canvas" style:width="{HEADER_WIDTH + layout.width}px" style:height="{layout.height}px">
    {#each rows as row (row.track)}
      {@const track = score.tracks[row.track]}
      {#if track}
        {@const isSelected = row.track === editor.track}
        <section
          class="row"
          class:selected={isSelected}
          class:collapsed={!row.layout}
          style:top="{row.top}px"
          style:height="{row.height}px"
          aria-label="Track {row.track + 1}: {track.name}"
        >
          <header class="head" style:width="{HEADER_WIDTH}px">
            <div class="title">
              <button
                type="button"
                class="fold"
                aria-expanded={row.layout !== null}
                aria-label="{row.layout ? 'Fold' : 'Unfold'} {track.name}"
                title={row.layout ? 'Fold this row down to its header' : 'Show the tablature of this row'}
                onmousedown={keepFocus}
                onclick={() => ontogglecollapse(row.track)}
              >
                <svg viewBox="0 0 12 12" aria-hidden="true"><path d={row.layout ? 'M2.5 4.5 6 8l3.5-3.5' : 'M4.5 2.5 8 6l-3.5 3.5'} /></svg>
              </button>
              <button
                type="button"
                class="name"
                title="Edit this track"
                onmousedown={keepFocus}
                onclick={() => editor.selectTrack(row.track)}>{track.name}</button
              >
            </div>
            {#if row.layout}
              <p class="instrument">{instrumentLabel(track.instrument)} · {track.tuning.length} strings</p>
            {/if}
            <div class="buttons">
              <button
                type="button"
                class="mix"
                class:on={playback.muted[row.track]}
                aria-pressed={playback.muted[row.track] === true}
                aria-label="Mute {track.name}"
                title="Mute"
                onmousedown={keepFocus}
                onclick={() => playback.toggleMute(row.track)}>M</button
              >
              <button
                type="button"
                class="mix"
                class:on={playback.solo[row.track]}
                aria-pressed={playback.solo[row.track] === true}
                aria-label="Solo {track.name}"
                title="Solo: play only the soloed tracks"
                onmousedown={keepFocus}
                onclick={() => playback.toggleSolo(row.track)}>S</button
              >
              {#if editor.canEdit}
                <button
                  type="button"
                  class="mix drop"
                  aria-label="Remove {track.name}"
                  title="Remove this track (Ctrl+Z brings it back)"
                  disabled={score.tracks.length < 2}
                  onmousedown={keepFocus}
                  onclick={() => onremove(row.track)}>×</button
                >
              {/if}
            </div>
          </header>

          {#if row.layout}
            <div class="tab">
              <TabView
                layout={row.layout}
                label={track.name}
                {visibleX}
                selection={isSelected ? editor.selection : null}
                onselect={editor.canEdit ? (target, options) => editor.select(target, { ...options, track: row.track }) : undefined}
                ondrag={editor.canEdit ? (area) => editor.dragSelect({ ...area, track: row.track }) : undefined}
                onseek={(tick) => playback.seek(tick)}
                onmenu={editor.canEdit && onmenu ? (target, at) => onmenu(target, at, row.track) : undefined}
              />
            </div>
          {/if}
        </section>
      {/if}
    {/each}

    {#if cursor}
      <!-- One cursor for every row: the tracks share their barlines and their time axis. -->
      <div class="playhead" aria-hidden="true" style:left="{HEADER_WIDTH}px" style:height="{layout.height}px">
        <div class="measure" style:left="{cursor.measureX}px" style:width="{cursor.measureWidth}px"></div>
        <div class="line" style:left="{cursor.x}px"></div>
      </div>
    {/if}
  </div>
</div>

<style>
  .scroller {
    position: absolute;
    inset: 0;
    overflow: auto;
    background: var(--paper);
  }

  .canvas {
    position: relative;
    min-width: 100%;
  }

  .row {
    position: absolute;
    left: 0;
    right: 0;
    display: flex;
    border-bottom: 1px solid var(--border);
  }

  .head {
    position: sticky;
    left: 0;
    z-index: 2;
    display: flex;
    flex: none;
    flex-direction: column;
    gap: 6px;
    box-sizing: border-box;
    padding: 8px 10px 8px 6px;
    border-right: 1px solid var(--border);
    border-left: 3px solid transparent;
    background: var(--surface);
  }

  .row.collapsed .head {
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
    padding-top: 0;
    padding-bottom: 0;
  }

  .row.selected .head {
    border-left-color: var(--accent);
    background: var(--surface-2);
  }

  .title {
    display: flex;
    align-items: center;
    gap: 2px;
    min-width: 0;
  }

  .fold {
    display: grid;
    flex: none;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--ink-muted);
    cursor: pointer;
  }

  .fold:hover {
    color: var(--ink);
    background: var(--border);
  }

  .fold svg {
    width: 12px;
    height: 12px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .name {
    overflow: hidden;
    padding: 2px 4px;
    border: 0;
    background: none;
    color: var(--ink);
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }

  .instrument {
    margin: 0 0 0 26px;
    overflow: hidden;
    color: var(--ink-muted);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .buttons {
    display: flex;
    flex: none;
    gap: 4px;
    margin-left: 26px;
  }

  .row.collapsed .buttons {
    margin-left: 0;
  }

  .mix {
    width: 22px;
    height: 20px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: none;
    color: var(--ink-muted);
    font: inherit;
    font-size: 10px;
    font-weight: 600;
    cursor: pointer;
  }

  .mix:hover {
    color: var(--ink);
  }

  .mix.on {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--on-accent);
  }

  .mix.drop:hover:not(:disabled) {
    border-color: var(--danger);
    color: var(--danger);
  }

  .mix:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .tab {
    flex: none;
  }

  .playhead {
    position: absolute;
    top: 0;
    right: 0;
    z-index: 1;
    pointer-events: none;
  }

  .playhead .measure {
    position: absolute;
    top: 0;
    bottom: 0;
    background: var(--accent);
    opacity: 0.06;
  }

  .playhead .line {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--accent);
  }
</style>
