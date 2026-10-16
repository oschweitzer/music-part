import type { Instrument, NoteValue, Track } from './score-types'

/**
 * Engraving of standard notation on five-line staves, as plain geometry: where note heads,
 * accidentals, ledger lines, stems, flags, beams, rests, dots and ties go. `tab-layout.ts` places
 * the staves of a system and the x of every beat; this module fills one measure of one staff.
 * See `docs/notation.md`.
 */

/** All distances are in SVG user units, like `METRICS` of the tablature. */
export const STAFF_METRICS = {
  /** Distance between two lines of a staff; a step (line to space) is half of it. */
  lineGap: 8,
  /** Room above the top staff of a system, for ledger lines and stems, under the measure numbers. */
  above: 18,
  /** Room under the last staff of a system, before the tablature or the next system. */
  below: 40,
  /** From the bottom line of the treble staff to the top line of the bass staff of a grand staff. */
  between: 48,
  stemLength: 26,
  headHalfWidth: 4.6,
  beamGap: 5,
  /** Room for a clef and the start of a staff, before the key signature. */
  clefWidth: 40,
  /** Width taken by each sharp or flat of a key signature. */
  keyAccidentalWidth: 7,
} as const

/** A clef, with the octave transposition of instruments written an octave above how they sound. */
export type Clef = 'treble' | 'treble8vb' | 'bass' | 'bass8vb'

/** A key signature as a number of sharps (positive) or flats (negative); 0 is C major. */
export type KeySignature = { fifths: number }

export const C_MAJOR: KeySignature = { fifths: 0 }

export type Accidental = 'sharp' | 'flat' | 'natural'

/** A note of a beat to engrave: its sounding pitch, its tie, and the row selecting it (pitch or string). */
export type StaffNoteInput = { pitch: number; tied: boolean; row: number }

/** A beat of a measure to engrave on one staff. */
export type StaffBeatInput = {
  x: number
  startTick: number
  value: NoteValue
  dots: number
  /** The notes of the beat that belong on this staff; none is a rest on this staff. */
  notes: StaffNoteInput[]
}

export type StaffHead = {
  pitch: number
  /** The pitch on a pitched track, the string on a fretted one: what a selection points at. */
  row: number
  /** Steps above the bottom line: 0 is the bottom line, 1 the space above, 8 the top line. */
  position: number
  x: number
  y: number
  isFilled: boolean
  accidental: Accidental | null
  accidentalX: number
  /** An arc from the previous note of the same pitch into this one, when it is tied. */
  tie: { fromX: number; toX: number; y: number; isAbove: boolean } | null
}

export type StaffSegment = { x1: number; x2: number; y: number }

export type StaffBeat = {
  x: number
  heads: StaffHead[]
  ledgers: StaffSegment[]
  stem: { x: number; y1: number; y2: number } | null
  isStemUp: boolean
  /** Flags drawn at the end of the stem of an eighth or shorter note that is not beamed. */
  flags: number
  dots: { x: number; y: number }[]
  /** The rest drawn when the beat has no note on this staff, centred on `x`, `y`. */
  rest: { value: NoteValue; x: number; y: number } | null
}

export type StaffMeasure = { beats: StaffBeat[]; beams: StaffSegment[] }

/** A staff of a system: its clef and where its five lines are. */
export type StaffBlock = {
  clef: Clef
  top: number
  bottom: number
  lineYs: number[]
}

const BEAM_LEVELS: Partial<Record<NoteValue, number>> = { Eighth: 1, Sixteenth: 2, ThirtySecond: 3 }

/** Semitones above C of the natural steps C D E F G A B. */
const STEP_SEMITONES = [0, 2, 4, 5, 7, 9, 11] as const

/** Steps of the sharps of a key signature in order (F C G D A E B), and of the flats (B E A D G C F). */
const SHARP_ORDER = [3, 0, 4, 1, 5, 2, 6] as const
const FLAT_ORDER = [6, 2, 5, 1, 4, 0, 3] as const

/** Where the sharps and flats of a key signature sit on a treble staff (positions); a bass staff is two steps lower. */
const SHARP_POSITIONS = [8, 5, 9, 6, 3, 7, 4] as const
const FLAT_POSITIONS = [4, 7, 3, 6, 2, 5, 1] as const

