import { describe, expect, it } from 'vitest'
import demo from '../../public/demo-score.json'
import type { Beat, Measure, Score, Track } from './score-types'
import {
  clipLayout,
  durationTicks,
  hitTest,
  layoutRows,
  layoutTab,
  locateTick,
  measureTicks,
  METRICS,
  notationOf,
  ROW_METRICS,
  visibleRows,
} from './tab-layout'

const score = demo as Score
const guitar = score.tracks[0] as Track
const bass = score.tracks[1] as Track

const fourFour = { numerator: 4, denominator: 4 }

function trackWith(measures: Measure[], tuning = guitar.tuning): Track {
  return { name: 'test', instrument: 'Guitar', sound: 'guitar/nylon', tuning, measures }
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

describe('hitTest', () => {
  const layout = layoutTab({ track: guitar, width: 900 })
  const system = layout.systems[0]!
  const measure = system.measures[0]!
  const beat = measure.beats[2]!

  it('should find the beat and string under the point', () => {
    const hit = hitTest({ layout, x: beat.x, y: system.stringYs[3]! })
    expect(hit).toEqual({ measure: 0, beat: 2, row: 4, tick: beat.startTick, area: 'tab', pitch: null })
  })

  it('should pick the nearest beat when the click falls between two', () => {
    const next = measure.beats[3]!
    const nearThird = hitTest({ layout, x: beat.x + (next.x - beat.x) * 0.3, y: system.stringYs[0]! })
    const nearFourth = hitTest({ layout, x: beat.x + (next.x - beat.x) * 0.7, y: system.stringYs[0]! })
    expect(nearThird?.beat).toBe(2)
    expect(nearFourth?.beat).toBe(3)
  })

  it('should pick the nearest string, with generous reach above and below the staff', () => {
    expect(hitTest({ layout, x: beat.x, y: system.stringYs[0]! - 5 })?.row).toBe(1)
    expect(hitTest({ layout, x: beat.x, y: system.stringYs[5]! + 5 })?.row).toBe(6)
    expect(hitTest({ layout, x: beat.x, y: system.stringYs[2]! + 4 })?.row).toBe(3)
  })

  it('should not hit anything outside the staves', () => {
    expect(hitTest({ layout, x: beat.x, y: system.staffTop - 30 })).toBeNull()
    expect(hitTest({ layout, x: beat.x, y: system.staffBottom + 25 })).toBeNull()
  })

  it('should treat the clef area as the first measure of the line', () => {
    expect(hitTest({ layout, x: system.left + 5, y: system.stringYs[0]! })?.measure).toBe(0)
  })

  it('should find measures on later systems', () => {
    const narrow = layoutTab({ track: guitar, width: 500 })
    const later = narrow.systems[1]!
    const target = later.measures[0]!
    const hit = hitTest({ layout: narrow, x: target.beats[0]!.x, y: later.stringYs[1]! })
    expect(hit?.measure).toBe(target.index)
    expect(hit?.row).toBe(2)
  })

  it('should return null for a layout without systems', () => {
    expect(hitTest({ layout: layoutTab({ track: trackWith([]), width: 900 }), x: 50, y: 50 })).toBeNull()
  })
})

describe('layoutRows', () => {
  const wholes = (count: number): Beat[] =>
    Array.from({ length: count }, () => ({ duration: { value: 'Whole' as const, dots: 0 }, notes: [{ string: 1, fret: 3, technique: null }] }))
  const busy = trackWith([measureOf(eighths(8)), measureOf(eighths(8))])
  const calm = trackWith([measureOf(wholes(1)), measureOf(wholes(1))], bass.tuning)
  const both = [
    { track: 0, isCollapsed: false },
    { track: 1, isCollapsed: false },
  ]
  const layout = layoutRows({ tracks: [busy, calm], shown: both, width: 400 })
  const measuresOf = (row: number) => layout.rows[row]!.layout!.systems[0]!.measures

  it('should give measure n the same x and width in every row', () => {
    expect(measuresOf(0).map((m) => [m.x, m.width])).toEqual(measuresOf(1).map((m) => [m.x, m.width]))
  })

  it('should make a measure as wide as the track that needs most room', () => {
    const alone = layoutRows({ tracks: [calm], shown: [{ track: 0, isCollapsed: false }], width: 0 })
    const aloneWidth = alone.rows[0]!.layout!.systems[0]!.measures[0]!.width
    expect(measuresOf(1)[0]!.width).toBeGreaterThan(aloneWidth)
    const busyAlone = layoutRows({ tracks: [busy], shown: [{ track: 0, isCollapsed: false }], width: 0 })
    expect(measuresOf(1)[0]!.width).toBeCloseTo(busyAlone.rows[0]!.layout!.systems[0]!.measures[0]!.width, 3)
  })

  it('should put notes that start together at the same x', () => {
    expect(measuresOf(1)[1]!.beats[0]!.x).toBe(measuresOf(0)[1]!.beats[0]!.x)
  })

  it('should keep each row on one line, as long as the piece', () => {
    for (const row of layout.rows) {
      expect(row.layout!.systems).toHaveLength(1)
      expect(row.layout!.width).toBe(layout.width)
    }
    expect(layout.width).toBeGreaterThan(400)
  })

  it('should stretch a short piece to the width it is given', () => {
    const wide = layoutRows({ tracks: [busy, calm], shown: both, width: 3000 })
    const last = wide.rows[0]!.layout!.systems[0]!.measures.at(-1)!
    expect(wide.width).toBe(3000)
    expect(last.x + last.width).toBeCloseTo(3000 - METRICS.marginX, 3)
  })

  it('should stack the rows one under the other, a folded one only as tall as its header', () => {
    const folded = layoutRows({ tracks: [busy, calm], shown: [{ track: 0, isCollapsed: true }, both[1]!], width: 400 })
    const [first, second] = folded.rows
    expect(first).toMatchObject({ top: 0, height: ROW_METRICS.collapsedHeight, layout: null })
    expect(second!.top).toBe(ROW_METRICS.collapsedHeight)
    expect(folded.height).toBe(second!.top + second!.height)
  })

  it('should not move the other rows when a row is folded', () => {
    const folded = layoutRows({ tracks: [busy, calm], shown: [{ track: 0, isCollapsed: true }, both[1]!], width: 400 })
    expect(folded.rows[1]!.layout!.systems[0]!.measures.map((m) => m.x)).toEqual(measuresOf(1).map((m) => m.x))
  })

  it('should show only the tracks asked for, in the order asked', () => {
    const one = layoutRows({ tracks: [busy, calm], shown: [{ track: 1, isCollapsed: false }], width: 400 })
    expect(one.rows.map((r) => r.track)).toEqual([1])
    expect(one.rows[0]!.layout!.systems[0]!.stringYs).toHaveLength(4)
  })

  it('should place a shared cursor on the notes of every row', () => {
    const tick = 128 + 16
    const cursor = locateTick({ layout: layout.timeline, tick })!
    expect(cursor.measureIndex).toBe(1)
    expect(cursor.x).toBeCloseTo(measuresOf(0)[1]!.beats[1]!.x, 3)
    expect(locateTick({ layout: layout.timeline, tick: 128 })!.x).toBe(measuresOf(1)[1]!.beats[0]!.x)
  })

  it('should let a click in a row find its beat and string', () => {
    const row = layout.rows[0]!.layout!
    const beat = measuresOf(0)[1]!.beats[3]!
    expect(hitTest({ layout: row, x: beat.x, y: row.systems[0]!.stringYs[2]! })).toMatchObject({ measure: 1, beat: 3, row: 3 })
  })

  it('should lay out a score without tracks', () => {
    expect(layoutRows({ tracks: [], shown: [], width: 500 })).toMatchObject({ height: 0, rows: [] })
  })
})

describe('visibleRows', () => {
  const tracks = Array.from({ length: 64 }, () => guitar)
  const layout = layoutRows({ tracks, shown: tracks.map((_, track) => ({ track, isCollapsed: false })), width: 900 })

  it('should keep only the rows that cross the band on screen', () => {
    const height = layout.rows[0]!.height
    const shown = visibleRows({ layout, top: height * 10 + 1, bottom: height * 12 })
    expect(shown.map((r) => r.track)).toEqual([10, 11])
  })

  it('should keep nothing below the last row', () => {
    expect(visibleRows({ layout, top: layout.height, bottom: layout.height + 500 })).toEqual([])
  })
})

describe('clipLayout', () => {
  it('should keep only the measures that cross the band', () => {
    const layout = layoutRows({ tracks: [guitar], shown: [{ track: 0, isCollapsed: false }], width: 0 }).rows[0]!.layout!
    const measures = layout.systems[0]!.measures
    const second = measures[1]!
    const clipped = clipLayout({ layout, left: second.x + 1, right: second.x + second.width - 1 })
    expect(clipped.systems[0]!.measures.map((m) => m.index)).toEqual([1])
    expect(clipped.width).toBe(layout.width)
  })
})

describe('staves', () => {
  const piano: Track = {
    name: 'Piano',
    instrument: 'Piano',
    sound: 'piano/grand',
    tuning: [],
    measures: [
      measureOf([
        { duration: { value: 'Half', dots: 0 }, notes: [{ pitch: 48, technique: null }, { pitch: 64, technique: null }] },
        { duration: { value: 'Half', dots: 0 }, notes: [{ pitch: 72, technique: null }] },
      ]),
    ],
  }

  it('should write a piano on a grand staff without tablature', () => {
    expect(notationOf({ track: piano, fretted: 'tab' })).toEqual({ clefs: ['treble', 'bass'], hasTab: false })
    const system = layoutTab({ track: piano, width: 900 }).systems[0]!
    expect(system.stringYs).toEqual([])
    expect(system.staves.map((s) => s.clef)).toEqual(['treble', 'bass'])
    expect(system.staves[0]!.bottom).toBeLessThan(system.staves[1]!.top)
    expect(system.measures[0]!.beams).toEqual([])
  })

  it('should write a voice on one staff in its clef, and an organ on a grand staff', () => {
    const voice = { ...piano, name: 'Voice', instrument: 'Voice' as const, sound: 'voice/aahs' as const, clef: 'bass' as const }
    const system = layoutTab({ track: voice, width: 900 }).systems[0]!
    expect(system.staves.map((s) => s.clef)).toEqual(['bass'])
    expect(system.measures[0]!.staves[0]!.beats[0]!.heads.map((h) => h.pitch)).toEqual([48, 64])
    const tenor = layoutTab({ track: { ...voice, clef: 'treble8vb' }, width: 900 }).systems[0]!
    expect(tenor.staves.map((s) => s.clef)).toEqual(['treble8vb'])
    const organ = { ...piano, instrument: 'Organ' as const, sound: 'organ/drawbar' as const }
    expect(notationOf({ track: organ })).toEqual({ clefs: ['treble', 'bass'], hasTab: false })
  })

  it('should split a chord between the staves at middle C, and rest where a staff has nothing', () => {
    const [upper, lower] = layoutTab({ track: piano, width: 900 }).systems[0]!.measures[0]!.staves
    expect(upper!.beats[0]!.heads.map((h) => h.pitch)).toEqual([64])
    expect(lower!.beats[0]!.heads.map((h) => h.pitch)).toEqual([48])
    expect(lower!.beats[1]!.rest?.value).toBe('Half')
  })

  it('should give a fretted track its tablature, its staff, or the staff above the tablature', () => {
    expect(notationOf({ track: guitar, fretted: 'tab' })).toEqual({ clefs: [], hasTab: true })
    expect(notationOf({ track: guitar, fretted: 'staff' })).toEqual({ clefs: ['treble8vb'], hasTab: false })
    const both = layoutTab({ track: guitar, width: 900, fretted: 'both' }).systems[0]!
    expect(both.staves[0]!.bottom).toBeLessThan(both.staffTop)
    expect(both.top).toBe(both.staves[0]!.top)
    const notes = both.measures[0]!.staves[0]!.beats.flatMap((b) => b.heads)
    expect(notes[0]).toMatchObject({ pitch: 45, row: 5 })
  })

  it('should make a system with a staff taller, and its beats keep their x', () => {
    const tab = layoutTab({ track: guitar, width: 900 })
    const both = layoutTab({ track: guitar, width: 900, fretted: 'both' })
    expect(both.height).toBeGreaterThan(tab.height)
    const xs = (layout: typeof tab) => layout.systems[0]!.measures[0]!.beats.map((b) => b.x)
    expect(xs(both)).toEqual(xs(tab))
  })

  it('should find the pitch under a click on a staff, the note there first', () => {
    const layout = layoutTab({ track: piano, width: 900 })
    const system = layout.systems[0]!
    const beat = system.measures[0]!.beats[1]!
    const treble = system.staves[0]!
    // The top line of the treble staff is F5, of the bass staff A3.
    expect(hitTest({ layout, x: beat.x, y: treble.top })).toMatchObject({ area: 'staff', pitch: 77, row: 77, beat: 1 })
    const head = system.measures[0]!.staves[0]!.beats[1]!.heads[0]!
    expect(hitTest({ layout, x: beat.x, y: head.y + 1 })).toMatchObject({ pitch: 72, row: 72 })
    expect(hitTest({ layout, x: beat.x, y: system.staves[1]!.top })).toMatchObject({ pitch: 57 })
  })

  it('should give no string for an empty spot of the staff of a fretted track', () => {
    const layout = layoutTab({ track: guitar, width: 900, fretted: 'both' })
    const system = layout.systems[0]!
    const beat = system.measures[0]!.beats[0]!
    expect(hitTest({ layout, x: beat.x, y: system.staves[0]!.top - 12 })).toMatchObject({ area: 'staff', row: null })
    expect(hitTest({ layout, x: beat.x, y: system.stringYs[1]! })).toMatchObject({ area: 'tab', row: 2 })
  })

  it('should keep the rows of a guitar and a piano on the same measure x', () => {
    const shown = [
      { track: 0, isCollapsed: false },
      { track: 1, isCollapsed: false },
    ]
    const rows = layoutRows({ tracks: [guitar, piano], shown, width: 600, fretted: 'both' })
    const [first, second] = rows.rows.map((r) => r.layout!.systems[0]!)
    expect(first!.measures[0]!.x).toBe(second!.measures[0]!.x)
    expect(rows.rows[1]!.layout!.rowKind).toBe('pitch')
    expect(second!.staves).toHaveLength(2)
    expect(rows.rows[1]!.height).toBeGreaterThan(ROW_METRICS.minHeight)
  })
})
