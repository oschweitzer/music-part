import { getCurrentWebview } from '@tauri-apps/api/webview'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { ask, open, save } from '@tauri-apps/plugin-dialog'
import type { ExportFormat } from './document-api'
import { isTauri } from './score-source'

/** The questions the editor asks the user about files. Injected so that tests can answer them. */
export type FilePrompts = {
  /** Resolves with a path, or null when the user cancels. */
  chooseFileToOpen(): Promise<string | null>
  chooseFileToSave(defaultName: string): Promise<string | null>
  /** Where to write an export, or null if the user cancels. */
  chooseFileToExport(args: { defaultName: string; format: ExportFormat }): Promise<string | null>
  /** True if the user accepts to lose their unsaved changes. */
  confirmDiscard(): Promise<boolean>
}

const MIDI_EXTENSIONS = ['mid', 'midi']
const MUSICXML_EXTENSIONS = ['musicxml', 'xml', 'mxl']
const SCORE_EXTENSIONS = ['mpart', ...MIDI_EXTENSIONS, ...MUSICXML_EXTENSIONS]

export function isScoreFile(path: string): boolean {
  return /\.(mpart|midi?|musicxml|xml|mxl)$/i.test(path)
}

export function createFilePrompts(): FilePrompts {
  if (!isTauri()) {
    return {
      chooseFileToOpen: async () => null,
      chooseFileToSave: async () => null,
      chooseFileToExport: async () => null,
      confirmDiscard: async () => true,
    }
  }

  return {
    async chooseFileToOpen() {
      const chosen = await open({
        multiple: false,
        directory: false,
        filters: [
          { name: 'Scores', extensions: SCORE_EXTENSIONS },
          { name: 'Music Part scores', extensions: ['mpart'] },
          { name: 'MIDI files', extensions: MIDI_EXTENSIONS },
          { name: 'MusicXML files', extensions: MUSICXML_EXTENSIONS },
        ],
      })
      return typeof chosen === 'string' ? chosen : null
    },
    async chooseFileToSave(defaultName) {
      return save({ defaultPath: defaultName, filters: [{ name: 'Music Part scores', extensions: ['mpart'] }] })
    },
    async chooseFileToExport({ defaultName, format }) {
      const filter =
        format === 'midi'
          ? { name: 'MIDI files', extensions: MIDI_EXTENSIONS }
          : { name: 'MusicXML files', extensions: MUSICXML_EXTENSIONS }
      return save({ defaultPath: defaultName, filters: [filter] })
    },
    confirmDiscard() {
      return ask('You have unsaved changes. Discard them?', {
        title: 'Unsaved changes',
        kind: 'warning',
        okLabel: 'Discard',
        cancelLabel: 'Cancel',
      })
    },
  }
}

/** A file name safe on every system, from a score title. */
export function fileNameFor(title: string, extension = 'mpart'): string {
  const cleaned = title.replace(/[\\/:*?"<>|\u0000-\u001f]/g, '-').replace(/\s+/g, ' ').trim().replace(/^\.+/, '')
  return `${cleaned || 'Untitled'}.${extension}`
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

/** Asks `shouldClose` before the window closes; closing is cancelled when it resolves false. */
export async function guardWindowClose(shouldClose: () => Promise<boolean>): Promise<() => void> {
  if (!isTauri()) return () => {}

  return getCurrentWindow().onCloseRequested(async (event) => {
    if (!(await shouldClose())) event.preventDefault()
  })
}

export function setWindowTitle(title: string): void {
  if (isTauri()) void getCurrentWindow().setTitle(title)
}
