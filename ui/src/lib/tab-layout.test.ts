import { describe, expect, it } from 'vitest'
import demo from '../../public/demo-score.json'
import type { Beat, Measure, Score, Track } from './score-types'
import { durationTicks, layoutTab, locateTick, measureTicks, METRICS } from './tab-layout'

const score = demo as Score
const guitar = score.tracks[0] as Track
const bass = score.tracks[1] as Track

const fourFour = { numerator: 4, denominator: 4 }

function trackWith(measures: Measure[], tuning = guitar.tuning): Track {
  return { name: 'test', instrument: 'Guitar', tuning, measures }
}

function measureOf(beats: Beat[]): Measure {
  return { time_signature: fourFour, voices: [{ beats }] }
}

function eighths(count: number): Beat[] {
  return Array.from({ length: count }, () => ({
    duration: { value: 'Eighth' as const, dots: 0 },
    notes: [{ string: 1, fret: 0, technique: null }],
  }))
}

describe('durations', () => {
  it('should count dotted notes', () => {
    expect(durationTicks({ value: 'Quarter', dots: 1 })).toBe(48)
    expect(durationTicks({ value: 'Half', dots: 2 })).toBe(112)
  })

  it('should compute the length of a measure from its time signature', () => {
    expect(measureTicks({ numerator: 6, denominator: 8 })).toBe(96)
  })
})

describe('layoutTab', () => {
  it('should place every measure exactly once, in order', () => {
    const layout = layoutTab({ track: guitar, width: 900 })
    const indexes = layout.systems.flatMap((s) => s.measures.map((m) => m.index))
    expect(indexes).toEqual(guitar.measures.map((_, i) => i))
  })

  it('should keep every system within the available width', () => {
    for (const width of [500, 700, 900, 1400]) {
      const layout = layoutTab({ track: guitar, width })
      for (const system of layout.systems) {
        const last = system.measures.at(-1)!
        expect(last.x + last.width).toBeLessThanOrEqual(system.right + 0.001)
      }
    }
  })

  it('should stretch a full line to the right edge', () => {
    const layout = layoutTab({ track: guitar, width: 900 })
    const first = layout.systems[0]!
    const last = first.measures.at(-1)!
    expect(last.x + last.width).toBeCloseTo(first.right, 3)
  })

  it('should use more systems when the page is narrower', () => {
    const wide = layoutTab({ track: guitar, width: 1400 })
    const narrow = layoutTab({ track: guitar, width: 500 })
    expect(narrow.systems.length).toBeGreaterThan(wide.systems.length)
  })

  it('should put string 1 on the top line', () => {
    const system = layoutTab({ track: guitar, width: 900 }).systems[0]!
    expect(system.stringYs).toHaveLength(6)
    const note = system.measures[0]!.beats[0]!.notes[0]!
    expect(note.string).toBe(5)
    expect(note.y).toBe(system.stringYs[4])
    expect(system.stringYs[0]!).toBeLessThan(system.stringYs[5]!)
  })

  it('should draw as many lines as the instrument has strings', () => {
    const system = layoutTab({ track: bass, width: 900 }).systems[0]!
    expect(system.stringYs).toHaveLength(4)
  })

  it('should show the time signature on the first measure only', () => {
    const shown = layoutTab({ track: guitar, width: 600 }).systems.flatMap((s) => s.measures).map((m) => m.timeSignature)
    expect(shown[0]).toEqual(fourFour)
    expect(shown.slice(1).every((t) => t === null)).toBe(true)
  })

  it('should show the time signature again when it changes', () => {
    const waltz = { ...measureOf(eighths(6)), time_signature: { numerator: 3, denominator: 4 } }
    const track = trackWith([measureOf(eighths(8)), waltz])
    const measures = layoutTab({ track, width: 900 }).systems.flatMap((s) => s.measures)
    expect(measures[1]!.timeSignature).toEqual({ numerator: 3, denominator: 4 })
  })

  it('should track the absolute start tick of each beat across measures', () => {
    const measures = layoutTab({ track: guitar, width: 900 }).systems.flatMap((s) => s.measures)
    expect(measures[0]!.beats[0]!.startTick).toBe(0)
    expect(measures[1]!.beats[0]!.startTick).toBe(128)
    expect(measures[1]!.beats[1]!.startTick).toBe(144)
  })

  it('should render an empty track without failing', () => {
    const layout = layoutTab({ track: trackWith([]), width: 900 })
    expect(layout.systems).toEqual([])
  })

  it('should survive a tiny width', () => {
    const layout = layoutTab({ track: guitar, width: 50 })
    expect(layout.systems.length).toBe(guitar.measures.length)
  })
})

