import { drumPiece, isPlayedByFoot, pieceAtPosition, piecePosition } from './drums'
import { hasStrings, isPercussionTrack, isPitchedTrack } from './instruments'
import { isFrettedNote, isPercussionNote, notePitch, noteRow } from './notes'
import type { Beat, Duration, Measure, NoteValue, Technique, TimeSignature, Track } from './score-types'
import {
  C_MAJOR,
  clefsFor,
  clefsOf,
  engraveMeasure,
  keySignatureWidth,
  pitchAtPosition,
  positionAt,
  STAFF_METRICS,
  staffBlock,
  staffIndexFor,
  type Clef,
  type KeySignature,
  type StaffBlock,
  type StaffMeasure,
  type StaffNoteInput,
} from './staff-layout'
import type { FrettedView } from './track-view'

/** All distances are in SVG user units (1 unit = 1 CSS pixel at zoom 1). */
export const METRICS = {
  stringGap: 13,
  marginX: 24,
  clefWidth: 40,
  topPad: 34,
  rhythmHeight: 46,
  systemGap: 26,
  measurePadLeft: 14,
  measurePadRight: 10,
  timeSignatureWidth: 26,
  stemLength: 24,
  stemOffset: 10,
  beamGap: 5,
  /** Fraction of a system a last line must fill before it is stretched to the full width. */
  minLastLineFill: 0.7,
} as const

const TICKS_PER_WHOLE = 128

const BEAM_LEVELS: Partial<Record<NoteValue, number>> = { Eighth: 1, Sixteenth: 2, ThirtySecond: 3 }

const BASE_TICKS: Record<NoteValue, number> = {
  Whole: 128,
  Half: 64,
  Quarter: 32,
  Eighth: 16,
  Sixteenth: 8,
  ThirtySecond: 4,
}

const TECHNIQUE_LABELS: Record<Technique, string> = {
  HammerOn: 'H',
  PullOff: 'P',
  Slide: '/',
  Bend: 'b',
  PalmMute: 'P.M.',
  Harmonic: 'Harm.',
}

export type Segment = { x1: number; x2: number; y: number }

export type NoteLayout = {
  string: number
  fret: number
  y: number
  /** True when the fret is shown in parentheses because it continues a note across a barline. */
  isGhost: boolean
  techniqueLabel: string | null
  /** Horizontal extent of the tie arc into this note, if any. */
  tie: { fromX: number; toX: number } | null
}

export type BeatLayout = {
  x: number
  /** Absolute start position in 1/128 of a whole note, to sync a playback cursor later. */
  startTick: number
  ticks: number
  value: NoteValue
  dots: number
  isRest: boolean
  notes: NoteLayout[]
  stem: { x: number; y1: number; y2: number } | null
  /** Number of flags to draw when the beat is a lone eighth or shorter (not beamed). */
  flags: number
}

export type MeasureLayout = {
  index: number
  /** The measure engraved on each staff of the system (`SystemLayout.staves`), in the same order. */
  staves: StaffMeasure[]
  /** Absolute start position and length in 1/128 of a whole note. */
  startTick: number
  ticks: number
  x: number
  width: number
  beats: BeatLayout[]
  beams: Segment[]
  timeSignature: TimeSignature | null
}

export type SystemLayout = {
  y: number
  /** Top and bottom of what the system draws (first staff line to the end of the tablature rhythm), for cursors and scrolling. */
  top: number
  bottom: number
  /** Standard notation staves, above the tablature: none, one, or a grand staff of two. */
  staves: StaffBlock[]
  /** The tablature: its first and last string, and every string. No strings when the track shows no tablature. */
  staffTop: number
  staffBottom: number
  stringYs: number[]
  left: number
  right: number
  measures: MeasureLayout[]
}

/** What a selection row means in a layout: a string of the tablature, a pitch of the staff, or a drum piece. */
export type RowKind = 'string' | 'pitch' | 'piece'

export type TabLayout = {
  width: number
  height: number
  systems: SystemLayout[]
  rowKind: RowKind
  key: KeySignature
}

