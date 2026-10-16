/**
 * The pieces of a drum kit, by General MIDI percussion key, with the name shown to the user and
 * where each is written on a percussion staff: the line or space, as the note a treble staff has
 * there, and the shape of its head. Mirrors `PIECES` in `crates/score-model/src/percussion.rs`; a
 * Rust test checks that every piece is listed here, in this exact form. See `docs/drums.md`.
 */

export type NoteHead = 'normal' | 'cross' | 'circleCross' | 'diamond' | 'triangle'

export type DrumPiece = { key: number; name: string; step: string; octave: number; head: NoteHead }

export const FIRST_PIECE = 35
export const LAST_PIECE = 81

/** The snare: where a drum track is selected when nothing else says. */
export const DEFAULT_PIECE = 38

export const DRUM_PIECES: DrumPiece[] = [
  { key: 35, name: 'Acoustic kick', step: 'E', octave: 4, head: 'normal' },
  { key: 36, name: 'Kick', step: 'F', octave: 4, head: 'normal' },
  { key: 37, name: 'Side stick', step: 'C', octave: 5, head: 'cross' },
  { key: 38, name: 'Snare', step: 'C', octave: 5, head: 'normal' },
  { key: 39, name: 'Hand clap', step: 'B', octave: 4, head: 'triangle' },
  { key: 40, name: 'Electric snare', step: 'C', octave: 5, head: 'diamond' },
  { key: 41, name: 'Low floor tom', step: 'G', octave: 4, head: 'normal' },
  { key: 42, name: 'Closed hi-hat', step: 'G', octave: 5, head: 'cross' },
  { key: 43, name: 'High floor tom', step: 'A', octave: 4, head: 'normal' },
  { key: 44, name: 'Pedal hi-hat', step: 'D', octave: 4, head: 'cross' },
  { key: 45, name: 'Low tom', step: 'B', octave: 4, head: 'normal' },
  { key: 46, name: 'Open hi-hat', step: 'G', octave: 5, head: 'circleCross' },
  { key: 47, name: 'Low-mid tom', step: 'D', octave: 5, head: 'normal' },
  { key: 48, name: 'High-mid tom', step: 'E', octave: 5, head: 'normal' },
  { key: 49, name: 'Crash', step: 'A', octave: 5, head: 'cross' },
  { key: 50, name: 'High tom', step: 'F', octave: 5, head: 'normal' },
  { key: 51, name: 'Ride', step: 'F', octave: 5, head: 'cross' },
  { key: 52, name: 'China', step: 'C', octave: 6, head: 'cross' },
  { key: 53, name: 'Ride bell', step: 'F', octave: 5, head: 'diamond' },
  { key: 54, name: 'Tambourine', step: 'D', octave: 5, head: 'triangle' },
  { key: 55, name: 'Splash', step: 'A', octave: 5, head: 'diamond' },
  { key: 56, name: 'Cowbell', step: 'E', octave: 5, head: 'triangle' },
  { key: 57, name: 'Crash 2', step: 'B', octave: 5, head: 'cross' },
  { key: 58, name: 'Vibraslap', step: 'F', octave: 4, head: 'triangle' },
  { key: 59, name: 'Ride 2', step: 'E', octave: 5, head: 'cross' },
  { key: 60, name: 'High bongo', step: 'E', octave: 5, head: 'diamond' },
  { key: 61, name: 'Low bongo', step: 'D', octave: 5, head: 'diamond' },
  { key: 62, name: 'Muted high conga', step: 'B', octave: 4, head: 'circleCross' },
  { key: 63, name: 'Open high conga', step: 'B', octave: 4, head: 'diamond' },
  { key: 64, name: 'Low conga', step: 'A', octave: 4, head: 'diamond' },
  { key: 65, name: 'High timbale', step: 'G', octave: 4, head: 'diamond' },
  { key: 66, name: 'Low timbale', step: 'F', octave: 4, head: 'diamond' },
  { key: 67, name: 'High agogo', step: 'A', octave: 5, head: 'triangle' },
  { key: 68, name: 'Low agogo', step: 'G', octave: 5, head: 'triangle' },
  { key: 69, name: 'Cabasa', step: 'D', octave: 5, head: 'cross' },
  { key: 70, name: 'Maracas', step: 'B', octave: 4, head: 'cross' },
  { key: 71, name: 'Short whistle', step: 'B', octave: 5, head: 'triangle' },
  { key: 72, name: 'Long whistle', step: 'C', octave: 6, head: 'triangle' },
  { key: 73, name: 'Short guiro', step: 'D', octave: 5, head: 'circleCross' },
  { key: 74, name: 'Long guiro', step: 'E', octave: 5, head: 'circleCross' },
  { key: 75, name: 'Claves', step: 'G', octave: 4, head: 'triangle' },
  { key: 76, name: 'High wood block', step: 'A', octave: 4, head: 'triangle' },
  { key: 77, name: 'Low wood block', step: 'E', octave: 4, head: 'triangle' },
  { key: 78, name: 'Muted cuica', step: 'F', octave: 4, head: 'circleCross' },
  { key: 79, name: 'Open cuica', step: 'E', octave: 4, head: 'circleCross' },
  { key: 80, name: 'Muted triangle', step: 'B', octave: 5, head: 'diamond' },
  { key: 81, name: 'Open triangle', step: 'C', octave: 6, head: 'diamond' },
]

