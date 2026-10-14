<script lang="ts">
  import EditBar from './components/EditBar.svelte'
  import TabView from './components/TabView.svelte'
  import { createDocumentApi, type ExportFormat } from './lib/document-api'
  import { Editor } from './lib/editor.svelte'
  import { createFilePrompts, guardWindowClose, isScoreFile, onFilesDropped, setWindowTitle } from './lib/files'
  import { interpretKey } from './lib/keymap'
  import type { Instrument } from './lib/score-types'
  import { formatTuning } from './lib/tuning'
  import { Playback } from './lib/playback.svelte'
  import { formatClock, ticksToSeconds, totalTicks } from './lib/score-time'
  import { layoutTab, locateTick } from './lib/tab-layout'
  import { createTransport } from './lib/transport'

  const playback = new Playback(createTransport())
  const editor = new Editor({ api: createDocumentApi(), playback, prompts: createFilePrompts() })

  let loadError = $state<string | null>(null)
  let paperWidth = $state(0)
  let stage = $state<HTMLElement>()
  let paper = $state<HTMLElement>()
  let isDragging = $state(false)

  const score = $derived(editor.document?.score ?? null)
  const track = $derived(score?.tracks[editor.track] ?? null)
  const layout = $derived(track && paperWidth > 0 ? layoutTab({ track, width: paperWidth }) : null)

  const hasStarted = $derived(playback.isPlaying || playback.tick > 0)
  const cursor = $derived(layout && hasStarted ? locateTick({ layout, tick: playback.tick }) : null)

  const banner = $derived(
    editor.message ?? (playback.error ? { tone: 'error' as const, text: playback.error } : null),
  )

  const duration = $derived(score ? ticksToSeconds({ ticks: totalTicks(score), tempoBpm: score.tempo_bpm }) : 0)
  const elapsed = $derived(score ? ticksToSeconds({ ticks: playback.tick, tempoBpm: score.tempo_bpm }) : 0)

  $effect(() => {
    playback.start()
    editor.start().catch((error: unknown) => (loadError = error instanceof Error ? error.message : String(error)))
    return () => playback.dispose()
  })

  $effect(() => {
    const stopListening = onFilesDropped({
      onHover: (isHovering) => (isDragging = isHovering),
      onDrop: (paths) => {
        const path = paths.find(isScoreFile)
        if (path) void editor.openPath(path)
        else editor.message = { tone: 'error', text: 'Only .mpart, MIDI (.mid) and MusicXML (.musicxml, .xml, .mxl) files can be opened.' }
      },
    })
    return () => void stopListening.then((stop) => stop())
  })

  // Closing the window with unsaved changes asks first.
  $effect(() => {
    const stopGuarding = guardWindowClose(() => editor.canDiscard())
    return () => void stopGuarding.then((stop) => stop())
  })

  $effect(() => {
    setWindowTitle(`${editor.isDirty ? '• ' : ''}${score?.title ?? 'Music Part'} — Music Part`)
  })

  // Keep the line being played in view: scroll when the cursor moves to a system that is hidden.
  $effect(() => {
    const index = cursor?.systemIndex
    const system = index === undefined ? undefined : layout?.systems[index]
    if (!system || !stage || !paper) return

    const top = paper.offsetTop + system.y
    const bottom = top + (system.staffBottom - system.y) + 60
    const isHidden = top < stage.scrollTop || bottom > stage.scrollTop + stage.clientHeight
    if (isHidden) stage.scrollTo({ top: Math.max(top - 32, 0), behavior: 'smooth' })
  })

  function handleKeydown(event: KeyboardEvent) {
    const target = event.target as HTMLElement
    // Typing in a field (the tempo) is not editing the score.
    if (target.closest('input, textarea, select, [contenteditable]')) return

    const command = interpretKey(event)
    if (!command) return
    // On a focused button, Space and Enter belong to the button itself.
    if (target.closest('button') && (event.key === ' ' || event.key === 'Enter') && !event.ctrlKey && !event.metaKey) return

    event.preventDefault()
    if (command.type === 'togglePlayback') void playback.toggle()
    else if (command.type === 'rewind') void playback.seek(0)
    else void editor.run(command)
  }

  function changeTempo(event: Event) {
    const input = event.currentTarget as HTMLInputElement
    const bpm = Math.round(Number(input.value))
    if (Number.isFinite(bpm) && bpm >= 1 && bpm <= 999) void editor.setTempo(bpm)
    else input.value = String(score?.tempo_bpm ?? 120)
  }

  function exportAs(event: MouseEvent, format: ExportFormat) {
    ;(event.currentTarget as HTMLElement).closest('details')?.removeAttribute('open')
    void editor.exportAs(format)
  }

  const instruments: Instrument[] = ['Guitar', 'Bass', 'Violin']

  function addTrack(event: MouseEvent, instrument: Instrument) {
    ;(event.currentTarget as HTMLElement).closest('details')?.removeAttribute('open')
    void editor.addTrack(instrument)
  }

  async function changeName(event: Event) {
    const input = event.currentTarget as HTMLInputElement
    await editor.renameTrack(input.value)
    input.value = track?.name ?? ''
  }

  function changeInstrument(event: Event) {
    void editor.setInstrument((event.currentTarget as HTMLSelectElement).value as Instrument)
  }

  async function changeTuning(event: Event) {
    const input = event.currentTarget as HTMLInputElement
    await editor.setTuning(input.value)
    // Show the tuning the score really has, whether or not the new one was accepted.
    input.value = formatTuning(track?.tuning ?? [])
  }

  // Keep focus where it was, so that typing a fret still works after clicking a toolbar button.
  function keepFocus(event: MouseEvent) {
    event.preventDefault()
  }

  function dismissBanner() {
    editor.dismissMessage()
    playback.dismissError()
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="app">
  <header class="toolbar">
    <div class="brand">
      <span class="logo" aria-hidden="true"></span>
      <h1 title={editor.document?.path ?? 'Not saved yet'}>
        {score?.title ?? 'Music Part'}{#if editor.isDirty}<span class="dirty" title="Unsaved changes">•</span>{/if}
      </h1>
      <div class="file-buttons" role="group" aria-label="File">
        <button type="button" class="file-button" disabled={!editor.canEdit} title="New score (Ctrl+N)" onmousedown={keepFocus} onclick={() => editor.newDocument()}>New</button>
        <button type="button" class="file-button" disabled={!editor.canEdit} title="Open a score or MIDI file (Ctrl+O)" onmousedown={keepFocus} onclick={() => editor.openFile()}>Open</button>
        <button type="button" class="file-button" disabled={!editor.canEdit} title="Save (Ctrl+S)" onmousedown={keepFocus} onclick={() => editor.save()}>Save</button>
        <details class="export-menu">
          <summary class="file-button" title="Write the score as a MIDI or MusicXML file">Export</summary>
          <div class="menu">
            <button type="button" disabled={!editor.canEdit} onmousedown={keepFocus} onclick={(event) => exportAs(event, 'midi')}>MIDI file (.mid)</button>
            <button type="button" disabled={!editor.canEdit} onmousedown={keepFocus} onclick={(event) => exportAs(event, 'musicXml')}>MusicXML (.musicxml)</button>
          </div>
        </details>
      </div>
    </div>

    {#if score}
      <div class="transport" role="group" aria-label="Playback">
        <button type="button" class="icon-button" aria-label="Stop" title="Stop" onmousedown={keepFocus} onclick={() => playback.stop()}>
          <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="6" y="6" width="12" height="12" rx="2" /></svg>
        </button>
        <button
          type="button"
          class="icon-button primary"
          aria-label={playback.isPlaying ? 'Pause' : 'Play'}
          title={playback.isPlaying ? 'Pause (Space)' : 'Play (Space)'}
          onmousedown={keepFocus}
          onclick={() => playback.toggle()}
        >
          {#if playback.isPlaying}
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <rect x="6" y="5" width="4" height="14" rx="1.2" />
              <rect x="14" y="5" width="4" height="14" rx="1.2" />
            </svg>
          {:else}
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5.5v13a1 1 0 0 0 1.5.86l10.4-6.5a1 1 0 0 0 0-1.72L9.5 4.64A1 1 0 0 0 8 5.5Z" /></svg>
          {/if}
        </button>
        <span class="clock" aria-label="Position">{formatClock(elapsed)} / {formatClock(duration)}</span>
      </div>

      <label class="tempo" title="Tempo (beats per minute)">
        <span class="tempo-note" aria-hidden="true">♩</span> =
        <input
          type="number"
          min="1"
          max="999"
          value={score.tempo_bpm}
          disabled={!editor.canEdit}
          aria-label="Tempo in beats per minute"
          onchange={changeTempo}
        />
      </label>
    {/if}
  </header>

  {#if banner}
    <div class="notice {banner.tone}" role={banner.tone === 'error' ? 'alert' : 'status'}>
      <span>{banner.text}</span>
      <button type="button" onclick={dismissBanner}>Dismiss</button>
    </div>
  {/if}

  {#if score}
    <nav class="tracks" aria-label="Tracks">
      {#each score.tracks as t, i (i)}
        <div class="track-tab" class:active={i === editor.track}>
          <button type="button" class="name" onmousedown={keepFocus} onclick={() => editor.selectTrack(i)}>
            {t.name}
            <span class="strings">{t.tuning.length} strings</span>
          </button>
          <button
            type="button"
            class="mix"
            class:on={playback.muted[i]}
            aria-pressed={playback.muted[i] === true}
            aria-label="Mute {t.name}"
            title="Mute"
            onmousedown={keepFocus}
            onclick={() => playback.toggleMute(i)}>M</button>
          <button
            type="button"
            class="mix"
            class:on={playback.solo[i]}
            aria-pressed={playback.solo[i] === true}
            aria-label="Solo {t.name}"
            title="Solo: play only the soloed tracks"
            onmousedown={keepFocus}
            onclick={() => playback.toggleSolo(i)}>S</button>
        </div>
      {/each}

      {#if editor.canEdit}
        <details class="add-track">
          <summary title="Add a track">+ Track</summary>
          <div class="menu">
            {#each instruments as instrument (instrument)}
              <button type="button" onmousedown={keepFocus} onclick={(event) => addTrack(event, instrument)}>{instrument}</button>
            {/each}
          </div>
        </details>

        {#if track}
          <label class="track-field" title="Name of this track">
            Name
            <input
              type="text"
              value={track.name}
              maxlength="64"
              spellcheck="false"
              aria-label="Name of this track"
              onchange={changeName}
            />
          </label>
          <label class="track-field" title="The sound of this track. The strings stay as they are.">
            Sound
            <select value={track.instrument} aria-label="Instrument of this track" onchange={changeInstrument}>
              {#each instruments as instrument (instrument)}
                <option value={instrument}>{instrument}</option>
              {/each}
            </select>
          </label>
          <label class="tuning" title="Open strings from the highest to the lowest, e.g. E4 B3 G3 D3 A2 D2">
            Tuning
            <input
              type="text"
              value={formatTuning(track.tuning)}
              spellcheck="false"
              aria-label="Tuning of this track"
              onchange={changeTuning}
            />
          </label>
          <button
            type="button"
            class="remove-track"
            title="Remove this track"
            disabled={(score?.tracks.length ?? 0) < 2}
            onmousedown={keepFocus}
            onclick={() => editor.removeTrack()}
          >
            Remove track
          </button>
        {/if}
      {/if}
    </nav>

    <EditBar {editor} />
  {/if}

  <main class="stage" class:dragging={isDragging} bind:this={stage}>
    {#if isDragging}
      <div class="drop-hint" aria-hidden="true">Drop a score or MIDI file to open it</div>
    {/if}
    {#if loadError}
      <p class="message error" role="alert">{loadError}</p>
    {:else if !track}
      <p class="message">Loading…</p>
    {:else}
      <div class="paper" bind:this={paper} bind:clientWidth={paperWidth}>
        {#if layout}
          <TabView
            {layout}
            label={track.name}
            {cursor}
            selection={editor.selection}
            onselect={editor.canEdit ? (target, options) => editor.select(target, options) : undefined}
            onseek={(tick) => playback.seek(tick)}
          />
        {/if}
      </div>
    {/if}
  </main>
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  .toolbar {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 16px;
    padding: 10px 20px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .logo {
    flex: none;
    width: 22px;
    height: 22px;
    border-radius: 6px;
    background: var(--accent);
  }

  h1 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    letter-spacing: -0.01em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .transport {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .icon-button {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 50%;
    background: var(--surface-2);
    color: var(--ink);
    cursor: pointer;
    transition: background 120ms, transform 120ms;
  }

  .icon-button svg {
    width: 18px;
    height: 18px;
    fill: currentColor;
  }

  .icon-button:hover {
    background: var(--border);
  }

  .icon-button:active {
    transform: scale(0.94);
  }

  .icon-button.primary {
    width: 42px;
    height: 42px;
    border-color: transparent;
    background: var(--accent);
    color: var(--on-accent);
  }

  .icon-button.primary:hover {
    background: var(--accent-hover);
  }

  button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .clock {
    min-width: 84px;
    margin-left: 6px;
    color: var(--ink-muted);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }

  .tempo {
    display: flex;
    align-items: center;
    gap: 4px;
    justify-self: end;
    padding: 4px 10px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--ink-muted);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }

  .notice {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 8px 20px;
    background: var(--danger-bg);
    color: var(--danger);
    border-bottom: 1px solid var(--border);
    font-size: 13px;
  }

  .notice.info {
    background: var(--surface-2);
    color: var(--ink);
  }

  .notice button {
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .tracks {
    display: flex;
    gap: 6px;
    padding: 10px 20px 0;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }

  .track-tab {
    display: flex;
    align-items: center;
    gap: 2px;
    border-bottom: 2px solid transparent;
  }

  .track-tab.active {
    border-bottom-color: var(--accent);
  }

  .track-tab .name {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 8px 8px 8px 14px;
    border: 0;
    background: none;
    color: var(--ink-muted);
    font: inherit;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
  }

  .track-tab .name:hover,
  .track-tab.active .name {
    color: var(--ink);
  }

  .mix {
    width: 20px;
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

  .tracks {
    align-items: center;
  }

  .add-track {
    position: relative;
  }

  .add-track summary {
    padding: 8px 10px;
    color: var(--ink-muted);
    font-size: 13px;
    cursor: pointer;
    list-style: none;
  }

  .add-track summary:hover {
    color: var(--ink);
  }

  .add-track .menu {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 2;
    display: flex;
    flex-direction: column;
    min-width: 120px;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    box-shadow: var(--shadow);
  }

  .add-track .menu button {
    padding: 6px 10px;
    border: 0;
    border-radius: 5px;
    background: none;
    color: var(--ink);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .add-track .menu button:hover {
    background: var(--surface-2);
  }

  .track-field,
  .tuning {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--ink-muted);
    font-size: 12px;
  }

  .track-field:first-of-type {
    margin-left: auto;
  }

  .track-field input,
  .track-field select,
  .tuning input {
    width: 130px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--paper);
    color: var(--ink);
    font: inherit;
    font-size: 13px;
  }

  .tuning input {
    width: 150px;
  }

  .remove-track {
    padding: 4px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: none;
    color: var(--danger);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }

  .remove-track:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .strings {
    color: var(--ink-muted);
    font-size: 11px;
    font-weight: 400;
  }

  .stage {
    position: relative;
    flex: 1;
    overflow: auto;
    padding: 24px;
  }

  .drop-hint {
    position: absolute;
    inset: 12px;
    z-index: 1;
    display: grid;
    place-items: center;
    border: 2px dashed var(--accent);
    border-radius: 12px;
    background: color-mix(in srgb, var(--bg) 82%, transparent);
    color: var(--accent);
    font-size: 16px;
    font-weight: 600;
    pointer-events: none;
  }

  .paper {
    max-width: 1100px;
    margin: 0 auto;
    background: var(--paper);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow);
    overflow: hidden;
  }

  .message {
    text-align: center;
    color: var(--ink-muted);
  }

  .message.error {
    color: var(--danger);
  }

  @media (max-width: 760px) {
    .toolbar {
      grid-template-columns: 1fr auto;
    }

    .tempo {
      display: none;
    }
  }

  .dirty {
    margin-left: 6px;
    color: var(--accent);
  }

  .file-buttons {
    display: flex;
    flex: none;
    gap: 4px;
    margin-left: 8px;
  }

  .export-menu {
    position: relative;
  }

  .export-menu summary {
    list-style: none;
    cursor: pointer;
  }

  .export-menu .menu {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 3;
    display: flex;
    flex-direction: column;
    min-width: 190px;
    margin-top: 4px;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    box-shadow: var(--shadow);
  }

  .export-menu .menu button {
    padding: 6px 10px;
    border: 0;
    border-radius: 5px;
    background: none;
    color: var(--ink);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .export-menu .menu button:hover:not(:disabled) {
    background: var(--surface-2);
  }

  .file-button {
    padding: 5px 11px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--surface);
    color: var(--ink);
    font: inherit;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
  }

  .file-button:hover:not(:disabled) {
    background: var(--surface-2);
  }

  .file-button:disabled {
    color: var(--ink-muted);
    opacity: 0.55;
    cursor: not-allowed;
  }

  .tempo input {
    width: 46px;
    padding: 2px 4px;
    border: 1px solid transparent;
    border-radius: 5px;
    background: transparent;
    color: var(--ink);
    font: inherit;
    font-variant-numeric: tabular-nums;
    text-align: left;
  }

  .tempo input:hover:not(:disabled),
  .tempo input:focus {
    border-color: var(--border);
    background: var(--surface);
  }
</style>