describe('rests and notes', () => {
  it('should mark a beat without notes as a rest with no stem', () => {
    const measure = layoutTab({ track: guitar, width: 900 }).systems.flatMap((s) => s.measures)[4]!
    const rest = measure.beats[2]!
    expect(rest.isRest).toBe(true)
    expect(rest.stem).toBeNull()
  })

  it('should give a whole note no stem', () => {
    const measure = layoutTab({ track: guitar, width: 900 }).systems.flatMap((s) => s.measures)[5]!
    expect(measure.beats[0]!.stem).toBeNull()
    expect(measure.beats[0]!.notes).toHaveLength(5)
  })

  it('should label a hammer-on', () => {
    const measure = layoutTab({ track: guitar, width: 900 }).systems.flatMap((s) => s.measures)[2]!
    expect(measure.beats[3]!.notes[0]!.techniqueLabel).toBe('H')
  })
})

describe('ties', () => {
  const measures = layoutTab({ track: guitar, width: 900 }).systems.flatMap((s) => s.measures)

  it('should draw a tie across a barline from the measure start, with a ghost fret', () => {
    const measure = measures[4]!
    const tied = measure.beats[0]!.notes[0]!
    expect(tied.isGhost).toBe(true)
    expect(tied.tie!.fromX).toBeLessThan(tied.tie!.toX)
    expect(tied.tie!.fromX).toBeGreaterThanOrEqual(measure.x)
  })

  it('should draw a tie inside a measure from the previous beat', () => {
    const track = trackWith([
      measureOf([
        { duration: { value: 'Half', dots: 0 }, notes: [{ string: 2, fret: 3, technique: null }] },
        { duration: { value: 'Half', dots: 0 }, notes: [{ string: 2, fret: 3, technique: null, tied: true }] },
      ]),
    ])
    const [first, second] = layoutTab({ track, width: 900 }).systems[0]!.measures[0]!.beats
    const tied = second!.notes[0]!
    expect(tied.isGhost).toBe(false)
    expect(tied.tie!.fromX).toBeGreaterThan(first!.x)
    expect(tied.tie!.toX).toBeLessThan(second!.x)
  })
})

