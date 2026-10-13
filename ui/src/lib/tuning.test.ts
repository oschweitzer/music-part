import { describe, expect, it } from 'vitest'
import { formatTuning, parseTuning } from './tuning'

describe('tuning names', () => {
  it('writes standard guitar tuning from the highest string', () => {
    expect(formatTuning([64, 59, 55, 50, 45, 40])).toBe('E4 B3 G3 D3 A2 E2')
  })

  it('writes sharps', () => {
    expect(formatTuning([61])).toBe('C#4')
  })

  it('reads what it writes', () => {
    const guitar = [64, 59, 55, 50, 45, 40]
    expect(parseTuning(formatTuning(guitar))).toEqual(guitar)
  })

  it('reads flats, lowercase letters and commas', () => {
    expect(parseTuning('eb4, b3 g3')).toEqual([63, 59, 55])
  })

  it('refuses anything that is not a note', () => {
    expect(parseTuning('')).toBeNull()
    expect(parseTuning('E4 H3')).toBeNull()
    expect(parseTuning('E')).toBeNull()
  })

  it('refuses a pitch outside the MIDI range', () => {
    expect(parseTuning('G#9')).toBeNull()
    expect(parseTuning('C-2')).toBeNull()
  })
})
