// Mirrors the serde JSON of `crates/score-model`. Keep both in sync by hand until the types are generated.

export type NoteValue = 'Whole' | 'Half' | 'Quarter' | 'Eighth' | 'Sixteenth' | 'ThirtySecond'

export type Technique = 'HammerOn' | 'PullOff' | 'Slide' | 'Bend' | 'PalmMute' | 'Harmonic'

export type Instrument = 'Guitar' | 'ElectricGuitar' | 'Bass' | 'Violin'

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
  /** MIDI pitches of the open strings, highest string first. */
  tuning: number[]
  measures: Measure[]
}

export type Score = { title: string; tempo_bpm: number; tracks: Track[] }
