import type { RefusedPack, SoundPack, SoundPacks } from './document-api'
import { instrumentLabel, instrumentOfSound, isBuiltInSound, programLabel, soundLabel, soundsOf, trackProgram } from './instruments'
import type { Instrument, SoundId, Track } from './score-types'

/** Where a sound comes from: the recordings of a pack, or the General MIDI program of the base SoundFont. */
export type SoundSource = { kind: 'pack'; pack: SoundPack; firstKey: number; lastKey: number } | { kind: 'generalMidi' }

/** The pack that plays `sound`: the last one listing it, as the audio engine does. */
export function soundSource({ packs, sound }: { packs: SoundPack[]; sound: SoundId }): SoundSource {
  for (const pack of [...packs].reverse()) {
    const listed = pack.sounds.find((s) => s.sound === sound)
    if (listed) return { kind: 'pack', pack, firstKey: listed.firstKey, lastKey: listed.lastKey }
  }
  return { kind: 'generalMidi' }
}

/** A sound a pack adds to the built-in table, as `DeclaredSound` in `crates/audio-engine`. */
export type DeclaredSound = { id: SoundId; name: string; program: number; instrument: Instrument; pack: string }

/** The new sounds the packs declare, in the order they first appear; a later pack gives the name of a sound two declare. */
export function declaredSounds(packs: SoundPack[]): DeclaredSound[] {
  const declared: DeclaredSound[] = []
  for (const pack of packs) {
    for (const s of pack.sounds) {
      const instrument = instrumentOfSound(s.sound)
      if (s.name === null || s.program === null || instrument === null) continue
      const entry = { id: s.sound, name: s.name, program: s.program, instrument, pack: pack.name }
      const at = declared.findIndex((d) => d.id === s.sound)
      if (at >= 0) declared[at] = entry
      else declared.push(entry)
    }
  }
  return declared
}

/** A sound the Sound menu offers: built in, declared by a pack, or the sound of the track whose pack is missing. */
export type SoundChoice = { id: SoundId; name: string; kind: 'builtIn' | 'pack' | 'missing' }

/** The sound of a track whose pack is not there, such as "twelve-string (pack missing)". */
export function missingSoundName(id: SoundId): string {
  return `${id.slice(id.indexOf('/') + 1)} (pack missing)`
}

/**
 * The sounds a track of `instrument` can be given: the built-in ones, then those the packs declare,
 * then the sound of `track` when no pack has it, named with the General MIDI program that plays it.
 */
export function soundChoices({
  instrument,
  packs,
  track = null,
}: {
  instrument: Instrument
  packs: SoundPack[]
  track?: Pick<Track, 'instrument' | 'sound' | 'fallback_program'> | null
}): SoundChoice[] {
  const choices: SoundChoice[] = [
    ...soundsOf(instrument).map((s) => ({ id: s.id, name: s.name, kind: 'builtIn' as const })),
    ...declaredSounds(packs)
      .filter((d) => d.instrument === instrument)
      .map((d) => ({ id: d.id, name: d.name, kind: 'pack' as const })),
  ]
  if (track && track.instrument === instrument && !choices.some((c) => c.id === track.sound)) {
    const name = `${missingSoundName(track.sound)}, plays General MIDI program ${trackProgram(track)}`
    choices.push({ id: track.sound, name, kind: 'missing' })
  }
  return choices
}

/** The name of any sound with its instrument: built in, declared by a pack, or of a pack that is missing. */
export function anySoundLabel({ packs, sound }: { packs: SoundPack[]; sound: SoundId }): string {
  if (isBuiltInSound(sound)) return soundLabel(sound)
  const instrument = instrumentOfSound(sound)
  const declared = declaredSounds(packs).find((d) => d.id === sound)
  const name = declared?.name ?? missingSoundName(sound)
  return instrument ? `${instrumentLabel(instrument)}: ${name}` : name
}

const NOTE_NAMES = ['C', 'C♯', 'D', 'D♯', 'E', 'F', 'F♯', 'G', 'G♯', 'A', 'A♯', 'B']

/** A MIDI key as a note name with its octave, such as 40 → "E2". */
export function keyName(key: number): string {
  return `${NOTE_NAMES[key % 12]}${Math.floor(key / 12) - 1}`
}

/** The keys a pack's recordings cover, such as "E2 to E6". */
export function keyRange({ firstKey, lastKey }: { firstKey: number; lastKey: number }): string {
  return `${keyName(firstKey)} to ${keyName(lastKey)}`
}

export type PanelSound = {
  label: string
  keys: string
  /** True for a sound the pack adds to the built-in ones. */
  isNew: boolean
  /** The name of a later pack playing the same sound instead, or null when this one is heard. */
  replacedBy: string | null
}

export type PanelPack = Omit<SoundPack, 'sounds'> & { sounds: PanelSound[] }

/** What the Sounds panel shows. */
export type SoundsPanel = {
  /** The sound of the selected track and where it comes from; null without a track. */
  current: { label: string; from: string; detail: string } | null
  packs: PanelPack[]
  refused: RefusedPack[]
  userFolder: string | null
}

/** Lays out the Sounds panel from the packs the app found and the selected track. */
export function soundsPanel({
  packs,
  track,
}: {
  packs: SoundPacks
  track: Pick<Track, 'instrument' | 'sound' | 'fallback_program'> | null
}): SoundsPanel {
  return {
    current: track === null ? null : describeSource({ packs: packs.packs, track }),
    packs: packs.packs.map((pack, index) => ({
      ...pack,
      sounds: pack.sounds.map((s) => ({
        label: s.name === null ? soundLabel(s.sound) : `${instrumentLabelOf(s.sound)}${s.name}`,
        keys: keyRange(s),
        isNew: s.name !== null,
        replacedBy: packs.packs.slice(index + 1).findLast((later) => later.sounds.some((l) => l.sound === s.sound))?.name ?? null,
      })),
    })),
    refused: packs.refused,
    userFolder: packs.userFolder,
  }
}

function instrumentLabelOf(sound: SoundId): string {
  const instrument = instrumentOfSound(sound)
  return instrument ? `${instrumentLabel(instrument)}: ` : ''
}

function describeSource({
  packs,
  track,
}: {
  packs: SoundPack[]
  track: Pick<Track, 'instrument' | 'sound' | 'fallback_program'>
}): SoundsPanel['current'] {
  const label = anySoundLabel({ packs, sound: track.sound })
  const source = soundSource({ packs, sound: track.sound })
  if (source.kind === 'generalMidi') {
    if (!isBuiltInSound(track.sound)) {
      return {
        label,
        from: 'General MIDI (pack missing)',
        detail: `No pack found here declares this sound: ${programLabel(trackProgram(track))} plays it, as the score recorded.`,
      }
    }
    return { label, from: 'General MIDI', detail: 'No pack records this sound: the base SoundFont plays it.' }
  }
  return {
    label,
    from: `${source.pack.name} (${source.pack.licence})`,
    detail: `Recorded from ${keyRange(source)}; General MIDI plays the notes outside.`,
  }
}
