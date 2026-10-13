import { ticksPerSecond, totalTicks } from './score-time'
import type { Score } from './score-types'
import type { Transport } from './transport'

/**
 * Reactive playback state for the UI. The engine reports its position about 30 times a second;
 * between two reports the position is extrapolated on every animation frame, so the cursor
 * moves smoothly instead of in steps.
 */
export class Playback {
  isPlaying = $state(false)
  /** Estimated position in 1/128 of a whole note. */
  tick = $state(0)
  error = $state<string | null>(null)

  #transport: Transport
  #ticksPerMs = 0
  #totalTicks = 0
  #lastReport = { tick: 0, at: 0 }
  #frame = 0
  #unsubscribe: (() => void) | null = null

  constructor(transport: Transport) {
    this.#transport = transport
  }

  async start(): Promise<void> {
    this.#unsubscribe = await this.#transport.onPosition(({ tick, isPlaying }) => {
      this.#lastReport = { tick, at: performance.now() }
      this.tick = tick
      this.isPlaying = isPlaying
      if (isPlaying) this.#startFrames()
      else this.#stopFrames()
    })
  }

  dispose(): void {
    this.#unsubscribe?.()
    this.#stopFrames()
  }

  async setScore(score: Score): Promise<void> {
    this.#ticksPerMs = ticksPerSecond(score.tempo_bpm) / 1000
    this.#totalTicks = totalTicks(score)
    this.tick = 0
    this.isPlaying = false
    this.#stopFrames()
    await this.#run(() => this.#transport.load(score))
  }

  toggle(): Promise<void> {
    return this.#run(() => (this.isPlaying ? this.#transport.pause() : this.#transport.play()))
  }

  stop(): Promise<void> {
    return this.#run(() => this.#transport.stop())
  }

  dismissError(): void {
    this.error = null
  }

  async #run(action: () => Promise<void>): Promise<void> {
    try {
      await action()
    } catch (error) {
      this.error = error instanceof Error ? error.message : String(error)
    }
  }

  #startFrames(): void {
    if (this.#frame) return
    const advance = () => {
      const elapsed = performance.now() - this.#lastReport.at
      this.tick = Math.min(this.#lastReport.tick + elapsed * this.#ticksPerMs, this.#totalTicks)
      this.#frame = requestAnimationFrame(advance)
    }
    this.#frame = requestAnimationFrame(advance)
  }

  #stopFrames(): void {
    cancelAnimationFrame(this.#frame)
    this.#frame = 0
  }
}
