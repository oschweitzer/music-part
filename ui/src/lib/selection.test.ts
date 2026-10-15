import { describe, expect, it } from 'vitest'
import demo from '../../public/demo-score.json'
import type { Score } from './score-types'
import { clampSelection, extendSelection, FIRST_NOTE, isBeatSelected, moveSelection, nextFret, selectedRange, selectedStrings } from './selection'

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

describe('extending a selection', () => {
  const extend = (selection: Parameters<typeof extendSelection>[0]['selection'], direction: 'left' | 'right') =>
    extendSelection({ score, track: 0, selection, direction })

  it('should keep the anchor where the selection began while the other end moves', () => {
    const once = extend({ measure: 0, beat: 2, string: 3 }, 'right')
    expect(once).toEqual({ measure: 0, beat: 3, string: 3, anchor: { measure: 0, beat: 2 } })
    const twice = extend(once, 'right')
    expect(twice).toEqual({ measure: 0, beat: 4, string: 3, anchor: { measure: 0, beat: 2 } })
  })

  it('should cross barlines and shrink back toward the anchor', () => {
    const last = beatsIn(0) - 1
    const across = extend({ measure: 0, beat: last, string: 1 }, 'right')
    expect(across).toMatchObject({ measure: 1, beat: 0, anchor: { measure: 0, beat: last } })
    expect(extend(across, 'left')).toMatchObject({ measure: 0, beat: last })
  })

  it('should drop the anchor when the selection moves normally', () => {
    const range = extend({ measure: 0, beat: 1, string: 1 }, 'right')
    expect(moveSelection({ score, track: 0, selection: range, direction: 'right' }).anchor).toBeUndefined()
  })

  it('should give the selected range in order whichever end is the anchor', () => {
    const forward = { measure: 0, beat: 4, string: 1, anchor: { measure: 0, beat: 2 } }
    const backward = { measure: 0, beat: 2, string: 1, anchor: { measure: 0, beat: 4 } }
    expect(selectedRange(forward)).toEqual({ from: { measure: 0, beat: 2 }, to: { measure: 0, beat: 4 } })
    expect(selectedRange(backward)).toEqual(selectedRange(forward))
    expect(selectedRange({ measure: 0, beat: 2, string: 1 })).toBeNull()
    expect(selectedRange({ measure: 0, beat: 2, string: 1, anchor: { measure: 0, beat: 2 } })).toBeNull()
  })

  it('should tell which beats are selected', () => {
    const selection = { measure: 1, beat: 1, string: 1, anchor: { measure: 0, beat: 6 } }
    const selected = (measure: number, beat: number) => isBeatSelected({ selection, measure, beat })
    expect(selected(0, 5)).toBe(false)
    expect(selected(0, 6)).toBe(true)
    expect(selected(1, 0)).toBe(true)
    expect(selected(1, 1)).toBe(true)
    expect(selected(1, 2)).toBe(false)
    expect(isBeatSelected({ selection: { measure: 0, beat: 0, string: 1 }, measure: 0, beat: 0 })).toBe(true)
    expect(isBeatSelected({ selection: null, measure: 0, beat: 0 })).toBe(false)
  })

  it('should take several strings of one beat as a range, and one string as a lone spot', () => {
    const chord = { measure: 2, beat: 0, string: 5, anchor: { measure: 2, beat: 0 }, anchorString: 2 }
    expect(selectedRange(chord)).toEqual({ from: { measure: 2, beat: 0 }, to: { measure: 2, beat: 0 } })
    expect(selectedStrings(chord)).toEqual({ first: 2, last: 5 })
    expect(selectedRange({ ...chord, string: 2 })).toBeNull()
  })

  it('should take every string unless the selection was dragged as a rectangle', () => {
    expect(selectedStrings({ measure: 0, beat: 3, string: 4, anchor: { measure: 0, beat: 1 } })).toBeNull()
    expect(selectedStrings({ measure: 0, beat: 3, string: 4 })).toBeNull()
    expect(selectedStrings(null)).toBeNull()
  })

  it('should keep the strings of a rectangle when extending it, and drop them on a move', () => {
    const rectangle = { measure: 0, beat: 2, string: 4, anchor: { measure: 0, beat: 1 }, anchorString: 2 }
    expect(extendSelection({ score, track: 0, selection: rectangle, direction: 'right' })).toMatchObject({ beat: 3, anchorString: 2 })
    expect(moveSelection({ score, track: 0, selection: rectangle, direction: 'right' })).not.toHaveProperty('anchorString')
  })

  it('should keep a valid anchor and pull a stale one back', () => {
    const stale = { measure: 0, beat: 0, string: 1, anchor: { measure: 99, beat: 99 } }
    expect(clampSelection({ score, track: 0, selection: stale }).anchor).toEqual({ measure: 5, beat: 0 })
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
