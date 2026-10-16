import { describe, expect, it } from 'vitest'
import { CLEF_CHOICES, INSTRUMENTS, SOUNDS, hasClefChoice, hasStrings, instrumentLabel, isPercussionTrack, isPitchedTrack, noteKindOf, soundLabel, soundsOf } from './instruments'

describe('instruments', () => {
  it('should give every instrument a distinct label', () => {
    const labels = INSTRUMENTS.map(instrumentLabel)
    expect(new Set(labels).size).toBe(INSTRUMENTS.length)
    expect(labels).toContain('Electric guitar')
  })
})

describe('sounds', () => {
  it('should give every instrument at least one sound, with distinct names', () => {
    for (const instrument of INSTRUMENTS) {
      const names = soundsOf(instrument).map((s) => s.name)
      expect(names.length).toBeGreaterThan(0)
      expect(new Set(names).size).toBe(names.length)
    }
    const counted = INSTRUMENTS.reduce((sum, instrument) => sum + soundsOf(instrument).length, 0)
    expect(counted).toBe(SOUNDS.length)
  })

  it('should list the default sound of an instrument first', () => {
    expect(soundsOf('ElectricGuitar')[0]?.id).toBe('electric-guitar/clean')
    expect(soundsOf('Guitar')[0]?.id).toBe('guitar/nylon')
    expect(soundsOf('Bass')[0]?.id).toBe('bass/finger')
  })

  it('should name a sound together with its instrument', () => {
    expect(soundLabel('electric-guitar/distortion')).toBe('Electric guitar: Distortion')
    expect(soundLabel('banjo/standard')).toBe('Banjo: Standard')
  })

  it('should give distinct General MIDI programs to distinct sounds, kits among kits', () => {
    for (const isKit of [false, true]) {
      const programs = SOUNDS.filter((s) => (s.instrument === 'Drums') === isKit).map((s) => s.program)
      expect(new Set(programs).size).toBe(programs.length)
      expect(programs.every((p) => p >= 0 && p <= 127)).toBe(true)
    }
  })

  it('should store drum notes as pieces, on a track without strings', () => {
    expect(INSTRUMENTS.filter((i) => noteKindOf(i) === 'percussion')).toEqual(['Drums'])
    expect(isPercussionTrack({ instrument: 'Drums' })).toBe(true)
    expect(hasStrings({ instrument: 'Drums' })).toBe(false)
    expect(hasStrings({ instrument: 'Banjo' })).toBe(true)
    expect(soundsOf('Drums')[0]?.id).toBe('drums/standard')
  })

  it('should store the notes of keyboards and voices as pitches, those of string instruments on strings', () => {
    expect(INSTRUMENTS.filter((i) => noteKindOf(i) === 'pitched')).toEqual(['Piano', 'Organ', 'Synth', 'Voice'])
    expect(isPitchedTrack({ instrument: 'Banjo' })).toBe(false)
  })

  it('should list the default sounds of keyboards and voices first', () => {
    expect(soundsOf('Piano')[0]?.id).toBe('piano/grand')
    expect(soundsOf('Organ')[0]?.id).toBe('organ/drawbar')
    expect(soundsOf('Synth')[0]?.id).toBe('synth/square-lead')
    expect(soundsOf('Voice')[0]?.id).toBe('voice/aahs')
    expect(soundLabel('voice/oohs')).toBe('Voice: Voice oohs')
  })

  it('should let only a voice choose its clef, among three', () => {
    expect(INSTRUMENTS.filter(hasClefChoice)).toEqual(['Voice'])
    expect(CLEF_CHOICES.map((c) => c.clef)).toEqual(['treble', 'treble8vb', 'bass'])
  })
})
