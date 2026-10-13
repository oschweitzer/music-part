import { describe, expect, it } from 'vitest'
import { createFilePrompts, fileNameFor, isScoreFile } from './files'

describe('files', () => {
  it('should recognise the files the app opens, whatever the case', () => {
    expect(isScoreFile('/home/me/song.mid')).toBe(true)
    expect(isScoreFile('C:\\Music\\SONG.MIDI')).toBe(true)
    expect(isScoreFile('/home/me/song.mpart')).toBe(true)
    expect(isScoreFile('/home/me/song.mid.txt')).toBe(false)
    expect(isScoreFile('/home/me/song.gp5')).toBe(false)
  })

  it('should build a safe file name from a title', () => {
    expect(fileNameFor('My Song')).toBe('My Song.mpart')
    expect(fileNameFor('AC/DC: Back in Black?')).toBe('AC-DC- Back in Black-.mpart')
    expect(fileNameFor('   ')).toBe('Untitled.mpart')
    expect(fileNameFor('..hidden')).toBe('hidden.mpart')
    expect(fileNameFor('a\tb\nc')).toBe('a-b-c.mpart')
  })

  it('should answer without a window outside the desktop app', async () => {
    const prompts = createFilePrompts()
    expect(await prompts.chooseFileToOpen()).toBeNull()
    expect(await prompts.chooseFileToSave('x.mpart')).toBeNull()
  })
})