/** How a track is written: its staves (clefs), and whether it has a tablature. */
export type Notation = { clefs: Clef[]; hasTab: boolean }

/**
 * A pitched or drum track is written on its staves only. A fretted track shows its tablature, its
 * staff or both, as the view asks (`FrettedView`).
 */
export function notationOf({ track, fretted = 'tab' }: { track: Pick<Track, 'instrument' | 'clef'>; fretted?: FrettedView }): Notation {
  if (!hasStrings(track)) return { clefs: clefsOf(track), hasTab: false }
  return { clefs: fretted === 'tab' ? [] : clefsFor(track.instrument), hasTab: fretted !== 'staff' }
}

export function durationTicks({ value, dots }: Duration): number {
  let total = BASE_TICKS[value]
  let extra = total
  for (let i = 0; i < dots; i++) {
    extra /= 2
    total += extra
  }
  return total
}

export function measureTicks({ numerator, denominator }: TimeSignature): number {
  return (numerator * TICKS_PER_WHOLE) / denominator
}

/**
 * Lays a track out on systems that wrap at `width`, like a printed page: its staves (when
 * `fretted` asks for them, or always for a pitched track) above its tablature (unless `fretted`
 * is 'staff', or the track has no strings).
 */
export function layoutTab({
  track,
  width,
  fretted = 'tab',
  key = C_MAJOR,
}: {
  track: Track
  width: number
  fretted?: FrettedView
  key?: KeySignature
}): TabLayout {
  const notation = notationOf({ track, fretted })
  const stringCount = track.tuning.length
  const head = headWidth({ notation, key })
  const systemHeight = stackSystem({ notation, stringCount, y: 0 }).height
  const innerWidth = Math.max(width - 2 * METRICS.marginX - head, 160)

  const draft = draftMeasures(track)
  const rows = breakIntoSystems({ draft, innerWidth })

  const systems = rows.map(({ measures, isLast }, rowIndex): SystemLayout => {
    const y = METRICS.marginX + rowIndex * (systemHeight + METRICS.systemGap)
    const stack = stackSystem({ notation, stringCount, y })
    const left = METRICS.marginX

    const fixed = measures.reduce((sum, m) => sum + m.fixedWidth, 0)
    const natural = fixed + measures.reduce((sum, m) => sum + m.slotsWidth, 0)
    const shouldStretch = !isLast || natural / innerWidth >= METRICS.minLastLineFill
    const scale = shouldStretch ? (innerWidth - fixed) / measures.reduce((s, m) => s + m.slotsWidth, 0) : 1

    let cursor = left + head
    const placed = measures.map((m) => {
      const firstBeatX = cursor + METRICS.measurePadLeft + (m.timeSignature ? METRICS.timeSignatureWidth : 0)
      const layout = placeMeasure({
        draft: m,
        x: cursor,
        width: m.fixedWidth + m.slotsWidth * scale,
        xs: beatXs(firstBeatX, m.slotWidths, scale),
        stack,
        key,
      })
      cursor += layout.width
      return layout
    })

    // A last line that is not stretched ends at its final barline.
    const right = shouldStretch ? METRICS.marginX + head + innerWidth : cursor

    return { y, ...systemOf(stack), left, right, measures: placed }
  })

  const height = METRICS.marginX * 2 + systems.length * systemHeight + Math.max(systems.length - 1, 0) * METRICS.systemGap
  return { width, height, systems, rowKind: rowKindOf(track), key }
}

function rowKindOf(track: Pick<Track, 'instrument'>): RowKind {
  if (isPercussionTrack(track)) return 'piece'
  return isPitchedTrack(track) ? 'pitch' : 'string'
}

/** Room at the start of a system: the clef (or TAB) and the key signature of the staves. */
function headWidth({ notation, key }: { notation: Notation; key: KeySignature }): number {
  return METRICS.clefWidth + (notation.clefs.length > 0 ? keySignatureWidth(key) : 0)
}

