import type { Score } from './score-types'

/** The beat and string being edited, in the track shown on screen. */
export type Selection = { measure: number; beat: number; string: number }

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
  const current = clampSelection({ score, track, selection })
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
  return { measure, beat, string }
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
