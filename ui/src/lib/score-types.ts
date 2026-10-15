// Mirrors the serde JSON of `crates/score-model`. Keep both in sync by hand until the types are generated.

export type NoteValue = 'Whole' | 'Half' | 'Quarter' | 'Eighth' | 'Sixteenth' | 'ThirtySecond'

export type Technique = 'HammerOn' | 'PullOff' | 'Slide' | 'Bend' | 'PalmMute' | 'Harmonic'

export type Instrument = 'Guitar' | 'ElectricGuitar' | 'Bass' | 'Violin' | 'Banjo'

/** The id of a sound of an instrument, `"<instrument>/<sound>"` (see `SOUNDS` in `instruments.ts`). */
export type SoundId =
  | 'guitar/nylon'
  | 'guitar/steel'
  | 'electric-guitar/clean'
  | 'electric-guitar/muted'
  | 'electric-guitar/overdrive'
  | 'electric-guitar/distortion'
  | 'bass/finger'
  | 'bass/pick'
  | 'bass/slap'
  | 'violin/standard'
  | 'banjo/standard'

export type Note = {
  /** String number, 1 is the highest string. */
  string: number
  fret: number
  technique: Technique | null
  tied?: boolean
}

export type Duration = { value: NoteValue; dots: number }

/** An empty `notes` array is a rest. */
export type Beat = { duration: Duration; notes: Note[] }

export type Voice = { beats: Beat[] }

export type TimeSignature = { numerator: number; denominator: number }

export type Measure = { time_signature: TimeSignature; voices: Voice[] }

export type Track = {
  name: string
  instrument: Instrument
  /** One of the sounds of `instrument`. */
  sound: SoundId
  /** MIDI pitches of the open strings, string 1 first (the highest, except a banjo's short fifth string). */
  tuning: number[]
  measures: Measure[]
}

export type Score = { title: string; tempo_bpm: number; tracks: Track[] }
