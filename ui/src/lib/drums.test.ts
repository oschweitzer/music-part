import { describe, expect, it } from 'vitest'
import {
  DRUM_PIECES,
  drumPiece,
  isPlayedByFoot,
  PALETTE,
  pieceAtPosition,
  pieceName,
  pieceOfDigit,
  piecePosition,
  STAFF_ORDER,
  stepPiece,
} from './drums'

describe('drum pieces', () => {
  it('should list every General MIDI percussion key from 35 to 81, in order', () => {
    expect(DRUM_PIECES.map((p) => p.key)).toEqual(Array.from({ length: 47 }, (_, i) => 35 + i))
    expect(drumPiece(42)?.name).toBe('Closed hi-hat')
    expect(drumPiece(34)).toBeNull()
    expect(pieceName(36)).toBe('Kick')
  })

  it('should write the kit where drummers read it', () => {
    expect(piecePosition(36)).toBe(1) // kick: bottom space
    expect(piecePosition(38)).toBe(5) // snare: the third space
    expect(piecePosition(42)).toBe(9) // hi-hat: above the top line
    expect(piecePosition(49)).toBe(10) // crash: first ledger line
    expect(piecePosition(44)).toBe(-1) // pedal hi-hat: under the staff
    expect(drumPiece(42)?.head).toBe('cross')
    expect(drumPiece(38)?.head).toBe('normal')
  })

  it('should give no two pieces the same place and head', () => {
    const places = DRUM_PIECES.map((p) => `${piecePosition(p.key)}:${p.head}`)
    expect(new Set(places).size).toBe(DRUM_PIECES.length)
  })

  it('should write the drum of a line on a click there, else its cymbal', () => {
    expect(pieceAtPosition(5)).toBe(38)
    expect(pieceAtPosition(1)).toBe(36)
    expect(pieceAtPosition(9)).toBe(42)
    expect(pieceAtPosition(-1)).toBe(44)
    expect(pieceAtPosition(-4)).toBeNull()
  })

  it('should step a line or a space up or down the staff, to its drum, else its cymbal', () => {
    expect(STAFF_ORDER).toHaveLength(47)
    expect(STAFF_ORDER[0]).toBe(44)
    expect(stepPiece({ piece: 38, direction: 1 })).toBe(47) // snare up: the low-mid tom
    expect(stepPiece({ piece: 38, direction: -1 })).toBe(45) // down: the low tom
    expect(stepPiece({ piece: 42, direction: -1 })).toBe(50) // hi-hat down: the high tom
    expect(stepPiece({ piece: 50, direction: 1 })).toBe(42) // and back up: the hi-hat
    expect(stepPiece({ piece: 42, direction: 1 })).toBe(49) // crash
    expect(stepPiece({ piece: 44, direction: -1 })).toBeNull()
  })

  it('should skip the pieces a beat already has', () => {
    expect(stepPiece({ piece: 42, direction: -1, taken: [50] })).toBe(51) // the ride, on the same line
    expect(stepPiece({ piece: 38, direction: 1, taken: [47, 54, 61, 69, 73] })).toBe(48)
  })

  it('should map the digit keys to the palette, and the feet to their pieces', () => {
    expect(pieceOfDigit(1)).toBe(36)
    expect(pieceOfDigit(0)).toBe(41)
    expect(new Set(PALETTE.map((p) => p.digit)).size).toBe(10)
    expect([35, 36, 44].every(isPlayedByFoot)).toBe(true)
    expect(isPlayedByFoot(38)).toBe(false)
  })
})
