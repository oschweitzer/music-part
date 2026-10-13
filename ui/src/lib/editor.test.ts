import { describe, expect, it, vi } from 'vitest'
import demo from '../../public/demo-score.json'
import type { DocumentApi, DocumentState, Edit, OpenedDocument } from './document-api'
import { Editor } from './editor.svelte'
import type { FilePrompts } from './files'
import type { Score } from './score-types'

const score = demo as Score

const state = (changes: Partial<DocumentState> = {}): DocumentState => ({
  score,
  path: null,
  isDirty: false,
  canUndo: false,
  canRedo: false,
  ...changes,
})

const opened = (changes: Partial<OpenedDocument> = {}): OpenedDocument => ({
  document: state(),
  skippedNotes: 0,
  notice: null,
  ...changes,
})

/** An API that records what it receives and answers with scripted states. */
function fakeApi(overrides: Partial<DocumentApi> = {}) {
  const edits: Edit[] = []
  const api: DocumentApi = {
    canEdit: true,
    startup: async () => opened(),
    newDocument: vi.fn(async () => state()),
    open: vi.fn(async () => opened()),
    save: vi.fn(async (path) => state({ path: path ?? '/music/current.mpart' })),
    edit: async (edit) => {
      edits.push(edit)
      return state({ isDirty: true, canUndo: true })
    },
    undo: vi.fn(async () => state()),
    redo: vi.fn(async () => state({ isDirty: true })),
    ...overrides,
  }
  return { api, edits }
}

function setup({
  api = fakeApi().api,
  prompts = {},
  now = () => 0,
}: { api?: DocumentApi; prompts?: Partial<FilePrompts>; now?: () => number } = {}) {
  const playback = { setScore: vi.fn(async (_score: Score) => {}) }
  const filePrompts: FilePrompts = {
    chooseFileToOpen: async () => '/music/chosen.mpart',
    chooseFileToSave: async () => '/music/saved.mpart',
    confirmDiscard: async () => true,
    ...prompts,
  }
  const editor = new Editor({ api, playback, prompts: filePrompts, now })
  return { editor, playback }
}

