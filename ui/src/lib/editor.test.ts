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
    exportScore: vi.fn(async ({ path }: { path: string }) => path),
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
  const playback = { setScore: vi.fn(async (_score: Score) => {}), dropTrackMix: vi.fn((_track: number) => {}) }
  const filePrompts: FilePrompts = {
    chooseFileToOpen: async () => '/music/chosen.mpart',
    chooseFileToSave: async () => '/music/saved.mpart',
    chooseFileToExport: async () => '/music/exported.mid',
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
    expect(editor.selection).toEqual({ measure: 0, beat: 0, row: 1 })
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
      editor.select({ measure: 2, beat: 1, row: 4 })
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
      editor.select({ measure: 0, beat: 3, row: 2 })
      await editor.run({ type: 'remove' })

      expect(edits).toEqual([{ type: 'removeNote', track: 0, measure: 0, beat: 3, string: 2 }])
    })

    it('should set the duration of the selected beat, without dots', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 2, beat: 0, row: 1 }) // a dotted quarter in the demo
      await editor.run({ type: 'duration', value: 'Half' })

      expect(edits).toEqual([
        { type: 'setDuration', track: 0, measure: 2, beat: 0, duration: { value: 'Half', dots: 0 } },
      ])
    })

    it('should toggle the dot of the selected beat', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 0, beat: 0, row: 1 }) // an eighth
      await editor.run({ type: 'dot' })
      editor.select({ measure: 2, beat: 0, row: 1 }) // a dotted quarter
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
      editor.select({ measure: 1, beat: 2, row: 3 })
      await editor.insertMeasure()

      expect(edits).toEqual([{ type: 'insertMeasure', at: 2 }])
      expect(editor.selection).toEqual({ measure: 2, beat: 0, row: 3 })
    })

    it('should delete the selected measure and keep a selection that exists', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 5, beat: 0, row: 1 })
      await editor.deleteMeasure()

      expect(edits).toEqual([{ type: 'deleteMeasure', index: 5, count: 1 }])
      expect(editor.selection).not.toBeNull()
    })

    it('should delete every measure a range of beats touches and drop the range', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 1, beat: 0, row: 2 })
      editor.select({ measure: 3, beat: 0, row: 2 }, { extend: true })
      await editor.deleteMeasure()

      expect(edits).toEqual([{ type: 'deleteMeasure', index: 1, count: 3 }])
      expect(editor.selection).toEqual({ measure: 1, beat: 0, row: 2 })
    })

    it('should refuse to delete all the measures', async () => {
      const { api } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      const last = editor.document!.score.tracks[0]!.measures.length - 1
      editor.select({ measure: 0, beat: 0, row: 1 })
      expect(editor.canDeleteMeasures).toBe(true)
      editor.select({ measure: last, beat: 0, row: 1 }, { extend: true })
      expect(editor.canDeleteMeasures).toBe(false)
    })

    it('should insert a measure before the selection', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 2, beat: 1, row: 3 })
      await editor.insertMeasure({ before: true })

      expect(edits).toEqual([{ type: 'insertMeasure', at: 2 }])
      expect(editor.selection).toEqual({ measure: 2, beat: 0, row: 3 })
    })

    it('should insert a measure after the end of a range', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 1, beat: 0, row: 1 })
      editor.select({ measure: 2, beat: 0, row: 1 }, { extend: true })
      await editor.insertMeasure()

      expect(edits).toEqual([{ type: 'insertMeasure', at: 3 }])
    })

    it('should keep a range when the menu opens inside it, and select the spot otherwise', async () => {
      const { api } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 1, beat: 0, row: 1 })
      editor.select({ measure: 3, beat: 0, row: 1 }, { extend: true })

      editor.selectForMenu({ measure: 2, beat: 0, row: 4 })
      expect(editor.selectedMeasures).toEqual({ from: 1, count: 3 })

      editor.selectForMenu({ measure: 5, beat: 0, row: 4 })
      expect(editor.selectedMeasures).toEqual({ from: 5, count: 1 })
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

      expect(editor.selection).toEqual({ measure: 0, beat: 1, row: 2 })
      expect(edits).toEqual([])
    })
  })

  describe('ties, techniques, tracks and tuning', () => {
    it('should tie the selected note, then untie it when it already is', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 0, beat: 1, row: 4 })
      await editor.run({ type: 'tie' })

      expect(edits).toEqual([{ type: 'setTied', track: 0, measure: 0, beat: 1, string: 4, tied: true }])
    })

    it('should set a technique, and clear it when pressed again on the same technique', async () => {
      const marked = structuredClone(score)
      marked.tracks[0]!.measures[0]!.voices[0]!.beats[1]!.notes[0]!.technique = 'Bend'
      const { api, edits } = fakeApi({
        edit: async (edit) => {
          edits.push(edit)
          return state({ score: marked })
        },
      })
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 0, beat: 1, row: 4 })
      await editor.run({ type: 'technique', technique: 'Slide' })
      await editor.run({ type: 'technique', technique: 'Bend' })

      expect(edits.map((e) => (e.type === 'setTechnique' ? e.technique : null))).toEqual(['Slide', null])
    })

    it('should tell the user a rest has no tie or technique, and send nothing', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 0, beat: 1, row: 1 }) // the note of that beat is on string 4
      await editor.run({ type: 'tie' })

      expect(edits).toEqual([])
      expect(editor.message?.text).toMatch(/select a note/i)
    })

    it('should add a track and move to it', async () => {
      const longer = structuredClone(score)
      longer.tracks.push(structuredClone(score.tracks[0]!))
      const { api, edits } = fakeApi({
        edit: async (edit) => {
          edits.push(edit)
          return state({ score: longer })
        },
      })
      const { editor } = setup({ api })
      await editor.start()
      await editor.addTrack('Bass')

      expect(edits).toEqual([{ type: 'addTrack', instrument: 'Bass' }])
      expect(editor.track).toBe(score.tracks.length)
    })

    it('should add a track with the sound picked in the menu', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      await editor.addTrack('ElectricGuitar', 'electric-guitar/distortion')

      expect(edits).toEqual([{ type: 'addTrack', instrument: 'ElectricGuitar', sound: 'electric-guitar/distortion' }])
    })

    it('should remove the current track', async () => {
      const { api, edits } = fakeApi()
      const { editor, playback } = setup({ api })
      await editor.start()
      editor.selectTrack(1)
      await editor.removeTrack()

      expect(edits).toEqual([{ type: 'removeTrack', index: 1 }])
      expect(playback.dropTrackMix).toHaveBeenCalledWith(1)
    })

    it('should remove a track other than the current one when given its index', async () => {
      const { api, edits } = fakeApi()
      const { editor, playback } = setup({ api })
      await editor.start()
      await editor.removeTrack(0)

      expect(edits).toEqual([{ type: 'removeTrack', index: 0 }])
      expect(playback.dropTrackMix).toHaveBeenCalledWith(0)
    })

    it('should retune the current track from note names', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      await editor.setTuning('E4 B3 G3 D3 A2 D2')

      expect(edits).toEqual([{ type: 'setTuning', track: 0, tuning: [64, 59, 55, 50, 45, 38] }])
    })

    it('should explain a tuning it cannot read instead of sending it', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      await editor.setTuning('banana')

      expect(edits).toEqual([])
      expect(editor.message?.tone).toBe('error')
    })
  })

  describe('selecting in several tracks', () => {
    it('should move the selection to the track of the row that is clicked', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 1, beat: 0, row: 2 }, { track: 1 })
      await editor.run({ type: 'digit', digit: 5 })

      expect(editor.track).toBe(1)
      expect(edits).toEqual([{ type: 'setFret', track: 1, measure: 1, beat: 0, string: 2, fret: 5 }])
    })

    it('should not extend a range from one track into another', async () => {
      const { editor } = setup()
      await editor.start()
      editor.select({ measure: 0, beat: 0, row: 1 })
      editor.select({ measure: 0, beat: 2, row: 1 }, { extend: true, track: 1 })

      expect(editor.selection).toEqual({ measure: 0, beat: 2, row: 1 })
      expect(editor.track).toBe(1)
    })

    it('should keep a drag in the track it started in', async () => {
      const { editor } = setup()
      await editor.start()
      editor.dragSelect({ from: { measure: 0, beat: 0, row: 1 }, to: { measure: 0, beat: 1, row: 2 }, track: 1 })

      expect(editor.track).toBe(1)
      expect(editor.selection).toEqual({ measure: 0, beat: 1, row: 2, anchor: { measure: 0, beat: 0 }, anchorRow: 1 })
    })

    it('should paste into the track of the selection', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 0, beat: 0, row: 1 })
      await editor.run({ type: 'copy' })
      editor.select({ measure: 2, beat: 0, row: 1 }, { track: 1 })
      await editor.run({ type: 'paste' })

      expect(edits[0]).toMatchObject({ type: 'paste', track: 1, measure: 2, beat: 0 })
    })

    it('should move to the next and previous track with the keyboard, at the same moment', async () => {
      const { editor } = setup()
      await editor.start()
      editor.select({ measure: 2, beat: 0, row: 3 })
      await editor.run({ type: 'switchTrack', step: 1 })
      expect(editor.track).toBe(1)
      expect(editor.selection).toMatchObject({ measure: 2, beat: 0, row: 3 })

      await editor.run({ type: 'switchTrack', step: 1 })
      expect(editor.track).toBe(1)
      await editor.run({ type: 'switchTrack', step: -1 })
      expect(editor.track).toBe(0)
    })

    it('should select the spot of a right-click in another track, even inside the same beats', async () => {
      const { editor } = setup()
      await editor.start()
      editor.select({ measure: 0, beat: 0, row: 1 })
      editor.select({ measure: 0, beat: 3, row: 1 }, { extend: true })
      editor.selectForMenu({ measure: 0, beat: 1, row: 2 }, { track: 1 })

      expect(editor.track).toBe(1)
      expect(editor.selection).toEqual({ measure: 0, beat: 1, row: 2 })
    })
  })

  describe('selecting several beats, copying and pasting', () => {
    it('should copy the selected range, all strings, with its first tie cleared', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 0, beat: 1, row: 2 })
      await editor.run({ type: 'extend', direction: 'right' })
      await editor.run({ type: 'extend', direction: 'right' })
      await editor.run({ type: 'copy' })

      const first = score.tracks[0]!.measures[0]!.voices[0]!.beats
      expect(editor.clipboard).toEqual(first.slice(1, 4).map((b, i) => ({ ...b, notes: b.notes.map((n) => ({ ...n, tied: i === 0 ? false : n.tied })) })))
      expect(editor.message?.text).toBe('Copied 3 beats.')
      expect(edits).toEqual([])
    })

    it('should paste at the start of the selection, over what is there', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 0, beat: 0, row: 1 })
      await editor.run({ type: 'copy' })
      editor.select({ measure: 2, beat: 1, row: 3 })
      await editor.run({ type: 'paste' })

      expect(edits).toHaveLength(1)
      expect(edits[0]).toMatchObject({ type: 'paste', track: 0, measure: 2, beat: 1 })
      expect(editor.selection).toEqual({ measure: 2, beat: 1, row: 3 })
    })

    it('should paste a range starting from its first beat, whichever end was clicked last', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 0, beat: 0, row: 1 })
      await editor.run({ type: 'copy' })
      editor.select({ measure: 1, beat: 3, row: 1 })
      editor.select({ measure: 1, beat: 1, row: 1 }, { extend: true })
      await editor.run({ type: 'paste' })

      expect(edits[0]).toMatchObject({ type: 'paste', measure: 1, beat: 1 })
    })

    it('should say there is nothing to paste before anything is copied', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      await editor.run({ type: 'paste' })

      expect(edits).toEqual([])
      expect(editor.message?.text).toMatch(/copy some beats/i)
    })

    it('should cut by copying and emptying the range, keeping its rhythm', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 0, beat: 0, row: 1 })
      await editor.run({ type: 'extend', direction: 'right' })
      await editor.run({ type: 'cut' })

      expect(edits).toEqual([
        { type: 'clearNotes', track: 0, from: { measure: 0, beat: 0 }, to: { measure: 0, beat: 1 }, strings: null, pitches: null },
      ])
      expect(editor.clipboard).toHaveLength(2)
      expect(editor.clipboard?.[0]?.notes.length).toBeGreaterThan(0)
    })

    it('should clear the whole range with Delete when several beats are selected', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 0, beat: 0, row: 1 })
      await editor.run({ type: 'extend', direction: 'right' })
      await editor.run({ type: 'remove' })

      expect(edits.map((e) => e.type)).toEqual(['clearNotes'])
    })

    it('should select a rectangle by dragging, and clear only the notes inside it', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.dragSelect({ from: { measure: 2, beat: 0, row: 5 }, to: { measure: 2, beat: 0, row: 2 } })
      await editor.run({ type: 'remove' })

      expect(edits).toEqual([
        { type: 'clearNotes', track: 0, from: { measure: 2, beat: 0 }, to: { measure: 2, beat: 0 }, strings: { first: 2, last: 5 }, pitches: null },
      ])
    })

    it('should copy only the strings of a rectangle', async () => {
      const { api } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.dragSelect({ from: { measure: 2, beat: 0, row: 2 }, to: { measure: 2, beat: 1, row: 4 } })
      await editor.run({ type: 'copy' })

      const strings = editor.clipboard?.[0]?.notes.map((n) => ('string' in n ? n.string : null))
      expect(strings).toEqual([4, 3, 2])
    })

    it('should keep the strings of a rectangle when Shift+arrow extends it', async () => {
      const { api } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.dragSelect({ from: { measure: 0, beat: 0, row: 2 }, to: { measure: 0, beat: 1, row: 3 } })
      await editor.run({ type: 'extend', direction: 'right' })

      expect(editor.selection).toMatchObject({ measure: 0, beat: 2, anchor: { measure: 0, beat: 0 }, anchorRow: 2, row: 3 })
    })

    describe('deleting rests', () => {
      /** The demo with its second measure's first two beats emptied. */
      const withRests = (): Score => {
        const copy = structuredClone(score)
        const beats = copy.tracks[0]!.measures[1]!.voices[0]!.beats
        beats[0]!.notes = []
        beats[1]!.notes = []
        return copy
      }
      const restApi = () => fakeApi({ startup: async () => opened({ document: state({ score: withRests() }) }) })

      it('should take a rest out of the timeline when Delete is pressed on it', async () => {
        const { api, edits } = restApi()
        const { editor } = setup({ api })
        await editor.start()
        editor.select({ measure: 1, beat: 0, row: 3 })
        await editor.run({ type: 'remove' })

        expect(edits).toEqual([{ type: 'removeRests', track: 0, from: { measure: 1, beat: 0 }, to: { measure: 1, beat: 0 } }])
        expect(editor.selection).toEqual({ measure: 1, beat: 0, row: 3 })
      })

      it('should take out a whole range of rests at once', async () => {
        const { api, edits } = restApi()
        const { editor } = setup({ api })
        await editor.start()
        editor.select({ measure: 1, beat: 0, row: 1 })
        await editor.run({ type: 'extend', direction: 'right' })
        await editor.run({ type: 'remove' })

        expect(edits).toEqual([{ type: 'removeRests', track: 0, from: { measure: 1, beat: 0 }, to: { measure: 1, beat: 1 } }])
        expect(editor.selection).toEqual({ measure: 1, beat: 0, row: 1 })
      })

      it('should clear the notes of a mixed range and leave the rests for the next Delete', async () => {
        const { api, edits } = restApi()
        const { editor } = setup({ api })
        await editor.start()
        editor.select({ measure: 1, beat: 1, row: 1 })
        await editor.run({ type: 'extend', direction: 'right' })
        await editor.run({ type: 'remove' })

        expect(edits.map((e) => e.type)).toEqual(['clearNotes'])
      })
    })

    it('should still remove one note with Delete when a single beat is selected', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 0, beat: 0, row: 5 })
      await editor.run({ type: 'remove' })

      expect(edits.map((e) => e.type)).toEqual(['removeNote'])
    })
  })

  describe('exporting', () => {
    it('should ask where to write, export, and say where the file went', async () => {
      const chooseFileToExport = vi.fn(async (_args: { defaultName: string; format: string }) => '/music/song.mid')
      const { api } = fakeApi()
      const { editor } = setup({ api, prompts: { chooseFileToExport } })
      await editor.start()
      await editor.exportAs('midi')

      expect(chooseFileToExport).toHaveBeenCalledWith({ defaultName: 'Demo in A minor.mid', format: 'midi' })
      expect(api.exportScore).toHaveBeenCalledWith({ path: '/music/song.mid', format: 'midi' })
      expect(editor.message?.text).toBe('Exported to /music/song.mid.')
      expect(editor.isDirty).toBe(false)
    })

    it('should do nothing when the user cancels', async () => {
      const { api } = fakeApi()
      const { editor } = setup({ api, prompts: { chooseFileToExport: async () => null } })
      await editor.start()
      await editor.exportAs('musicXml')

      expect(api.exportScore).not.toHaveBeenCalled()
      expect(editor.message).toBeNull()
    })

    it('should show why an export failed', async () => {
      const { api } = fakeApi({ exportScore: async () => Promise.reject(new Error('Cannot export to /x: no space')) })
      const { editor } = setup({ api })
      await editor.start()
      await editor.exportAs('musicXml')

      expect(editor.message).toEqual({ tone: 'error', text: 'Cannot export to /x: no space' })
    })
  })

  describe('time signature, name and instrument', () => {
    it('should change the time signature of the selected measure', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 2, beat: 0, row: 1 })
      expect(editor.currentTimeSignature).toEqual({ numerator: 4, denominator: 4 })
      await editor.setTimeSignature({ numerator: 3, denominator: 4 })

      expect(edits).toEqual([{ type: 'setTimeSignature', measure: 2, numerator: 3, denominator: 4 }])
    })

    it('should rename the current track and change its instrument', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.selectTrack(1)
      await editor.renameTrack('Walking bass')
      await editor.setInstrument('Violin')

      expect(edits).toEqual([
        { type: 'renameTrack', track: 1, name: 'Walking bass' },
        { type: 'setInstrument', track: 1, instrument: 'Violin' },
      ])
    })

    it('should change the sound of the current track', async () => {
      const { api, edits } = fakeApi()
      const { editor } = setup({ api })
      await editor.start()
      editor.selectTrack(1)
      await editor.setSound('bass/slap')

      expect(edits).toEqual([{ type: 'setSound', track: 1, sound: 'bass/slap' }])
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
      editor.select({ measure: 0, beat: 1, row: 1 })
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
      editor.select({ measure: 3, beat: 1, row: 5 })
      await editor.undo()

      expect(api.undo).toHaveBeenCalled()
      expect(editor.selection).toEqual({ measure: 3, beat: 1, row: 5 })
    })

    it('should pull the selection back when the score became shorter', async () => {
      const shorter: Score = { ...score, tracks: score.tracks.map((t) => ({ ...t, measures: t.measures.slice(0, 2) })) }
      const api = fakeApi({ undo: async () => state({ score: shorter }) }).api
      const { editor } = setup({ api })
      await editor.start()
      editor.select({ measure: 5, beat: 0, row: 1 })
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
      expect(editor.selection).toEqual({ measure: 0, beat: 0, row: 1 })
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

  describe('a pitched track', () => {
    const whole = { value: 'Whole' as const, dots: 0 }
    const half = { value: 'Half' as const, dots: 0 }
    /** The demo with a third track, a piano: C4 E4 then a rest in its first measure. */
    const withPiano: Score = {
      ...score,
      tracks: [
        ...score.tracks,
        {
          name: 'Piano',
          instrument: 'Piano',
          sound: 'piano/grand',
          tuning: [],
          measures: score.tracks[0]!.measures.map((m, i) => ({
            time_signature: m.time_signature,
            voices: [
              {
                beats:
                  i === 0
                    ? [
                        { duration: half, notes: [{ pitch: 60, technique: null }, { pitch: 64, technique: null, tied: false }] },
                        { duration: half, notes: [] },
                      ]
                    : [{ duration: whole, notes: [] }],
              },
            ],
          })),
        },
      ],
    }

    async function onPiano(selection: { measure: number; beat: number; row: number }) {
      const { api, edits } = fakeApi({
        startup: async () => opened({ document: state({ score: withPiano }) }),
        edit: async (edit) => {
          edits.push(edit)
          return state({ score: withPiano, isDirty: true })
        },
      })
      const { editor } = setup({ api })
      await editor.start()
      editor.select(selection, { track: 2 })
      return { editor, edits }
    }

    it('should write the selected pitch with Enter, and the nearest named note with Shift and a letter', async () => {
      const { editor, edits } = await onPiano({ measure: 0, beat: 1, row: 62 })
      await editor.run({ type: 'enterNote' })
      await editor.run({ type: 'noteName', letter: 'A' })
      expect(edits).toEqual([
        { type: 'addPitch', track: 2, measure: 0, beat: 1, pitch: 62 },
        { type: 'addPitch', track: 2, measure: 0, beat: 1, pitch: 57 },
      ])
    })

    it('should move the selected note a semitone with up and down, the selection following it', async () => {
      const { editor, edits } = await onPiano({ measure: 0, beat: 0, row: 64 })
      await editor.run({ type: 'move', direction: 'up' })
      expect(edits).toEqual([{ type: 'movePitch', track: 2, measure: 0, beat: 0, pitch: 64, to: 65 }])
      expect(editor.selection?.row).toBe(65)
    })

    it('should move an empty spot of the staff a line or a space with up and down', async () => {
      const { editor, edits } = await onPiano({ measure: 0, beat: 1, row: 64 })
      await editor.run({ type: 'move', direction: 'up' })
      expect(editor.selection?.row).toBe(65)
      await editor.run({ type: 'move', direction: 'down' })
      await editor.run({ type: 'move', direction: 'down' })
      expect(editor.selection?.row).toBe(62)
      expect(edits).toEqual([])
    })

    it('should tie and delete the note at the selected pitch', async () => {
      const { editor, edits } = await onPiano({ measure: 0, beat: 0, row: 60 })
      await editor.run({ type: 'tie' })
      await editor.run({ type: 'remove' })
      expect(edits).toEqual([
        { type: 'setPitchTied', track: 2, measure: 0, beat: 0, pitch: 60, tied: true },
        { type: 'removePitch', track: 2, measure: 0, beat: 0, pitch: 60 },
      ])
    })

    it('should explain that digits do not write on a staff, and leave techniques to strings', async () => {
      const { editor, edits } = await onPiano({ measure: 0, beat: 0, row: 60 })
      await editor.run({ type: 'digit', digit: 3 })
      expect(editor.message?.text).toMatch(/staff/)
      await editor.run({ type: 'technique', technique: 'Bend' })
      expect(editor.message?.text).toMatch(/strings/)
      expect(edits).toEqual([])
    })

    it('should clear only the pitches of a dragged rectangle', async () => {
      const { editor, edits } = await onPiano({ measure: 0, beat: 0, row: 60 })
      editor.dragSelect({ from: { measure: 0, beat: 0, row: 59 }, to: { measure: 0, beat: 1, row: 62 }, track: 2 })
      await editor.run({ type: 'remove' })
      expect(edits).toEqual([
        { type: 'clearNotes', track: 2, from: { measure: 0, beat: 0 }, to: { measure: 0, beat: 1 }, strings: null, pitches: { low: 59, high: 62 } },
      ])
    })

    it('should start at middle C when moving from a fretted track to a pitched one', async () => {
      const { editor } = await onPiano({ measure: 0, beat: 0, row: 60 })
      editor.select({ measure: 1, beat: 0, row: 3 }, { track: 0 })
      editor.selectTrack(2)
      expect(editor.selection).toEqual({ measure: 1, beat: 0, row: 60 })
    })

    it('should select the string a pitch written on a guitar staff went to', async () => {
      const placed: Score = structuredClone(score)
      placed.tracks[0]!.measures[1]!.voices[0]!.beats[0]!.notes = [{ string: 3, fret: 2, technique: null }]
      const { api, edits } = fakeApi({
        edit: async (edit) => {
          edits.push(edit)
          return state({ score: placed, isDirty: true })
        },
      })
      const { editor } = setup({ api })
      await editor.start()
      await editor.enterPitch({ measure: 1, beat: 0, pitch: 57 })
      expect(edits).toEqual([{ type: 'addPitch', track: 0, measure: 1, beat: 0, pitch: 57 }])
      expect(editor.selection).toEqual({ measure: 1, beat: 0, row: 3 })
    })
  })
})