/** Where the parts of a system go, from top to bottom: room for the measure numbers, the staves, the tablature and its rhythm. */
type SystemStack = {
  staves: StaffBlock[]
  staffTop: number
  staffBottom: number
  stringYs: number[]
  top: number
  bottom: number
  /** From the system's y to the end of what it draws. */
  height: number
}

function stackSystem({ notation, stringCount, y }: { notation: Notation; stringCount: number; y: number }): SystemStack {
  let cursor = y + METRICS.topPad
  const staves = notation.clefs.map((clef, i) => {
    cursor += i === 0 ? STAFF_METRICS.above : STAFF_METRICS.between
    const block = staffBlock({ clef, top: cursor })
    cursor = block.bottom
    return block
  })
  const top = staves[0]?.top ?? cursor
  if (!notation.hasTab) {
    const bottom = cursor + STAFF_METRICS.below
    return { staves, staffTop: cursor, staffBottom: cursor, stringYs: [], top, bottom, height: bottom - y }
  }
  if (staves.length > 0) cursor += STAFF_METRICS.below
  const staffTop = cursor
  const stringYs = Array.from({ length: stringCount }, (_, i) => staffTop + i * METRICS.stringGap)
  const staffBottom = staffTop + Math.max(stringCount - 1, 0) * METRICS.stringGap
  const bottom = staffBottom + METRICS.rhythmHeight
  return { staves, staffTop, staffBottom, stringYs, top, bottom, height: bottom - y }
}

function systemOf(stack: SystemStack): Pick<SystemLayout, 'top' | 'bottom' | 'staves' | 'staffTop' | 'staffBottom' | 'stringYs'> {
  const { staves, staffTop, staffBottom, stringYs, top, bottom } = stack
  return { staves, staffTop, staffBottom, stringYs, top, bottom }
}

/** A track as the rows view shows it: its tablature in full, or folded down to its header. */
export type ShownTrack = { track: number; isCollapsed: boolean }

export const ROW_METRICS = {
  /** Height of a folded row: its header only. */
  collapsedHeight: 40,
  bottomPad: 8,
  /** Room for the header of an unfolded row, whatever the number of strings. */
  minHeight: 112,
} as const

export type TrackRow = {
  track: number
  /** Where the row starts in the rows view, and its height. */
  top: number
  height: number
  /** One system as wide as the whole piece, or null when the row is folded. */
  layout: TabLayout | null
}

export type RowsLayout = {
  width: number
  height: number
  rows: TrackRow[]
  /**
   * The time axis shared by every row: one beat at each moment where any track starts a beat. A
   * cursor placed on it with `locateTick` is at the same x in every row.
   */
  timeline: TabLayout
}

/**
 * Lays out several tracks one under the other, each on a single line, on one time axis: measure n
 * of every track has the same x and width (the widest the tracks need), and notes that start at the
 * same moment are at the same x. Measure widths are computed from every track of `tracks`, not only
 * the ones shown, so folding a row does not move the others. A piece shorter than `width` is
 * stretched to it.
 */
export function layoutRows({
  tracks,
  shown,
  width,
  fretted = 'tab',
  key = C_MAJOR,
}: {
  tracks: Track[]
  shown: ShownTrack[]
  width: number
  fretted?: FrettedView
  key?: KeySignature
}): RowsLayout {
  const drafts = tracks.map(draftMeasures)
  // Every row starts its measures at the same x, so the head is as wide as the widest one needs.
  const head = Math.max(METRICS.clefWidth, ...tracks.map((track) => headWidth({ notation: notationOf({ track, fretted }), key })))
  const axis = layoutColumns({ drafts, width, head })

  let top = 0
  const rows = shown.map(({ track, isCollapsed }): TrackRow => {
    const draft = drafts[track]
    const source = tracks[track]
    const layout =
      isCollapsed || !draft || !source
        ? null
        : placeRow({ draft, axis, notation: notationOf({ track: source, fretted }), stringCount: source.tuning.length, rowKind: rowKindOf(source), key })
    const row = { track, top, height: layout?.height ?? ROW_METRICS.collapsedHeight, layout }
    top += row.height
    return row
  })

  return { width: axis.width, height: top, rows, timeline: timelineOf(axis) }
}

