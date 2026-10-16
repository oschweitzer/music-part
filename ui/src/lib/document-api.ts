import { invoke } from '@tauri-apps/api/core'
import { isTauri } from './score-source'
import type { Beat, Duration, Instrument, Score, SoundId, Technique, TrackClef } from './score-types'

export type BeatAddress = { measure: number; beat: number }

/** Same shapes as the `Edit` enum of `crates/score-model`, in camelCase. */
export type Edit =
  | { type: 'setFret'; track: number; measure: number; beat: number; string: number; fret: number }
  | { type: 'removeNote'; track: number; measure: number; beat: number; string: number }
  | { type: 'setDuration'; track: number; measure: number; beat: number; duration: Duration }
  | { type: 'insertMeasure'; at: number }
  | { type: 'deleteMeasure'; index: number; count: number }
  | { type: 'setTempo'; bpm: number }
  | { type: 'setTied'; track: number; measure: number; beat: number; string: number; tied: boolean }
  | { type: 'setTechnique'; track: number; measure: number; beat: number; string: number; technique: Technique | null }
  | { type: 'addTrack'; instrument: Instrument; sound?: SoundId; fallbackProgram?: number }
  | { type: 'removeTrack'; index: number }
  | { type: 'setTuning'; track: number; tuning: number[] }
  | { type: 'paste'; track: number; measure: number; beat: number; beats: Beat[] }
  | { type: 'setTimeSignature'; measure: number; numerator: number; denominator: number }
  | { type: 'renameTrack'; track: number; name: string }
  | { type: 'setInstrument'; track: number; instrument: Instrument }
  | { type: 'setSound'; track: number; sound: SoundId; fallbackProgram?: number }
  | {
      type: 'clearNotes'
      track: number
      from: BeatAddress
      to: BeatAddress
      strings: { first: number; last: number } | null
      pitches?: { low: number; high: number } | null
    }
  | { type: 'addPitch'; track: number; measure: number; beat: number; pitch: number }
  | { type: 'removePitch'; track: number; measure: number; beat: number; pitch: number }
  | { type: 'movePitch'; track: number; measure: number; beat: number; pitch: number; to: number }
  | { type: 'setPitchTied'; track: number; measure: number; beat: number; pitch: number; tied: boolean }
  | { type: 'removeRests'; track: number; from: BeatAddress; to: BeatAddress }
  | { type: 'setClef'; track: number; clef: TrackClef }

/** A format a score can be exported to, besides the native one. */
export type ExportFormat = 'midi' | 'musicXml'

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

/**
 * A sound a pack plays, and the MIDI keys its recordings cover (General MIDI plays the others).
 * A new sound the pack declares has a `name` and the General MIDI `program` that plays it
 * without the pack; both are null for a sound of the built-in table.
 */
export type PackSound = { sound: SoundId; firstKey: number; lastKey: number; name: string | null; program: number | null }

/** A sound pack in use. Mirrors `PackInfo` in `src-tauri/src/service.rs`. */
export type SoundPack = {
  id: string
  name: string
  author: string
  /** SPDX identifier of the licence of the samples, such as `CC0-1.0`. */
  licence: string
  /** Where the samples come from. */
  source: string | null
  folder: string
  /** A sound whose sample file could not be used is not listed: it is in `refused`. */
  sounds: PackSound[]
}

/** A pack folder, or a file of a pack, that could not be used. */
export type RefusedPack = { folder: string; reason: string }

/** Mirrors `SoundPacks` in `src-tauri/src/service.rs`. */
export type SoundPacks = {
  /** Lowest priority first: when two packs play the same sound, the later one is heard. */
  packs: SoundPack[]
  refused: RefusedPack[]
  /** Where the user adds packs, when the app knows it. */
  userFolder: string | null
}

export const NO_SOUND_PACKS: SoundPacks = { packs: [], refused: [], userFolder: null }

/** The document lives in the desktop app. Rejected promises carry a message fit to show the user. */
export type DocumentApi = {
  /** False in a plain browser, where the demo can be shown but not changed. */
  canEdit: boolean
  startup(): Promise<OpenedDocument>
  newDocument(): Promise<DocumentState>
  open(path: string): Promise<OpenedDocument>
  /** Saves to `path`, or to the current file when null. */
  save(path: string | null): Promise<DocumentState>
  /** Writes the score in another format and resolves with the path written. */
  exportScore(args: { path: string; format: ExportFormat }): Promise<string>
  edit(edit: Edit): Promise<DocumentState>
  undo(): Promise<DocumentState>
  redo(): Promise<DocumentState>
  /** The sound packs found at start, and the ones refused. */
  soundPacks(): Promise<SoundPacks>
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
    exportScore: ({ path, format }) => call('export_file', { path, format }),
    edit: (edit) => call('edit_score', { edit }),
    undo: () => call('undo'),
    redo: () => call('redo'),
    soundPacks: () => call('sound_packs'),
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
    exportScore: needsDesktop,
    edit: needsDesktop,
    undo: needsDesktop,
    redo: needsDesktop,
    // Without the desktop app there is no audio engine, so no pack either.
    soundPacks: async () => NO_SOUND_PACKS,
  }
}
