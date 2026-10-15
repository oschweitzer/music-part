<script lang="ts">
  import ContextMenu, { type MenuItem } from './components/ContextMenu.svelte'
  import EditBar from './components/EditBar.svelte'
  import TabView from './components/TabView.svelte'
  import TrackRows from './components/TrackRows.svelte'
  import { createDocumentApi, type ExportFormat } from './lib/document-api'
  import { Editor } from './lib/editor.svelte'
  import { createFilePrompts, guardWindowClose, isScoreFile, onFilesDropped, setWindowTitle } from './lib/files'
  import { INSTRUMENTS, instrumentLabel, isPitchedTrack, soundsOf } from './lib/instruments'
  import { interpretKey } from './lib/keymap'
  import type { Instrument, SoundId } from './lib/score-types'
  import type { Selection } from './lib/selection'
  import { formatTuning } from './lib/tuning'
  import { Playback } from './lib/playback.svelte'
  import { formatClock, ticksToSeconds, totalTicks } from './lib/score-time'
  import { layoutTab, locateTick } from './lib/tab-layout'
  import { applyTheme, nextTheme, readTheme, saveTheme, themeLabel, type Theme } from './lib/theme'
  import { createTransport } from './lib/transport'
  import {
    dropTrackFromView,
    FRETTED_VIEWS,
    frettedViewLabel,
    readFrettedView,
    readViewMode,
    saveFrettedView,
    saveViewMode,
    shownTracks,
    toggleCollapsed,
    viewModeLabel,
  } from './lib/track-view'
  import type { FrettedView, TrackView, ViewMode } from './lib/track-view'

  const playback = new Playback(createTransport())
  const editor = new Editor({ api: createDocumentApi(), playback, prompts: createFilePrompts() })

  let theme = $state<Theme>(readTheme())

  function cycleTheme() {
    theme = nextTheme(theme)
    applyTheme({ theme, root: document.documentElement })
    saveTheme(theme)
  }

  let view = $state<TrackView>({ mode: readViewMode(), collapsed: [] })
  const viewModes: ViewMode[] = ['tabs', 'rows']

  function changeViewMode(mode: ViewMode) {
    view = { ...view, mode }
    saveViewMode(mode)
  }

  let fretted = $state<FrettedView>(readFrettedView())

  function changeFrettedView(next: FrettedView) {
    fretted = next
    saveFrettedView(next)
  }

  const frettedViewTitle: Record<FrettedView, string> = {
    tab: 'Write string instruments as tablature',
    staff: 'Write string instruments on a staff',
    both: 'Write string instruments on a staff above their tablature',
  }

  let loadError = $state<string | null>(null)
  let paperWidth = $state(0)
  let stage = $state<HTMLElement>()
  let paper = $state<HTMLElement>()
  let isDragging = $state(false)

  const score = $derived(editor.document?.score ?? null)
  const track = $derived(score?.tracks[editor.track] ?? null)
  const shown = $derived(shownTracks({ view, trackCount: score?.tracks.length ?? 0, current: editor.track }))
  const isRows = $derived(view.mode === 'rows')
  const layout = $derived(track && !isRows && paperWidth > 0 ? layoutTab({ track, width: paperWidth, fretted }) : null)

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
    const bottom = paper.offsetTop + system.bottom + 20
    const isHidden = top < stage.scrollTop || bottom > stage.scrollTop + stage.clientHeight
    if (isHidden) stage.scrollTo({ top: Math.max(top - 32, 0), behavior: 'smooth' })
  })

  function handleKeydown(event: KeyboardEvent) {
    if (menu) {
      menu = null
      // Escape only closes the menu; it must not also clear the selection.
      if (event.key === 'Escape') {
        event.preventDefault()
        return
      }
    }
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


  /** Removes a track; its row fold goes with it, once the score really lost the track. */
  async function removeTrack(index: number) {
    const before = score?.tracks.length ?? 0
    await editor.removeTrack(index)
    if ((score?.tracks.length ?? 0) < before) view = dropTrackFromView({ view, track: index })
  }

  function pickTrack(event: Event) {
    editor.selectTrack(Number((event.currentTarget as HTMLSelectElement).value))
  }

  // With many tracks the tabs scroll sideways: keep the tab of the edited track in view.
  let tabList = $state<HTMLElement>()
  $effect(() => {
    const active = editor.track
    const tab = tabList?.children[active] as HTMLElement | undefined
    if (!tabList || !tab) return
    if (tab.offsetLeft < tabList.scrollLeft) tabList.scrollLeft = tab.offsetLeft
    else if (tab.offsetLeft + tab.offsetWidth > tabList.scrollLeft + tabList.clientWidth) {
      tabList.scrollLeft = tab.offsetLeft + tab.offsetWidth - tabList.clientWidth
    }
  })

  function addTrack(event: MouseEvent, instrument: Instrument, sound: SoundId) {
    ;(event.currentTarget as HTMLElement).closest('details')?.removeAttribute('open')
    void editor.addTrack(instrument, sound)
  }

  async function changeName(event: Event) {
    const input = event.currentTarget as HTMLInputElement
    await editor.renameTrack(input.value)
    input.value = track?.name ?? ''
  }

  function changeInstrument(event: Event) {
    void editor.setInstrument((event.currentTarget as HTMLSelectElement).value as Instrument)
  }

  function changeSound(event: Event) {
    void editor.setSound((event.currentTarget as HTMLSelectElement).value as SoundId)
  }

  async function changeTuning(event: Event) {
    const input = event.currentTarget as HTMLInputElement
    await editor.setTuning(input.value)
    // Show the tuning the score really has, whether or not the new one was accepted.
    input.value = formatTuning(track?.tuning ?? [])
  }

  // Keep focus where it was, so that typing a fret still works after clicking a toolbar button.
  // The menu of a right-click on the tablature acts on the selection.
  let menu: { x: number; y: number } | null = $state(null)

  function openMenu(target: Selection, at: { x: number; y: number }, track: number = editor.track) {
    editor.selectForMenu(target, { track })
    menu = at
  }

  const menuItems = $derived.by((): MenuItem[] => {
    const measures = editor.selectedMeasures
    const count = measures?.count ?? 1
    return [
      { label: 'Cut', onselect: () => void editor.run({ type: 'cut' }) },
      { label: 'Copy', onselect: () => void editor.run({ type: 'copy' }) },
      { label: 'Paste', disabled: !editor.clipboard, onselect: () => void editor.run({ type: 'paste' }) },
      'separator',
      { label: 'Insert measure before', onselect: () => void editor.insertMeasure({ before: true }) },
      { label: 'Insert measure after', onselect: () => void editor.insertMeasure() },
      {
        label: count === 1 ? `Delete measure ${(measures?.from ?? 0) + 1}` : `Delete ${count} measures`,
        danger: true,
        disabled: !editor.canDeleteMeasures,
        onselect: () => void editor.deleteMeasure(),
      },
    ]
  })

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
    {/if}

    <div class="right">
      <div class="view-switch" role="group" aria-label="Track view">
        {#each viewModes as mode (mode)}
          <button
            type="button"
            aria-pressed={view.mode === mode}
            title={mode === 'tabs' ? 'One track at a time, chosen from its tab' : 'Every track in a row, one under the other'}
            onmousedown={keepFocus}
            onclick={() => changeViewMode(mode)}>{viewModeLabel[mode]}</button
          >
        {/each}
      </div>
      <div class="view-switch" role="group" aria-label="String notation">
        {#each FRETTED_VIEWS as mode (mode)}
          <button
            type="button"
            aria-pressed={fretted === mode}
            title={frettedViewTitle[mode]}
            onmousedown={keepFocus}
            onclick={() => changeFrettedView(mode)}>{frettedViewLabel[mode]}</button
          >
        {/each}
      </div>
      {#if score}
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
      <button
        type="button"
        class="icon-button small theme-button"
        aria-label="Theme: {themeLabel[theme]}"
        title="Theme: {themeLabel[theme]}. Click for {themeLabel[nextTheme(theme)]}"
        onmousedown={keepFocus}
        onclick={cycleTheme}
      >
        {#if theme === 'light'}
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <circle cx="12" cy="12" r="4.2" />
            <path d="M12 2.5v2.6M12 18.9v2.6M2.5 12h2.6M18.9 12h2.6M5.3 5.3l1.8 1.8M16.9 16.9l1.8 1.8M5.3 18.7l1.8-1.8M16.9 7.1l1.8-1.8" />
          </svg>
        {:else if theme === 'dark'}
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20.5 14.2A8.6 8.6 0 0 1 9.8 3.5a8.6 8.6 0 1 0 10.7 10.7Z" /></svg>
        {:else}
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <circle cx="12" cy="12" r="8.2" fill="none" />
            <path d="M12 3.8a8.2 8.2 0 0 1 0 16.4Z" fill="currentColor" />
          </svg>
        {/if}
      </button>
    </div>
  </header>

  {#if banner}
    <div class="notice {banner.tone}" role={banner.tone === 'error' ? 'alert' : 'status'}>
      <span>{banner.text}</span>
      <button type="button" onclick={dismissBanner}>Dismiss</button>
    </div>
  {/if}

  {#if score}
    <nav class="tracks" aria-label="Tracks">
      {#if !isRows}
        <div class="tab-list" bind:this={tabList}>
          {#each score.tracks as t, i (i)}
            <div class="track-tab" class:active={i === editor.track}>
              <button type="button" class="name" onmousedown={keepFocus} onclick={() => editor.selectTrack(i)}>
                {t.name}
                <span class="strings">{isPitchedTrack(t) ? instrumentLabel(t.instrument) : `${t.tuning.length} strings`}</span>
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
              {#if editor.canEdit}
                <button
                  type="button"
                  class="mix drop"
                  aria-label="Remove {t.name}"
                  title="Remove this track (Ctrl+Z brings it back)"
                  disabled={score.tracks.length < 2}
                  onmousedown={keepFocus}
                  onclick={() => removeTrack(i)}>×</button>
              {/if}
            </div>
          {/each}
        </div>
      {/if}

      {#if editor.canEdit}
        <details class="add-track">
          <summary title="Add a track">+ Track</summary>
          <div class="menu">
            {#each INSTRUMENTS as instrument (instrument)}
              <div class="family" role="group" aria-label={instrumentLabel(instrument)}>
                <span>{instrumentLabel(instrument)}</span>
                {#each soundsOf(instrument) as sound (sound.id)}
                  <button
                    type="button"
                    class="sound"
                    aria-label="{instrumentLabel(instrument)}: {sound.name}"
                    onmousedown={keepFocus}
                    onclick={(event) => addTrack(event, instrument, sound.id)}>{sound.name}</button>
                {/each}
              </div>
            {/each}
          </div>
        </details>
      {/if}

      <select class="track-picker" value={editor.track} aria-label="Track to edit" title="The track to edit" onchange={pickTrack}>
        {#each score.tracks as t, i (i)}
          <option value={i}>{i + 1}. {t.name}</option>
        {/each}
      </select>

      {#if editor.canEdit}
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
          <label class="track-field" title="The instrument of this track. Between string instruments the strings stay as they are.">
            Instrument
            <select value={track.instrument} aria-label="Instrument of this track" onchange={changeInstrument}>
              {#each INSTRUMENTS as instrument (instrument)}
                <option value={instrument}>{instrumentLabel(instrument)}</option>
              {/each}
            </select>
          </label>
          <label class="track-field" title="How this track sounds, among the sounds of its instrument.">
            Sound
            <select value={track.sound} aria-label="Sound of this track" onchange={changeSound}>
              {#each soundsOf(track.instrument) as sound (sound.id)}
                <option value={sound.id}>{sound.name}</option>
              {/each}
            </select>
          </label>
          {#if !isPitchedTrack(track)}
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
          {/if}
        {/if}
      {/if}
    </nav>

    <EditBar {editor} />
  {/if}

  <main class="stage" class:dragging={isDragging} class:rows={isRows && score !== null} bind:this={stage}>
    {#if isDragging}
      <div class="drop-hint" aria-hidden="true">Drop a score or MIDI file to open it</div>
    {/if}
    {#if loadError}
      <p class="message error" role="alert">{loadError}</p>
    {:else if !track || !score}
      <p class="message">Loading…</p>
    {:else if isRows}
      <TrackRows
        {score}
        {shown}
        {fretted}
        {editor}
        {playback}
        ontogglecollapse={(index) => (view = toggleCollapsed({ view, track: index }))}
        onremove={(index) => void removeTrack(index)}
        onmenu={openMenu}
      />
    {:else}
      <div class="paper" bind:this={paper} bind:clientWidth={paperWidth}>
        {#if layout}
          <TabView
            {layout}
            label={track.name}
            {cursor}
            selection={editor.selection}
            onselect={editor.canEdit ? (target, options) => editor.select(target, options) : undefined}
            ondrag={editor.canEdit ? (area) => editor.dragSelect(area) : undefined}
            onseek={(tick) => playback.seek(tick)}
            onenter={editor.canEdit ? (target) => editor.enterPitch(target) : undefined}
            onmenu={editor.canEdit ? openMenu : undefined}
          />
        {/if}
      </div>
    {/if}
  </main>

  {#if menu}
    <ContextMenu items={menuItems} x={menu.x} y={menu.y} onclose={() => (menu = null)} />
  {/if}
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

  .right {
    grid-column: 3;
    justify-self: end;
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .icon-button.small {
    width: 30px;
    height: 30px;
  }

  .theme-button svg {
    width: 16px;
    height: 16px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
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

  .tab-list {
    display: flex;
    flex: 0 1 auto;
    gap: 6px;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: thin;
  }

  .track-tab {
    flex: none;
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

  .track-picker {
    max-width: 160px;
    margin-left: auto;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--paper);
    color: var(--ink);
    font: inherit;
    font-size: 13px;
  }

  .view-switch {
    display: flex;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
  }

  .view-switch button {
    padding: 3px 10px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--ink-muted);
    font: inherit;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
  }

  .view-switch button[aria-pressed='true'] {
    background: var(--surface);
    color: var(--ink);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.12);
  }

  .add-track .family {
    display: flex;
    flex-direction: column;
    white-space: nowrap;
  }

  .add-track .family span {
    padding: 6px 10px 2px;
    color: var(--ink-muted);
    font-size: 12px;
  }

  .add-track .menu button.sound {
    padding-left: 20px;
  }

  .track-field,
  .tuning {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--ink-muted);
    font-size: 12px;
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

  .mix.drop:hover:not(:disabled) {
    border-color: var(--danger);
    color: var(--danger);
  }

  .mix:disabled {
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

  .stage.rows {
    padding: 0;
    overflow: hidden;
  }

  .drop-hint {
    position: absolute;
    inset: 12px;
    z-index: 3;
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
      grid-template-columns: 1fr auto auto;
    }

    .right {
      grid-column: auto;
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
    width: 4.5rem;
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
