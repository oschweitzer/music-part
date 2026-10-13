import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import demo from '../../public/demo-score.json'
import type { Score } from './score-types'
import { createSimulatedTransport, type PositionUpdate } from './transport'

const score = demo as Score

async function setup() {
  const transport = createSimulatedTransport()
  const updates: PositionUpdate[] = []
  await transport.onPosition((u) => updates.push(u))
  await transport.load(score)
  return { transport, updates, last: () => updates.at(-1)! }
}

describe('simulated transport', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  it('should advance the position at the tempo of the score while playing', async () => {
    const { transport, last } = await setup()
    await transport.play()
    vi.advanceTimersByTime(1000)

    // 100 bpm: 1 s = 100 * 32 / 60 ticks.
    expect(last().isPlaying).toBe(true)
    expect(last().tick).toBeCloseTo(53.3, 0)
  })

  it('should keep the position when paused', async () => {
    const { transport, last } = await setup()
    await transport.play()
    vi.advanceTimersByTime(500)
    await transport.pause()
    const paused = last()
    vi.advanceTimersByTime(2000)

    expect(paused.isPlaying).toBe(false)
    expect(paused.tick).toBeGreaterThan(0)
    expect(last()).toEqual(paused)
  })

  it('should resume from where it paused', async () => {
    const { transport, last } = await setup()
    await transport.play()
    vi.advanceTimersByTime(1000)
    await transport.pause()
    const before = last().tick
    await transport.play()
    vi.advanceTimersByTime(1000)

    expect(last().tick).toBeCloseTo(before + 53.3, 0)
  })

  it('should rewind on stop', async () => {
    const { transport, last } = await setup()
    await transport.play()
    vi.advanceTimersByTime(700)
    await transport.stop()

    expect(last()).toEqual({ tick: 0, isPlaying: false })
  })

  it('should stop and rewind by itself at the end', async () => {
    const { transport, last } = await setup()
    await transport.play()
    vi.advanceTimersByTime(20_000)

    expect(last()).toEqual({ tick: 0, isPlaying: false })
  })

  it('should ignore play before a score is loaded', async () => {
    const transport = createSimulatedTransport()
    const updates: PositionUpdate[] = []
    await transport.onPosition((u) => updates.push(u))
    await transport.play()

    expect(updates).toEqual([])
  })

  it('should stop notifying an unsubscribed listener', async () => {
    const transport = createSimulatedTransport()
    const updates: PositionUpdate[] = []
    const unsubscribe = await transport.onPosition((u) => updates.push(u))
    await transport.load(score)
    unsubscribe()
    await transport.play()
    vi.advanceTimersByTime(300)

    expect(updates).toEqual([])
  })
})