describe('Editor', () => {
  it('should start on the first note of the score the app gives it', async () => {
    const { editor, playback } = setup()
    await editor.start()

    expect(editor.document?.score.title).toBe(score.title)
    expect(editor.selection).toEqual({ measure: 0, beat: 0, string: 1 })
    expect(playback.setScore).toHaveBeenCalledWith(score)
  })

  it('should report what the file import left out, or why the file was not opened', async () => {
    const skipped = setup({ api: fakeApi({ startup: async () => opened({ skippedNotes: 3 }) }).api })
    await skipped.editor.start()
    expect(skipped.editor.message).toEqual({ tone: 'info', text: '3 notes could not be placed on the strings and were left out.' })

    const one = setup({ api: fakeApi({ startup: async () => opened({ skippedNotes: 1 }) }).api })
    await one.editor.start()
    expect(one.editor.message?.text).toBe('1 note could not be placed on the strings and was left out.')

    const refused = setup({ api: fakeApi({ startup: async () => opened({ notice: 'Cannot open song.mid' }) }).api })
    await refused.editor.start()
    expect(refused.editor.message).toEqual({ tone: 'error', text: 'Cannot open song.mid' })
  })

  describe('typing frets', () => {
    it('should set the fret of the selected string and beat', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 2, beat: 1, string: 4 })
      await editor.run({ type: 'digit', digit: 5 })

      expect(edits).toEqual([{ type: 'setFret', track: 0, measure: 2, beat: 1, string: 4, fret: 5 }])
    })

    it('should join two digits typed quickly on the same note', async () => {
      const { api, edits } = fakeApi()
      let time = 1000
      const { editor } = setup({ api, now: () => time })
      await editor.start()
      await editor.run({ type: 'digit', digit: 1 })
      time += 300
      await editor.run({ type: 'digit', digit: 2 })

      expect(edits.map((e) => (e.type === 'setFret' ? e.fret : null))).toEqual([1, 12])
    })

    it('should not join digits typed on different notes', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api, now: () => 1000 })
      await editor.start()
      await editor.run({ type: 'digit', digit: 1 })
      await editor.run({ type: 'move', direction: 'right' })
      await editor.run({ type: 'digit', digit: 2 })

      expect(edits.map((e) => (e.type === 'setFret' ? e.fret : null))).toEqual([1, 2])
    })

    it('should do nothing without a selection', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      await editor.run({ type: 'clearSelection' })
      await editor.run({ type: 'digit', digit: 3 })
      await editor.run({ type: 'remove' })

      expect(edits).toEqual([])
    })
  })

  describe('editing', () => {
    it('should remove the note of the selected string', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 0, beat: 3, string: 2 })
      await editor.run({ type: 'remove' })

      expect(edits).toEqual([{ type: 'removeNote', track: 0, measure: 0, beat: 3, string: 2 }])
    })

    it('should set the duration of the selected beat, without dots', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 2, beat: 0, string: 1 }) // a dotted quarter in the demo
      await editor.run({ type: 'duration', value: 'Half' })

      expect(edits).toEqual([
        { type: 'setDuration', track: 0, measure: 2, beat: 0, duration: { value: 'Half', dots: 0 } },
      ])
    })

    it('should toggle the dot of the selected beat', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 0, beat: 0, string: 1 }) // an eighth
      await editor.run({ type: 'dot' })
      editor.select({ measure: 2, beat: 0, string: 1 }) // a dotted quarter
      await editor.run({ type: 'dot' })

      expect(edits).toEqual([
        { type: 'setDuration', track: 0, measure: 0, beat: 0, duration: { value: 'Eighth', dots: 1 } },
        { type: 'setDuration', track: 0, measure: 2, beat: 0, duration: { value: 'Quarter', dots: 0 } },
      ])
    })

    it('should insert a measure after the selected one and select it', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 1, beat: 2, string: 3 })
      await editor.insertMeasure()

      expect(edits).toEqual([{ type: 'insertMeasure', at: 2 }])
      expect(editor.selection).toEqual({ measure: 2, beat: 0, string: 3 })
    })

    it('should delete the selected measure and keep a selection that exists', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 5, beat: 0, string: 1 })
      await editor.deleteMeasure()

      expect(edits).toEqual([{ type: 'deleteMeasure', index: 5 }])
      expect(editor.selection).not.toBeNull()
    })

    it('should change the tempo', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      await editor.setTempo(90)

      expect(edits).toEqual([{ type: 'setTempo', bpm: 90 }])
    })

    it('should move the selection without bothering the document', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      await editor.run({ type: 'move', direction: 'down' })
      await editor.run({ type: 'move', direction: 'right' })

      expect(editor.selection).toEqual({ measure: 0, beat: 1, string: 2 })
      expect(edits).toEqual([])
    })
  })

  describe('sending edits', () => {
    it('should send edits one at a time, in the order they were made', async () => {
      const order: string[] = []
      let releaseFirst: () => void = () => {}
      const firstDone = new Promise<void>((resolve) => (releaseFirst = resolve))
      const api = fakeApi({
        edit: async (edit) => {
          const label = edit.type === 'setFret' ? `fret ${edit.fret}` : edit.type
          order.push(`start ${label}`)
          if (label === 'fret 1') await firstDone
          order.push(`end ${label}`)
          return state({ isDirty: true })
        },
      }).api
      const { editor } = setup({ api, now: () => 0 })
      await editor.start()

      const first = editor.run({ type: 'digit', digit: 1 })
      editor.select({ measure: 0, beat: 1, string: 1 })
      const second = editor.run({ type: 'digit', digit: 2 })
      releaseFirst()
      await Promise.all([first, second])

      expect(order).toEqual(['start fret 1', 'end fret 1', 'start fret 2', 'end fret 2'])
    })

    it('should show a refused edit as a message and keep going afterwards', async () => {
      const api = fakeApi({
        edit: vi
          .fn()
          .mockRejectedValueOnce(new Error('There is not enough room left in the measure.'))
          .mockResolvedValue(state({ isDirty: true })),
      }).api
      const { editor } = setup({ api })
      await editor.start()
      await editor.run({ type: 'duration', value: 'Whole' })

      expect(editor.message).toEqual({ tone: 'error', text: 'There is not enough room left in the measure.' })
      expect(editor.document?.isDirty).toBe(false)

      await editor.run({ type: 'digit', digit: 3 })
      expect(editor.document?.isDirty).toBe(true)
    })

    it('should refuse to edit in a plain browser and say why', async () => {
      const { api, edits } = fakeApi({ canEdit: false })
      const { editor } = setup({ api })
      await editor.start()
      await editor.run({ type: 'digit', digit: 3 })

      expect(edits).toEqual([])
      expect(editor.message?.text).toContain('desktop app')
      expect(editor.canEdit).toBe(false)
    })

    it('should reload the playback after every change', async () => {
      const { editor, playback } = setup()
      await editor.start()
      await editor.run({ type: 'digit', digit: 3 })
      await editor.undo()

      expect(playback.setScore).toHaveBeenCalledTimes(3)
    })
  })

  describe('undo and redo', () => {
    it('should adopt the state the app returns and keep a selection that still exists', async () => {
      const { api } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 3, beat: 1, string: 5 })
      await editor.undo()

      expect(api.undo).toHaveBeenCalled()
      expect(editor.selection).toEqual({ measure: 3, beat: 1, string: 5 })
    })

    it('should pull the selection back when the score became shorter', async () => {
      const shorter: Score = { ...score, tracks: score.tracks.map((t) => ({ ...t, measures: t.measures.slice(0, 2) })) }
      const api = fakeApi({ undo: async () => state({ score: shorter }) }).api
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 5, beat: 0, string: 1 })
      await editor.undo()

      expect(editor.selection?.measure).toBe(1)
    })
  })

  describe('files', () => {
    it('should ask before discarding changes when opening a file, and stop if refused', async () => {
      const confirmDiscard = vi.fn(async () => false)
      const { api } = fakeApi({ startup: async () => opened({ document: state({ isDirty: true }) }) })
      const { editor } = setup({ api, prompts: { confirmDiscard } })
      await editor.start()
      await editor.openFile()

      expect(confirmDiscard).toHaveBeenCalled()
      expect(api.open).not.toHaveBeenCalled()
    })

    it('should not ask when there is nothing to lose', async () => {
      const confirmDiscard = vi.fn(async () => false)
      const { api } = fakeApi()
      const { editor } = setup({ api, prompts: { confirmDiscard } })
      await editor.start()
      await editor.openFile()

      expect(confirmDiscard).not.toHaveBeenCalled()
      expect(api.open).toHaveBeenCalledWith('/music/chosen.mpart')
    })

    it('should do nothing when the user cancels the file dialog', async () => {
      const { api } = fakeApi()
      const { editor } = setup({ api, prompts: { chooseFileToOpen: async () => null } })
      await editor.start()
      await editor.openFile()

      expect(api.open).not.toHaveBeenCalled()
    })

    it('should ask before discarding changes for a new score too', async () => {
      const confirmDiscard = vi.fn(async () => true)
      const { api } = fakeApi({ startup: async () => opened({ document: state({ isDirty: true }) }) })
      const { editor } = setup({ api, prompts: { confirmDiscard } })
      await editor.start()
      await editor.newDocument()

      expect(confirmDiscard).toHaveBeenCalled()
      expect(api.newDocument).toHaveBeenCalled()
      expect(editor.selection).toEqual({ measure: 0, beat: 0, string: 1 })
    })

    it('should save to the current file when there is one', async () => {
      const { api } = fakeApi({ startup: async () => opened({ document: state({ path: '/music/song.mpart', isDirty: true }) }) })
      const chooseFileToSave = vi.fn(async () => '/elsewhere.mpart')
      const { editor } = setup({ api, prompts: { chooseFileToSave } })
      await editor.start()
      await editor.save()

      expect(api.save).toHaveBeenCalledWith(null)
      expect(chooseFileToSave).not.toHaveBeenCalled()
      expect(editor.document?.isDirty).toBe(false)
    })

    it('should ask where to save a score that has no file yet, and propose its title', async () => {
      const { api } = fakeApi()
      const chooseFileToSave = vi.fn(async (_name: string) => '/music/new.mpart')
      const { editor } = setup({ api, prompts: { chooseFileToSave } })
      await editor.start()
      await editor.save()

      expect(chooseFileToSave).toHaveBeenCalledWith('Demo in A minor.mpart')
      expect(api.save).toHaveBeenCalledWith('/music/new.mpart')
      expect(editor.document?.path).toBe('/music/new.mpart')
    })

    it('should not save when the user cancels the save dialog', async () => {
      const { api } = fakeApi()
      const { editor } = setup({ api, prompts: { chooseFileToSave: async () => null } })
      await editor.start()
      await editor.saveAs()

      expect(api.save).not.toHaveBeenCalled()
    })

    it('should keep the unsaved state and show the reason when saving fails', async () => {
      const api = fakeApi({
        startup: async () => opened({ document: state({ path: '/ro/song.mpart', isDirty: true }) }),
        save: async () => {
          throw new Error('Cannot save /ro/song.mpart: permission denied')
        },
      }).api
      const { editor } = setup({ api })
      await editor.start()
      await editor.save()

      expect(editor.message?.text).toContain('permission denied')
      expect(editor.isDirty).toBe(true)
    })
  })
})
