import { describe, expect, it } from 'vitest'
import demo from '../../public/demo-score.json'
import { formatClock, ticksPerSecond, ticksToSeconds, totalTicks } from './score-time'
import type { Score } from './score-types'

const score = demo as Score

describe('score time', () => {
  it('should convert tempo to ticks per second', () => {
    // 120 bpm: 2 quarter notes per second, 32 ticks each.
    expect(ticksPerSecond(120)).toBe(64)
  })

  it('should measure the length of the demo in ticks', () => {
    expect(totalTicks(score)).toBe(6 * 128)
  })

  it('should convert ticks to seconds', () => {
    expect(ticksToSeconds({ ticks: 128, tempoBpm: 120 })).toBe(2)
    expect(ticksToSeconds({ ticks: 6 * 128, tempoBpm: 100 })).toBeCloseTo(14.4, 5)
  })

  it('should take the longest track', () => {
    const longer = { ...score, tracks: [{ ...score.tracks[0]!, measures: score.tracks[0]!.measures.slice(0, 2) }, score.tracks[1]!] }
    expect(totalTicks(longer)).toBe(6 * 128)
    expect(totalTicks({ ...score, tracks: [] })).toBe(0)
  })

  it('should format a clock', () => {
    expect(formatClock(0)).toBe('0:00')
    expect(formatClock(9.9)).toBe('0:09')
    expect(formatClock(75.4)).toBe('1:15')
    expect(formatClock(-3)).toBe('0:00')
  })
})