/** The staves of an instrument: a grand staff for the piano and the organ, else one staff in the clef it is written in. */
export function clefsFor(instrument: Instrument): Clef[] {
  switch (instrument) {
    case 'Piano':
    case 'Organ':
      return ['treble', 'bass']
    case 'Synth':
    case 'Voice':
      return ['treble']
    case 'Bass':
      return ['bass8vb']
    case 'Violin':
      return ['treble']
    case 'Guitar':
    case 'ElectricGuitar':
    case 'Banjo':
      return ['treble8vb']
  }
}

/** The staves of a track: the clef it chose (a voice), else those of its instrument. */
export function clefsOf(track: Pick<Track, 'instrument' | 'clef'>): Clef[] {
  return track.clef ? [track.clef] : clefsFor(track.instrument)
}

/** The staff of a grand staff a pitch goes on: middle C and above on the treble staff. */
export function staffIndexFor({ pitch, staffCount }: { pitch: number; staffCount: number }): number {
  return staffCount > 1 && pitch < 60 ? 1 : 0
}

/** The diatonic step number (C-1 is 0, middle C is 35) of the bottom line of a clef, as written. */
function bottomLine(clef: Clef): number {
  switch (clef) {
    case 'treble':
      return 30 // E4
    case 'treble8vb':
      return 23 // E3 sounding, written E4
    case 'bass':
      return 18 // G2
    case 'bass8vb':
      return 11 // G1 sounding, written G2
  }
}

/** The alteration a key signature gives to each step C to B. */
function keyAlterations(key: KeySignature): number[] {
  const alters = [0, 0, 0, 0, 0, 0, 0]
  const count = Math.min(Math.abs(key.fifths), 7)
  const order = key.fifths >= 0 ? SHARP_ORDER : FLAT_ORDER
  for (let i = 0; i < count; i++) alters[order[i] as number] = key.fifths >= 0 ? 1 : -1
  return alters
}

/**
 * How a pitch is written in a key: its step (0 is C), octave (4 holds middle C) and alteration.
 * Notes of the key take its sharps or flats; other black keys are sharps in sharp keys and flats
 * in flat keys, and in C major C#, Eb, F#, G#, Bb, the spellings most often read.
 */
export function spell({ pitch, key = C_MAJOR }: { pitch: number; key?: KeySignature }): { step: number; octave: number; alter: number } {
  const octave = Math.floor(pitch / 12) - 1
  const semitone = ((pitch % 12) + 12) % 12
  const alters = keyAlterations(key)
  // A note of the scale first.
  for (let step = 0; step < 7; step++) {
    const sounding = (STEP_SEMITONES[step] as number) + (alters[step] as number)
    if (((sounding % 12) + 12) % 12 === semitone) {
      // Cb and B# change octave.
      const shift = sounding < 0 ? 1 : sounding >= 12 ? -1 : 0
      return { step, octave: octave + shift, alter: alters[step] as number }
    }
  }
  const natural = STEP_SEMITONES.indexOf(semitone as (typeof STEP_SEMITONES)[number])
  if (natural >= 0) return { step: natural, octave, alter: 0 }
  const useFlat = key.fifths < 0 || (key.fifths === 0 && (semitone === 3 || semitone === 10))
  if (useFlat) return { step: STEP_SEMITONES.indexOf((semitone + 1) as (typeof STEP_SEMITONES)[number]), octave, alter: -1 }
  return { step: STEP_SEMITONES.indexOf((semitone - 1) as (typeof STEP_SEMITONES)[number]), octave, alter: 1 }
}

/** Steps above the bottom line of a staff where a pitch is written. */
export function staffPosition({ pitch, clef, key = C_MAJOR }: { pitch: number; clef: Clef; key?: KeySignature }): number {
  const { step, octave } = spell({ pitch, key })
  return octave * 7 + step - bottomLine(clef)
}

/** The y of a staff position on a staff whose bottom line is at `bottom`. */
export function positionY({ position, bottom }: { position: number; bottom: number }): number {
  return bottom - (position * STAFF_METRICS.lineGap) / 2
}

/** The staff position nearest to a y, the inverse of `positionY`. */
export function positionAt({ y, bottom }: { y: number; bottom: number }): number {
  // `+ 0` turns the -0 that rounding gives just below the bottom line into 0.
  return Math.round(((bottom - y) * 2) / STAFF_METRICS.lineGap) + 0
}

