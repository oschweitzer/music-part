<script lang="ts">
  import { isBeatSelected, type Selection } from '../lib/selection'
  import { hitTest, METRICS } from '../lib/tab-layout'
  import type { BeatLayout, CursorPosition, TabLayout } from '../lib/tab-layout'

  let {
    layout,
    label,
    cursor = null,
    selection = null,
    onselect,
    onseek,
    onmenu,
  }: {
    layout: TabLayout
    label: string
    cursor?: CursorPosition | null
    selection?: Selection | null
    onselect?: (target: Selection, options: { extend: boolean }) => void
    /** Asked to move playback to a position in 1/128 of a whole note. */
    onseek?: (tick: number) => void
    /** Asked to open the menu of a spot, at a position of the window. */
    onmenu?: (target: Selection, at: { x: number; y: number }) => void
  } = $props()

  /** What is under the mouse. The drawing is scaled if the page squeezes it, so map back to drawing units. */
  function targetAt(event: MouseEvent) {
    const box = (event.currentTarget as SVGSVGElement).getBoundingClientRect()
    const scale = box.width > 0 ? layout.width / box.width : 1
    return hitTest({ layout, x: (event.clientX - box.left) * scale, y: (event.clientY - box.top) * scale })
  }

  // Dragging over several beats selects them all: the press picks the first, the beat under the
  // mouse is the last. A drag ends in a click, which must not select the single beat again.
  let dragFrom: Selection | null = null
  let dragTo: { measure: number; beat: number } | null = null
  let isDragging = false

  function handlePointerDown(event: PointerEvent) {
    isDragging = false
    dragTo = null
    dragFrom = null
    if (!onselect || event.button !== 0 || event.shiftKey) return
    const target = targetAt(event)
    if (target) dragFrom = { measure: target.measure, beat: target.beat, string: target.string }
  }

  function handlePointerMove(event: PointerEvent) {
    if (!onselect || !dragFrom || event.buttons !== 1) return
    const target = targetAt(event)
    if (!target) return
    if (!isDragging) {
      if (target.measure === dragFrom.measure && target.beat === dragFrom.beat) return
      isDragging = true
      // Keep receiving the moves when the mouse leaves the drawing.
      ;(event.currentTarget as SVGSVGElement).setPointerCapture(event.pointerId)
      onselect(dragFrom, { extend: false })
    }
    if (dragTo?.measure === target.measure && dragTo.beat === target.beat) return
    dragTo = { measure: target.measure, beat: target.beat }
    onselect({ measure: target.measure, beat: target.beat, string: dragFrom.string }, { extend: true })
  }

  function handleClick(event: MouseEvent) {
    if (!onselect) return
    if (isDragging) {
      isDragging = false
      return
    }
    const target = targetAt(event)
    if (target) onselect({ measure: target.measure, beat: target.beat, string: target.string }, { extend: event.shiftKey })
  }

  function handleContextMenu(event: MouseEvent) {
    if (!onmenu) return
    const target = targetAt(event)
    if (!target) return
    event.preventDefault()
    onmenu({ measure: target.measure, beat: target.beat, string: target.string }, { x: event.clientX, y: event.clientY })
  }

  /** Double-clicking a beat moves playback there. */
  function handleDoubleClick(event: MouseEvent) {
    if (!onseek) return
    const target = targetAt(event)
    if (target) onseek(target.tick)
  }

  const tabLetters = ['T', 'A', 'B']

  function fretLabel(fret: number, isGhost: boolean): string {
    return isGhost ? `(${fret})` : String(fret)
  }

  function labelBackgroundWidth(label: string): number {
    return label.length * 7 + 5
  }

  function tiePath({ fromX, toX, y }: { fromX: number; toX: number; y: number }): string {
    const mid = (fromX + toX) / 2
    return `M${fromX} ${y - 6} Q${mid} ${y - 16} ${toX} ${y - 6}`
  }

  function flagPath({ x, y, index }: { x: number; y: number; index: number }): string {
    const top = y - index * 5
    return `M${x} ${top} q7 3 6 11`
  }

  /** Rests are drawn under the staff, where the stems of played notes are. */
  function restTop(staffBottom: number): number {
    return staffBottom + METRICS.stemOffset + 2
  }

  function restSlashDots(beat: BeatLayout): number {
    return beat.value === 'Eighth' ? 1 : beat.value === 'Sixteenth' ? 2 : beat.value === 'ThirtySecond' ? 3 : 0
  }
