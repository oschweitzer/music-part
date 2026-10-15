import { afterEach, describe, expect, it, vi } from 'vitest'
import { dropTrackFromView, readFrettedView, readViewMode, saveFrettedView, saveViewMode, shownTracks, toggleCollapsed } from './track-view'
import type { TrackView } from './track-view'

afterEach(() => vi.unstubAllGlobals())

const tabs: TrackView = { mode: 'tabs', collapsed: [] }
const rows: TrackView = { mode: 'rows', collapsed: [] }

describe('shownTracks', () => {
  it('should show only the edited track in tabs mode', () => {
    expect(shownTracks({ view: tabs, trackCount: 3, current: 1 })).toEqual([{ track: 1, isCollapsed: false }])
  })

  it('should show every track in order in rows mode, folded or not', () => {
    const view: TrackView = { mode: 'rows', collapsed: [false, true] }
    expect(shownTracks({ view, trackCount: 3, current: 0 })).toEqual([
      { track: 0, isCollapsed: false },
      { track: 1, isCollapsed: true },
      { track: 2, isCollapsed: false },
    ])
  })

  it('should never ask for a track that does not exist', () => {
    expect(shownTracks({ view: tabs, trackCount: 2, current: 5 })).toEqual([{ track: 1, isCollapsed: false }])
    expect(shownTracks({ view: rows, trackCount: 0, current: 0 })).toEqual([])
  })
})

describe('folding rows', () => {
  it('should fold a row and unfold it again', () => {
    const folded = toggleCollapsed({ view: rows, track: 2 })
    expect(folded.collapsed).toEqual([false, false, true])
    expect(toggleCollapsed({ view: folded, track: 2 }).collapsed).toEqual([false, false, false])
  })

  it('should move the folds of the following rows up when a track is removed', () => {
    const view: TrackView = { mode: 'rows', collapsed: [false, false, true] }
    expect(dropTrackFromView({ view, track: 1 }).collapsed).toEqual([false, true])
  })
})

describe('the saved view mode', () => {
  const stubStorage = (store: Record<string, string>) =>
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => store[key] ?? null,
      setItem: (key: string, value: string) => void (store[key] = value),
    })

  it('should come back as it was saved', () => {
    stubStorage({})
    saveViewMode('rows')
    expect(readViewMode()).toBe('rows')
  })

  it('should be tabs when nothing, or something unknown, was saved', () => {
    stubStorage({})
    expect(readViewMode()).toBe('tabs')
    stubStorage({ 'music-part-view': 'grid' })
    expect(readViewMode()).toBe('tabs')
  })

  it('should survive a storage that throws', () => {
    const blocked = () => {
      throw new Error('blocked')
    }
    vi.stubGlobal('localStorage', { getItem: blocked, setItem: blocked })
    expect(readViewMode()).toBe('tabs')
    expect(() => saveViewMode('rows')).not.toThrow()
  })

  it('should keep the way fretted tracks are written, tablature by default', () => {
    stubStorage({})
    expect(readFrettedView()).toBe('tab')
    saveFrettedView('both')
    expect(readFrettedView()).toBe('both')
    stubStorage({ 'music-part-fretted-view': 'score' })
    expect(readFrettedView()).toBe('tab')
  })
})
