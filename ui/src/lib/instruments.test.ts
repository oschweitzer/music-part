import { describe, expect, it } from 'vitest'
import { INSTRUMENTS, SOUNDS, instrumentLabel, isPitchedTrack, noteKindOf, soundLabel, soundsOf } from './instruments'

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

  it('should give distinct General MIDI programs to distinct sounds', () => {
    const programs = SOUNDS.map((s) => s.program)
    expect(new Set(programs).size).toBe(SOUNDS.length)
    expect(programs.every((p) => p >= 0 && p <= 127)).toBe(true)
  })

  it('should store the notes of the piano as pitches, those of string instruments on strings', () => {
    expect(INSTRUMENTS.filter((i) => noteKindOf(i) === 'pitched')).toEqual(['Piano'])
    expect(isPitchedTrack({ instrument: 'Banjo' })).toBe(false)
  })
})
