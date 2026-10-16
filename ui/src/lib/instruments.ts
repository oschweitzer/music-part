import type { BuiltInSoundId, Instrument, SoundId, Track, TrackClef } from './score-types'

/** Every instrument a track can have, in the order menus list them. */
export const INSTRUMENTS: Instrument[] = ['Guitar', 'ElectricGuitar', 'Bass', 'Violin', 'Banjo', 'Piano', 'Organ', 'Synth', 'Voice', 'Drums']

/**
 * How the notes of an instrument are stored, as `NoteKind` in `crates/score-model`: on strings and
 * frets (tablature, and a staff on request), as pitches (a staff only), or as the pieces of a drum
 * kit (a percussion staff).
 */
export type NoteKind = 'fretted' | 'pitched' | 'percussion'

const PITCHED: readonly Instrument[] = ['Piano', 'Organ', 'Synth', 'Voice']

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
  Organ: 'Organ',
  Synth: 'Synthesizer',
  Voice: 'Voice',
  Drums: 'Drums',
}

/** The name shown to the user: the stored value is a code of the file format. */
export function instrumentLabel(instrument: Instrument): string {
  return LABELS[instrument]
}

/** A sound of an instrument, played with a General MIDI program. */
export type SoundInfo = { id: BuiltInSoundId; name: string; program: number; instrument: Instrument }

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

/** True for a sound of the built-in table, false for one a sound pack declares. */
export function isBuiltInSound(id: SoundId): boolean {
  return SOUNDS.some((s) => s.id === id)
}

/** The first part of the sound ids of each instrument, as `Instrument::sound_prefix` in `crates/score-model`. */
const SOUND_PREFIXES: Record<Instrument, string> = {
  Guitar: 'guitar',
  ElectricGuitar: 'electric-guitar',
  Bass: 'bass',
  Violin: 'violin',
  Banjo: 'banjo',
  Piano: 'piano',
  Organ: 'organ',
  Synth: 'synth',
  Voice: 'voice',
  Drums: 'drums',
}

/** The instrument a sound id names before its slash, or null for an id of no instrument. */
export function instrumentOfSound(id: SoundId): Instrument | null {
  const prefix = id.slice(0, id.indexOf('/'))
  return INSTRUMENTS.find((instrument) => SOUND_PREFIXES[instrument] === prefix) ?? null
}

/**
 * The General MIDI program that plays a track when no pack does, as `Track::midi_program` in
 * `crates/score-model`: the program of a built-in sound, else the fallback a pack declared, else
 * the program of the default sound of the instrument.
 */
export function trackProgram(track: Pick<Track, 'instrument' | 'sound' | 'fallback_program'>): number {
  return SOUNDS.find((s) => s.id === track.sound)?.program ?? track.fallback_program ?? soundsOf(track.instrument)[0]?.program ?? 0
}

/** A General MIDI program with the built-in sound it plays, if any: "General MIDI program 25 (Acoustic guitar: Steel strings)". */
export function programLabel(program: number): string {
  const sound = SOUNDS.find((s) => s.program === program && s.instrument !== 'Drums')
  return `General MIDI program ${program}${sound ? ` (${soundLabel(sound.id)})` : ''}`
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
