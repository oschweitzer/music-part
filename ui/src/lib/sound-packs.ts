import type { RefusedPack, SoundPack, SoundPacks } from './document-api'
import { soundLabel } from './instruments'
import type { SoundId } from './score-types'

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

/** Lays out the Sounds panel from the packs the app found and the sound of the selected track. */
export function soundsPanel({ packs, sound }: { packs: SoundPacks; sound: SoundId | null }): SoundsPanel {
  return {
    current: sound === null ? null : describeSource({ packs: packs.packs, sound }),
    packs: packs.packs.map((pack, index) => ({
      ...pack,
      sounds: pack.sounds.map((s) => ({
        label: soundLabel(s.sound),
        keys: keyRange(s),
        replacedBy: packs.packs.slice(index + 1).findLast((later) => later.sounds.some((l) => l.sound === s.sound))?.name ?? null,
      })),
    })),
    refused: packs.refused,
    userFolder: packs.userFolder,
  }
}

function describeSource({ packs, sound }: { packs: SoundPack[]; sound: SoundId }): SoundsPanel['current'] {
  const label = soundLabel(sound)
  const source = soundSource({ packs, sound })
  if (source.kind === 'generalMidi') {
    return { label, from: 'General MIDI', detail: 'No pack records this sound: the base SoundFont plays it.' }
  }
  return {
    label,
    from: `${source.pack.name} (${source.pack.licence})`,
    detail: `Recorded from ${keyRange(source)}; General MIDI plays the notes outside.`,
  }
}
