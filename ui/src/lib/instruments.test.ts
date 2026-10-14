import { describe, expect, it } from 'vitest'
import { INSTRUMENTS, instrumentLabel } from './instruments'

describe('instruments', () => {
  it('should give every instrument a distinct label', () => {
    const labels = INSTRUMENTS.map(instrumentLabel)
    expect(new Set(labels).size).toBe(INSTRUMENTS.length)
    expect(labels).toContain('Electric guitar')
  })
})
