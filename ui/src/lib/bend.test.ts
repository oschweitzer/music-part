import { describe, expect, it } from 'vitest'
import { bendAmount, bendTechnique, steppedBend } from './bend'

describe('bendAmount', () => {
  it('should name the amount of each bend, and nothing for the other techniques', () => {
    expect(bendAmount('BendQuarter')).toBe('quarter')
    expect(bendAmount('BendHalf')).toBe('half')
    expect(bendAmount('Bend')).toBe('full')
    expect(bendAmount('Slide')).toBeNull()
    expect(bendAmount(null)).toBeNull()
  })

  it('should give back the technique of an amount', () => {
    expect(bendTechnique('quarter')).toBe('BendQuarter')
    expect(bendTechnique('half')).toBe('BendHalf')
    expect(bendTechnique('full')).toBe('Bend')
  })
})

describe('steppedBend', () => {
  it('should go up and down the three amounts', () => {
    expect(steppedBend({ technique: 'BendQuarter', step: 1 })).toBe('BendHalf')
    expect(steppedBend({ technique: 'BendHalf', step: 1 })).toBe('Bend')
    expect(steppedBend({ technique: 'Bend', step: -1 })).toBe('BendHalf')
    expect(steppedBend({ technique: 'BendHalf', step: -1 })).toBe('BendQuarter')
  })

  it('should stay on the first and the last amount', () => {
    expect(steppedBend({ technique: 'Bend', step: 1 })).toBe('Bend')
    expect(steppedBend({ technique: 'BendQuarter', step: -1 })).toBe('BendQuarter')
  })

  it('should leave alone what is not a bend', () => {
    expect(steppedBend({ technique: 'Slide', step: 1 })).toBeNull()
    expect(steppedBend({ technique: null, step: -1 })).toBeNull()
  })
})
