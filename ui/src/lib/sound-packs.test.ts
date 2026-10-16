import { describe, expect, it } from 'vitest'
import type { PackSound, SoundPack, SoundPacks } from './document-api'
import { instrumentOfSound, isBuiltInSound, programLabel, trackProgram } from './instruments'
import type { Instrument, SoundId } from './score-types'
import { anySoundLabel, declaredSounds, keyName, keyRange, soundChoices, soundSource, soundsPanel } from './sound-packs'

const pack = (id: string, sounds: SoundPack['sounds']): SoundPack => ({
  id,
  name: `Pack ${id}`,
  author: 'Someone',
  licence: 'CC0-1.0',
  source: null,
  folder: `/packs/${id}`,
  sounds,
})

const builtIn = (sound: SoundId, firstKey: number, lastKey: number): PackSound => ({ sound, firstKey, lastKey, name: null, program: null })

const nylon = pack('nylon', [builtIn('guitar/nylon', 40, 88)])
const better = pack('better', [builtIn('guitar/nylon', 28, 100)])
const bass = pack('bass', [builtIn('bass/finger', 26, 45)])
const twelve = pack('twelve', [{ sound: 'guitar/twelve-string', firstKey: 40, lastKey: 88, name: 'Twelve-string', program: 25 }])

const packs = (list: SoundPack[]): SoundPacks => ({ packs: list, refused: [], userFolder: '/home/me/packs' })

const trackOf = (instrument: Instrument, sound: SoundId, fallbackProgram?: number) => ({
  instrument,
  sound,
  ...(fallbackProgram === undefined ? {} : { fallback_program: fallbackProgram }),
})

describe('soundSource', () => {
  it('should give the last pack listing the sound, as the engine plays it', () => {
    const source = soundSource({ packs: [nylon, bass, better], sound: 'guitar/nylon' })
    expect(source.kind === 'pack' && source.pack.id).toBe('better')
  })

  it('should fall back on General MIDI when no pack has the sound', () => {
    expect(soundSource({ packs: [nylon], sound: 'guitar/steel' })).toEqual({ kind: 'generalMidi' })
  })
})

describe('keyName', () => {
  it('should name MIDI keys with their octave', () => {
    expect([keyName(60), keyName(40), keyName(26)]).toEqual(['C4', 'E2', 'D1'])
    expect(keyRange({ firstKey: 40, lastKey: 88 })).toBe('E2 to E6')
  })
})

describe('sounds a pack declares', () => {
  it('should know the instrument of a sound by its id and tell built-in sounds apart', () => {
    expect(instrumentOfSound('electric-guitar/fuzz')).toBe('ElectricGuitar')
    expect(instrumentOfSound('guitar/twelve-string')).toBe('Guitar')
    expect(isBuiltInSound('guitar/steel')).toBe(true)
    expect(isBuiltInSound('guitar/twelve-string')).toBe(false)
  })

  it('should list the new sounds of the packs, a later pack giving the name of a sound two declare', () => {
    const renamed = pack('renamed', [{ ...twelve.sounds[0]!, name: 'Twelve strings', program: 26 }])
    expect(declaredSounds([twelve, nylon, renamed])).toEqual([
      { id: 'guitar/twelve-string', name: 'Twelve strings', program: 26, instrument: 'Guitar', pack: 'Pack renamed' },
    ])
  })

  it('should offer the built-in sounds then the new sounds of the instrument', () => {
    const choices = soundChoices({ instrument: 'Guitar', packs: [twelve, bass] })
    expect(choices.map((c) => [c.id, c.kind])).toEqual([
      ['guitar/nylon', 'builtIn'],
      ['guitar/steel', 'builtIn'],
      ['guitar/twelve-string', 'pack'],
    ])
    expect(soundChoices({ instrument: 'Bass', packs: [twelve] }).some((c) => c.kind === 'pack')).toBe(false)
  })

  it('should offer the sound of a track whose pack is missing, named with the program that plays it', () => {
    const choices = soundChoices({ instrument: 'Guitar', packs: [], track: trackOf('Guitar', 'guitar/twelve-string', 25) })
    expect(choices.at(-1)).toEqual({
      id: 'guitar/twelve-string',
      name: 'twelve-string (pack missing), plays General MIDI program 25',
      kind: 'missing',
    })
    expect(soundChoices({ instrument: 'Guitar', packs: [twelve], track: trackOf('Guitar', 'guitar/twelve-string', 25) })).toHaveLength(3)
  })

  it('should play a track whose pack is missing with its fallback, else the default program of its instrument', () => {
    expect(trackProgram(trackOf('Guitar', 'guitar/steel'))).toBe(25)
    expect(trackProgram(trackOf('Guitar', 'guitar/twelve-string', 26))).toBe(26)
    expect(trackProgram(trackOf('ElectricGuitar', 'electric-guitar/fuzz'))).toBe(27)
    expect(programLabel(25)).toBe('General MIDI program 25 (Acoustic guitar: Steel strings)')
    expect(programLabel(73)).toBe('General MIDI program 73')
  })

  it('should name any sound with its instrument', () => {
    expect(anySoundLabel({ packs: [twelve], sound: 'guitar/twelve-string' })).toBe('Acoustic guitar: Twelve-string')
    expect(anySoundLabel({ packs: [], sound: 'guitar/twelve-string' })).toBe('Acoustic guitar: twelve-string (pack missing)')
    expect(anySoundLabel({ packs: [], sound: 'bass/slap' })).toBe('Electric bass: Slap')
  })
})

