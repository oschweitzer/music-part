import { describe, expect, it } from 'vitest'
import { canOpenFiles, isMidiPath } from './files'

describe('files', () => {
  it('should recognise MIDI files whatever the case', () => {
    expect(isMidiPath('/home/me/song.mid')).toBe(true)
    expect(isMidiPath('C:\\Music\\SONG.MIDI')).toBe(true)
    expect(isMidiPath('/home/me/song.mid.txt')).toBe(false)
    expect(isMidiPath('/home/me/song.gp5')).toBe(false)
  })

  it('should not offer to open files outside the desktop app', () => {
    // Node has no window, so there is no Tauri bridge.
    expect(canOpenFiles()).toBe(false)
  })
})
