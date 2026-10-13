import { invoke } from '@tauri-apps/api/core'
import { isTauri } from './score-source'
import type { Duration, Score } from './score-types'

/** Same shapes as the `Edit` enum of `crates/score-model`, in camelCase. */
export type Edit =
  | { type: 'setFret'; track: number; measure: number; beat: number; string: number; fret: number }
  | { type: 'removeNote'; track: number; measure: number; beat: number; string: number }
  | { type: 'setDuration'; track: number; measure: number; beat: number; duration: Duration }
  | { type: 'insertMeasure'; at: number }
  | { type: 'deleteMeasure'; index: number }
  | { type: 'setTempo'; bpm: number }

export type DocumentState = {
  score: Score
  /** Where the score is saved, or null if it has no file yet. */
  path: string | null
  isDirty: boolean
  canUndo: boolean
  canRedo: boolean
}

export type OpenedDocument = {
  document: DocumentState
  /** Notes no string could play, left out when importing a MIDI file. */
  skippedNotes: number
  /** Why the requested file could not be opened, when the demo is shown instead. */
  notice: string | null
}

/** The document lives in the desktop app. Rejected promises carry a message fit to show the user. */
export type DocumentApi = {
  /** False in a plain browser, where the demo can be shown but not changed. */
  canEdit: boolean
  startup(): Promise<OpenedDocument>
  newDocument(): Promise<DocumentState>
  open(path: string): Promise<OpenedDocument>
  /** Saves to `path`, or to the current file when null. */
  save(path: string | null): Promise<DocumentState>
  edit(edit: Edit): Promise<DocumentState>
  undo(): Promise<DocumentState>
  redo(): Promise<DocumentState>
}

export function createDocumentApi(): DocumentApi {
  return isTauri() ? createTauriDocumentApi() : createBrowserDocumentApi()
}

function createTauriDocumentApi(): DocumentApi {
  const call = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
    try {
      return await invoke<T>(command, args)
    } catch (error) {
      throw new Error(typeof error === 'string' ? error : String(error))
    }
  }

  return {
    canEdit: true,
    startup: () => call('startup'),
    newDocument: () => call('new_document'),
    open: (path) => call('open_file', { path }),
    save: (path) => call('save_file', { path }),
    edit: (edit) => call('edit_score', { edit }),
    undo: () => call('undo'),
    redo: () => call('redo'),
  }
}

/** Shows the exported demo so the UI can be developed in a browser. Nothing can be changed. */
function createBrowserDocumentApi(): DocumentApi {
  const needsDesktop = () => Promise.reject(new Error('Editing and files need the desktop app.'))

  return {
    canEdit: false,
    async startup() {
      const response = await fetch('/demo-score.json')
      if (!response.ok) throw new Error(`Could not load the demo score: HTTP ${response.status}`)
      const score = (await response.json()) as Score
      const document = { score, path: null, isDirty: false, canUndo: false, canRedo: false }
      return { document, skippedNotes: 0, notice: null }
    },
    newDocument: needsDesktop,
    open: needsDesktop,
    save: needsDesktop,
    edit: needsDesktop,
    undo: needsDesktop,
    redo: needsDesktop,
  }
}