</script>

<!-- Keyboard editing is handled for the whole window (see App.svelte), so the click needs no key handler. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<svg
  class="tab"
  class:editable={onselect !== undefined}
  onpointerdown={handlePointerDown}
  onpointermove={handlePointerMove}
  oncontextmenu={handleContextMenu}
  onclick={handleClick}
  ondblclick={handleDoubleClick}
  width={layout.width}
  height={layout.height}
  viewBox="0 0 {layout.width} {layout.height}"
  role="img"
  aria-label="Tablature of {label}"
>
  {#each layout.systems as system, systemIndex (system.y)}
    {@const staffMid = (system.staffTop + system.staffBottom) / 2}
    {@const letterGap = Math.min(14, (system.staffBottom - system.staffTop) / 3)}
    <g class="system">
      {#each system.stringYs as y (y)}
        <line class="string" x1={system.left} x2={system.right} y1={y} y2={y} />
      {/each}

      <line class="bar" x1={system.left} x2={system.left} y1={system.staffTop} y2={system.staffBottom} />

      {#each tabLetters as letter, i (letter)}
        <text class="clef" x={system.left + METRICS.clefWidth / 2} y={staffMid + (i - 1) * letterGap} text-anchor="middle" dominant-baseline="central">
          {letter}
        </text>
      {/each}

      {#each system.measures as measure (measure.index)}
        {#if measure.index === system.measures[0]?.index || measure.index % 4 === 0}
          <text class="measure-number" x={measure.x + 4} y={system.staffTop - 20}>{measure.index + 1}</text>
        {/if}

        {#if onseek}
          <!-- The strip above a measure moves playback to its start; the click must not select a note. -->
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <rect
            class="seek-strip"
            x={measure.x}
            y={system.staffTop - 30}
            width={measure.width}
            height="22"
            onclick={(event) => {
              event.stopPropagation()
              onseek(measure.startTick)
            }}
          />
        {/if}

        {#if measure.timeSignature}
          <g class="time-signature" text-anchor="middle" dominant-baseline="central">
            <text x={measure.x + 18} y={staffMid - letterGap / 2 - 1}>{measure.timeSignature.numerator}</text>
            <text x={measure.x + 18} y={staffMid + letterGap / 2 + 1}>{measure.timeSignature.denominator}</text>
          </g>
        {/if}

        {#each measure.beats as beat, beatIndex (beat.startTick)}
          {#if beat.isRest}
            {@const top = restTop(system.staffBottom)}
            <g class="rest">
              {#if beat.value === 'Whole'}
                <rect x={beat.x - 5} y={top} width="10" height="4" />
              {:else if beat.value === 'Half'}
                <rect x={beat.x - 5} y={top + 4} width="10" height="4" />
              {:else if beat.value === 'Quarter'}
                <path d="M{beat.x - 3} {top} l6 5 l-6 5 l6 5 l-3 4" fill="none" />
              {:else}
                <line x1={beat.x + 3} y1={top} x2={beat.x - 3} y2={top + 14} />
                {#each { length: restSlashDots(beat) } as _, i (i)}
                  <circle cx={beat.x - 5 - i * 1.5} cy={top + 3 + i * 4} r="1.6" />
                {/each}
              {/if}
            </g>
          {/if}

          {#if isBeatSelected({ selection, measure: measure.index, beat: beatIndex })}
            <rect
              class="beat-highlight"
              x={beat.x - 13}
              y={system.staffTop - 4}
              width="26"
              height={system.staffBottom - system.staffTop + 8}
              rx="6"
            />
          {/if}
          {#if selection && selection.measure === measure.index && selection.beat === beatIndex}
            {@const selectedY = system.stringYs[selection.string - 1] ?? system.stringYs[0] ?? 0}
            <rect class="selection" x={beat.x - 12} y={selectedY - 9} width="24" height="18" rx="5" />
          {/if}

          {#each beat.notes as note, noteIndex (noteIndex)}
            {@const label = fretLabel(note.fret, note.isGhost)}
            {#if note.tie}
              <path class="tie" d={tiePath({ fromX: note.tie.fromX, toX: note.tie.toX, y: note.y })} />
            {/if}
            <rect
              class="fret-bg"
              x={beat.x - labelBackgroundWidth(label) / 2}
              y={note.y - 7}
              width={labelBackgroundWidth(label)}
              height="14"
              rx="3"
            />
            <text class="fret" x={beat.x} y={note.y} text-anchor="middle" dominant-baseline="central">{label}</text>
            {#if note.techniqueLabel}
              <text class="technique" x={beat.x} y={system.staffTop - 8} text-anchor="middle">{note.techniqueLabel}</text>
            {/if}
          {/each}

          {#if beat.stem}
            <line class="stem" x1={beat.stem.x} x2={beat.stem.x} y1={beat.stem.y1} y2={beat.stem.y2} />
            {#each { length: beat.flags } as _, i (i)}
              <path class="flag" d={flagPath({ x: beat.stem.x, y: beat.stem.y2, index: i })} />
            {/each}
          {/if}

          {#if beat.dots > 0 && !beat.isRest}
            {#each { length: beat.dots } as _, i (i)}
              <circle class="dot" cx={beat.x + 6 + i * 5} cy={(beat.stem?.y2 ?? system.staffBottom + 30) - 3} r="1.6" />
            {/each}
          {/if}
        {/each}

        {#if cursor && cursor.systemIndex === systemIndex && cursor.measureIndex === measure.index}
          <rect
            class="measure-highlight"
            x={measure.x}
            y={system.staffTop - 6}
            width={measure.width}
            height={system.staffBottom - system.staffTop + METRICS.rhythmHeight + 6}
          />
        {/if}

        {#each measure.beams as beam, i (i)}
          <line class="beam" x1={beam.x1} x2={beam.x2} y1={beam.y} y2={beam.y} />
        {/each}

        <line class="bar" x1={measure.x + measure.width} x2={measure.x + measure.width} y1={system.staffTop} y2={system.staffBottom} />
      {/each}
      {#if cursor && cursor.systemIndex === systemIndex}
        <g class="cursor" aria-hidden="true">
          <line x1={cursor.x} x2={cursor.x} y1={system.staffTop - 8} y2={system.staffBottom + METRICS.rhythmHeight - 6} />
          <path d="M{cursor.x - 4} {system.staffTop - 12} h8 l-4 5 z" />
        </g>
      {/if}
    </g>
  {/each}
</svg>

<style>
  .tab {
    display: block;
    font-family: var(--font-ui);
    /* A drag selects beats, not the text of the drawing. */
    user-select: none;
    -webkit-user-select: none;
  }

  .tab.editable {
    cursor: pointer;
  }

  .beat-highlight {
    fill: var(--accent);
    opacity: 0.1;
  }

  .selection {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2;
  }

  .string {
    stroke: var(--staff-line);
    stroke-width: 1;
  }

  .bar {
    stroke: var(--ink);
    stroke-width: 1.2;
  }

  .clef {
    fill: var(--ink);
    font-size: 15px;
    font-weight: 700;
  }

  .measure-number {
    fill: var(--ink-muted);
    font-size: 10px;
  }

  .time-signature text {
    fill: var(--ink);
    font-size: 15px;
    font-weight: 700;
  }

  .measure-highlight {
    fill: var(--accent);
    opacity: 0.07;
  }

  .cursor line {
    stroke: var(--accent);
    stroke-width: 2;
    stroke-linecap: round;
  }

  .cursor path {
    fill: var(--accent);
  }

  .fret-bg {
    fill: var(--paper);
  }

  .fret {
    fill: var(--ink);
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .seek-strip {
    fill: transparent;
    cursor: pointer;
  }

  .seek-strip:hover {
    fill: color-mix(in srgb, var(--accent) 10%, transparent);
  }

  .technique {
    fill: var(--accent);
    font-size: 10px;
    font-style: italic;
    font-weight: 600;
  }

  .tie {
    fill: none;
    stroke: var(--ink-muted);
    stroke-width: 1.2;
    stroke-linecap: round;
  }

  .stem,
  .flag {
    stroke: var(--ink);
    stroke-width: 1.2;
    fill: none;
  }

  .beam {
    stroke: var(--ink);
    stroke-width: 2.6;
  }

  .dot {
    fill: var(--ink);
  }

  .rest rect,
  .rest circle {
    fill: var(--ink);
  }

  .rest path,
  .rest line {
    stroke: var(--ink);
    stroke-width: 1.4;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
