import { describe, expect, it } from 'vitest'
import demo from '../../public/demo-score.json'
import type { Score } from './score-types'
import { clampSelection, extendSelection, FIRST_NOTE, firstSelection, isBeatSelected, moveSelection, moveToTrack, nextFret, selectedRange, selectedRows } from './selection'
import type { Selection } from './selection'
import { durationTicks } from './tab-layout'

const score = demo as Score
const beatsIn = (measure: number) => score.tracks[0]!.measures[measure]!.voices[0]!.beats.length

describe('moveSelection', () => {
  const move = (selection: Selection, direction: 'left' | 'right' | 'up' | 'down') =>
    moveSelection({ score, track: 0, selection, direction })

  it('should move along the beats of a measure', () => {
    expect(move({ measure: 0, beat: 2, row: 3 }, 'right')).toEqual({ measure: 0, beat: 3, row: 3 })
    expect(move({ measure: 0, beat: 2, row: 3 }, 'left')).toEqual({ measure: 0, beat: 1, row: 3 })
  })

  it('should cross barlines in both directions', () => {
    const last = beatsIn(0) - 1
    expect(move({ measure: 0, beat: last, row: 1 }, 'right')).toEqual({ measure: 1, beat: 0, row: 1 })
    expect(move({ measure: 1, beat: 0, row: 1 }, 'left')).toEqual({ measure: 0, beat: last, row: 1 })
  })

  it('should stop at the start and the end of the piece', () => {
    expect(move({ measure: 0, beat: 0, row: 1 }, 'left')).toEqual({ measure: 0, beat: 0, row: 1 })
    const lastMeasure = score.tracks[0]!.measures.length - 1
    const end = { measure: lastMeasure, beat: beatsIn(lastMeasure) - 1, row: 1 }
    expect(move(end, 'right')).toEqual(end)
  })

  it('should move between strings and stay within them', () => {
    expect(move({ measure: 0, beat: 0, row: 1 }, 'up').row).toBe(1)
    expect(move({ measure: 0, beat: 0, row: 1 }, 'down').row).toBe(2)
    expect(move({ measure: 0, beat: 0, row: 6 }, 'down').row).toBe(6)
  })
})

describe('clampSelection', () => {
  it('should default to the first note', () => {
    expect(clampSelection({ score, track: 0, selection: null })).toEqual(FIRST_NOTE)
  })

  it('should pull a selection back onto what exists', () => {
    expect(clampSelection({ score, track: 0, selection: { measure: 99, beat: 99, row: 99 } })).toEqual({
      measure: 5,
      beat: 0,
      row: 6,
    })
    expect(clampSelection({ score, track: 1, selection: { measure: 0, beat: 0, row: 6 } }).row).toBe(4)
    expect(clampSelection({ score, track: 0, selection: { measure: -3, beat: -1, row: 0 } })).toEqual(FIRST_NOTE)
  })

  it('should keep a valid selection as it is', () => {
    const selection = { measure: 2, beat: 3, row: 4 }
    expect(clampSelection({ score, track: 0, selection })).toEqual(selection)
  })
})

