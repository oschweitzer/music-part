import type { ShownTrack } from './tab-layout'

/**
 * How the tracks are laid out: one at a time behind tabs, or every track in a row of its own, one
 * under the other on the same time axis (like a DAW).
 */
export type ViewMode = 'tabs' | 'rows'

/**
 * What is on screen: which tracks are shown and how. The view does not say which track is being
 * edited (that is where the selection is, `Editor.track`); in tabs mode it shows that one.
 */
export type TrackView = {
  mode: ViewMode
  /** Rows folded down to their header, by track index. Only the rows mode folds. */
  collapsed: boolean[]
}

/**
 * How the tracks of a fretted instrument are written: tablature, a standard staff, or both (the
 * staff above the tablature). A pitched track (piano) is always written on its staff.
 */
export type FrettedView = 'tab' | 'staff' | 'both'

const STORAGE_KEY = 'music-part-view'
const FRETTED_KEY = 'music-part-fretted-view'
const MODES: readonly ViewMode[] = ['tabs', 'rows']
export const FRETTED_VIEWS: readonly FrettedView[] = ['tab', 'staff', 'both']

export const frettedViewLabel: Record<FrettedView, string> = { tab: 'Tab', staff: 'Staff', both: 'Both' }

export const viewModeLabel: Record<ViewMode, string> = { tabs: 'Tabs', rows: 'Rows' }

/** The tracks to draw, in order: the edited one in tabs mode, all of them in rows mode. */
export function shownTracks({ view, trackCount, current }: { view: TrackView; trackCount: number; current: number }): ShownTrack[] {
  if (trackCount === 0) return []
  if (view.mode === 'tabs') return [{ track: Math.min(Math.max(current, 0), trackCount - 1), isCollapsed: false }]
  return Array.from({ length: trackCount }, (_, track) => ({ track, isCollapsed: view.collapsed[track] === true }))
}

/** Folds a row down to its header, or unfolds it. */
export function toggleCollapsed({ view, track }: { view: TrackView; track: number }): TrackView {
  const collapsed = Array.from({ length: Math.max(view.collapsed.length, track + 1) }, (_, i) => view.collapsed[i] === true)
  collapsed[track] = !collapsed[track]
  return { ...view, collapsed }
}

/** A track was removed: the rows after it move up, and so does whether they are folded. */
export function dropTrackFromView({ view, track }: { view: TrackView; track: number }): TrackView {
  return { ...view, collapsed: view.collapsed.filter((_, i) => i !== track) }
}

/** The saved mode. Storage can be missing or refuse (private windows), so anything odd means tabs. */
export function readViewMode(): ViewMode {
  try {
    const saved = globalThis.localStorage?.getItem(STORAGE_KEY)
    return MODES.find((mode) => mode === saved) ?? 'tabs'
  } catch {
    return 'tabs'
  }
}

export function saveViewMode(mode: ViewMode): void {
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, mode)
  } catch {
    // The choice just does not survive a restart.
  }
}

/** The saved way of writing fretted tracks; anything odd, or no storage, means tablature only. */
export function readFrettedView(): FrettedView {
  try {
    const saved = globalThis.localStorage?.getItem(FRETTED_KEY)
    return FRETTED_VIEWS.find((view) => view === saved) ?? 'tab'
  } catch {
    return 'tab'
  }
}

export function saveFrettedView(view: FrettedView): void {
  try {
    globalThis.localStorage?.setItem(FRETTED_KEY, view)
  } catch {
    // The choice just does not survive a restart.
  }
}
