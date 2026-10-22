import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import demo from '../../public/demo-score.json'
import { Playback } from './playback.svelte'
import type { Score } from './score-types'
import type { PositionUpdate, Transport } from './transport'

const score = demo as Score

function fakeTransport({ failPreview = false }: { failPreview?: boolean } = {}) {
  let listener: (update: PositionUpdate) => void = () => {}
  const calls: string[] = []
  const transport: Transport = {
    load: async () => void calls.push('load'),
    play: async () => void calls.push('play'),
    pause: async () => void calls.push('pause'),
    stop: async () => void calls.push('stop'),
    seek: async (tick) => void calls.push(`seek ${tick}`),
    setAudible: async (audible) => void calls.push(`audible ${audible.map(Number).join('')}`),
    preview: async ({ track, notes }) => {
      if (failPreview) throw new Error('no audio')
      calls.push(`preview ${track} ${notes.length}`)
    },
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

  describe('seeking and the mix', () => {
    it('should move the cursor at once and tell the engine', async () => {
      const { transport, calls } = fakeTransport()
      const playback = new Playback(transport)
      await playback.start()
      await playback.setScore(score)
      await playback.seek(96)

      expect(playback.tick).toBe(96)
      expect(calls).toContain('seek 96')
    })

    it('should keep a seek inside the piece', async () => {
      const { transport, calls } = fakeTransport()
      const playback = new Playback(transport)
      await playback.start()
      await playback.setScore(score)
      await playback.seek(-10)
      await playback.seek(1e9)

      const targets = calls.filter((c) => c.startsWith('seek')).map((c) => Number(c.slice(5)))
      expect(targets[0]).toBe(0)
      expect(targets[1]).toBeGreaterThan(0)
      expect(targets[1]).toBeLessThan(1e9)
    })

    it('should send which tracks sound when the score loads and when mute or solo change', async () => {
      const { transport, calls } = fakeTransport()
      const playback = new Playback(transport)
      await playback.start()
      await playback.setScore(score) // two tracks
      await playback.toggleMute(0)
      await playback.toggleSolo(0)
      await playback.toggleSolo(0)
      await playback.toggleMute(0)

      expect(calls.filter((c) => c.startsWith('audible'))).toEqual([
        'audible 11',
        'audible 01',
        'audible 10', // track 0 soloed: it plays alone, muted or not
        'audible 01',
        'audible 11',
      ])
    })

    describe('preview', () => {
      const notes = [{ string: 2, fret: 3, technique: null, tied: false }]

      it('should ask the engine to sound the notes', async () => {
        const { transport, calls } = fakeTransport()
        const playback = new Playback(transport)
        await playback.setScore(score)
        playback.preview({ track: 1, notes })

        expect(calls).toContain('preview 1 1')
      })

      it('should stay silent over the playback', async () => {
        const { transport, calls, report } = fakeTransport()
        const playback = new Playback(transport)
        await playback.start()
        await playback.setScore(score)
        report({ tick: 10, isPlaying: true })
        playback.preview({ track: 0, notes })

        expect(calls.some((c) => c.startsWith('preview'))).toBe(false)
      })

      it('should follow the mute of the track and ignore the solo of the others', async () => {
        const { transport, calls } = fakeTransport()
        const playback = new Playback(transport)
        await playback.setScore(score)
        await playback.toggleSolo(1)
        playback.preview({ track: 0, notes }) // track 1 plays alone in the song, but this one is not muted
        await playback.toggleMute(0)
        playback.preview({ track: 0, notes })

        expect(calls.filter((c) => c.startsWith('preview'))).toEqual(['preview 0 1'])
      })

      it('should say nothing when the engine cannot sound it', async () => {
        const { transport } = fakeTransport({ failPreview: true })
        const playback = new Playback(transport)
        await playback.setScore(score)
        playback.preview({ track: 0, notes })
        await Promise.resolve()

        expect(playback.error).toBeNull()
      })

      it('should not call the engine for no notes', async () => {
        const { transport, calls } = fakeTransport()
        const playback = new Playback(transport)
        await playback.setScore(score)
        playback.preview({ track: 0, notes: [] })

        expect(calls.some((c) => c.startsWith('preview'))).toBe(false)
      })
    })

    it('should forget the mix of a removed track', async () => {
      const { transport } = fakeTransport()
      const playback = new Playback(transport)
      await playback.start()
      await playback.setScore(score)
      await playback.toggleMute(1)
      playback.dropTrackMix(0)

      expect(playback.muted).toEqual([true])
    })
  })
})
