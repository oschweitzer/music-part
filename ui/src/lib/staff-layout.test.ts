import { describe, expect, it } from 'vitest'
import type { NoteValue } from './score-types'
import {
  clefsFor,
  engraveMeasure,
  keySignatureGlyphs,
  pitchAtPosition,
  pitchOfLetter,
  positionAt,
  positionY,
  spell,
  staffBlock,
  staffIndexFor,
  staffPosition,
  STAFF_METRICS,
  stepPitch,
  type StaffBeatInput,
} from './staff-layout'

const treble = staffBlock({ clef: 'treble', top: 100 })
const bass = staffBlock({ clef: 'bass', top: 100 })

/** Beats one after the other, 30 apart, each a list of pitches (none for a rest). */
function beats(value: NoteValue, chords: number[][], tied: boolean[] = []): StaffBeatInput[] {
  const ticks: Record<NoteValue, number> = { Whole: 128, Half: 64, Quarter: 32, Eighth: 16, Sixteenth: 8, ThirtySecond: 4 }
  return chords.map((pitches, i) => ({
    x: 50 + i * 30,
    startTick: i * ticks[value],
    value,
    dots: 0,
    notes: pitches.map((pitch) => ({ pitch, tied: tied[i] === true, row: pitch })),
  }))
}

const engrave = (input: StaffBeatInput[], block = treble, carried: number[] = []) =>
  engraveMeasure({ beats: input, block, measureX: 30, carried })

describe('pitches on a staff', () => {
  it('should spell black keys as the usual sharps and flats of C major', () => {
    expect(spell({ pitch: 61 })).toEqual({ step: 0, octave: 4, alter: 1 })
    expect(spell({ pitch: 63 })).toEqual({ step: 2, octave: 4, alter: -1 })
    expect(spell({ pitch: 70 })).toEqual({ step: 6, octave: 4, alter: -1 })
    expect(spell({ pitch: 66 })).toEqual({ step: 3, octave: 4, alter: 1 })
  })

  it('should spell with the sharps or flats of a key signature', () => {
    expect(spell({ pitch: 66, key: { fifths: 1 } })).toEqual({ step: 3, octave: 4, alter: 1 })
    expect(spell({ pitch: 63, key: { fifths: 2 } })).toEqual({ step: 1, octave: 4, alter: 1 })
    expect(spell({ pitch: 59, key: { fifths: -7 } })).toEqual({ step: 0, octave: 4, alter: -1 })
  })

  it('should put E4 on the bottom line of the treble staff and G2 on that of the bass staff', () => {
    expect(staffPosition({ pitch: 64, clef: 'treble' })).toBe(0)
    expect(staffPosition({ pitch: 77, clef: 'treble' })).toBe(8)
    expect(staffPosition({ pitch: 43, clef: 'bass' })).toBe(0)
    expect(staffPosition({ pitch: 60, clef: 'treble' })).toBe(-2)
    expect(staffPosition({ pitch: 60, clef: 'bass' })).toBe(10)
  })

  it('should write a guitar an octave above how it sounds', () => {
    expect(clefsFor('Guitar')).toEqual(['treble8vb'])
    expect(clefsFor('Bass')).toEqual(['bass8vb'])
    expect(staffPosition({ pitch: 52, clef: 'treble8vb' })).toBe(0)
    expect(clefsFor('Piano')).toEqual(['treble', 'bass'])
  })

  it('should find back the pitch of a line or space, and its y', () => {
    for (const position of [-4, -1, 0, 3, 8, 11]) {
      const y = positionY({ position, bottom: treble.bottom })
      expect(positionAt({ y: y + 1.5, bottom: treble.bottom })).toBe(position)
      expect(staffPosition({ pitch: pitchAtPosition({ position, clef: 'treble' }), clef: 'treble' })).toBe(position)
    }
    expect(pitchAtPosition({ position: 3, clef: 'treble' })).toBe(69)
    expect(pitchAtPosition({ position: 4, clef: 'treble', key: { fifths: -1 } })).toBe(70)
  })

  it('should step to the next note of the key and find the nearest note of a name', () => {
    expect(stepPitch({ pitch: 60, direction: 1 })).toBe(62)
    expect(stepPitch({ pitch: 64, direction: 1 })).toBe(65)
    expect(stepPitch({ pitch: 61, direction: -1 })).toBe(60)
    expect(stepPitch({ pitch: 127, direction: 1 })).toBe(127)
    expect(pitchOfLetter({ letter: 'A', near: 62 })).toBe(57)
    expect(pitchOfLetter({ letter: 'g', near: 61 })).toBe(67)
    expect(pitchOfLetter({ letter: 'H', near: 60 })).toBeNull()
  })

  it('should put middle C and above on the treble staff of a grand staff', () => {
    expect(staffIndexFor({ pitch: 60, staffCount: 2 })).toBe(0)
    expect(staffIndexFor({ pitch: 59, staffCount: 2 })).toBe(1)
    expect(staffIndexFor({ pitch: 30, staffCount: 1 })).toBe(0)
  })
})

