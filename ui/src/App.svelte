<script lang="ts">
  import TabView from './components/TabView.svelte'
  import { Playback } from './lib/playback.svelte'
  import { loadDemoScore } from './lib/score-source'
  import { formatClock, ticksToSeconds, totalTicks } from './lib/score-time'
  import type { Score } from './lib/score-types'
  import { layoutTab, locateTick } from './lib/tab-layout'
  import { createTransport } from './lib/transport'

  const playback = new Playback(createTransport())

  let score = $state<Score | null>(null)
  let loadError = $state<string | null>(null)
  let selectedTrack = $state(0)
  let paperWidth = $state(0)
  let stage = $state<HTMLElement>()
  let paper = $state<HTMLElement>()

  const track = $derived(score?.tracks[selectedTrack] ?? null)
  const layout = $derived(track && paperWidth > 0 ? layoutTab({ track, width: paperWidth }) : null)

  const hasStarted = $derived(playback.isPlaying || playback.tick > 0)
  const cursor = $derived(layout && hasStarted ? locateTick({ layout, tick: playback.tick }) : null)

  const duration = $derived(score ? ticksToSeconds({ ticks: totalTicks(score), tempoBpm: score.tempo_bpm }) : 0)
  const elapsed = $derived(score ? ticksToSeconds({ ticks: playback.tick, tempoBpm: score.tempo_bpm }) : 0)

  $effect(() => {
    playback.start()
    loadDemoScore()
      .then(async (loaded) => {
        score = loaded
        await playback.setScore(loaded)
      })
      .catch((error: unknown) => (loadError = error instanceof Error ? error.message : String(error)))
    return () => playback.dispose()
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
    if (event.code !== 'Space' || target.closest('button, input, select, textarea')) return
    event.preventDefault()
    void playback.toggle()
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="app">
  <header class="toolbar">
    <div class="brand">
      <span class="logo" aria-hidden="true"></span>
      <h1>{score?.title ?? 'Music Part'}</h1>
    </div>

    {#if score}
      <div class="transport" role="group" aria-label="Playback">
        <button type="button" class="icon-button" aria-label="Stop" title="Stop" onclick={() => playback.stop()}>
          <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="6" y="6" width="12" height="12" rx="2" /></svg>
        </button>
        <button
          type="button"
          class="icon-button primary"
          aria-label={playback.isPlaying ? 'Pause' : 'Play'}
          title={playback.isPlaying ? 'Pause (Space)' : 'Play (Space)'}
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

      <div class="tempo" title="Tempo">
        <span class="tempo-note" aria-hidden="true">♩</span> = {score.tempo_bpm}
      </div>
    {/if}
  </header>

  {#if playback.error}
    <div class="notice" role="alert">
      <span>{playback.error}</span>
      <button type="button" onclick={() => playback.dismissError()}>Dismiss</button>
    </div>
  {/if}

  {#if score}
    <nav class="tracks" aria-label="Tracks">
      {#each score.tracks as t, i (t.name)}
        <button type="button" class:active={i === selectedTrack} onclick={() => (selectedTrack = i)}>
          {t.name}
          <span class="strings">{t.tuning.length} strings</span>
        </button>
      {/each}
    </nav>
  {/if}

  <main class="stage" bind:this={stage}>
    {#if loadError}
      <p class="message error" role="alert">{loadError}</p>
    {:else if !track}
      <p class="message">Loading…</p>
    {:else}
      <div class="paper" bind:this={paper} bind:clientWidth={paperWidth}>
        {#if layout}
          <TabView {layout} label={track.name} {cursor} />
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

  .tracks button {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 8px 14px;
    border: 0;
    border-bottom: 2px solid transparent;
    background: none;
    color: var(--ink-muted);
    font: inherit;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
  }

  .tracks button:hover {
    color: var(--ink);
  }

  .tracks button.active {
    color: var(--ink);
    border-bottom-color: var(--accent);
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
</style>
