import { audibleTracks, resized } from './mix'
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
  /** Per track, by index. A soloed track plays alone with the other soloed ones. */
  muted = $state<boolean[]>([])
  solo = $state<boolean[]>([])

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
    this.muted = resized({ flags: this.muted, length: score.tracks.length })
    this.solo = resized({ flags: this.solo, length: score.tracks.length })
    await this.#run(async () => {
      await this.#transport.load(score)
      await this.#transport.setAudible(audibleTracks({ muted: this.muted, solo: this.solo }))
    })
  }

  /** Moves the playback position, playing or not. */
  seek(tick: number): Promise<void> {
    const target = Math.min(Math.max(tick, 0), this.#totalTicks)
    this.tick = target
    this.#lastReport = { tick: target, at: performance.now() }
    return this.#run(() => this.#transport.seek(target))
  }

  toggleMute(track: number): Promise<void> {
    this.muted = this.muted.map((isMuted, i) => (i === track ? !isMuted : isMuted))
    return this.#sendMix()
  }

  toggleSolo(track: number): Promise<void> {
    this.solo = this.solo.map((isSolo, i) => (i === track ? !isSolo : isSolo))
    return this.#sendMix()
  }

  /** A track was removed from the score: the ones after it move up, and so do their buttons. */
  dropTrackMix(track: number): void {
    this.muted = this.muted.filter((_, i) => i !== track)
    this.solo = this.solo.filter((_, i) => i !== track)
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

  #sendMix(): Promise<void> {
    return this.#run(() => this.#transport.setAudible(audibleTracks({ muted: this.muted, solo: this.solo })))
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
