import { describe, expect, it } from 'vitest'
import { fretPitch, isPitchedNote, notePitch, noteRow } from './notes'

const guitar = { instrument: 'Guitar' as const, tuning: [64, 59, 55, 50, 45, 40] }
const banjo = { instrument: 'Banjo' as const, tuning: [62, 59, 55, 50, 67] }

describe('notes', () => {
  it('should tell a note by its string or its pitch', () => {
    const pitched = { pitch: 61, technique: null }
    const fretted = { string: 2, fret: 3, technique: null }
    expect(isPitchedNote(pitched)).toBe(true)
    expect(noteRow(pitched)).toBe(61)
    expect(noteRow(fretted)).toBe(2)
  })

  it('should find the pitch of a fret as the model does, the short banjo string included', () => {
    expect(fretPitch({ track: guitar, string: 5, fret: 0 })).toBe(45)
    expect(notePitch({ track: guitar, note: { string: 1, fret: 5, technique: null } })).toBe(69)
    expect(fretPitch({ track: banjo, string: 5, fret: 3 })).toBeNull()
    expect(fretPitch({ track: banjo, string: 5, fret: 7 })).toBe(69)
    expect(fretPitch({ track: guitar, string: 7, fret: 0 })).toBeNull()
    expect(notePitch({ track: { instrument: 'Piano', tuning: [] }, note: { pitch: 30, technique: null } })).toBe(30)
  })
})
