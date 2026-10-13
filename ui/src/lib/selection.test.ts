import { describe, expect, it } from 'vitest'
import demo from '../../public/demo-score.json'
import type { Score } from './score-types'
import { clampSelection, FIRST_NOTE, moveSelection, nextFret } from './selection'

const score = demo as Score
const beatsIn = (measure: number) => score.tracks[0]!.measures[measure]!.voices[0]!.beats.length

describe('moveSelection', () => {
  const move = (selection: { measure: number; beat: number; string: number }, direction: 'left' | 'right' | 'up' | 'down') =>
    moveSelection({ score, track: 0, selection, direction })

  it('should move along the beats of a measure', () => {
    expect(move({ measure: 0, beat: 2, string: 3 }, 'right')).toEqual({ measure: 0, beat: 3, string: 3 })
    expect(move({ measure: 0, beat: 2, string: 3 }, 'left')).toEqual({ measure: 0, beat: 1, string: 3 })
  })

  it('should cross barlines in both directions', () => {
    const last = beatsIn(0) - 1
    expect(move({ measure: 0, beat: last, string: 1 }, 'right')).toEqual({ measure: 1, beat: 0, string: 1 })
    expect(move({ measure: 1, beat: 0, string: 1 }, 'left')).toEqual({ measure: 0, beat: last, string: 1 })
  })

  it('should stop at the start and the end of the piece', () => {
    expect(move({ measure: 0, beat: 0, string: 1 }, 'left')).toEqual({ measure: 0, beat: 0, string: 1 })
    const lastMeasure = score.tracks[0]!.measures.length - 1
    const end = { measure: lastMeasure, beat: beatsIn(lastMeasure) - 1, string: 1 }
    expect(move(end, 'right')).toEqual(end)
  })

  it('should move between strings and stay within them', () => {
    expect(move({ measure: 0, beat: 0, string: 1 }, 'up').string).toBe(1)
    expect(move({ measure: 0, beat: 0, string: 1 }, 'down').string).toBe(2)
    expect(move({ measure: 0, beat: 0, string: 6 }, 'down').string).toBe(6)
  })
})

describe('clampSelection', () => {
  it('should default to the first note', () => {
    expect(clampSelection({ score, track: 0, selection: null })).toEqual(FIRST_NOTE)
  })

  it('should pull a selection back onto what exists', () => {
    expect(clampSelection({ score, track: 0, selection: { measure: 99, beat: 99, string: 99 } })).toEqual({
      measure: 5,
      beat: 0,
      string: 6,
    })
    expect(clampSelection({ score, track: 1, selection: { measure: 0, beat: 0, string: 6 } }).string).toBe(4)
    expect(clampSelection({ score, track: 0, selection: { measure: -3, beat: -1, string: 0 } })).toEqual(FIRST_NOTE)
  })

  it('should keep a valid selection as it is', () => {
    const selection = { measure: 2, beat: 3, string: 4 }
    expect(clampSelection({ score, track: 0, selection })).toEqual(selection)
  })
})

describe('nextFret', () => {
  it('should use a lone digit as it is', () => {
    expect(nextFret({ previous: null, digit: 7, now: 1000 })).toBe(7)
  })

  it('should join two digits typed quickly into one fret', () => {
    expect(nextFret({ previous: { fret: 1, at: 1000 }, digit: 2, now: 1300 })).toBe(12)
    expect(nextFret({ previous: { fret: 2, at: 1000 }, digit: 4, now: 1500 })).toBe(24)
  })

  it('should start over when the second digit comes too late', () => {
    expect(nextFret({ previous: { fret: 1, at: 1000 }, digit: 2, now: 2500 })).toBe(2)
  })

  it('should start over when the joined number would be beyond the last fret', () => {
    expect(nextFret({ previous: { fret: 2, at: 1000 }, digit: 5, now: 1200 })).toBe(5)
    expect(nextFret({ previous: { fret: 5, at: 1000 }, digit: 0, now: 1200 })).toBe(0)
  })

  it('should not make "05" out of a zero followed by a digit', () => {
    expect(nextFret({ previous: { fret: 0, at: 1000 }, digit: 5, now: 1200 })).toBe(5)
  })
})
