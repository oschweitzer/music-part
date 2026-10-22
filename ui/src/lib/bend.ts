import type { Technique } from './score-types'

export type BendAmount = 'quarter' | 'half' | 'full'

/** The amounts of a bend, from the smallest to the largest. `full` is the plain `Bend` of older scores. */
export const BEND_AMOUNTS: { amount: BendAmount; technique: Technique; label: string; name: string }[] = [
  { amount: 'quarter', technique: 'BendQuarter', label: '1/4', name: 'Quarter tone' },
  { amount: 'half', technique: 'BendHalf', label: '1/2', name: 'Half tone' },
  { amount: 'full', technique: 'Bend', label: 'full', name: 'Whole tone' },
]

/** The amount of a bend, or null for no technique or one that is not a bend. */
export function bendAmount(technique: Technique | null): BendAmount | null {
  return BEND_AMOUNTS.find((entry) => entry.technique === technique)?.amount ?? null
}

/** The technique that bends by this amount. */
export function bendTechnique(amount: BendAmount): Technique {
  return BEND_AMOUNTS.find((entry) => entry.amount === amount)?.technique ?? 'Bend'
}

/**
 * The bend one step up (1) or down (-1) the amounts, staying on the first or last one. Null when
 * the technique is not a bend: there is nothing to step.
 */
export function steppedBend({ technique, step }: { technique: Technique | null; step: 1 | -1 }): Technique | null {
  const index = BEND_AMOUNTS.findIndex((entry) => entry.technique === technique)
  if (index < 0) return null
  const target = Math.min(Math.max(index + step, 0), BEND_AMOUNTS.length - 1)
  return BEND_AMOUNTS[target]?.technique ?? null
}
