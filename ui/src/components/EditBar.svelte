<script lang="ts">
  import { DRUM_PIECES, PALETTE, pieceName } from '../lib/drums'
  import type { Editor } from '../lib/editor.svelte'
  import { noteRow } from '../lib/notes'
  import type { NoteValue, Technique } from '../lib/score-types'

  let { editor }: { editor: Editor } = $props()

  const durations: { value: NoteValue; label: string; name: string; key: string }[] = [
    { value: 'Whole', label: '1', name: 'Whole note', key: 'W' },
    { value: 'Half', label: '1/2', name: 'Half note', key: 'H' },
    { value: 'Quarter', label: '1/4', name: 'Quarter note', key: 'Q' },
    { value: 'Eighth', label: '1/8', name: 'Eighth note', key: 'E' },
    { value: 'Sixteenth', label: '1/16', name: 'Sixteenth note', key: 'S' },
    { value: 'ThirtySecond', label: '1/32', name: 'Thirty-second note', key: 'T' },
  ]

  const techniques: { value: Technique; label: string; name: string }[] = [
    { value: 'HammerOn', label: 'H', name: 'Hammer-on' },
    { value: 'PullOff', label: 'P', name: 'Pull-off' },
    { value: 'Slide', label: '/', name: 'Slide (/)' },
    { value: 'Bend', label: 'B', name: 'Bend (B)' },
    { value: 'PalmMute', label: 'PM', name: 'Palm mute (M)' },
    { value: 'Harmonic', label: '◇', name: 'Harmonic' },
  ]

  const beat = $derived(editor.currentBeat)
  const isDrums = $derived(editor.isPercussion)
  const struck = $derived(new Set(beat?.notes.map(noteRow) ?? []))
  const inPalette = new Set(PALETTE.map((p) => p.key))
  const otherPieces = DRUM_PIECES.filter((p) => !inPalette.has(p.key))

  function strikeOther(event: Event) {
    const select = event.currentTarget as HTMLSelectElement
    const piece = Number(select.value)
    select.value = ''
    if (Number.isInteger(piece) && piece > 0) void editor.togglePiece(piece)
  }
  const note = $derived(editor.currentNote)
  const canEdit = $derived(editor.canEdit)

  const denominators = [1, 2, 4, 8, 16, 32]
  const signature = $derived(editor.currentTimeSignature)

  function changeNumerator(event: Event) {
    const input = event.currentTarget as HTMLInputElement
    const numerator = Math.round(Number(input.value))
    if (signature && Number.isFinite(numerator) && numerator >= 1 && numerator <= 255) {
      void editor.setTimeSignature({ numerator, denominator: signature.denominator })
    } else {
      input.value = String(signature?.numerator ?? 4)
    }
  }

  function changeDenominator(event: Event) {
    const select = event.currentTarget as HTMLSelectElement
    if (signature) void editor.setTimeSignature({ numerator: signature.numerator, denominator: Number(select.value) })
  }

  // Keep keyboard focus where it was, so that typing a fret still works after a click here.
  function keepFocus(event: MouseEvent) {
    event.preventDefault()
  }
</script>