/**
 * The pieces of the palette, in the order of the digit keys that write them (1 to 9, then 0): the
 * usual drum kit. Every other piece is in the list after them.
 */
export const PALETTE: { key: number; label: string; digit: number }[] = [
  { key: 36, label: 'Kick', digit: 1 },
  { key: 38, label: 'Snare', digit: 2 },
  { key: 42, label: 'Hi-hat', digit: 3 },
  { key: 46, label: 'Open hat', digit: 4 },
  { key: 44, label: 'Pedal hat', digit: 5 },
  { key: 49, label: 'Crash', digit: 6 },
  { key: 51, label: 'Ride', digit: 7 },
  { key: 50, label: 'High tom', digit: 8 },
  { key: 47, label: 'Mid tom', digit: 9 },
  { key: 41, label: 'Floor tom', digit: 0 },
]

/** Order of the heads when several pieces share a line: the drum first, then the cymbal. */
const HEAD_ORDER: NoteHead[] = ['normal', 'cross', 'circleCross', 'diamond', 'triangle']

const STEPS = 'CDEFGAB'

/** The diatonic step of the bottom line of a percussion staff (E4), as `bottomLine` of a treble staff. */
const BOTTOM_LINE = 30

export function drumPiece(key: number): DrumPiece | null {
  return DRUM_PIECES[key - FIRST_PIECE] ?? null
}

/** The name of a piece, such as "Closed hi-hat", or the key when it is no piece. */
export function pieceName(key: number): string {
  return drumPiece(key)?.name ?? `Key ${key}`
}

/** The piece written by a digit key, or null. */
export function pieceOfDigit(digit: number): number | null {
  return PALETTE.find((p) => p.digit === digit)?.key ?? null
}

/** Steps above the bottom line of the percussion staff where a piece is written (0 is the bottom line). */
export function piecePosition(key: number): number | null {
  const piece = drumPiece(key)
  if (!piece) return null
  return piece.octave * 7 + STEPS.indexOf(piece.step) - BOTTOM_LINE
}

/** True for the pieces played with a foot: the kicks and the pedal hi-hat. */
export function isPlayedByFoot(key: number): boolean {
  return key === 35 || key === 36 || key === 44
}

/** Every piece from the bottom of the staff to the top; on one line the drum before the cymbal. */
export const STAFF_ORDER: number[] = [...DRUM_PIECES]
  .sort(
    (a, b) =>
      (piecePosition(a.key) ?? 0) - (piecePosition(b.key) ?? 0) ||
      HEAD_ORDER.indexOf(a.head) - HEAD_ORDER.indexOf(b.head) ||
      a.key - b.key,
  )
  .map((p) => p.key)

/** Every line and space that has a piece, from the bottom up. */
const POSITIONS: number[] = [...new Set(DRUM_PIECES.map((p) => piecePosition(p.key) ?? 0))].sort((a, b) => a - b)

/**
 * The piece one line or space above (`direction` 1) or below (-1) a piece on the staff: the drum
 * written there, else its cymbal, else the other pieces of that line, skipping those in `taken`
 * (and lines where every piece is taken). Null at the top or bottom. The other pieces of a line
 * are a key or a click of the palette away, so the arrows move as they do on any staff.
 */
export function stepPiece({ piece, direction, taken = [] }: { piece: number; direction: 1 | -1; taken?: number[] }): number | null {
  const from = piecePosition(piece)
  if (from === null) return null
  const ahead = direction > 0 ? POSITIONS.filter((p) => p > from) : POSITIONS.filter((p) => p < from).reverse()
  for (const position of ahead) {
    const free = STAFF_ORDER.filter((key) => piecePosition(key) === position && !taken.includes(key))
    if (free[0] !== undefined) return free[0]
  }
  return null
}

/** The piece a click at a staff position writes: the drum written there, else the cymbal, else null. */
export function pieceAtPosition(position: number): number | null {
  const here = DRUM_PIECES.filter((p) => piecePosition(p.key) === position)
  for (const head of HEAD_ORDER) {
    const found = here.find((p) => p.head === head)
    if (found) return found.key
  }
  return null
}