/** The rows that cross the band from `top` to `bottom`, to draw only what is on screen. */
export function visibleRows({ layout, top, bottom }: { layout: RowsLayout; top: number; bottom: number }): TrackRow[] {
  return layout.rows.filter((row) => row.top < bottom && row.top + row.height > top)
}

/** The same layout with only the measures that cross the band from `left` to `right`. */
export function clipLayout({ layout, left, right }: { layout: TabLayout; left: number; right: number }): TabLayout {
  return {
    ...layout,
    systems: layout.systems.map((system) => ({
      ...system,
      measures: system.measures.filter((m) => m.x < right && m.x + m.width > left),
    })),
  }
}

type Column = {
  index: number
  startTick: number
  ticks: number
  x: number
  width: number
  /** Ticks from the start of the measure at which some track starts a beat, and their x. */
  onsets: { tick: number; ticks: number; x: number }[]
}

type Axis = { width: number; columns: Column[]; head: number }

type DraftMeasure = {
  index: number
  measure: Measure
  beats: Beat[]
  startTick: number
  timeSignature: TimeSignature | null
  slotWidths: number[]
  slotsWidth: number
  /** Width that does not stretch: padding and the time signature. */
  fixedWidth: number
  /** The sounding pitches of each beat, for the staves (for drums the pieces, at their places). */
  pitches: StaffNoteInput[][]
  /** The way the stem of each beat must go, when it is fixed (drums: down for the feet alone). */
  stems: ('up' | 'down' | undefined)[]
  /** Pitches sounding at the end of the previous measure, which a tie can continue. */
  carried: number[]
}

function draftMeasures(track: Track): DraftMeasure[] {
  let startTick = 0
  let previous: TimeSignature | null = null
  let carried: number[] = []

  return track.measures.map((measure, index) => {
    const beats = measure.voices[0]?.beats ?? []
    const pitches = beats.map((beat) =>
      beat.notes.flatMap((note): StaffNoteInput[] => {
        const pitch = notePitch({ track, note })
        if (pitch === null) return []
        const input: StaffNoteInput = { pitch, tied: note.tied === true, row: noteRow(note) }
        if (isPercussionNote(note)) {
          input.position = piecePosition(note.piece) ?? 0
          input.head = drumPiece(note.piece)?.head ?? 'normal'
        }
        return [input]
      }),
    )
    // Drummers read the hands with stems up and the feet with stems down.
    const stems = beats.map((beat) => {
      if (!isPercussionTrack(track) || beat.notes.length === 0) return undefined
      return beat.notes.every((n) => isPlayedByFoot(noteRow(n))) ? ('down' as const) : ('up' as const)
    })
    const signature = measure.time_signature
    const shouldShowSignature = previous === null || !sameSignature(previous, signature)
    previous = signature

    const slotWidths = beats.map((beat) => slotWidth(durationTicks(beat.duration)))
    const draft: DraftMeasure = {
      index,
      measure,
      beats,
      startTick,
      timeSignature: shouldShowSignature ? signature : null,
      slotWidths,
      slotsWidth: slotWidths.reduce((sum, w) => sum + w, 0),
      fixedWidth:
        METRICS.measurePadLeft + METRICS.measurePadRight + (shouldShowSignature ? METRICS.timeSignatureWidth : 0),
      pitches,
      stems,
      carried,
    }
    carried = (pitches.at(-1) ?? []).map((p) => p.pitch)
    startTick += measureTicks(signature)
    return draft
  })
}

function sameSignature(a: TimeSignature, b: TimeSignature): boolean {
  return a.numerator === b.numerator && a.denominator === b.denominator
}

/** Space grows with the square root of the duration, so short notes stay readable. */
function slotWidth(ticks: number): number {
  return 18 + 12 * Math.sqrt(ticks / 4)
}

