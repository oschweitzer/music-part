import { invoke } from '@tauri-apps/api/core'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { open } from '@tauri-apps/plugin-dialog'
import { isTauri } from './score-source'
import type { Score } from './score-types'

export type OpenedScore = {
  score: Score
  /** Notes the instrument could not play, left out of the score. */
  skippedNotes: number
}

/** Opening files needs the desktop app: the importer is written in Rust. */
export function canOpenFiles(): boolean {
  return isTauri()
}

export function isMidiPath(path: string): boolean {
  return /\.midi?$/i.test(path)
}

/** Asks the user for a MIDI file. Resolves with its path, or null if they cancel. */
export async function chooseMidiFile(): Promise<string | null> {
  const chosen = await open({
    multiple: false,
    directory: false,
    filters: [{ name: 'MIDI files', extensions: ['mid', 'midi'] }],
  })
  return typeof chosen === 'string' ? chosen : null
}

/** The MIDI file the app was started with (command line or "Open with"), if any. */
export async function initialFile(): Promise<string | null> {
  if (!isTauri()) return null
  return invoke<string | null>('initial_file')
}

/** Rejects with a message fit to show the user. */
export async function openMidiFile(path: string): Promise<OpenedScore> {
  try {
    return await invoke<OpenedScore>('open_midi', { path })
  } catch (error) {
    throw new Error(typeof error === 'string' ? error : String(error))
  }
}

/**
 * Reports files dropped on the window, and whether a drag is hovering it. Resolves with the
 * function that stops listening. Does nothing in a plain browser.
 */
export async function onFilesDropped({
  onDrop,
  onHover,
}: {
  onDrop: (paths: string[]) => void
  onHover: (isHovering: boolean) => void
}): Promise<() => void> {
  if (!isTauri()) return () => {}

  return getCurrentWebview().onDragDropEvent((event) => {
    const { type } = event.payload
    if (type === 'enter' || type === 'over') onHover(true)
    else if (type === 'leave') onHover(false)
    else if (type === 'drop') {
      onHover(false)
      onDrop(event.payload.paths)
    }
  })
}
