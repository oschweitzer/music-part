import { describe, expect, it } from 'vitest'
import { audibleTracks, resized } from './mix'

describe('audibleTracks', () => {
  it('should let every track sound when nothing is muted or soloed', () => {
    expect(audibleTracks({ muted: [false, false], solo: [false, false] })).toEqual([true, true])
  })

  it('should silence the muted tracks', () => {
    expect(audibleTracks({ muted: [false, true, false], solo: [false, false, false] })).toEqual([true, false, true])
  })

  it('should play only the soloed tracks, whatever is muted', () => {
    expect(audibleTracks({ muted: [false, false, true], solo: [false, true, true] })).toEqual([false, true, true])
  })
})

describe('resized', () => {
  it('should cut and pad with false', () => {
    expect(resized({ flags: [true, true, true], length: 2 })).toEqual([true, true])
    expect(resized({ flags: [true], length: 3 })).toEqual([true, false, false])
  })
})