function breakIntoSystems({ draft, innerWidth }: { draft: DraftMeasure[]; innerWidth: number }) {
  const rows: { measures: DraftMeasure[]; isLast: boolean }[] = []
  let current: DraftMeasure[] = []
  let used = 0

  for (const m of draft) {
    const natural = m.fixedWidth + m.slotsWidth
    if (current.length > 0 && used + natural > innerWidth) {
      rows.push({ measures: current, isLast: false })
      current = []
      used = 0
    }
    current.push(m)
    used += natural
  }
  if (current.length > 0) rows.push({ measures: current, isLast: true })
  return rows
}

/** Places the beats of a measure at the given positions (`xs`, one per beat) in a measure of `width`. */
function placeMeasure({
  draft,
  x,
  width,
  xs,
  stack,
  key,
}: {
  draft: DraftMeasure
  x: number
  width: number
  xs: number[]
  stack: Pick<SystemStack, 'staves' | 'stringYs' | 'staffBottom'>
  key: KeySignature
}): MeasureLayout {
  const { stringYs, staffBottom } = stack
  const { beats, timeSignature } = draft
  const firstBeatX = x + METRICS.measurePadLeft + (timeSignature ? METRICS.timeSignatureWidth : 0)

  const stemTop = staffBottom + METRICS.stemOffset
  const stemBottom = stemTop + METRICS.stemLength

  let tick = draft.startTick
  const placed: BeatLayout[] = beats.map((beat, i) => {
    const beatX = xs[i] ?? firstBeatX
    const previousX = xs[i - 1]
    const previousBeat = beats[i - 1]
    const ticks = durationTicks(beat.duration)

    // Only a fretted note has a place on the tablature.
    const notes = beat.notes.filter(isFrettedNote).map((note) => {
      const isTied = note.tied === true
      const hasLocalSource = previousBeat?.notes.some((n) => isFrettedNote(n) && n.string === note.string) ?? false
      const tie = isTied
        ? { fromX: hasLocalSource && previousX !== undefined ? previousX + 6 : x + 2, toX: beatX - 7 }
        : null
      return {
        string: note.string,
        fret: note.fret,
        y: stringYs[note.string - 1] ?? stringYs[0] ?? 0,
        isGhost: isTied && !hasLocalSource,
        techniqueLabel: note.technique ? TECHNIQUE_LABELS[note.technique] : null,
        tie,
      }
    })

    const isRest = notes.length === 0
    const hasStem = !isRest && beat.duration.value !== 'Whole'
    const level = BEAM_LEVELS[beat.duration.value] ?? 0
    const layout: BeatLayout = {
      x: beatX,
      startTick: tick,
      ticks,
      value: beat.duration.value,
      dots: beat.duration.dots,
      isRest,
      notes,
      stem: hasStem ? { x: beatX, y1: stemTop, y2: stemBottom } : null,
      flags: level,
    }
    tick += ticks
    return layout
  })

  const staffCount = stack.staves.length
  const staves = stack.staves.map((block, staff) =>
    engraveMeasure({
      beats: placed.map((beat, i) => ({
        x: beat.x,
        startTick: beat.startTick,
        value: beat.value,
        dots: beat.dots,
        notes: (draft.pitches[i] ?? []).filter((n) => staffIndexFor({ pitch: n.pitch, staffCount }) === staff),
        ...(draft.stems[i] ? { stem: draft.stems[i] } : {}),
      })),
      block,
      key,
      measureX: x,
      carried: draft.carried.filter((pitch) => staffIndexFor({ pitch, staffCount }) === staff),
    }),
  )

  return {
    index: draft.index,
    staves,
    startTick: draft.startTick,
    ticks: measureTicks(draft.measure.time_signature),
    x,
    width,
    beats: placed,
    beams: stringYs.length > 0 ? buildBeams({ beats: placed, stemBottom }) : [],
    timeSignature,
  }
}

