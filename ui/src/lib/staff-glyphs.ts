import type { NoteValue } from './score-types'
import { STAFF_METRICS } from './staff-layout'

/**
 * SVG path data of the symbols of standard notation, drawn by hand so that no music font is
 * needed. Each takes the point it is drawn at; shapes are in units of a staff space.
 */

const S = STAFF_METRICS.lineGap

/** Turns points given in staff spaces from an origin into SVG path data. */
function path(origin: { x: number; y: number }, commands: (string | number)[]): string {
  let isX = true
  return commands
    .map((part) => {
      if (typeof part === 'string') {
        isX = true
        return part
      }
      const value = isX ? origin.x + part * S : origin.y + part * S
      isX = !isX
      return value.toFixed(2)
    })
    .join(' ')
}

/** A treble (G) clef curled around the second line from the bottom of a staff. */
export function trebleClefPath({ x, bottom }: { x: number; bottom: number }): string {
  const origin = { x, y: bottom - S }
  return path(origin, [
    'M', 0.2, 0.3,
    'C', 0.9, 0.2, 0.85, -0.9, 0.0, -0.9,
    'C', -0.95, -0.9, -1.15, 0.35, -0.6, 0.95,
    'C', -0.05, 1.45, 1.15, 1.3, 1.2, 0.25,
    'C', 1.25, -0.65, 0.55, -1.2, 0.0, -1.8,
    'C', -0.6, -2.45, -0.65, -3.65, 0.1, -4.4,
    'C', 0.6, -4.9, 0.85, -3.95, 0.45, -3.25,
    'C', 0.15, -2.7, 0.0, -1.0, 0.25, 2.2,
    'C', 0.35, 2.9, -0.3, 3.05, -0.55, 2.6,
  ])
}

/** The dot at the foot of a treble clef. */
export function trebleClefDot({ x, bottom }: { x: number; bottom: number }): { cx: number; cy: number; r: number } {
  return { cx: x - 0.3 * S, cy: bottom - S + 2.5 * S, r: 0.32 * S }
}

/** A bass (F) clef whose dots sit around the second line from the top. */
export function bassClefPath({ x, bottom }: { x: number; bottom: number }): string {
  const origin = { x, y: bottom - 3 * S }
  return path(origin, ['M', -0.85, 0.05, 'C', -0.8, -0.95, 0.95, -1.05, 1.0, 0.1, 'C', 1.05, 1.25, 0.1, 2.2, -1.0, 2.85])
}

/** The round head and the two dots of a bass clef. */
export function bassClefDots({ x, bottom }: { x: number; bottom: number }): { cx: number; cy: number; r: number }[] {
  const y = bottom - 3 * S
  return [
    { cx: x - 0.7 * S, cy: y + 0.1 * S, r: 0.32 * S },
    { cx: x + 1.55 * S, cy: y - 0.5 * S, r: 0.17 * S },
    { cx: x + 1.55 * S, cy: y + 0.5 * S, r: 0.17 * S },
  ]
}

export function sharpPath({ x, y }: { x: number; y: number }): string {
  return path({ x, y }, [
    'M', -0.2, -0.95, 'L', -0.2, 0.9,
    'M', 0.2, -0.9, 'L', 0.2, 0.95,
    'M', -0.45, -0.2, 'L', 0.45, -0.4,
    'M', -0.45, 0.4, 'L', 0.45, 0.2,
  ])
}

export function flatPath({ x, y }: { x: number; y: number }): string {
  return path({ x, y }, ['M', -0.25, -1.25, 'L', -0.25, 0.45, 'C', 0.55, 0.0, 0.5, -0.65, -0.25, -0.2])
}

export function naturalPath({ x, y }: { x: number; y: number }): string {
  return path({ x, y }, ['M', -0.25, -0.95, 'L', -0.25, 0.4, 'L', 0.25, 0.25, 'M', 0.25, 0.95, 'L', 0.25, -0.4, 'L', -0.25, -0.25])
}

/** The path of a rest centred on the middle line (or hanging from the fourth line for a whole rest). */
export function restPath({ value, x, y }: { value: NoteValue; x: number; y: number }): { d: string; isFilled: boolean } {
  switch (value) {
    case 'Whole':
      return { d: path({ x, y }, ['M', -0.6, 0, 'L', 0.6, 0, 'L', 0.6, 0.5, 'L', -0.6, 0.5, 'Z']), isFilled: true }
    case 'Half':
      return { d: path({ x, y }, ['M', -0.6, 0, 'L', 0.6, 0, 'L', 0.6, -0.5, 'L', -0.6, -0.5, 'Z']), isFilled: true }
    case 'Quarter':
      return {
        d: path({ x, y }, ['M', -0.3, -1.4, 'L', 0.35, -0.65, 'L', -0.3, 0.05, 'L', 0.35, 0.75, 'C', -0.2, 0.55, -0.4, 1.1, -0.05, 1.4]),
        isFilled: false,
      }
    default: {
      const count = value === 'Eighth' ? 1 : value === 'Sixteenth' ? 2 : 3
      const commands: (string | number)[] = ['M', 0.45, -0.9 - 0.25 * (count - 1), 'L', -0.15, 1.1]
      for (let i = 0; i < count; i++) {
        const top = -0.9 + i * 0.8 - 0.25 * (count - 1)
        commands.push('M', 0.45 - i * 0.22, top, 'C', 0.0 - i * 0.22, top + 0.45, -0.4 - i * 0.22, top + 0.3, -0.45 - i * 0.22, top + 0.05)
      }
      return { d: path({ x, y }, commands), isFilled: false }
    }
  }
}

/** The `index`th flag (0 first) at the end of a stem that ends at `x`, `y`. */
export function staffFlagPath({ x, y, isStemUp, index }: { x: number; y: number; isStemUp: boolean; index: number }): string {
  const direction = isStemUp ? 1 : -1
  const start = y + direction * index * 0.65 * S
  return `M${x} ${start} q${0.9 * S} ${direction * 0.5 * S} ${0.6 * S} ${direction * 1.5 * S}`
}
