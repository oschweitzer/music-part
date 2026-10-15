import type { Score } from './score-types'
import { durationTicks } from './tab-layout'

export type BeatPosition = { measure: number; beat: number }

/**
 * The beat and string being edited, in the track shown on screen. With an `anchor`, the beats
 * from the anchor to this one are selected together (for copying or clearing), all their strings.
 * With an `anchorString` too, only the notes on the strings between it and this one are: a
 * rectangle over the tablature, as dragged with the mouse.
 */
export type Selection = { measure: number; beat: number; string: number; anchor?: BeatPosition; anchorString?: number }

export type Direction = 'left' | 'right' | 'up' | 'down'

/** Two digits typed within this time make one fret number ("1" then "2" is fret 12). */
export const FRET_TYPING_WINDOW_MS = 900

const MAX_FRET = 24

export const FIRST_NOTE: Selection = { measure: 0, beat: 0, string: 1 }

function beatCount(score: Score, track: number, measure: number): number {
  return score.tracks[track]?.measures[measure]?.voices[0]?.beats.length ?? 0
}

/** Moves along the beats (crossing barlines) or between strings. Stays put at the edges. */
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
  const { anchor: _anchor, anchorString: _anchorString, ...current } = clampSelection({ score, track, selection })
  const { measure, beat, string } = current
  const measures = score.tracks[track]?.measures.length ?? 0
  const strings = score.tracks[track]?.tuning.length ?? 1

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
      return { ...current, string: Math.max(1, string - 1) }
    case 'down':
      return { ...current, string: Math.min(strings, string + 1) }
  }
}

/**
 * Moves the selection to the previous (`step` -1) or next (`step` 1) track, at the same moment: the
 * same measure, on the beat of that track that sounds when the selected beat starts. The string is
 * kept when the track has it. A range stays behind, since it belongs to its track. At the first or
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

  const { measure, beat, string } = clampSelection({ score, track, selection })
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
  return { track: target, selection: clampSelection({ score, track: target, selection: { measure, beat: found, string } }) }
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
  const strings = score.tracks[track]?.tuning.length ?? 1
  const wanted = selection ?? FIRST_NOTE

  const measure = Math.min(Math.max(wanted.measure, 0), Math.max(measures - 1, 0))
  const beat = Math.min(Math.max(wanted.beat, 0), Math.max(beatCount(score, track, measure) - 1, 0))
  const string = Math.min(Math.max(wanted.string, 1), strings)
  const result: Selection = { measure, beat, string }
  if (wanted.anchor && wanted.anchorString !== undefined) {
    result.anchorString = Math.min(Math.max(wanted.anchorString, 1), strings)
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
  return { ...moved, anchor, ...(current.anchorString === undefined ? {} : { anchorString: current.anchorString }) }
}

const compare = (a: BeatPosition, b: BeatPosition) => a.measure - b.measure || a.beat - b.beat

/**
 * The first and last selected beat, or null for a lone spot. One beat is a range when the selection
 * covers several of its strings.
 */
export function selectedRange(selection: Selection | null): { from: BeatPosition; to: BeatPosition } | null {
  if (!selection?.anchor) return null
  const head = { measure: selection.measure, beat: selection.beat }
  const coversStrings = selection.anchorString !== undefined && selection.anchorString !== selection.string
  if (compare(head, selection.anchor) === 0 && !coversStrings) return null
  return compare(selection.anchor, head) < 0 ? { from: selection.anchor, to: head } : { from: head, to: selection.anchor }
}

/** The strings a range is limited to, or null when it takes every string of its beats. */
export function selectedStrings(selection: Selection | null): { first: number; last: number } | null {
  if (!selection?.anchor || selection.anchorString === undefined) return null
  return { first: Math.min(selection.anchorString, selection.string), last: Math.max(selection.anchorString, selection.string) }
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
