import { describe, expect, it } from 'vitest'
import { interpretKey, type KeyInput } from './keymap'

const press = (key: string, modifiers: Partial<KeyInput> = {}) =>
  interpretKey({ key, ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, ...modifiers })

describe('interpretKey', () => {
  it('should read digits as frets', () => {
    expect(press('0')).toEqual({ type: 'digit', digit: 0 })
    expect(press('9')).toEqual({ type: 'digit', digit: 9 })
  })

  it('should read the arrows as moves', () => {
    expect(press('ArrowLeft')).toEqual({ type: 'move', direction: 'left' })
    expect(press('ArrowDown')).toEqual({ type: 'move', direction: 'down' })
  })

  it('should move to the previous or next track with Alt and the up and down arrows', () => {
    expect(press('ArrowUp', { altKey: true })).toEqual({ type: 'switchTrack', step: -1 })
    expect(press('ArrowDown', { altKey: true })).toEqual({ type: 'switchTrack', step: 1 })
    expect(press('ArrowLeft', { altKey: true })).toBeNull()
  })

  it('should read delete and backspace as removing the note', () => {
    expect(press('Delete')).toEqual({ type: 'remove' })
    expect(press('Backspace')).toEqual({ type: 'remove' })
  })

  it('should choose a note value with a letter, whatever the case lock', () => {
    expect(press('q')).toEqual({ type: 'duration', value: 'Quarter' })
    // Caps lock gives an upper-case letter without Shift; Shift+letter is kept free.
    expect(press('Q')).toEqual({ type: 'duration', value: 'Quarter' })
    expect(press('Q', { shiftKey: true })).toBeNull()
    expect(press('w')).toEqual({ type: 'duration', value: 'Whole' })
    expect(press('t')).toEqual({ type: 'duration', value: 'ThirtySecond' })
    expect(press('.')).toEqual({ type: 'dot' })
  })

  it('should undo and redo with ctrl or cmd', () => {
    expect(press('z', { ctrlKey: true })).toEqual({ type: 'undo' })
    expect(press('z', { metaKey: true })).toEqual({ type: 'undo' })
    expect(press('Z', { ctrlKey: true, shiftKey: true })).toEqual({ type: 'redo' })
    expect(press('y', { ctrlKey: true })).toEqual({ type: 'redo' })
  })

  it('should save, save as, open and create with ctrl or cmd', () => {
    expect(press('s', { ctrlKey: true })).toEqual({ type: 'save' })
    expect(press('S', { ctrlKey: true, shiftKey: true })).toEqual({ type: 'saveAs' })
    expect(press('o', { metaKey: true })).toEqual({ type: 'open' })
    expect(press('n', { ctrlKey: true })).toEqual({ type: 'new' })
    expect(press('Enter', { ctrlKey: true })).toEqual({ type: 'insertMeasure' })
    expect(press('Delete', { ctrlKey: true })).toEqual({ type: 'deleteMeasure' })
  })

  it('should not turn a command shortcut into typing', () => {
    // Ctrl+5 or Ctrl+Q must not set a fret or a duration.
    expect(press('5', { ctrlKey: true })).toBeNull()
    expect(press('q', { ctrlKey: true })).toBeNull()
    expect(press('q', { altKey: true })).toBeNull()
  })

  it('should read space as play and pause, and escape as clearing the selection', () => {
    expect(press(' ')).toEqual({ type: 'togglePlayback' })
    expect(press('Escape')).toEqual({ type: 'clearSelection' })
    expect(press('Home')).toEqual({ type: 'rewind' })
  })

  it('should read L as a tie and a few letters as techniques', () => {
    expect(press('l')).toEqual({ type: 'tie' })
    expect(press('b')).toEqual({ type: 'technique', technique: 'Bend' })
    expect(press('m')).toEqual({ type: 'technique', technique: 'PalmMute' })
    expect(press('/')).toEqual({ type: 'technique', technique: 'Slide' })
    expect(press('l', { ctrlKey: true })).toBeNull()
  })

  it('should read shift with left or right as extending the selection, and copy, cut, paste', () => {
    expect(press('ArrowRight', { shiftKey: true })).toEqual({ type: 'extend', direction: 'right' })
    expect(press('ArrowLeft', { shiftKey: true })).toEqual({ type: 'extend', direction: 'left' })
    expect(press('ArrowDown', { shiftKey: true })).toEqual({ type: 'move', direction: 'down' })
    expect(press('c', { ctrlKey: true })).toEqual({ type: 'copy' })
    expect(press('x', { metaKey: true })).toEqual({ type: 'cut' })
    expect(press('v', { ctrlKey: true })).toEqual({ type: 'paste' })
    expect(press('c')).toBeNull()
  })

  it('should write a note with Enter, or with shift and a note name', () => {
    expect(press('Enter')).toEqual({ type: 'enterNote' })
    expect(press('Enter', { ctrlKey: true })).toEqual({ type: 'insertMeasure' })
    expect(press('E', { shiftKey: true })).toEqual({ type: 'noteName', letter: 'E' })
    expect(press('g', { shiftKey: true })).toEqual({ type: 'noteName', letter: 'G' })
    expect(press('H', { shiftKey: true })).toBeNull()
  })

  it('should ignore every other key', () => {
    expect(press('a')).toBeNull()
    expect(press('F5')).toBeNull()
    expect(press('Tab')).toBeNull()
  })
})
