<script lang="ts">
  import { piecePosition } from '../lib/drums'
  import { isBeatSelected, selectedRows, type Selection } from '../lib/selection'
  import {
    bassClefDots,
    bassClefPath,
    flatPath,
    naturalPath,
    restPath,
    sharpPath,
    staffFlagPath,
    trebleClefDot,
    trebleClefPath,
  } from '../lib/staff-glyphs'
  import { keySignatureGlyphs, positionY, STAFF_METRICS, staffIndexFor, staffPosition } from '../lib/staff-layout'
  import type { Accidental, StaffBlock, StaffHead, StaffMeasure } from '../lib/staff-layout'
  import { clipLayout, hitTest, METRICS } from '../lib/tab-layout'
  import type { BeatLayout, CursorPosition, HitTarget, SystemLayout, TabLayout } from '../lib/tab-layout'

  let {
    layout,
    label,
    cursor = null,
    selection = null,
    onselect,
    ondrag,
    onseek,
    onenter,
    onmenu,
    visibleX = null,
  }: {
    layout: TabLayout
    label: string
    cursor?: CursorPosition | null
    selection?: Selection | null
    onselect?: (target: Selection, options: { extend: boolean }) => void
    /** Asked to select the rectangle dragged from one spot to another; `byRows` false for whole beats. */
    ondrag?: (area: { from: Selection; to: Selection; byRows: boolean }) => void
    /** Asked to move playback to a position in 1/128 of a whole note. */
    onseek?: (tick: number) => void
    /** Asked to write a note at a pitch, double-clicked on a staff. */
    onenter?: (target: { measure: number; beat: number; pitch: number }) => void
    /** Asked to open the menu of a spot, at a position of the window. */
    onmenu?: (target: Selection, at: { x: number; y: number }) => void
    /** Only the measures across this band of x are drawn, for a line much wider than the screen. */
    visibleX?: { left: number; right: number } | null
  } = $props()

  const drawn = $derived(visibleX ? clipLayout({ layout, left: visibleX.left, right: visibleX.right }) : layout)
  // The rows of a pitched or drum track are on its staff (pitches or pieces), not strings.
  const isPitchRows = $derived(layout.rowKind !== 'string')

  /** What is under the mouse. The drawing is scaled if the page squeezes it, so map back to drawing units. */
  function targetAt(event: MouseEvent): HitTarget | null {
    const box = (event.currentTarget as SVGSVGElement).getBoundingClientRect()
    const scale = box.width > 0 ? layout.width / box.width : 1
    return hitTest({ layout, x: (event.clientX - box.left) * scale, y: (event.clientY - box.top) * scale })
  }

  /** The spot a hit selects. An empty spot of the staff of a fretted track keeps the selected string. */
  function spotOf(target: HitTarget): Selection {
    return { measure: target.measure, beat: target.beat, row: target.row ?? selection?.row ?? 1 }
  }

  // Dragging draws a rectangle, like selecting text: the beats from the press to the mouse, and
  // the rows between them. A drag ends in a click, which must not select the single spot again.
  let dragFrom: Selection | null = null
  let dragByRows = true
  let dragTo: Selection | null = null
  let isDragging = false

  function handlePointerDown(event: PointerEvent) {
    isDragging = false
    dragTo = null
    dragFrom = null
    if (!ondrag || event.button !== 0 || event.shiftKey) return
    const target = targetAt(event)
    if (!target) return
    dragFrom = spotOf(target)
    // On the staff of a fretted track the rows are strings, which the staff does not show.
    dragByRows = target.area === 'tab' || isPitchRows
  }

  function handlePointerMove(event: PointerEvent) {
    if (!ondrag || !dragFrom || event.buttons !== 1) return
    const target = targetAt(event)
    if (!target) return
    const to = spotOf(target)
    if (!isDragging) {
      if (to.measure === dragFrom.measure && to.beat === dragFrom.beat && to.row === dragFrom.row) return
      isDragging = true
      // Keep receiving the moves when the mouse leaves the drawing.
      ;(event.currentTarget as SVGSVGElement).setPointerCapture(event.pointerId)
    }
    if (dragTo?.measure === to.measure && dragTo.beat === to.beat && dragTo.row === to.row) return
    dragTo = to
    ondrag({ from: dragFrom, to, byRows: dragByRows })
  }

  function handleClick(event: MouseEvent) {
    if (!onselect) return
    if (isDragging) {
      isDragging = false
      return
    }
    const target = targetAt(event)
    if (target) onselect(spotOf(target), { extend: event.shiftKey })
  }

  function handleContextMenu(event: MouseEvent) {
    if (!onmenu) return
    const target = targetAt(event)
    if (!target) return
    event.preventDefault()
    onmenu(spotOf(target), { x: event.clientX, y: event.clientY })
  }

  /** Double-clicking a staff writes a note there; double-clicking the tablature moves playback. */
  function handleDoubleClick(event: MouseEvent) {
    const target = targetAt(event)
    if (!target) return
    if (target.area === 'staff' && target.pitch !== null && onenter) {
      onenter({ measure: target.measure, beat: target.beat, pitch: target.pitch })
    } else if (onseek) {
      onseek(target.tick)
    }
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

  function staffTiePath({ fromX, toX, y, isAbove }: { fromX: number; toX: number; y: number; isAbove: boolean }): string {
    const mid = (fromX + toX) / 2
    const bend = isAbove ? -6 : 6
    return `M${fromX} ${y} Q${mid} ${y + bend} ${toX} ${y}`
  }

  function flagPath({ x, y, index }: { x: number; y: number; index: number }): string {
    const top = y - index * 5
    return `M${x} ${top} q7 3 6 11`
  }

  function accidentalPath(kind: Accidental, x: number, y: number): string {
    return kind === 'sharp' ? sharpPath({ x, y }) : kind === 'flat' ? flatPath({ x, y }) : naturalPath({ x, y })
  }

  /** Rests are drawn under the staff, where the stems of played notes are. */
  function restTop(staffBottom: number): number {
    return staffBottom + METRICS.stemOffset + 2
  }

  function restSlashDots(beat: BeatLayout): number {
    return beat.value === 'Eighth' ? 1 : beat.value === 'Sixteenth' ? 2 : beat.value === 'ThirtySecond' ? 3 : 0
  }

  function isBassClef(block: StaffBlock): boolean {
    return block.clef === 'bass' || block.clef === 'bass8vb'
  }

  /** Where a pitch (on drums, a piece) is written on the staves of a system: the y of its line or space. */
  function pitchY(system: SystemLayout, pitch: number): number | null {
    const block = system.staves[staffIndexFor({ pitch, staffCount: system.staves.length })]
    if (!block) return null
    if (block.clef === 'percussion') {
      const position = piecePosition(pitch)
      return position === null ? null : positionY({ position, bottom: block.bottom })
    }
    return positionY({ position: staffPosition({ pitch, clef: block.clef, key: layout.key }), bottom: block.bottom })
  }

  /** The outline of a drum head that is not an oval: a diamond or a triangle. */
  function headPolygon(head: StaffHead): string {
    const { x, y } = head
    const w = STAFF_METRICS.headHalfWidth + 0.4
    return head.shape === 'diamond'
      ? `${x - w},${y} ${x},${y - 4} ${x + w},${y} ${x},${y + 4}`
      : `${x - w},${y + 3.6} ${x},${y - 4.2} ${x + w},${y + 3.6}`
  }

  /** The two strokes of a cross head, a little smaller inside a circle. */
  function crossPath(head: StaffHead): string {
    const d = head.shape === 'circleCross' ? 2.9 : 3.8
    const { x, y } = head
    return `M${x - d} ${y - d} L${x + d} ${y + d} M${x - d} ${y + d} L${x + d} ${y - d}`
  }

  /** The head of a staff that the selected row points at, on a beat. */
  function selectedHead(staves: StaffMeasure[], beatIndex: number, row: number) {
    for (const staff of staves) {
      const head = staff.beats[beatIndex]?.heads.find((h) => h.row === row)
      if (head) return head
    }
    return null
  }

  /** The band of y a selected beat covers on the staves, limited to the pitches of a rectangle. */
  function staffBand(system: SystemLayout): { top: number; bottom: number } | null {
    const first = system.staves[0]
    const last = system.staves.at(-1)
    if (!first || !last) return null
    const rows = isPitchRows ? selectedRows(selection) : null
    if (rows) {
      const high = pitchY(system, rows.last)
      const low = pitchY(system, rows.first)
      if (high !== null && low !== null) return { top: high - 7, bottom: low + 7 }
    }
    return { top: first.top - 6, bottom: last.bottom + 6 }
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
  aria-label="{layout.systems[0]?.stringYs.length ? 'Tablature' : 'Staff'} of {label}"
>
  {#each drawn.systems as system, systemIndex (system.y)}
    {@const hasTab = system.stringYs.length > 0}
    {@const staffMid = (system.staffTop + system.staffBottom) / 2}
    {@const letterGap = Math.min(14, (system.staffBottom - system.staffTop) / 3)}
    {@const firstStaff = system.staves[0]}
    {@const lastStaff = system.staves.at(-1)}
    <g class="system">
      {#each system.staves as block, staffIndex (staffIndex)}
        <g class="staff" data-clef={block.clef}>
          {#each block.lineYs as y (y)}
            <line class="staff-line" x1={system.left} x2={system.right} y1={y} y2={y} />
          {/each}
          {#if block.clef === 'percussion'}
            <!-- The percussion clef: two thick bars across the middle of the staff. -->
            <rect class="clef-bar" x={system.left + 13} y={positionY({ position: 6, bottom: block.bottom })} width="3.4" height={STAFF_METRICS.lineGap * 2} />
            <rect class="clef-bar" x={system.left + 20} y={positionY({ position: 6, bottom: block.bottom })} width="3.4" height={STAFF_METRICS.lineGap * 2} />
          {:else if isBassClef(block)}
            <path class="clef-glyph" d={bassClefPath({ x: system.left + 16, bottom: block.bottom })} />
            {#each bassClefDots({ x: system.left + 16, bottom: block.bottom }) as dot, i (i)}
              <circle class="clef-dot" cx={dot.cx} cy={dot.cy} r={dot.r} />
            {/each}
          {:else}
            {@const dot = trebleClefDot({ x: system.left + 18, bottom: block.bottom })}
            <path class="clef-glyph" d={trebleClefPath({ x: system.left + 18, bottom: block.bottom })} />
            <circle class="clef-dot" cx={dot.cx} cy={dot.cy} r={dot.r} />
          {/if}
          {#if block.clef === 'treble8vb' || block.clef === 'bass8vb'}
            <text class="octave" x={system.left + 18} y={block.bottom + (isBassClef(block) ? 12 : 30)} text-anchor="middle">8</text>
          {/if}
          {#each block.clef === 'percussion' ? [] : keySignatureGlyphs({ key: layout.key, clef: block.clef, x: system.left + METRICS.clefWidth - 2, bottom: block.bottom }) as glyph, i (i)}
            <path class="accidental" d={glyph.kind === 'sharp' ? sharpPath(glyph) : flatPath(glyph)} />
          {/each}
        </g>
      {/each}
      {#if firstStaff && lastStaff}
        <line class="bar" x1={system.left} x2={system.left} y1={firstStaff.top} y2={lastStaff.bottom} />
        {#if system.staves.length > 1}
          <path
            class="brace"
            d="M{system.left - 4} {firstStaff.top} q-7 {(lastStaff.bottom - firstStaff.top) / 4} 0 {(lastStaff.bottom - firstStaff.top) / 2} q-7 {(lastStaff.bottom - firstStaff.top) / 4} 0 {(lastStaff.bottom - firstStaff.top) / 2}"
          />
        {/if}
      {/if}

      {#if hasTab}
        {#each system.stringYs as y (y)}
          <line class="string" x1={system.left} x2={system.right} y1={y} y2={y} />
        {/each}

        <line class="bar" x1={system.left} x2={system.left} y1={system.staffTop} y2={system.staffBottom} />

        {#each tabLetters as letter, i (letter)}
          <text class="clef" x={system.left + METRICS.clefWidth / 2} y={staffMid + (i - 1) * letterGap} text-anchor="middle" dominant-baseline="central">
            {letter}
          </text>
        {/each}
      {/if}

      {#each system.measures as measure (measure.index)}
        {#if measure.index === system.measures[0]?.index || measure.index % 4 === 0}
          <text class="measure-number" x={measure.x + 4} y={system.top - 20}>{measure.index + 1}</text>
        {/if}

        {#if onseek}
          <!-- The strip above a measure moves playback to its start; the click must not select a note. -->
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <rect
            class="seek-strip"
            x={measure.x}
            y={system.top - 30}
            width={measure.width}
            height="22"
            onclick={(event) => {
              event.stopPropagation()
              onseek(measure.startTick)
            }}
          />
        {/if}

        {#if measure.timeSignature}
          {#if hasTab}
            <g class="time-signature" text-anchor="middle" dominant-baseline="central">
              <text x={measure.x + 18} y={staffMid - letterGap / 2 - 1}>{measure.timeSignature.numerator}</text>
              <text x={measure.x + 18} y={staffMid + letterGap / 2 + 1}>{measure.timeSignature.denominator}</text>
            </g>
          {/if}
          {#each system.staves as block, staffIndex (staffIndex)}
            <g class="time-signature staff-time" text-anchor="middle" dominant-baseline="central">
              <text x={measure.x + 18} y={positionY({ position: 6, bottom: block.bottom })}>{measure.timeSignature.numerator}</text>
              <text x={measure.x + 18} y={positionY({ position: 2, bottom: block.bottom })}>{measure.timeSignature.denominator}</text>
            </g>
          {/each}
        {/if}

        {#each measure.beats as beat, beatIndex (beat.startTick)}
          {@const isSelectedBeat = selection !== null && selection.measure === measure.index && selection.beat === beatIndex}
          {#if beat.isRest && hasTab}
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
            {#if hasTab}
              <!-- A rectangle selection covers only the rows of its strings. -->
              {@const rows = isPitchRows ? null : selectedRows(selection)}
              {@const top = rows ? (system.stringYs[rows.first - 1] ?? system.staffTop) - 9 : system.staffTop - 4}
              {@const bottom = rows ? (system.stringYs[rows.last - 1] ?? system.staffBottom) + 9 : system.staffBottom + 4}
              <rect class="beat-highlight" x={beat.x - 13} y={top} width="26" height={bottom - top} rx="6" />
            {/if}
            {@const band = staffBand(system)}
            {#if band}
              <rect class="beat-highlight" x={beat.x - 13} y={band.top} width="26" height={band.bottom - band.top} rx="6" />
            {/if}
          {/if}
          {#if isSelectedBeat && selection}
            {#if hasTab && !isPitchRows}
              {@const selectedY = system.stringYs[selection.row - 1] ?? system.stringYs[0] ?? 0}
              <rect class="selection" x={beat.x - 12} y={selectedY - 9} width="24" height="18" rx="5" />
            {/if}
            {@const head = selectedHead(measure.staves, beatIndex, selection.row)}
            {#if head}
              <rect class="selection staff-selection" x={head.x - 8} y={head.y - 7} width="16" height="14" rx="5" />
            {:else if isPitchRows}
              {@const ghostY = pitchY(system, selection.row)}
              {#if ghostY !== null}
                <ellipse class="ghost-head" cx={beat.x} cy={ghostY} rx={STAFF_METRICS.headHalfWidth} ry="3.4" transform="rotate(-20 {beat.x} {ghostY})" />
                <rect class="selection staff-selection" x={beat.x - 8} y={ghostY - 7} width="16" height="14" rx="5" />
              {/if}
            {/if}
          {/if}

          {#if hasTab}
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
          {/if}
        {/each}

        {#each measure.staves as staff, staffIndex (staffIndex)}
          <g class="staff-notes">
            {#each staff.beats as sb, i (i)}
              {#each sb.ledgers as ledger, l (l)}
                <line class="ledger" x1={ledger.x1} x2={ledger.x2} y1={ledger.y} y2={ledger.y} />
              {/each}
              {#if sb.rest}
                {@const rest = restPath(sb.rest)}
                <path class="staff-rest" class:filled={rest.isFilled} d={rest.d} />
              {/if}
              {#each sb.heads as head, h (h)}
                {#if head.tie}
                  <path class="tie" d={staffTiePath(head.tie)} />
                {/if}
                {#if head.accidental}
                  <path class="accidental" d={accidentalPath(head.accidental, head.accidentalX, head.y)} />
                {/if}
                {#if head.shape === 'normal'}
                  <ellipse
                    class="head"
                    class:hollow={!head.isFilled}
                    cx={head.x}
                    cy={head.y}
                    rx={STAFF_METRICS.headHalfWidth}
                    ry="3.4"
                    transform="rotate(-20 {head.x} {head.y})"
                  />
                {:else if head.shape === 'cross' || head.shape === 'circleCross'}
                  <g class="head cross-head" data-shape={head.shape}>
                    {#if head.shape === 'circleCross'}
                      <circle cx={head.x} cy={head.y} r={STAFF_METRICS.headHalfWidth} />
                    {/if}
                    <path d={crossPath(head)} />
                  </g>
                {:else}
                  <polygon class="head" class:hollow={!head.isFilled} data-shape={head.shape} points={headPolygon(head)} />
                {/if}
              {/each}
              {#if sb.stem}
                <line class="stem" x1={sb.stem.x} x2={sb.stem.x} y1={sb.stem.y1} y2={sb.stem.y2} />
                {#each { length: sb.flags } as _, f (f)}
                  <path class="flag" d={staffFlagPath({ x: sb.stem.x, y: sb.stem.y2, isStemUp: sb.isStemUp, index: f })} />
                {/each}
              {/if}
              {#each sb.dots as dot, d (d)}
                <circle class="dot" cx={dot.x} cy={dot.y} r="1.5" />
              {/each}
            {/each}
            {#each staff.beams as beam, b (b)}
              <line class="beam" x1={beam.x1} x2={beam.x2} y1={beam.y} y2={beam.y} />
            {/each}
          </g>
        {/each}

        {#if cursor && cursor.systemIndex === systemIndex && cursor.measureIndex === measure.index}
          <rect class="measure-highlight" x={measure.x} y={system.top - 6} width={measure.width} height={system.bottom - system.top + 6} />
        {/if}

        {#if hasTab}
          {#each measure.beams as beam, i (i)}
            <line class="beam" x1={beam.x1} x2={beam.x2} y1={beam.y} y2={beam.y} />
          {/each}
          <line class="bar" x1={measure.x + measure.width} x2={measure.x + measure.width} y1={system.staffTop} y2={system.staffBottom} />
        {/if}
        {#if firstStaff && lastStaff}
          <line class="bar" x1={measure.x + measure.width} x2={measure.x + measure.width} y1={firstStaff.top} y2={lastStaff.bottom} />
        {/if}
      {/each}
      {#if cursor && cursor.systemIndex === systemIndex}
        <g class="cursor" aria-hidden="true">
          <line x1={cursor.x} x2={cursor.x} y1={system.top - 8} y2={system.bottom - 6} />
          <path d="M{cursor.x - 4} {system.top - 12} h8 l-4 5 z" />
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

  .string,
  .staff-line,
  .ledger {
    stroke: var(--staff-line);
    stroke-width: 1;
  }

  .ledger {
    stroke: var(--ink);
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

  .clef-glyph,
  .brace {
    fill: none;
    stroke: var(--ink);
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .clef-dot,
  .clef-bar {
    fill: var(--ink);
  }

  .cross-head path,
  .cross-head circle {
    fill: none;
    stroke: var(--ink);
    stroke-width: 1.5;
    stroke-linecap: round;
  }

  .octave {
    fill: var(--ink);
    font-size: 9px;
    font-weight: 600;
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

  .head {
    fill: var(--ink);
    stroke: var(--ink);
    stroke-width: 1;
  }

  .head.hollow {
    fill: var(--paper);
    stroke-width: 1.4;
  }

  .ghost-head {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.2;
    stroke-dasharray: 2 1.5;
  }

  .accidental {
    fill: none;
    stroke: var(--ink);
    stroke-width: 1.2;
    stroke-linecap: round;
  }

  .staff-rest {
    fill: none;
    stroke: var(--ink);
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .staff-rest.filled {
    fill: var(--ink);
    stroke-width: 0.6;
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
