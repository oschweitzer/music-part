import { DEFAULT_PIECE, FIRST_PIECE, LAST_PIECE, stepPiece } from './drums'
import { isPercussionTrack, isPitchedTrack, noteKindOf, type NoteKind } from './instruments'
import type { Score } from './score-types'
import { stepPitch } from './staff-layout'
import { durationTicks } from './tab-layout'

export type BeatPosition = { measure: number; beat: number }

/**
 * The beat and row being edited, in the track of the editor. The row is a string (1 is the
 * highest) on a fretted track, a MIDI pitch on a pitched track, the key of a piece on a drum track
 * (`drums.ts`): what tells the notes of a beat
 * apart (`noteRow`). With an `anchor`, the beats from the anchor to this one are selected together
 * (for copying or clearing), all their notes. With an `anchorRow` too, only the notes on the
 * strings, or at the pitches, between it and this one are: a rectangle as dragged with the mouse.
 */
export type Selection = { measure: number; beat: number; row: number; anchor?: BeatPosition; anchorRow?: number }

export type Direction = 'left' | 'right' | 'up' | 'down'

/** Two digits typed within this time make one fret number ("1" then "2" is fret 12). */
export const FRET_TYPING_WINDOW_MS = 900

const MAX_FRET = 24

export const FIRST_NOTE: Selection = { measure: 0, beat: 0, row: 1 }

/** Where a pitched track is selected when nothing else says: middle C. */
export const DEFAULT_PITCH = 60

function isPitched(score: Score, track: number): boolean {
  const target = score.tracks[track]
  return target !== undefined && isPitchedTrack(target)
}

function isPercussion(score: Score, track: number): boolean {
  const target = score.tracks[track]
  return target !== undefined && isPercussionTrack(target)
}

/** What the rows of a track are: strings, pitches or drum pieces. */
function rowsOf(score: Score, track: number): NoteKind | null {
  const target = score.tracks[track]
  return target ? noteKindOf(target.instrument) : null
}

/** The first beat of a track, on its first string, at middle C, or on the snare. */
export function firstSelection({ score, track }: { score: Score; track: number }): Selection {
  const row = isPercussion(score, track) ? DEFAULT_PIECE : isPitched(score, track) ? DEFAULT_PITCH : 1
  return { measure: 0, beat: 0, row }
}

/** The lowest and highest row of a track: its strings, every MIDI pitch, or every drum piece. */
function rowRange(score: Score, track: number): { low: number; high: number } {
  if (isPercussion(score, track)) return { low: FIRST_PIECE, high: LAST_PIECE }
  if (isPitched(score, track)) return { low: 0, high: 127 }
  return { low: 1, high: Math.max(score.tracks[track]?.tuning.length ?? 1, 1) }
}

function beatCount(score: Score, track: number, measure: number): number {
  return score.tracks[track]?.measures[measure]?.voices[0]?.beats.length ?? 0
}

/**
 * Moves along the beats (crossing barlines), or between strings, or on a pitched track one step of
 * the staff (a line or a space) up or down, or on a drum track to the next piece up or down the
 * staff. Stays put at the edges.
 */
export function moveSelection({
  score,
  track,
  selection,
  direction,
}: {
  score: Score
  track: number
  selection: Selection
  direction: Direction
}): Selection {
  const { anchor: _anchor, anchorRow: _anchorRow, ...current } = clampSelection({ score, track, selection })
  const { measure, beat, row } = current
  const measures = score.tracks[track]?.measures.length ?? 0
  const { low, high } = rowRange(score, track)
  const pitched = isPitched(score, track)
  const percussion = isPercussion(score, track)

  switch (direction) {
    case 'left':
      if (beat > 0) return { ...current, beat: beat - 1 }
      if (measure > 0) return { ...current, measure: measure - 1, beat: beatCount(score, track, measure - 1) - 1 }
      return current
    case 'right':
      if (beat < beatCount(score, track, measure) - 1) return { ...current, beat: beat + 1 }
      if (measure < measures - 1) return { ...current, measure: measure + 1, beat: 0 }
      return current
    case 'up':
      if (percussion) return { ...current, row: stepPiece({ piece: row, direction: 1 }) ?? row }
      return { ...current, row: pitched ? stepPitch({ pitch: row, direction: 1 }) : Math.max(low, row - 1) }
    case 'down':
      if (percussion) return { ...current, row: stepPiece({ piece: row, direction: -1 }) ?? row }
      return { ...current, row: pitched ? stepPitch({ pitch: row, direction: -1 }) : Math.min(high, row + 1) }
  }
}

/**
 * Moves the selection to the previous (`step` -1) or next (`step` 1) track, at the same moment: the
 * same measure, on the beat of that track that sounds when the selected beat starts. The row is
 * kept when the track has it and its rows mean the same (strings, pitches or drum pieces); else it
 * is the first string, middle C or the snare. A range stays behind, since it belongs to its track. At the first or
 * last track nothing moves.
 */