function beatXs(firstBeatX: number, slotWidths: number[], scale: number): number[] {
  let offset = 0
  return slotWidths.map((w) => {
    const slot = w * scale
    const x = firstBeatX + offset + Math.min(slot / 2, 10)
    offset += slot
    return x
  })
}

/** Measure n of every track in one column, as wide as the track that needs most room. */
function layoutColumns({ drafts, width, head }: { drafts: DraftMeasure[][]; width: number; head: number }): Axis {
  const count = Math.max(0, ...drafts.map((d) => d.length))
  const raw = Array.from({ length: count }, (_, index) => {
    const inMeasure = drafts.flatMap((d) => d[index] ?? [])
    const first = inMeasure[0]
    const ticks = first ? measureTicks(first.measure.time_signature) : TICKS_PER_WHOLE
    const starts = new Set<number>()
    for (const draft of inMeasure) {
      let tick = 0
      for (const beat of draft.beats) {
        if (tick < ticks) starts.add(tick)
        tick += durationTicks(beat.duration)
      }
    }
    const sorted = [...starts].sort((a, b) => a - b)
    const gaps = sorted.map((tick, i) => (sorted[i + 1] ?? ticks) - tick)
    const slotWidths = gaps.map(slotWidth)
    const hasSignature = inMeasure.some((d) => d.timeSignature !== null)
    return {
      index,
      startTick: first?.startTick ?? index * TICKS_PER_WHOLE,
      ticks,
      sorted,
      gaps,
      slotWidths,
      slotsWidth: slotWidths.reduce((sum, w) => sum + w, 0),
      signatureWidth: hasSignature ? METRICS.timeSignatureWidth : 0,
      fixedWidth: METRICS.measurePadLeft + METRICS.measurePadRight + (hasSignature ? METRICS.timeSignatureWidth : 0),
    }
  })

  const margins = 2 * METRICS.marginX + head
  const fixed = raw.reduce((sum, c) => sum + c.fixedWidth, 0)
  const slots = raw.reduce((sum, c) => sum + c.slotsWidth, 0)
  const natural = margins + fixed + slots
  const scale = natural < width && slots > 0 ? (width - margins - fixed) / slots : 1

  let x = METRICS.marginX + head
  const columns = raw.map((c): Column => {
    const xs = beatXs(x + METRICS.measurePadLeft + c.signatureWidth, c.slotWidths, scale)
    const column = {
      index: c.index,
      startTick: c.startTick,
      ticks: c.ticks,
      x,
      width: c.fixedWidth + c.slotsWidth * scale,
      onsets: c.sorted.map((tick, i) => ({ tick, ticks: c.gaps[i] ?? 0, x: xs[i] ?? x })),
    }
    x += column.width
    return column
  })
  return { width: Math.max(width, x + METRICS.marginX), columns, head }
}

/** One track on one line, its beats on the shared columns. */
function placeRow({
  draft,
  axis,
  notation,
  stringCount,
  rowKind,
  key,
}: {
  draft: DraftMeasure[]
  axis: Axis
  notation: Notation
  stringCount: number
  rowKind: RowKind
  key: KeySignature
}): TabLayout {
  const stack = stackSystem({ notation, stringCount, y: 0 })

  const measures = draft.flatMap((m) => {
    const column = axis.columns[m.index]
    if (!column) return []
    const xOf = new Map(column.onsets.map((onset) => [onset.tick, onset.x]))
    let tick = 0
    const xs = m.beats.map((beat) => {
      const x = xOf.get(tick) ?? column.x
      tick += durationTicks(beat.duration)
      return x
    })
    return [placeMeasure({ draft: m, x: column.x, width: column.width, xs, stack, key })]
  })

  const system: SystemLayout = {
    y: 0,
    ...systemOf(stack),
    left: METRICS.marginX,
    right: axis.width - METRICS.marginX,
    measures,
  }
  const height = Math.max(stack.height + ROW_METRICS.bottomPad, ROW_METRICS.minHeight)
  return { width: axis.width, height, systems: [system], rowKind, key }
}