/** The pitch written at a staff position without accidental: a note of the key. Clamped to MIDI. */
export function pitchAtPosition({ position, clef, key = C_MAJOR }: { position: number; clef: Clef; key?: KeySignature }): number {
  const diatonic = position + bottomLine(clef)
  const step = ((diatonic % 7) + 7) % 7
  const octave = Math.floor(diatonic / 7)
  const pitch = (octave + 1) * 12 + (STEP_SEMITONES[step] as number) + (keyAlterations(key)[step] as number)
  return Math.min(Math.max(pitch, 0), 127)
}

/** The next note of the key above (`direction` 1) or below (-1) a pitch: one step on the staff. */
export function stepPitch({ pitch, direction, key = C_MAJOR }: { pitch: number; direction: 1 | -1; key?: KeySignature }): number {
  const alters = keyAlterations(key)
  const isInKey = (p: number) => {
    const semitone = ((p % 12) + 12) % 12
    return alters.some((alter, step) => ((((STEP_SEMITONES[step] as number) + alter) % 12) + 12) % 12 === semitone)
  }
  let next = pitch + direction
  while (next > 0 && next < 127 && !isInKey(next)) next += direction
  return Math.min(Math.max(next, 0), 127)
}

/** The pitch named by a letter (A to G) nearest to `near`, in the key; the higher one when two are as near. */
export function pitchOfLetter({ letter, near, key = C_MAJOR }: { letter: string; near: number; key?: KeySignature }): number | null {
  const step = 'CDEFGAB'.indexOf(letter.toUpperCase())
  if (step < 0 || letter.length !== 1) return null
  const semitone = (STEP_SEMITONES[step] as number) + (keyAlterations(key)[step] as number)
  let best: number | null = null
  for (let octave = -1; octave <= 9; octave++) {
    const pitch = (octave + 1) * 12 + semitone
    if (pitch < 0 || pitch > 127) continue
    if (best === null || Math.abs(pitch - near) <= Math.abs(best - near)) best = pitch
  }
  return best
}

/** The five lines of a staff whose top line is at `top`. */
export function staffBlock({ clef, top }: { clef: Clef; top: number }): StaffBlock {
  const lineYs = Array.from({ length: 5 }, (_, i) => top + i * STAFF_METRICS.lineGap)
  return { clef, top, bottom: top + 4 * STAFF_METRICS.lineGap, lineYs }
}

/** The sharps or flats of a key signature at the start of a staff, from `x` on. */
export function keySignatureGlyphs({
  key,
  clef,
  x,
  bottom,
}: {
  key: KeySignature
  clef: Clef
  x: number
  bottom: number
}): { kind: 'sharp' | 'flat'; x: number; y: number }[] {
  const count = Math.min(Math.abs(key.fifths), 7)
  const isBass = clef === 'bass' || clef === 'bass8vb'
  const positions = key.fifths >= 0 ? SHARP_POSITIONS : FLAT_POSITIONS
  return Array.from({ length: count }, (_, i) => ({
    kind: key.fifths >= 0 ? ('sharp' as const) : ('flat' as const),
    x: x + i * STAFF_METRICS.keyAccidentalWidth,
    y: positionY({ position: (positions[i] as number) - (isBass ? 2 : 0), bottom }),
  }))
}

/** Width the key signature takes after the clef. */
export function keySignatureWidth(key: KeySignature): number {
  return Math.min(Math.abs(key.fifths), 7) * STAFF_METRICS.keyAccidentalWidth
}

/**
 * Engraves the beats of one measure on one staff. `carried` lists the pitches sounding at the
 * end of the previous measure, so that a note tied over the barline gets its arc (from the
 * start of the measure) and no accidental.
 */