describe('beams', () => {
  it('should beam eighth notes two by two within a quarter note', () => {
    const track = trackWith([measureOf(eighths(8))])
    const measure = layoutTab({ track, width: 900 }).systems[0]!.measures[0]!
    expect(measure.beams).toHaveLength(4)
    expect(measure.beats.every((b) => b.flags === 0)).toBe(true)
  })

  it('should keep the flag of a lone eighth note', () => {
    const lone: Beat[] = [
      { duration: { value: 'Quarter', dots: 1 }, notes: [{ string: 1, fret: 0, technique: null }] },
      { duration: { value: 'Eighth', dots: 0 }, notes: [{ string: 1, fret: 0, technique: null }] },
      { duration: { value: 'Half', dots: 0 }, notes: [{ string: 1, fret: 0, technique: null }] },
    ]
    const measure = layoutTab({ track: trackWith([measureOf(lone)]), width: 900 }).systems[0]!.measures[0]!
    expect(measure.beams).toHaveLength(0)
    expect(measure.beats[1]!.flags).toBe(1)
  })

  it('should add a second beam for sixteenth notes', () => {
    const sixteenths: Beat[] = Array.from({ length: 4 }, () => ({
      duration: { value: 'Sixteenth' as const, dots: 0 },
      notes: [{ string: 1, fret: 0, technique: null }],
    }))
    const rest: Beat = { duration: { value: 'Half', dots: 1 }, notes: [] }
    const measure = layoutTab({ track: trackWith([measureOf([...sixteenths, rest])]), width: 900 }).systems[0]!.measures[0]!
    const ys = new Set(measure.beams.map((b) => b.y))
    expect(ys.size).toBe(2)
  })

  it('should not beam across a rest', () => {
    const beats: Beat[] = [
      { duration: { value: 'Eighth', dots: 0 }, notes: [{ string: 1, fret: 0, technique: null }] },
      { duration: { value: 'Eighth', dots: 0 }, notes: [] },
      { duration: { value: 'Eighth', dots: 0 }, notes: [{ string: 1, fret: 0, technique: null }] },
      { duration: { value: 'Eighth', dots: 0 }, notes: [{ string: 1, fret: 0, technique: null }] },
      { duration: { value: 'Half', dots: 0 }, notes: [] },
    ]
    const measure = layoutTab({ track: trackWith([measureOf(beats)]), width: 900 }).systems[0]!.measures[0]!
    expect(measure.beams).toHaveLength(1)
    expect(measure.beats[0]!.flags).toBe(1)
  })
})

describe('metrics', () => {
  it('should reserve room for the clef before the first measure', () => {
    const system = layoutTab({ track: guitar, width: 900 }).systems[0]!
    expect(system.measures[0]!.x).toBe(system.left + METRICS.clefWidth)
  })
})

describe('locateTick', () => {
  const layout = layoutTab({ track: guitar, width: 900 })
  const measures = layout.systems.flatMap((s) => s.measures)

  it('should put the start of the piece on the first beat', () => {
    const cursor = locateTick({ layout, tick: 0 })!
    expect(cursor.systemIndex).toBe(0)
    expect(cursor.measureIndex).toBe(0)
    expect(cursor.x).toBe(measures[0]!.beats[0]!.x)
  })

  it('should glide between two beats', () => {
    const [a, b] = measures[0]!.beats
    const halfway = locateTick({ layout, tick: a!.startTick + a!.ticks / 2 })!
    expect(halfway.x).toBeCloseTo((a!.x + b!.x) / 2, 5)
  })

  it('should move to the next measure on the barline', () => {
    const cursor = locateTick({ layout, tick: 128 })!
    expect(cursor.measureIndex).toBe(1)
    expect(cursor.x).toBe(measures[1]!.beats[0]!.x)
  })

  it('should find a measure on another system', () => {
    const narrow = layoutTab({ track: guitar, width: 500 })
    const cursor = locateTick({ layout: narrow, tick: 5 * 128 + 10 })!
    expect(cursor.measureIndex).toBe(5)
    expect(cursor.systemIndex).toBeGreaterThan(0)
  })

  it('should never go backwards as time advances within a system', () => {
    let previous = -Infinity
    for (let tick = 0; tick < 2 * 128; tick += 3) {
      const cursor = locateTick({ layout, tick })!
      if (cursor.systemIndex === 0) {
        expect(cursor.x).toBeGreaterThanOrEqual(previous)
        previous = cursor.x
      }
    }
  })

  it('should stay on the first beat before the start', () => {
    expect(locateTick({ layout, tick: -10 })!.x).toBe(measures[0]!.beats[0]!.x)
  })

  it('should stop at the end of the last measure past the end', () => {
    const cursor = locateTick({ layout, tick: 1_000_000 })!
    const lastMeasure = measures.at(-1)!
    expect(cursor.measureIndex).toBe(lastMeasure.index)
    expect(cursor.x).toBe(lastMeasure.x + lastMeasure.width - METRICS.measurePadRight)
  })

  it('should return null for an empty layout', () => {
    expect(locateTick({ layout: layoutTab({ track: trackWith([]), width: 900 }), tick: 0 })).toBeNull()
  })
})