/** The columns as a layout of empty beats, one per onset, for `locateTick`. */
function timelineOf(axis: Axis): TabLayout {
  const measures = axis.columns.map(
    (column): MeasureLayout => ({
      index: column.index,
      startTick: column.startTick,
      ticks: column.ticks,
      x: column.x,
      width: column.width,
      staves: [],
      beats: column.onsets.map((onset) => ({
        x: onset.x,
        startTick: column.startTick + onset.tick,
        ticks: onset.ticks,
        value: 'Quarter',
        dots: 0,
        isRest: true,
        notes: [],
        stem: null,
        flags: 0,
      })),
      beams: [],
      timeSignature: null,
    }),
  )
  const system: SystemLayout = {
    y: 0,
    top: 0,
    bottom: 0,
    staves: [],
    staffTop: 0,
    staffBottom: 0,
    stringYs: [],
    left: METRICS.marginX,
    right: axis.width - METRICS.marginX,
    measures,
  }
  return { width: axis.width, height: 0, systems: [system], rowKind: 'string', key: C_MAJOR }
}

/**
 * Groups consecutive short notes that fall in the same quarter note and connects them with beams.
 * A note left alone in its group keeps its flags instead.
 */
function buildBeams({ beats, stemBottom }: { beats: BeatLayout[]; stemBottom: number }): Segment[] {
  const segments: Segment[] = []
  const groups: BeatLayout[][] = []
  let group: BeatLayout[] = []
  let groupQuarter = -1

  const flush = () => {
    if (group.length > 0) groups.push(group)
    group = []
  }

  for (const beat of beats) {
    const isBeamable = !beat.isRest && beat.flags > 0
    const quarter = Math.floor(beat.startTick / 32)
    if (!isBeamable || quarter !== groupQuarter) flush()
    if (isBeamable) {
      group.push(beat)
      groupQuarter = quarter
    }
  }
  flush()

  for (const g of groups) {
    if (g.length < 2) continue

    const first = g[0]
    const last = g[g.length - 1]
    if (!first || !last) continue

    for (let level = 1; level <= 3; level++) {
      const y = stemBottom - (level - 1) * METRICS.beamGap
      let runStart: BeatLayout | null = null
      let runEnd: BeatLayout | null = null

      const closeRun = () => {
        if (!runStart || !runEnd) return
        const isStub = runStart === runEnd
        const x1 = isStub ? runStart.x - (runStart === first ? 0 : 6) : runStart.x
        const x2 = isStub ? runStart.x + (runStart === first ? 6 : 0) : runEnd.x
        segments.push({ x1, x2, y })
        runStart = null
        runEnd = null
      }

      for (const beat of g) {
        if (beat.flags >= level) {
          runStart ??= beat
          runEnd = beat
        } else {
          closeRun()
        }
      }
      closeRun()
    }

    // A beamed beat does not draw its own flags.
    for (const beat of g) beat.flags = 0
  }

  return segments
}

export type CursorPosition = {
  systemIndex: number
  measureIndex: number
  measureX: number
  measureWidth: number
  x: number
}

/**
 * Finds where a playback position falls on the page. Between two beats the cursor glides, so it
 * moves smoothly. A position before the start or past the end sticks to the nearest edge.
 */
export function locateTick({ layout, tick }: { layout: TabLayout; tick: number }): CursorPosition | null {
  const entries = layout.systems.flatMap((system, systemIndex) =>
    system.measures.map((measure) => ({ system, systemIndex, measure })),
  )
  const last = entries.at(-1)
  if (!last) return null

  const found =
    entries.find(({ measure }) => tick >= measure.startTick && tick < measure.startTick + measure.ticks) ??
    (tick < 0 ? entries[0] : last)
  if (!found) return null

  const { measure, systemIndex } = found
  const measureEnd = measure.x + measure.width - METRICS.measurePadRight
  const isPastEnd = tick >= measure.startTick + measure.ticks

  let x = measure.x + METRICS.measurePadLeft
  const beatIndex = measure.beats.findLastIndex((beat) => beat.startTick <= tick)
  const beat = measure.beats[beatIndex]
  if (isPastEnd) {
    x = measureEnd
  } else if (beat) {
    const nextX = measure.beats[beatIndex + 1]?.x ?? measureEnd
    const progress = Math.min(Math.max((tick - beat.startTick) / beat.ticks, 0), 1)
    x = beat.x + progress * (nextX - beat.x)
  } else if (measure.beats[0]) {
    x = measure.beats[0].x
  }

  return { systemIndex, measureIndex: measure.index, measureX: measure.x, measureWidth: measure.width, x }
}

