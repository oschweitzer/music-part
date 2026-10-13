import type { NoteValue } from './score-types'
import type { Direction } from './selection'

export type Command =
  | { type: 'digit'; digit: number }
  | { type: 'move'; direction: Direction }
  | { type: 'remove' }
  | { type: 'duration'; value: NoteValue }
  | { type: 'dot' }
  | { type: 'insertMeasure' }
  | { type: 'undo' }
  | { type: 'redo' }
  | { type: 'save' }
  | { type: 'saveAs' }
  | { type: 'new' }
  | { type: 'open' }
  | { type: 'clearSelection' }
  | { type: 'togglePlayback' }

export type KeyInput = {
  key: string
  ctrlKey: boolean
  metaKey: boolean
  shiftKey: boolean
  altKey: boolean
}

const DURATION_KEYS: Record<string, NoteValue> = {
  w: 'Whole',
  h: 'Half',
  q: 'Quarter',
  e: 'Eighth',
  s: 'Sixteenth',
  t: 'ThirtySecond',
}

const ARROWS: Record<string, Direction> = {
  ArrowLeft: 'left',
  ArrowRight: 'right',
  ArrowUp: 'up',
  ArrowDown: 'down',
}

/** Maps a key press to what the editor should do, or null when the key means nothing here. */
export function interpretKey(input: KeyInput): Command | null {
  const { key, shiftKey, altKey } = input
  const hasCommandKey = input.ctrlKey || input.metaKey
  const lower = key.toLowerCase()

  if (hasCommandKey) {
    if (altKey) return null
    if (lower === 'z') return shiftKey ? { type: 'redo' } : { type: 'undo' }
    if (lower === 'y') return { type: 'redo' }
    if (lower === 's') return shiftKey ? { type: 'saveAs' } : { type: 'save' }
    if (lower === 'o') return { type: 'open' }
    if (lower === 'n') return { type: 'new' }
    if (key === 'Enter') return { type: 'insertMeasure' }
    return null
  }
  if (altKey) return null

  if (key === ' ') return { type: 'togglePlayback' }
  if (/^[0-9]$/.test(key)) return { type: 'digit', digit: Number(key) }
  if (key in ARROWS) return { type: 'move', direction: ARROWS[key] as Direction }
  if (key === 'Delete' || key === 'Backspace') return { type: 'remove' }
  if (key === 'Escape') return { type: 'clearSelection' }
  if (key === '.') return { type: 'dot' }
  if (!shiftKey && lower in DURATION_KEYS) return { type: 'duration', value: DURATION_KEYS[lower] as NoteValue }
  return null
}
