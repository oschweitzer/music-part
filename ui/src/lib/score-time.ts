import type { Score } from './score-types'
import { measureTicks } from './tab-layout'

/** A quarter note is 32 ticks (the model counts in 1/128 of a whole note). */
const TICKS_PER_QUARTER = 32

export function ticksPerSecond(tempoBpm: number): number {
  return (Math.max(tempoBpm, 1) * TICKS_PER_QUARTER) / 60
}

/** Length of the longest track, in ticks. */
export function totalTicks(score: Score): number {
  return Math.max(
    0,
    ...score.tracks.map((track) => track.measures.reduce((sum, m) => sum + measureTicks(m.time_signature), 0)),
  )
}

export function ticksToSeconds({ ticks, tempoBpm }: { ticks: number; tempoBpm: number }): number {
  return ticks / ticksPerSecond(tempoBpm)
}

/** 75.4 -> "1:15" */
export function formatClock(seconds: number): string {
  const whole = Math.max(0, Math.floor(seconds))
  const minutes = Math.floor(whole / 60)
  const rest = whole % 60
  return `${minutes}:${String(rest).padStart(2, '0')}`
}
