import type { FrettedNote, Instrument, Note, PercussionNote, PitchedNote, Track } from './score-types'

/** True for a note stored as a pitch (a piano note). */
export function isPitchedNote(note: Note): note is PitchedNote {
  return 'pitch' in note
}

export function isFrettedNote(note: Note): note is FrettedNote {
  return 'string' in note
}

/** True for a hit on a drum piece. */
export function isPercussionNote(note: Note): note is PercussionNote {
  return 'piece' in note
}

/**
 * What tells a note apart from the others of its beat, as `Note::key` in the model: its string on
 * a fretted track, its pitch on a pitched one, its piece on a drum track. A selection points at a
 * note by this row.
 */
export function noteRow(note: Note): number {
  if (isPercussionNote(note)) return note.piece
  return isPitchedNote(note) ? note.pitch : note.string
}

/** The fret where a string starts: 5 for the short fifth string of a five-string banjo, else 0. */
function firstFret({ instrument, string, stringCount }: { instrument: Instrument; string: number; stringCount: number }): number {
  return instrument === 'Banjo' && string === 5 && stringCount === 5 ? 5 : 0
}

/** MIDI pitch of a fret of a string, as `fret_pitch` in the model, or null when there is none. */
export function fretPitch({ track, string, fret }: { track: Pick<Track, 'instrument' | 'tuning'>; string: number; fret: number }): number | null {
  const open = track.tuning[string - 1]
  if (open === undefined) return null
  if (fret === 0) return open
  const above = fret - firstFret({ instrument: track.instrument, string, stringCount: track.tuning.length })
  if (above <= 0) return null
  const pitch = open + above
  return pitch <= 127 ? pitch : null
}

/**
 * The pitch a note sounds at on its track, or null when it has none. A drum note gives the key of
 * its piece, as `Track::pitch` in the model.
 */
export function notePitch({ track, note }: { track: Pick<Track, 'instrument' | 'tuning'>; note: Note }): number | null {
  if (isPercussionNote(note)) return note.piece >= 35 && note.piece <= 81 ? note.piece : null
  if (isPitchedNote(note)) return note.pitch
  return fretPitch({ track, string: note.string, fret: note.fret })
}
