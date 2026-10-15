import type { Instrument, SoundId } from './score-types'

/** Every instrument a track can have, in the order menus list them. */
export const INSTRUMENTS: Instrument[] = ['Guitar', 'ElectricGuitar', 'Bass', 'Violin', 'Banjo']

const LABELS: Record<Instrument, string> = {
  Guitar: 'Acoustic guitar',
  ElectricGuitar: 'Electric guitar',
  Bass: 'Electric bass',
  Violin: 'Violin',
  Banjo: 'Banjo',
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