export function engraveMeasure({
  beats,
  block,
  key = C_MAJOR,
  measureX,
  carried = [],
}: {
  beats: StaffBeatInput[]
  block: StaffBlock
  key?: KeySignature
  measureX: number
  carried?: number[]
}): StaffMeasure {
  const { headHalfWidth } = STAFF_METRICS
  const bottom = block.bottom
  // Alterations in force for each written step (with octave), from the key until an accidental.
  const alters = keyAlterations(key)
  const inForce = new Map<number, number>()
  const alterOf = (diatonic: number) => inForce.get(diatonic) ?? (alters[((diatonic % 7) + 7) % 7] as number)

  let previous: { x: number; pitches: number[] } = { x: measureX + 2, pitches: carried }

  const engraved: StaffBeat[] = beats.map((beat) => {
    if (beat.notes.length === 0) {
      const restY = beat.value === 'Whole' ? positionY({ position: 6, bottom }) : positionY({ position: 4, bottom })
      const restDots = Array.from({ length: beat.dots }, (_, i) => ({ x: beat.x + 8 + i * 5, y: positionY({ position: 5, bottom }) }))
      previous = { x: beat.x, pitches: [] }
      return { x: beat.x, heads: [], ledgers: [], stem: null, isStemUp: true, flags: 0, dots: restDots, rest: { value: beat.value, x: beat.x, y: restY } }
    }

    const notes = [...beat.notes].sort((a, b) => a.pitch - b.pitch)
    const written = notes.map((note) => {
      const { step, octave, alter } = spell({ pitch: note.pitch, key })
      const diatonic = octave * 7 + step
      return { note, diatonic, alter, position: diatonic - bottomLine(block.clef) }
    })
    const positions = written.map((w) => w.position)
    const isStemUp = 4 - Math.min(...positions) > Math.max(...positions) - 4
    const hasStem = beat.value !== 'Whole'
    const isFilled = beat.value !== 'Whole' && beat.value !== 'Half'

    // Heads a second apart go on either side of the stem.
    const displaced = new Set<number>()
    const order = isStemUp ? written.map((_, i) => i) : written.map((_, i) => written.length - 1 - i)
    for (let k = 1; k < order.length; k++) {
      const here = order[k] as number
      const before = order[k - 1] as number
      const gap = Math.abs((written[here]?.position ?? 0) - (written[before]?.position ?? 0))
      if (gap === 1 && !displaced.has(before)) displaced.add(here)
    }
    const side = isStemUp || !hasStem ? 1 : -1
    const headX = (i: number) => beat.x + (displaced.has(i) ? side * 2 * headHalfWidth : 0)

    // Accidentals, in columns from the heads leftwards so that close ones do not collide.
    const columns: number[][] = []
    const heads: StaffHead[] = written.map((w, i) => {
      const isTieShown = w.note.tied && previous.pitches.includes(w.note.pitch)
      let accidental: Accidental | null = null
      if (!isTieShown && alterOf(w.diatonic) !== w.alter) {
        accidental = w.alter > 0 ? 'sharp' : w.alter < 0 ? 'flat' : 'natural'
      }
      inForce.set(w.diatonic, w.alter)
      const y = positionY({ position: w.position, bottom })
      return {
        pitch: w.note.pitch,
        row: w.note.row,
        position: w.position,
        x: headX(i),
        y,
        isFilled,
        accidental,
        accidentalX: 0,
        tie: isTieShown
          ? { fromX: previous.x + headHalfWidth + 2, toX: headX(i) - headHalfWidth - 1, y: y + (isStemUp || !hasStem ? 5 : -5), isAbove: !(isStemUp || !hasStem) }
          : null,
      }
    })
    const leftmost = Math.min(...heads.map((h) => h.x))
    for (let i = heads.length - 1; i >= 0; i--) {
      const head = heads[i]
      if (!head?.accidental) continue
      let column = columns.findIndex((used) => used.every((p) => Math.abs(p - head.position) >= 6))
      if (column < 0) column = columns.push([]) - 1
      columns[column]?.push(head.position)
      head.accidentalX = leftmost - headHalfWidth - 6 - column * 8
    }

    const ledgers = ledgerLines({ heads, bottom })
    const top = heads[heads.length - 1]
    const low = heads[0]
    const middleY = positionY({ position: 4, bottom })
    const stemX = beat.x + (isStemUp ? headHalfWidth - 0.6 : -headHalfWidth + 0.6)
    const stem =
      hasStem && top && low
        ? isStemUp
          ? { x: stemX, y1: low.y, y2: Math.min(top.y - STAFF_METRICS.stemLength, middleY) }
          : { x: stemX, y1: top.y, y2: Math.max(low.y + STAFF_METRICS.stemLength, middleY) }
        : null

    const rightmost = Math.max(...heads.map((h) => h.x))
    const dots = heads.flatMap((head) =>
      Array.from({ length: beat.dots }, (_, i) => ({
        x: rightmost + headHalfWidth + 4 + i * 5,
        y: positionY({ position: head.position % 2 === 0 ? head.position + 1 : head.position, bottom }),
      })),
    )

    previous = { x: beat.x, pitches: notes.map((n) => n.pitch) }
    return { x: beat.x, heads, ledgers, stem, isStemUp, flags: hasStem ? (BEAM_LEVELS[beat.value] ?? 0) : 0, dots, rest: null }
  })

  const beams = beamGroups({ inputs: beats, beats: engraved })
  return { beats: engraved, beams }
}

