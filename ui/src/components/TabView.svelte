<script lang="ts">
  import { METRICS } from '../lib/tab-layout'
  import type { BeatLayout, CursorPosition, TabLayout } from '../lib/tab-layout'

  let { layout, label, cursor = null }: { layout: TabLayout; label: string; cursor?: CursorPosition | null } = $props()

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

<svg
  class="tab"
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

        {#if measure.timeSignature}
          <g class="time-signature" text-anchor="middle" dominant-baseline="central">
            <text x={measure.x + 18} y={staffMid - letterGap / 2 - 1}>{measure.timeSignature.numerator}</text>
            <text x={measure.x + 18} y={staffMid + letterGap / 2 + 1}>{measure.timeSignature.denominator}</text>
          </g>
        {/if}

        {#each measure.beats as beat (beat.startTick)}
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

          {#each beat.notes as note (note.string)}
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
