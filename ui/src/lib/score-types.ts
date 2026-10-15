// Mirrors the serde JSON of `crates/score-model`. Keep both in sync by hand until the types are generated.

export type NoteValue = 'Whole' | 'Half' | 'Quarter' | 'Eighth' | 'Sixteenth' | 'ThirtySecond'

export type Technique = 'HammerOn' | 'PullOff' | 'Slide' | 'Bend' | 'PalmMute' | 'Harmonic'

export type Instrument = 'Guitar' | 'ElectricGuitar' | 'Bass' | 'Violin' | 'Banjo' | 'Piano'

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
  | 'piano/grand'

/** A note on a string and a fret, on a track of a fretted instrument (guitar, bass...). */
export type FrettedNote = {
  /** String number, 1 is the highest string. */
  string: number
  fret: number
  technique: Technique | null
  tied?: boolean
}

/** A note at a MIDI pitch (60 is middle C), on a track of a pitched instrument (piano). */
export type PitchedNote = {
  pitch: number
  technique: Technique | null
  tied?: boolean
}

/** `Note` and `Place` of the model: the place fields are written flat in the note. */
export type Note = FrettedNote | PitchedNote

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
  /** MIDI pitches of the open strings, string 1 first (the highest, except a banjo's short fifth string). Empty for a pitched instrument. */
  tuning: number[]
  measures: Measure[]
}

export type Score = { title: string; tempo_bpm: number; tracks: Track[] }