/** Short lines through or next to the heads above and below the staff. */
function ledgerLines({ heads, bottom }: { heads: StaffHead[]; bottom: number }): StaffSegment[] {
  const byPosition = new Map<number, { x1: number; x2: number }>()
  const extend = (position: number, head: StaffHead) => {
    const span = { x1: head.x - STAFF_METRICS.headHalfWidth - 3, x2: head.x + STAFF_METRICS.headHalfWidth + 3 }
    const known = byPosition.get(position)
    byPosition.set(position, known ? { x1: Math.min(known.x1, span.x1), x2: Math.max(known.x2, span.x2) } : span)
  }
  for (const head of heads) {
    for (let p = -2; p >= head.position; p -= 2) extend(p, head)
    for (let p = 10; p <= head.position; p += 2) extend(p, head)
  }
  return [...byPosition.entries()].map(([position, { x1, x2 }]) => ({ x1, x2, y: positionY({ position, bottom }) }))
}

/**
 * Joins consecutive eighth notes or shorter that fall in the same quarter note with flat beams:
 * every stem of a group goes the same way and reaches the beam. A lone note keeps its flags.
 */
function beamGroups({ inputs, beats }: { inputs: StaffBeatInput[]; beats: StaffBeat[] }): StaffSegment[] {
  const groups: number[][] = []
  let group: number[] = []
  let quarter = -1
  inputs.forEach((input, i) => {
    const beat = beats[i]
    const isBeamable = beat !== undefined && beat.flags > 0 && beat.stem !== null
    const here = Math.floor(input.startTick / 32)
    if (!isBeamable || here !== quarter) {
      if (group.length > 0) groups.push(group)
      group = []
    }
    if (isBeamable) {
      group.push(i)
      quarter = here
    }
  })
  if (group.length > 0) groups.push(group)

  const segments: StaffSegment[] = []
  for (const indexes of groups) {
    if (indexes.length < 2) continue
    const members = indexes.map((i) => beats[i]).filter((b): b is StaffBeat => b !== undefined)
    const heads = members.flatMap((b) => b.heads)
    const positions = heads.map((h) => h.position)
    const isUp = 4 - Math.min(...positions) > Math.max(...positions) - 4
    const ys = heads.map((h) => h.y)
    const middle = members[0]?.stem ? (isUp ? Math.min(...members.map((b) => b.stem?.y2 ?? 0)) : Math.max(...members.map((b) => b.stem?.y2 ?? 0))) : 0
    const beamY = isUp
      ? Math.min(Math.min(...ys) - STAFF_METRICS.stemLength, middle)
      : Math.max(Math.max(...ys) + STAFF_METRICS.stemLength, middle)

    for (const beat of members) {
      const low = Math.max(...beat.heads.map((h) => h.y))
      const high = Math.min(...beat.heads.map((h) => h.y))
      const x = beat.x + (isUp ? STAFF_METRICS.headHalfWidth - 0.6 : -STAFF_METRICS.headHalfWidth + 0.6)
      beat.isStemUp = isUp
      beat.stem = { x, y1: isUp ? low : high, y2: beamY }
      for (const head of beat.heads) {
        if (head.tie) {
          head.tie = { ...head.tie, y: head.y + (isUp ? 5 : -5), isAbove: !isUp }
        }
      }
    }

    const first = members[0]
    const last = members[members.length - 1]
    if (!first || !last) continue
    for (let level = 1; level <= 3; level++) {
      const y = beamY + (isUp ? 1 : -1) * (level - 1) * STAFF_METRICS.beamGap
      let runStart: StaffBeat | null = null
      let runEnd: StaffBeat | null = null
      const closeRun = () => {
        if (!runStart?.stem || !runEnd?.stem) return
        const isStub = runStart === runEnd
        const x1 = isStub ? runStart.stem.x - (runStart === first ? 0 : 6) : runStart.stem.x
        const x2 = isStub ? runStart.stem.x + (runStart === first ? 6 : 0) : runEnd.stem.x
        segments.push({ x1, x2, y })
        runStart = null
        runEnd = null
      }
      for (const beat of members) {
        if (beat.flags >= level) {
          runStart ??= beat
          runEnd = beat
        } else {
          closeRun()
        }
      }
      closeRun()
    }
    for (const beat of members) beat.flags = 0
  }
  return segments
}