<div class="edit-bar" role="toolbar" aria-label="Editing">
  <div class="group">
    <button
      type="button"
      class="tool"
      aria-label="Undo"
      title="Undo (Ctrl+Z)"
      disabled={!canEdit || !editor.document?.canUndo}
      onmousedown={keepFocus}
      onclick={() => editor.undo()}
    >
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 7 4 12l5 5V13c4.5 0 7 1.4 9 5-.5-5.2-3.4-9-9-9.4Z" /></svg>
    </button>
    <button
      type="button"
      class="tool"
      aria-label="Redo"
      title="Redo (Ctrl+Shift+Z)"
      disabled={!canEdit || !editor.document?.canRedo}
      onmousedown={keepFocus}
      onclick={() => editor.redo()}
    >
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m15 7 5 5-5 5V13c-4.5 0-7 1.4-9 5 .5-5.2 3.4-9 9-9.4Z" /></svg>
    </button>
  </div>

  <div class="group" role="group" aria-label="Note duration">
    {#each durations as d (d.value)}
      <button
        type="button"
        class="tool text"
        class:active={beat?.duration.value === d.value}
        aria-pressed={beat?.duration.value === d.value}
        title="{d.name} ({d.key})"
        disabled={!canEdit}
        onmousedown={keepFocus}
        onclick={() => editor.run({ type: 'duration', value: d.value })}
      >
        {d.label}
      </button>
    {/each}
    <button
      type="button"
      class="tool text"
      class:active={(beat?.duration.dots ?? 0) > 0}
      aria-pressed={(beat?.duration.dots ?? 0) > 0}
      title="Dotted (.)"
      disabled={!canEdit}
      onmousedown={keepFocus}
      onclick={() => editor.run({ type: 'dot' })}
    >
      •
    </button>
  </div>

  {#if isDrums}
    <div class="group drum-palette" role="group" aria-label="Drum pieces">
      {#each PALETTE as p (p.key)}
        <button
          type="button"
          class="tool text"
          class:active={struck.has(p.key)}
          aria-pressed={struck.has(p.key)}
          title="{pieceName(p.key)} ({p.digit}): strike it on the selected beat, or take it away"
          disabled={!canEdit}
          onmousedown={keepFocus}
          onclick={() => editor.togglePiece(p.key)}
        >
          {p.label}
        </button>
      {/each}
      <select class="more-pieces" aria-label="Other drum piece" title="Strike another piece on the selected beat" disabled={!canEdit} value="" onchange={strikeOther}>
        <option value="">More…</option>
        {#each otherPieces as p (p.key)}
          <option value={p.key}>{struck.has(p.key) ? '✓ ' : ''}{p.name}</option>
        {/each}
      </select>
    </div>
  {/if}

  <div class="group" role="group" aria-label="Note effects">
    <button
      type="button"
      class="tool text"
      class:active={note?.tied === true}
      aria-pressed={note?.tied === true}
      title="Tie to the previous note (L)"
      disabled={!canEdit}
      onmousedown={keepFocus}
      onclick={() => editor.run({ type: 'tie' })}
    >
      Tie
    </button>
    {#each isDrums ? [] : techniques as t (t.value)}
      <button
        type="button"
        class="tool text"
        class:active={note?.technique === t.value}
        aria-pressed={note?.technique === t.value}
        title={t.name}
        disabled={!canEdit}
        onmousedown={keepFocus}
        onclick={() => editor.run({ type: 'technique', technique: t.value })}
      >
        {t.label}
      </button>
    {/each}
  </div>

  <div class="group">
    <button
      type="button"
      class="tool text"
      title="Insert a measure after this one (Ctrl+Enter)"
      disabled={!canEdit}
      onmousedown={keepFocus}
      onclick={() => editor.insertMeasure()}
    >
      + Measure
    </button>
    <button
      type="button"
      class="tool text"
      title="Delete this measure, or every measure of the selected beats (Ctrl+Delete)"
      disabled={!canEdit || !editor.canDeleteMeasures}
      onmousedown={keepFocus}
      onclick={() => editor.deleteMeasure()}
    >
      − Measure
    </button>
  </div>

  <div class="group" role="group" aria-label="Clipboard">
    <button type="button" class="tool text" title="Copy the selected beats (Ctrl+C). Shift+arrows select several" disabled={!canEdit || !editor.selection} onmousedown={keepFocus} onclick={() => editor.run({ type: 'copy' })}>Copy</button>
    <button type="button" class="tool text" title="Cut the selected beats (Ctrl+X)" disabled={!canEdit || !editor.selection} onmousedown={keepFocus} onclick={() => editor.run({ type: 'cut' })}>Cut</button>
    <button type="button" class="tool text" title="Paste at the selected beat (Ctrl+V)" disabled={!canEdit || !editor.selection || !editor.clipboard} onmousedown={keepFocus} onclick={() => editor.run({ type: 'paste' })}>Paste</button>
  </div>

  <div class="group time-signature" role="group" aria-label="Time signature of the selected measure">
    <input
      type="number"
      min="1"
      max="255"
      value={signature?.numerator ?? 4}
      disabled={!canEdit}
      aria-label="Beats per measure"
      title="Time signature: applies to this measure and the next ones that share it"
      onchange={changeNumerator}
    />
    <span aria-hidden="true">/</span>
    <select
      value={signature?.denominator ?? 4}
      disabled={!canEdit}
      aria-label="Beat unit"
      onchange={changeDenominator}
    >
      {#each denominators as d (d)}
        <option value={d}>{d}</option>
      {/each}
    </select>
  </div>
</div>

<style>
  .edit-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 18px;
    padding: 8px 20px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }

  .group {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .tool {
    display: grid;
    place-items: center;
    min-width: 32px;
    height: 30px;
    padding: 0 8px;
    border: 1px solid transparent;
    border-radius: 7px;
    background: none;
    color: var(--ink);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }

  .tool.text {
    font-variant-numeric: tabular-nums;
  }

  .time-signature {
    color: var(--ink-muted);
  }

  .drum-palette {
    flex-wrap: wrap;
  }

  .more-pieces,
  .time-signature input,
  .time-signature select {
    height: 28px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--paper);
    color: var(--ink);
    font: inherit;
    font-size: 13px;
  }

  .time-signature input {
    width: 52px;
    padding: 0 6px;
  }

  .tool svg {
    width: 17px;
    height: 17px;
    fill: currentColor;
  }

  .tool:hover:not(:disabled) {
    background: var(--surface-2);
  }

  .tool.active {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
    font-weight: 600;
  }

  .tool:disabled {
    color: var(--ink-muted);
    opacity: 0.45;
    cursor: not-allowed;
  }

  .tool:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>
