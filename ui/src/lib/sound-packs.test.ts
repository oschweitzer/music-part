import { describe, expect, it } from 'vitest'
import type { SoundPack, SoundPacks } from './document-api'
import { keyName, keyRange, soundSource, soundsPanel } from './sound-packs'

const pack = (id: string, sounds: SoundPack['sounds']): SoundPack => ({
  id,
  name: `Pack ${id}`,
  author: 'Someone',
  licence: 'CC0-1.0',
  source: null,
  folder: `/packs/${id}`,
  sounds,
})

const nylon = pack('nylon', [{ sound: 'guitar/nylon', firstKey: 40, lastKey: 88 }])
const better = pack('better', [{ sound: 'guitar/nylon', firstKey: 28, lastKey: 100 }])
const bass = pack('bass', [{ sound: 'bass/finger', firstKey: 26, lastKey: 45 }])

const packs = (list: SoundPack[]): SoundPacks => ({ packs: list, refused: [], userFolder: '/home/me/packs' })

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

describe('soundsPanel', () => {
  it('should say which pack plays the sound of the selected track, with its licence', () => {
    const panel = soundsPanel({ packs: packs([nylon]), sound: 'guitar/nylon' })
    expect(panel.current?.label).toBe('Acoustic guitar: Nylon strings')
    expect(panel.current?.from).toBe('Pack nylon (CC0-1.0)')
    expect(panel.current?.detail).toContain('E2 to E6')
  })

  it('should say General MIDI plays a sound no pack records', () => {
    expect(soundsPanel({ packs: packs([nylon]), sound: 'bass/slap' }).current?.from).toBe('General MIDI')
  })

  it('should have no current sound without a track', () => {
    expect(soundsPanel({ packs: packs([nylon]), sound: null }).current).toBeNull()
  })

  it('should mark a sound a later pack replaces', () => {
    const panel = soundsPanel({ packs: packs([nylon, bass, better]), sound: null })
    expect(panel.packs.map((p) => p.sounds[0]?.replacedBy)).toEqual(['Pack better', null, null])
    expect(panel.packs[1]?.sounds[0]?.label).toBe('Electric bass: Finger')
  })

  it('should pass on the refused packs and the user folder', () => {
    const refused = [{ folder: '/packs/broken', reason: 'invalid pack.json' }]
    const panel = soundsPanel({ packs: { packs: [], refused, userFolder: '/home/me/packs' }, sound: 'guitar/nylon' })
    expect(panel.refused).toEqual(refused)
    expect(panel.userFolder).toBe('/home/me/packs')
    expect(panel.packs).toEqual([])
  })
})