export type HitTarget = {
  measure: number
  beat: number
  /** Where the beat starts, in 1/128 of a whole note: the position to seek to. */
  tick: number
  /** On the tablature or on a staff. */
  area: 'tab' | 'staff'
  /**
   * What a selection points at there (see `RowKind`): the string under the point on the
   * tablature; on a staff, the row of the note written at that line or space, else the pitch on
   * a pitched track, else null (a fretted track has no string at an empty spot of its staff).
   */
  row: number | null
  /** On a staff: the pitch of the note there, or the note of the key written at that line or space. */
  pitch: number | null
}

/**
 * Finds the beat and the string or pitch under a point of the page, or null if the point is not
 * on a staff or a tablature. A click between two beats picks the nearest one, so no beat is hard
 * to hit. The band of a staff reaches halfway to what is above and below it, for ledger lines.
 */
export function hitTest({ layout, x, y }: { layout: TabLayout; x: number; y: number }): HitTarget | null {
  const half = METRICS.stringGap / 2

  for (const system of layout.systems) {
    const hasTab = system.stringYs.length > 0
    const tabTop = system.staffTop - half
    const bands = system.staves.map((block, i) => {
      const above = system.staves[i - 1]
      const below = system.staves[i + 1]
      return {
        block,
        index: i,
        top: above ? (above.bottom + block.top) / 2 : block.top - STAFF_METRICS.above - METRICS.topPad / 2,
        bottom: below ? (block.bottom + below.top) / 2 : hasTab ? (block.bottom + tabTop) / 2 : system.bottom,
      }
    })
    const band = bands.find((b) => y >= b.top && y < b.bottom)
    const isOnTab = hasTab && y >= tabTop && y <= system.staffBottom + half
    if (!band && !isOnTab) continue

    const measure = system.measures.find((m) => x >= m.x && x < m.x + m.width) ?? nearestMeasure(system, x)
    if (!measure || measure.beats.length === 0) return null

    let beat = 0
    measure.beats.forEach((candidate, index) => {
      const best = measure.beats[beat]
      if (best && Math.abs(candidate.x - x) < Math.abs(best.x - x)) beat = index
    })
    const tick = measure.beats[beat]?.startTick ?? measure.startTick

    if (band) {
      const position = positionAt({ y, bottom: band.block.bottom })
      const head = measure.staves[band.index]?.beats[beat]?.heads.find((h) => h.position === position)
      // On a percussion staff the line or space names a piece, not a pitch.
      const pitch =
        head?.pitch ??
        (band.block.clef === 'percussion' ? pieceAtPosition(position) : pitchAtPosition({ position, clef: band.block.clef, key: layout.key }))
      const row = head?.row ?? (layout.rowKind === 'string' ? null : pitch)
      return { measure: measure.index, beat, tick, area: 'staff', row, pitch }
    }
    const lines = system.stringYs.length
    const string = Math.min(Math.max(Math.round((y - system.staffTop) / METRICS.stringGap), 0), lines - 1) + 1
    return { measure: measure.index, beat, tick, area: 'tab', row: string, pitch: null }
  }
  return null
}

/** The first or last measure of a system, for a click in the clef area or past the last barline. */
function nearestMeasure(system: SystemLayout, x: number): MeasureLayout | undefined {
  const first = system.measures[0]
  const last = system.measures.at(-1)
  if (!first || !last) return undefined
  return x < first.x ? first : last
}
