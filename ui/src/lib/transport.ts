import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { isTauri } from './score-source'
import { ticksPerSecond, totalTicks } from './score-time'
import type { Score } from './score-types'

export type PositionUpdate = {
  /** Position in 1/128 of a whole note. */
  tick: number
  isPlaying: boolean
}

/** What the UI needs from playback. Rejected promises carry a message fit to show the user. */
export type Transport = {
  load(score: Score): Promise<void>
  play(): Promise<void>
  pause(): Promise<void>
  stop(): Promise<void>
  /** Subscribes to position updates. Resolves with the function that unsubscribes. */
  onPosition(listener: (update: PositionUpdate) => void): Promise<() => void>
}

export function createTransport(): Transport {
  return isTauri() ? createTauriTransport() : createSimulatedTransport()
}

function createTauriTransport(): Transport {
  const call = async (command: string, args?: Record<string, unknown>) => {
    try {
      await invoke(command, args)
    } catch (error) {
      throw new Error(typeof error === 'string' ? error : String(error))
    }
  }

  return {
    // The desktop app reloads the audio engine itself whenever the document changes.
    load: async () => {},
    play: () => call('play'),
    pause: () => call('pause'),
    stop: () => call('stop'),
    onPosition: (listener) => listen<PositionUpdate>('playback-position', (event) => listener(event.payload)),
  }
}

/**
 * Moves a position forward in time without any sound, so the UI can be developed and tested in a
 * plain browser. It rewinds by itself at the end, like the audio engine does.
 */
export function createSimulatedTransport(): Transport {
  const INTERVAL_MS = 33
  const listeners = new Set<(update: PositionUpdate) => void>()
  let score: Score | null = null
  let tick = 0
  let startedAt: { at: number; tick: number } | null = null
  let timer: ReturnType<typeof setInterval> | undefined

  const emit = () => listeners.forEach((listener) => listener({ tick, isPlaying: startedAt !== null }))

  const halt = () => {
    clearInterval(timer)
    timer = undefined
    startedAt = null
  }

  const step = () => {
    if (!score || !startedAt) return
    tick = startedAt.tick + ((performance.now() - startedAt.at) / 1000) * ticksPerSecond(score.tempo_bpm)
    if (tick >= totalTicks(score)) {
      halt()
      tick = 0
    }
    emit()
  }

  return {
    async load(next) {
      halt()
      score = next
      tick = 0
    },
    async play() {
      if (!score || startedAt) return
      startedAt = { at: performance.now(), tick }
      timer = setInterval(step, INTERVAL_MS)
      emit()
    },
    async pause() {
      step()
      halt()
      emit()
    },
    async stop() {
      halt()
      tick = 0
      emit()
    },
    async onPosition(listener) {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
  }
}