describe('soundsPanel', () => {
  it('should say which pack plays the sound of the selected track, with its licence', () => {
    const panel = soundsPanel({ packs: packs([nylon]), track: trackOf('Guitar', 'guitar/nylon') })
    expect(panel.current?.label).toBe('Acoustic guitar: Nylon strings')
    expect(panel.current?.from).toBe('Pack nylon (CC0-1.0)')
    expect(panel.current?.detail).toContain('E2 to E6')
  })

  it('should say General MIDI plays a sound no pack records', () => {
    expect(soundsPanel({ packs: packs([nylon]), track: trackOf('Bass', 'bass/slap') }).current?.from).toBe('General MIDI')
  })

  it('should say a pack sound is missing and which program plays it instead', () => {
    const current = soundsPanel({ packs: packs([nylon]), track: trackOf('Guitar', 'guitar/twelve-string', 25) }).current
    expect(current?.from).toBe('General MIDI (pack missing)')
    expect(current?.detail).toContain('General MIDI program 25 (Acoustic guitar: Steel strings)')
  })

  it('should have no current sound without a track', () => {
    expect(soundsPanel({ packs: packs([nylon]), track: null }).current).toBeNull()
  })

  it('should mark a sound a later pack replaces', () => {
    const panel = soundsPanel({ packs: packs([nylon, bass, better]), track: null })
    expect(panel.packs.map((p) => p.sounds[0]?.replacedBy)).toEqual(['Pack better', null, null])
    expect(panel.packs[1]?.sounds[0]?.label).toBe('Electric bass: Finger')
  })

  it('should say which sounds of a pack are new', () => {
    const panel = soundsPanel({ packs: packs([nylon, twelve]), track: trackOf('Guitar', 'guitar/twelve-string', 25) })
    expect(panel.packs.map((p) => p.sounds[0]?.isNew)).toEqual([false, true])
    expect(panel.packs[1]?.sounds[0]?.label).toBe('Acoustic guitar: Twelve-string')
    expect(panel.current?.label).toBe('Acoustic guitar: Twelve-string')
    expect(panel.current?.from).toBe('Pack twelve (CC0-1.0)')
  })

  it('should pass on the refused packs and the user folder', () => {
    const refused = [{ folder: '/packs/broken', reason: 'invalid pack.json' }]
    const panel = soundsPanel({ packs: { packs: [], refused, userFolder: '/home/me/packs' }, track: trackOf('Guitar', 'guitar/nylon') })
    expect(panel.refused).toEqual(refused)
    expect(panel.userFolder).toBe('/home/me/packs')
    expect(panel.packs).toEqual([])
  })
})
