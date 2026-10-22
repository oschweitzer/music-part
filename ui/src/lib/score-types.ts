// Mirrors the serde JSON of `crates/score-model`. Keep both in sync by hand until the types are generated.

export type NoteValue = 'Whole' | 'Half' | 'Quarter' | 'Eighth' | 'Sixteenth' | 'ThirtySecond'

/** `Bend` is the whole-tone bend, the one files wrote before bend amounts existed. */
export type Technique = 'HammerOn' | 'PullOff' | 'Slide' | 'Bend' | 'BendHalf' | 'BendQuarter' | 'PalmMute' | 'Harmonic'

export type Instrument = 'Guitar' | 'ElectricGuitar' | 'Bass' | 'Violin' | 'Banjo' | 'Piano' | 'Organ' | 'Synth' | 'Voice' | 'Drums'

/**
 * The id of a sound of an instrument, `"<instrument>/<sound>"`: a built-in sound, or one a sound
 * pack declares, such as `"guitar/twelve-string"` (`Sound` in `crates/score-model`).
 */
export type SoundId = BuiltInSoundId | `${string}/${string}`

/** The id of a sound of the built-in table (see `SOUNDS` in `instruments.ts`). */
export type BuiltInSoundId =
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
  | 'drums/standard'
  | 'drums/room'
  | 'drums/power'
  | 'drums/electronic'
  | 'drums/jazz'
  | 'drums/brush'
  | 'piano/bright'
  | 'piano/electric'
  | 'piano/electric-fm'
  | 'organ/drawbar'
  | 'organ/rock'
  | 'organ/church'
  | 'organ/reed'
  | 'synth/square-lead'
  | 'synth/saw-lead'
  | 'synth/warm-pad'
  | 'synth/choir-pad'
  | 'voice/aahs'
  | 'voice/oohs'
  | 'voice/synth'

/** `Clef` of the model: the clef a voice track chooses (treble8vb is the treble clef an octave down). */
export type TrackClef = 'treble' | 'treble8vb' | 'bass'

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

/** A hit on a piece of a drum kit, by its General MIDI percussion key (35 to 81; see `drums.ts`). */
export type PercussionNote = {
  piece: number
  technique: Technique | null
  tied?: boolean
}

/** `Note` and `Place` of the model: the place fields are written flat in the note. */
export type Note = FrettedNote | PitchedNote | PercussionNote

export type Duration = { value: NoteValue; dots: number }

/** An empty `notes` array is a rest. */
export type Beat = { duration: Duration; notes: Note[] }

export type Voice = { beats: Beat[] }

export type TimeSignature = { numerator: number; denominator: number }

export type Measure = { time_signature: TimeSignature; voices: Voice[] }

export type Track = {
  name: string
  instrument: Instrument
  /** A sound of `instrument`: built in, or declared by a sound pack that may not be installed. */
  sound: SoundId
  /** The General MIDI program that plays a pack's sound when its pack is missing; absent for a built-in sound. */
  fallback_program?: number
  /** MIDI pitches of the open strings, string 1 first (the highest, except a banjo's short fifth string). Empty for a pitched instrument or drums. */
  tuning: number[]
  measures: Measure[]
  /** The clef of a voice, which chooses it; absent for every other instrument. */
  clef?: TrackClef
}

export type Score = { title: string; tempo_bpm: number; tracks: Track[] }