export function moveToTrack({
  score,
  track,
  selection,
  step,
}: {
  score: Score
  track: number
  selection: Selection
  step: -1 | 1
}): { track: number; selection: Selection } {
  const target = track + step
  if (target < 0 || target >= score.tracks.length) return { track, selection }

  const { measure, beat, row: current } = clampSelection({ score, track, selection })
  const row = rowsOf(score, track) === rowsOf(score, target) ? current : firstSelection({ score, track: target }).row
  const startOf = (t: number, upTo: number) =>
    (score.tracks[t]?.measures[measure]?.voices[0]?.beats ?? [])
      .slice(0, upTo)
      .reduce((sum, b) => sum + durationTicks(b.duration), 0)
  const tick = startOf(track, beat)

  const beats = score.tracks[target]?.measures[measure]?.voices[0]?.beats ?? []
  let start = 0
  let found = 0
  beats.forEach((b, i) => {
    if (start <= tick) found = i
    start += durationTicks(b.duration)
  })
  return { track: target, selection: clampSelection({ score, track: target, selection: { measure, beat: found, row } }) }
}

/** Brings a selection back onto something that exists, after the score changed under it. */
export function clampSelection({
  score,
  track,
  selection,
}: {
  score: Score
  track: number
  selection: Selection | null
}): Selection {
  const measures = score.tracks[track]?.measures.length ?? 0
  const { low, high } = rowRange(score, track)
  const wanted = selection ?? firstSelection({ score, track })

  const measure = Math.min(Math.max(wanted.measure, 0), Math.max(measures - 1, 0))
  const beat = Math.min(Math.max(wanted.beat, 0), Math.max(beatCount(score, track, measure) - 1, 0))
  const row = Math.min(Math.max(wanted.row, low), high)
  const result: Selection = { measure, beat, row }
  if (wanted.anchor && wanted.anchorRow !== undefined) {
    result.anchorRow = Math.min(Math.max(wanted.anchorRow, low), high)
  }
  if (wanted.anchor) {
    const anchorMeasure = Math.min(Math.max(wanted.anchor.measure, 0), Math.max(measures - 1, 0))
    const anchorBeat = Math.min(Math.max(wanted.anchor.beat, 0), Math.max(beatCount(score, track, anchorMeasure) - 1, 0))
    result.anchor = { measure: anchorMeasure, beat: anchorBeat }
  }
  return result
}

/** Selects more beats to the left or right of the anchor, which stays where the selection began. */
export function extendSelection({
  score,
  track,
  selection,
  direction,
}: {
  score: Score
  track: number
  selection: Selection
  direction: 'left' | 'right'
}): Selection {
  const current = clampSelection({ score, track, selection })
  const anchor = current.anchor ?? { measure: current.measure, beat: current.beat }
  const moved = moveSelection({ score, track, selection: current, direction })
  return { ...moved, anchor, ...(current.anchorRow === undefined ? {} : { anchorRow: current.anchorRow }) }
}

const compare = (a: BeatPosition, b: BeatPosition) => a.measure - b.measure || a.beat - b.beat

/**
 * The first and last selected beat, or null for a lone spot. One beat is a range when the selection
 * covers several of its rows.
 */
export function selectedRange(selection: Selection | null): { from: BeatPosition; to: BeatPosition } | null {
  if (!selection?.anchor) return null
  const head = { measure: selection.measure, beat: selection.beat }
  const coversRows = selection.anchorRow !== undefined && selection.anchorRow !== selection.row
  if (compare(head, selection.anchor) === 0 && !coversRows) return null
  return compare(selection.anchor, head) < 0 ? { from: selection.anchor, to: head } : { from: head, to: selection.anchor }
}

/**
 * The rows a range is limited to (strings, or pitches), lowest number first, or null when it takes
 * every note of its beats.
 */
export function selectedRows(selection: Selection | null): { first: number; last: number } | null {
  if (!selection?.anchor || selection.anchorRow === undefined) return null
  return { first: Math.min(selection.anchorRow, selection.row), last: Math.max(selection.anchorRow, selection.row) }
}

/** True for the beat of a lone selection, or any beat inside a selected range. */
export function isBeatSelected({ selection, measure, beat }: { selection: Selection | null; measure: number; beat: number }): boolean {
  if (!selection) return false
  const range = selectedRange(selection)
  if (!range) return selection.measure === measure && selection.beat === beat
  const at = { measure, beat }
  return compare(range.from, at) <= 0 && compare(at, range.to) <= 0
}

/**
 * The fret that results from typing a digit. Typing a second digit right after the first extends
 * it when the result is still a playable fret; otherwise the digit starts a new number.
 */
export function nextFret({
  previous,
  digit,
  now,
}: {
  previous: { fret: number; at: number } | null
  digit: number
  now: number
}): number {
  if (previous && now - previous.at <= FRET_TYPING_WINDOW_MS) {
    const combined = previous.fret * 10 + digit
    if (previous.fret > 0 && combined <= MAX_FRET) return combined
  }
  return digit
}
