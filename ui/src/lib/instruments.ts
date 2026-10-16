import type { Instrument, SoundId, Track, TrackClef } from './score-types'

/** Every instrument a track can have, in the order menus list them. */
export const INSTRUMENTS: Instrument[] = ['Guitar', 'ElectricGuitar', 'Bass', 'Violin', 'Banjo', 'Piano', 'Organ', 'Synth', 'Voice']

/**
 * How the notes of an instrument are stored, as `NoteKind` in `crates/score-model`: on strings and
 * frets (tablature, and a staff on request), or as pitches (a staff only).
 */
export type NoteKind = 'fretted' | 'pitched'

const PITCHED: readonly Instrument[] = ['Piano', 'Organ', 'Synth', 'Voice']

export function noteKindOf(instrument: Instrument): NoteKind {
  return PITCHED.includes(instrument) ? 'pitched' : 'fretted'
}

/** True for a track whose notes are pitches, without strings. */
export function isPitchedTrack(track: Pick<Track, 'instrument'>): boolean {
  return noteKindOf(track.instrument) === 'pitched'
}

const LABELS: Record<Instrument, string> = {
  Guitar: 'Acoustic guitar',
  ElectricGuitar: 'Electric guitar',
  Bass: 'Electric bass',
  Violin: 'Violin',
  Banjo: 'Banjo',
  Piano: 'Piano',
  Organ: 'Organ',
  Synth: 'Synthesizer',
  Voice: 'Voice',
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
  { id: 'piano/bright', name: 'Bright piano', program: 1, instrument: 'Piano' },
  { id: 'piano/electric', name: 'Electric piano', program: 4, instrument: 'Piano' },
  { id: 'piano/electric-fm', name: 'FM electric piano', program: 5, instrument: 'Piano' },
  { id: 'organ/drawbar', name: 'Drawbar', program: 16, instrument: 'Organ' },
  { id: 'organ/rock', name: 'Rock', program: 18, instrument: 'Organ' },
  { id: 'organ/church', name: 'Church', program: 19, instrument: 'Organ' },
  { id: 'organ/reed', name: 'Reed', program: 20, instrument: 'Organ' },
  { id: 'synth/square-lead', name: 'Square lead', program: 80, instrument: 'Synth' },
  { id: 'synth/saw-lead', name: 'Saw lead', program: 81, instrument: 'Synth' },
  { id: 'synth/warm-pad', name: 'Warm pad', program: 89, instrument: 'Synth' },
  { id: 'synth/choir-pad', name: 'Choir pad', program: 91, instrument: 'Synth' },
  { id: 'voice/aahs', name: 'Choir aahs', program: 52, instrument: 'Voice' },
  { id: 'voice/oohs', name: 'Voice oohs', program: 53, instrument: 'Voice' },
  { id: 'voice/synth', name: 'Synth voice', program: 54, instrument: 'Voice' },
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

/** True for an instrument whose track chooses its clef (`Track.clef`): a voice. */
export function hasClefChoice(instrument: Instrument): boolean {
  return instrument === 'Voice'
}

/** The clefs a voice can be written in, with the range each one suits. */
export const CLEF_CHOICES: { clef: TrackClef; label: string }[] = [
  { clef: 'treble', label: 'Treble (soprano, alto)' },
  { clef: 'treble8vb', label: 'Treble 8vb (tenor)' },
  { clef: 'bass', label: 'Bass (baritone, bass)' },
]
