<script lang="ts">
  import TabView from './components/TabView.svelte'
  import { loadDemoScore } from './lib/score-source'
  import type { Score } from './lib/score-types'

  let score = $state<Score | null>(null)
  let loadError = $state<string | null>(null)
  let selectedTrack = $state(0)
  let paperWidth = $state(0)

  const track = $derived(score?.tracks[selectedTrack] ?? null)

  $effect(() => {
    loadDemoScore()
      .then((loaded) => (score = loaded))
      .catch((error: unknown) => (loadError = error instanceof Error ? error.message : String(error)))
  })
</script>

<div class="app">
  <header class="toolbar">
    <div class="brand">
      <span class="logo" aria-hidden="true"></span>
      <h1>{score?.title ?? 'Music Part'}</h1>
    </div>

    {#if score}
      <div class="tempo" title="Tempo">
        <span class="tempo-note" aria-hidden="true">♩</span> = {score.tempo_bpm}
      </div>
    {/if}
  </header>

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

  <main class="stage">
    {#if loadError}
      <p class="message error" role="alert">{loadError}</p>
    {:else if !track}
      <p class="message">Loading…</p>
    {:else}
      <div class="paper" bind:clientWidth={paperWidth}>
        {#if paperWidth > 0}
          <TabView {track} width={paperWidth} />
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
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 12px 20px;
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

  .tempo {
    flex: none;
    padding: 4px 10px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--ink-muted);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
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
</style>