describe('engraveMeasure', () => {
  it('should give notes far from the staff their ledger lines', () => {
    const [middleC, highA] = engrave(beats('Quarter', [[60], [81]])).beats
    expect(middleC?.ledgers.map((l) => l.y)).toEqual([treble.bottom + STAFF_METRICS.lineGap])
    expect(highA?.ledgers.map((l) => l.y)).toEqual([treble.top - STAFF_METRICS.lineGap])
  })

  it('should point stems up below the middle line and down from it', () => {
    const [low, middle, high] = engrave(beats('Quarter', [[64], [71], [77]])).beats
    expect(low?.isStemUp).toBe(true)
    expect(low!.stem!.y2).toBeLessThan(low!.stem!.y1)
    expect(middle?.isStemUp).toBe(false)
    expect(high?.isStemUp).toBe(false)
    expect(high!.stem!.y2).toBeGreaterThan(high!.stem!.y1)
  })

  it('should draw an accidental once per measure, and a natural to cancel it', () => {
    const heads = engrave(beats('Quarter', [[66], [66], [65], [61]])).beats.map((b) => b.heads[0]?.accidental)
    expect(heads).toEqual(['sharp', null, 'natural', 'sharp'])
  })

  it('should leave the accidental off a note tied over the barline', () => {
    const [tied] = engrave(beats('Half', [[66]], [true]), treble, [66]).beats
    expect(tied?.heads[0]?.accidental).toBeNull()
    expect(tied?.heads[0]?.tie?.fromX).toBeLessThan(tied!.heads[0]!.x)
  })

  it('should draw a tie only to a note of the same pitch', () => {
    const [, tied, lost] = engrave(beats('Quarter', [[60], [60], [62]], [false, true, true])).beats
    expect(tied?.heads[0]?.tie).not.toBeNull()
    expect(lost?.heads[0]?.tie).toBeNull()
  })

  it('should put a note a second away on the other side of the stem', () => {
    const [up] = engrave(beats('Quarter', [[60, 62]])).beats
    const [c, d] = up!.heads
    expect(c!.x).toBe(50)
    expect(d!.x).toBeGreaterThan(50)
    const [down] = engrave(beats('Quarter', [[76, 77]])).beats
    expect(down?.isStemUp).toBe(false)
    expect(down!.heads[0]!.x).toBeLessThan(50)
  })

  it('should beam eighths within a quarter, all stems the same way', () => {
    const measure = engrave(beats('Eighth', [[60], [79], [62], [64], [65]]))
    expect(measure.beams).toHaveLength(2)
    const [a, b] = measure.beats
    expect(a?.isStemUp).toBe(b?.isStemUp)
    expect(a?.stem?.y2).toBe(b?.stem?.y2)
    expect(measure.beats.slice(0, 4).every((beat) => beat.flags === 0)).toBe(true)
    expect(measure.beats[4]?.flags).toBe(1)
  })

  it('should draw a rest where the beat has no note on this staff', () => {
    const [rest, note] = engrave(beats('Whole', [[], [60]]), bass).beats
    expect(rest?.rest).toEqual({ value: 'Whole', x: 50, y: positionY({ position: 6, bottom: bass.bottom }) })
    expect(note?.rest).toBeNull()
    expect(note?.stem).toBeNull()
  })

  it('should dot a note in the space above a line', () => {
    const input = beats('Quarter', [[64]])
    input[0]!.dots = 1
    const [dotted] = engrave(input).beats
    expect(dotted?.dots).toEqual([{ x: expect.any(Number), y: positionY({ position: 1, bottom: treble.bottom }) }])
  })
})

describe('key signatures', () => {
  it('should place the sharps of D major on F and C, lower on a bass staff', () => {
    const glyphs = keySignatureGlyphs({ key: { fifths: 2 }, clef: 'treble', x: 10, bottom: treble.bottom })
    expect(glyphs.map((g) => g.kind)).toEqual(['sharp', 'sharp'])
    expect(glyphs.map((g) => g.y)).toEqual([treble.top, positionY({ position: 5, bottom: treble.bottom })])
    const low = keySignatureGlyphs({ key: { fifths: -1 }, clef: 'bass', x: 10, bottom: bass.bottom })
    expect(low).toEqual([{ kind: 'flat', x: 10, y: positionY({ position: 2, bottom: bass.bottom }) }])
  })
})
