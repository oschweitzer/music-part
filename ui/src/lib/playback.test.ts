import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import demo from '../../public/demo-score.json'
import { Playback } from './playback.svelte'
import type { Score } from './score-types'
import type { PositionUpdate, Transport } from './transport'

const score = demo as Score

function fakeTransport() {
  let listener: (update: PositionUpdate) => void = () => {}
  const calls: string[] = []
  const transport: Transport = {
    load: async () => void calls.push('load'),
    play: async () => void calls.push('play'),
    pause: async () => void calls.push('pause'),
    stop: async () => void calls.push('stop'),
    onPosition: async (l) => {
      listener = l
      return () => void calls.push('unsubscribe')
    },
  }
  return { transport, calls, report: (u: PositionUpdate) => listener(u) }
}

describe('Playback', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.stubGlobal('requestAnimationFrame', (cb: () => void) => setTimeout(cb, 16) as unknown as number)
    vi.stubGlobal('cancelAnimationFrame', (id: number) => clearTimeout(id))
  })
  afterEach(() => {
    vi.useRealTimers()
    vi.unstubAllGlobals()
  })

  it('should follow the position reported by the engine', async () => {
    const { transport, report } = fakeTransport()
    const playback = new Playback(transport)
    await playback.start()
    report({ tick: 40, isPlaying: false })

    expect(playback.tick).toBe(40)
    expect(playback.isPlaying).toBe(false)
  })

  it('should extrapolate between two reports while playing', async () => {
    const { transport, report } = fakeTransport()
    const playback = new Playback(transport)
    await playback.start()
    await playback.setScore(score)
    report({ tick: 0, isPlaying: true })
    vi.advanceTimersByTime(1000)

    // 100 bpm is about 53.3 ticks per second.
    expect(playback.tick).toBeGreaterThan(45)
    expect(playback.tick).toBeLessThan(60)
  })

  it('should never extrapolate past the end of the score', async () => {
    const { transport, report } = fakeTransport()
    const playback = new Playback(transport)
    await playback.start()
    await playback.setScore(score)
    report({ tick: 760, isPlaying: true })
    vi.advanceTimersByTime(5000)

    expect(playback.tick).toBe(768)
  })

  it('should stop moving once the engine says it stopped', async () => {
    const { transport, report } = fakeTransport()
    const playback = new Playback(transport)
    await playback.start()
    await playback.setScore(score)
    report({ tick: 10, isPlaying: true })
    vi.advanceTimersByTime(200)
    report({ tick: 0, isPlaying: false })
    vi.advanceTimersByTime(1000)

    expect(playback.tick).toBe(0)
    expect(playback.isPlaying).toBe(false)
  })

  it('should toggle between play and pause', async () => {
    const { transport, calls, report } = fakeTransport()
    const playback = new Playback(transport)
    await playback.start()
    await playback.toggle()
    report({ tick: 0, isPlaying: true })
    await playback.toggle()

    expect(calls).toEqual(['play', 'pause'])
  })

  it('should keep the message when a command fails, until dismissed', async () => {
    const { transport } = fakeTransport()
    transport.play = async () => {
      throw new Error('no audio output')
    }
    const playback = new Playback(transport)
    await playback.toggle()
    expect(playback.error).toBe('no audio output')

    playback.dismissError()
    expect(playback.error).toBeNull()
  })

  it('should rewind when a new score is loaded', async () => {
    const { transport, calls, report } = fakeTransport()
    const playback = new Playback(transport)
    await playback.start()
    report({ tick: 90, isPlaying: false })
    await playback.setScore(score)

    expect(playback.tick).toBe(0)
    expect(calls).toContain('load')
  })
})
