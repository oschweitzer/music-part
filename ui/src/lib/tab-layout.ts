import type { Beat, Duration, Measure, NoteValue, Technique, TimeSignature, Track } from './score-types'

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
  x: number
  width: number
  beats: BeatLayout[]
  beams: Segment[]
  timeSignature: TimeSignature | null
}

export type SystemLayout = {
  y: number
  staffTop: number
  staffBottom: number
  stringYs: number[]
  left: number
  right: number
  measures: MeasureLayout[]
}

export type TabLayout = { width: number; height: number; systems: SystemLayout[] }

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

export function layoutTab({ track, width }: { track: Track; width: number }): TabLayout {
  const stringCount = track.tuning.length
  const staffHeight = (stringCount - 1) * METRICS.stringGap
  const systemHeight = METRICS.topPad + staffHeight + METRICS.rhythmHeight
  const innerWidth = Math.max(width - 2 * METRICS.marginX - METRICS.clefWidth, 160)

  const draft = draftMeasures(track)
  const rows = breakIntoSystems({ draft, innerWidth })

  const systems = rows.map(({ measures, isLast }, rowIndex): SystemLayout => {
    const y = METRICS.marginX + rowIndex * (systemHeight + METRICS.systemGap)
    const staffTop = y + METRICS.topPad
    const stringYs = Array.from({ length: stringCount }, (_, i) => staffTop + i * METRICS.stringGap)
    const left = METRICS.marginX

    const fixed = measures.reduce((sum, m) => sum + m.fixedWidth, 0)
    const natural = fixed + measures.reduce((sum, m) => sum + m.slotsWidth, 0)
    const shouldStretch = !isLast || natural / innerWidth >= METRICS.minLastLineFill
    const scale = shouldStretch ? (innerWidth - fixed) / measures.reduce((s, m) => s + m.slotsWidth, 0) : 1

    let cursor = left + METRICS.clefWidth
    const placed = measures.map((m) => {
      const layout = placeMeasure({ draft: m, x: cursor, scale, stringYs, staffBottom: staffTop + staffHeight })
      cursor += layout.width
      return layout
    })

    // A last line that is not stretched ends at its final barline.
    const right = shouldStretch ? METRICS.marginX + METRICS.clefWidth + innerWidth : cursor

    return { y, staffTop, staffBottom: staffTop + staffHeight, stringYs, left, right, measures: placed }
  })

  const height = METRICS.marginX * 2 + systems.length * systemHeight + Math.max(systems.length - 1, 0) * METRICS.systemGap
  return { width, height, systems }
}

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
}

function draftMeasures(track: Track): DraftMeasure[] {
  let startTick = 0
  let previous: TimeSignature | null = null

  return track.measures.map((measure, index) => {
    const beats = measure.voices[0]?.beats ?? []
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
    }
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

function placeMeasure({
  draft,
  x,
  scale,
  stringYs,
  staffBottom,
}: {
  draft: DraftMeasure
  x: number
  scale: number
  stringYs: number[]
  staffBottom: number
}): MeasureLayout {
  const { beats, slotWidths, timeSignature } = draft
  const slotsWidth = draft.slotsWidth * scale
  const width = draft.fixedWidth + slotsWidth
  const firstBeatX = x + METRICS.measurePadLeft + (timeSignature ? METRICS.timeSignatureWidth : 0)

  const stemTop = staffBottom + METRICS.stemOffset
  const stemBottom = stemTop + METRICS.stemLength

  const xs = beatXs(firstBeatX, slotWidths, scale)
  let tick = draft.startTick
  const placed: BeatLayout[] = beats.map((beat, i) => {
    const beatX = xs[i] ?? firstBeatX
    const previousX = xs[i - 1]
    const previousBeat = beats[i - 1]
    const ticks = durationTicks(beat.duration)

    const notes = beat.notes.map((note) => {
      const isTied = note.tied === true
      const hasLocalSource = previousBeat?.notes.some((n) => n.string === note.string) ?? false
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

  return {
    index: draft.index,
    x,
    width,
    beats: placed,
    beams: buildBeams({ beats: placed, stemBottom }),
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
