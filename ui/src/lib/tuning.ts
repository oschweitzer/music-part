const NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B']
const SEMITONES: Record<string, number> = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 }

/** MIDI pitches to note names, "E4 B3 G3 D3 A2 E2". Middle C (60) is C4. */
export function formatTuning(pitches: number[]): string {
  return pitches.map((pitch) => `${NAMES[pitch % 12]}${Math.floor(pitch / 12) - 1}`).join(' ')
}

/**
 * Note names (letter, optional # or b, octave) separated by spaces or commas, to MIDI pitches.
 * Returns null if anything is not a note or falls outside the MIDI range.
 */
export function parseTuning(text: string): number[] | null {
  const words = text.split(/[\s,]+/).filter((word) => word !== '')
  if (words.length === 0) return null

  const pitches: number[] = []
  for (const word of words) {
    const match = /^([A-Ga-g])([#b]?)(-?\d)$/.exec(word)
    if (!match) return null
    const [, letter, accidental, octave] = match
    const accidentalShift = accidental === '#' ? 1 : accidental === 'b' ? -1 : 0
    const pitch = (Number(octave) + 1) * 12 + (SEMITONES[letter!.toUpperCase()] ?? 0) + accidentalShift
    if (pitch < 0 || pitch > 127) return null
    pitches.push(pitch)
  }
  return pitches
}
