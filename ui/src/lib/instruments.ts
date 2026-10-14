import type { Instrument } from './score-types'

/** Every instrument a track can have, in the order menus list them. */
export const INSTRUMENTS: Instrument[] = ['Guitar', 'ElectricGuitar', 'Bass', 'Violin']

const LABELS: Record<Instrument, string> = {
  Guitar: 'Nylon guitar',
  ElectricGuitar: 'Electric guitar',
  Bass: 'Electric bass',
  Violin: 'Violin',
}

/** The name shown to the user: the stored value is a code of the file format. */
export function instrumentLabel(instrument: Instrument): string {
  return LABELS[instrument]
}
