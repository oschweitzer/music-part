import type { Instrument, SoundId, Track } from './score-types'

/** Every instrument a track can have, in the order menus list them. */
export const INSTRUMENTS: Instrument[] = ['Guitar', 'ElectricGuitar', 'Bass', 'Violin', 'Banjo', 'Piano', 'Drums']

/**
 * How the notes of an instrument are stored, as `NoteKind` in `crates/score-model`: on strings and
 * frets (tablature, and a staff on request), as pitches (a staff only), or as the pieces of a drum
 * kit (a percussion staff).
 */
export type NoteKind = 'fretted' | 'pitched' | 'percussion'

const PITCHED: readonly Instrument[] = ['Piano']

const PERCUSSION: readonly Instrument[] = ['Drums']

export function noteKindOf(instrument: Instrument): NoteKind {
  if (PERCUSSION.includes(instrument)) return 'percussion'
  return PITCHED.includes(instrument) ? 'pitched' : 'fretted'
}

/** True for a track whose notes are pitches, without strings. */
export function isPitchedTrack(track: Pick<Track, 'instrument'>): boolean {
  return noteKindOf(track.instrument) === 'pitched'
}

/** True for a drum track: its notes are pieces of a kit (`drums.ts`). */
export function isPercussionTrack(track: Pick<Track, 'instrument'>): boolean {
  return noteKindOf(track.instrument) === 'percussion'
}

/** True for a track of a string instrument: it has a tuning and a tablature. */
export function hasStrings(track: Pick<Track, 'instrument'>): boolean {
  return noteKindOf(track.instrument) === 'fretted'
}

const LABELS: Record<Instrument, string> = {
  Guitar: 'Acoustic guitar',
  ElectricGuitar: 'Electric guitar',
  Bass: 'Electric bass',
  Violin: 'Violin',
  Banjo: 'Banjo',
  Piano: 'Piano',
  Drums: 'Drums',
}

/** The name shown to the user: the stored value is a code of the file format. */
export function instrumentLabel(instrument: Instrument): string {
  return LABELS[instrument]
}

/** A sound of an instrument, played with a General MIDI program. */
export type SoundInfo = { id: SoundId; name: string; program: number; instrument: Instrument }

/**
 * Mirrors the sounds table of `crates/score-model/src/sound.rs`, the default sound of each
 * instrument first. A Rust test checks that every sound is listed here, in this exact form.
 */
export const SOUNDS: SoundInfo[] = [
  { id: 'guitar/nylon', name: 'Nylon strings', program: 24, instrument: 'Guitar' },
  { id: 'guitar/steel', name: 'Steel strings', program: 25, instrument: 'Guitar' },
  { id: 'electric-guitar/clean', name: 'Clean', program: 27, instrument: 'ElectricGuitar' },
  { id: 'electric-guitar/muted', name: 'Muted', program: 28, instrument: 'ElectricGuitar' },
  { id: 'electric-guitar/overdrive', name: 'Overdrive', program: 29, instrument: 'ElectricGuitar' },
  { id: 'electric-guitar/distortion', name: 'Distortion', program: 30, instrument: 'ElectricGuitar' },
  { id: 'bass/finger', name: 'Finger', program: 33, instrument: 'Bass' },
  { id: 'bass/pick', name: 'Pick', program: 34, instrument: 'Bass' },
  { id: 'bass/slap', name: 'Slap', program: 36, instrument: 'Bass' },
  { id: 'violin/standard', name: 'Standard', program: 40, instrument: 'Violin' },
  { id: 'banjo/standard', name: 'Standard', program: 105, instrument: 'Banjo' },
  { id: 'piano/grand', name: 'Grand piano', program: 0, instrument: 'Piano' },
  // Drum kits: the program is the kit on the percussion channel.
  { id: 'drums/standard', name: 'Standard kit', program: 0, instrument: 'Drums' },
  { id: 'drums/room', name: 'Room kit', program: 8, instrument: 'Drums' },
  { id: 'drums/power', name: 'Power kit', program: 16, instrument: 'Drums' },
  { id: 'drums/electronic', name: 'Electronic kit', program: 24, instrument: 'Drums' },
  { id: 'drums/jazz', name: 'Jazz kit', program: 32, instrument: 'Drums' },
  { id: 'drums/brush', name: 'Brush kit', program: 40, instrument: 'Drums' },
]

/** The sounds of an instrument, its default sound first. */
export function soundsOf(instrument: Instrument): SoundInfo[] {
  return SOUNDS.filter((sound) => sound.instrument === instrument)
}

/** The name of a sound with its instrument, such as "Electric guitar: Distortion". */
export function soundLabel(id: SoundId): string {
  const sound = SOUNDS.find((s) => s.id === id)
  return sound ? `${instrumentLabel(sound.instrument)}: ${sound.name}` : id
}