describe('extending a selection', () => {
  const extend = (selection: Parameters<typeof extendSelection>[0]['selection'], direction: 'left' | 'right') =>
    extendSelection({ score, track: 0, selection, direction })

  it('should keep the anchor where the selection began while the other end moves', () => {
    const once = extend({ measure: 0, beat: 2, row: 3 }, 'right')
    expect(once).toEqual({ measure: 0, beat: 3, row: 3, anchor: { measure: 0, beat: 2 } })
    const twice = extend(once, 'right')
    expect(twice).toEqual({ measure: 0, beat: 4, row: 3, anchor: { measure: 0, beat: 2 } })
  })

  it('should cross barlines and shrink back toward the anchor', () => {
    const last = beatsIn(0) - 1
    const across = extend({ measure: 0, beat: last, row: 1 }, 'right')
    expect(across).toMatchObject({ measure: 1, beat: 0, anchor: { measure: 0, beat: last } })
    expect(extend(across, 'left')).toMatchObject({ measure: 0, beat: last })
  })

  it('should drop the anchor when the selection moves normally', () => {
    const range = extend({ measure: 0, beat: 1, row: 1 }, 'right')
    expect(moveSelection({ score, track: 0, selection: range, direction: 'right' }).anchor).toBeUndefined()
  })

  it('should give the selected range in order whichever end is the anchor', () => {
    const forward = { measure: 0, beat: 4, row: 1, anchor: { measure: 0, beat: 2 } }
    const backward = { measure: 0, beat: 2, row: 1, anchor: { measure: 0, beat: 4 } }
    expect(selectedRange(forward)).toEqual({ from: { measure: 0, beat: 2 }, to: { measure: 0, beat: 4 } })
    expect(selectedRange(backward)).toEqual(selectedRange(forward))
    expect(selectedRange({ measure: 0, beat: 2, row: 1 })).toBeNull()
    expect(selectedRange({ measure: 0, beat: 2, row: 1, anchor: { measure: 0, beat: 2 } })).toBeNull()
  })

  it('should tell which beats are selected', () => {
    const selection = { measure: 1, beat: 1, row: 1, anchor: { measure: 0, beat: 6 } }
    const selected = (measure: number, beat: number) => isBeatSelected({ selection, measure, beat })
    expect(selected(0, 5)).toBe(false)
    expect(selected(0, 6)).toBe(true)
    expect(selected(1, 0)).toBe(true)
    expect(selected(1, 1)).toBe(true)
    expect(selected(1, 2)).toBe(false)
    expect(isBeatSelected({ selection: { measure: 0, beat: 0, row: 1 }, measure: 0, beat: 0 })).toBe(true)
    expect(isBeatSelected({ selection: null, measure: 0, beat: 0 })).toBe(false)
  })

  it('should take several strings of one beat as a range, and one string as a lone spot', () => {
    const chord = { measure: 2, beat: 0, row: 5, anchor: { measure: 2, beat: 0 }, anchorRow: 2 }
    expect(selectedRange(chord)).toEqual({ from: { measure: 2, beat: 0 }, to: { measure: 2, beat: 0 } })
    expect(selectedRows(chord)).toEqual({ first: 2, last: 5 })
    expect(selectedRange({ ...chord, row: 2 })).toBeNull()
  })

  it('should take every string unless the selection was dragged as a rectangle', () => {
    expect(selectedRows({ measure: 0, beat: 3, row: 4, anchor: { measure: 0, beat: 1 } })).toBeNull()
    expect(selectedRows({ measure: 0, beat: 3, row: 4 })).toBeNull()
    expect(selectedRows(null)).toBeNull()
  })

  it('should keep the strings of a rectangle when extending it, and drop them on a move', () => {
    const rectangle = { measure: 0, beat: 2, row: 4, anchor: { measure: 0, beat: 1 }, anchorRow: 2 }
    expect(extendSelection({ score, track: 0, selection: rectangle, direction: 'right' })).toMatchObject({ beat: 3, anchorRow: 2 })
    expect(moveSelection({ score, track: 0, selection: rectangle, direction: 'right' })).not.toHaveProperty('anchorRow')
  })

  it('should keep a valid anchor and pull a stale one back', () => {
    const stale = { measure: 0, beat: 0, row: 1, anchor: { measure: 99, beat: 99 } }
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

describe('moveToTrack', () => {
  const move = (track: number, selection: Selection, step: -1 | 1) => moveToTrack({ score, track, selection, step })
  const startOf = (track: number, beat: number) =>
    score.tracks[track]!.measures[0]!.voices[0]!.beats.slice(0, beat).reduce((sum, b) => sum + durationTicks(b.duration), 0)

  it('should land on the beat of the other track that sounds at the same moment', () => {
    const { track, selection } = move(0, { measure: 0, beat: 5, row: 2 }, 1)
    expect(track).toBe(1)
    expect(selection.measure).toBe(0)
    // Guitar beat 5 is the sixth eighth note; it sounds during the third bass quarter.
    expect(startOf(0, 5)).toBe(80)
    expect(selection.beat).toBe(2)
    expect(startOf(1, 2)).toBe(64)
  })

  it('should keep the string when the other track has it, else take its lowest', () => {
    expect(move(0, { measure: 0, beat: 0, row: 2 }, 1).selection.row).toBe(2)
    expect(move(0, { measure: 0, beat: 0, row: 6 }, 1).selection.row).toBe(4)
  })

  it('should leave a range behind', () => {
    const { selection } = move(0, { measure: 0, beat: 2, row: 1, anchor: { measure: 0, beat: 0 } }, 1)
    expect(selection.anchor).toBeUndefined()
  })

  it('should stay put at the first and the last track', () => {
    const spot = { measure: 1, beat: 0, row: 1 }
    expect(move(0, spot, -1)).toEqual({ track: 0, selection: spot })
    expect(move(1, spot, 1)).toEqual({ track: 1, selection: spot })
  })
})

describe('on a drum track', () => {
  const withDrums: Score = {
    ...score,
    tracks: [
      ...score.tracks,
      {
        name: 'Drums',
        instrument: 'Drums',
        sound: 'drums/standard',
        tuning: [],
        measures: score.tracks[0]!.measures.map((m) => ({ time_signature: m.time_signature, voices: [{ beats: [{ duration: { value: 'Whole', dots: 0 }, notes: [] }] }] })),
      },
    ],
  }

  it('should start on the snare and keep the selection on drum pieces', () => {
    expect(firstSelection({ score: withDrums, track: 2 })).toEqual({ measure: 0, beat: 0, row: 38 })
    expect(clampSelection({ score: withDrums, track: 2, selection: { measure: 0, beat: 0, row: 3 } }).row).toBe(35)
    expect(clampSelection({ score: withDrums, track: 2, selection: { measure: 0, beat: 0, row: 100 } }).row).toBe(81)
  })

  it('should move up and down to the next piece on the staff', () => {
    const at = (row: number, direction: 'up' | 'down') =>
      moveSelection({ score: withDrums, track: 2, selection: { measure: 0, beat: 0, row }, direction }).row
    expect(at(36, 'down')).toBe(35)
    expect(at(36, 'up')).toBe(41)
    expect(at(42, 'up')).toBe(49)
    expect(at(44, 'down')).toBe(44)
  })

  it('should land on the snare when moving from a string to a drum track', () => {
    const down = moveToTrack({ score: withDrums, track: 1, selection: { measure: 0, beat: 0, row: 3 }, step: 1 })
    expect(down).toEqual({ track: 2, selection: { measure: 0, beat: 0, row: 38 } })
  })
})

describe('on a pitched track', () => {
  const withPiano: Score = {
    ...score,
    tracks: [
      ...score.tracks,
      {
        name: 'Piano',
        instrument: 'Piano',
        sound: 'piano/grand',
        tuning: [],
        measures: score.tracks[0]!.measures.map((m) => ({ time_signature: m.time_signature, voices: [{ beats: [{ duration: { value: 'Whole', dots: 0 }, notes: [] }] }] })),
      },
    ],
  }

  it('should move up and down a line or a space of the staff', () => {
    const at = (row: number, direction: 'up' | 'down') =>
      moveSelection({ score: withPiano, track: 2, selection: { measure: 0, beat: 0, row }, direction }).row
    expect(at(60, 'up')).toBe(62)
    expect(at(64, 'up')).toBe(65)
    expect(at(61, 'down')).toBe(60)
  })

  it('should start at middle C and keep any MIDI pitch', () => {
    expect(firstSelection({ score: withPiano, track: 2 })).toEqual({ measure: 0, beat: 0, row: 60 })
    expect(firstSelection({ score: withPiano, track: 0 })).toEqual(FIRST_NOTE)
    expect(clampSelection({ score: withPiano, track: 2, selection: { measure: 0, beat: 0, row: 100 } }).row).toBe(100)
    expect(clampSelection({ score: withPiano, track: 2, selection: { measure: 0, beat: 0, row: 300 } }).row).toBe(127)
  })

  it('should land on middle C when moving from a string to a pitched track, and back on the first string', () => {
    const down = moveToTrack({ score: withPiano, track: 1, selection: { measure: 0, beat: 0, row: 3 }, step: 1 })
    expect(down).toEqual({ track: 2, selection: { measure: 0, beat: 0, row: 60 } })
    const up = moveToTrack({ score: withPiano, track: 2, selection: { measure: 0, beat: 0, row: 72 }, step: -1 })
    expect(up.selection.row).toBe(1)
  })
})
