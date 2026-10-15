import type { DocumentApi, DocumentState, Edit, ExportFormat, OpenedDocument } from './document-api'
import { fileNameFor, type FilePrompts } from './files'
import type { Command } from './keymap'
import type { Playback } from './playback.svelte'
import type { Beat, Instrument, Note, NoteValue, TimeSignature } from './score-types'
import {
  clampSelection,
  extendSelection,
  FIRST_NOTE,
  moveSelection,
  nextFret,
  selectedRange,
  type Selection,
} from './selection'
import { parseTuning } from './tuning'

export type Message = { tone: 'error' | 'info'; text: string }

type Where = { track: number; measure: number; beat: number; string: number }

/**
 * Turns what the user does into calls on the document, and keeps what the screen needs: the
 * selected beat and string, the current message, whether there is something to save.
 * The score itself is owned by the desktop app; this class only holds the latest copy it sent.
 */
export class Editor {
  document = $state<DocumentState | null>(null)
  track = $state(0)
  selection = $state<Selection | null>(null)
  message = $state<Message | null>(null)
  /** Beats copied with Copy or Cut, ready to paste. Kept when another score is opened. */
  clipboard = $state<Beat[] | null>(null)

  #api: DocumentApi
  #playback: Pick<Playback, 'setScore' | 'dropTrackMix'>
  #prompts: FilePrompts
  #now: () => number
  /** Edits are sent one at a time, in the order they were made. */
  #queue: Promise<unknown> = Promise.resolve()
  #typed: { fret: number; at: number; cell: string } | null = null

  constructor({
    api,
    playback,
    prompts,
    now = () => performance.now(),
  }: {
    api: DocumentApi
    playback: Pick<Playback, 'setScore' | 'dropTrackMix'>
    prompts: FilePrompts
    now?: () => number
  }) {
    this.#api = api
    this.#playback = playback
    this.#prompts = prompts
    this.#now = now
  }

  get canEdit(): boolean {
    return this.#api.canEdit && this.document !== null
  }

  get isDirty(): boolean {
    return this.document?.isDirty ?? false
  }

  /** The note under the selection, if the selected string has one in this beat. */
  get currentNote(): Note | null {
    const string = this.selection?.string
    return this.currentBeat?.notes.find((n) => n.string === string) ?? null
  }

  /** The beat under the selection, to show its duration. */
  get currentBeat(): Beat | null {
    const { document, selection, track } = this
    return document?.score.tracks[track]?.measures[selection?.measure ?? -1]?.voices[0]?.beats[selection?.beat ?? -1] ?? null
  }

  async start(): Promise<void> {
    await this.#guard(async () => this.#adopt(await this.#api.startup()))
  }

  dismissMessage(): void {
    this.message = null
  }

  /** Selects a spot; with `extend`, selects the beats from the current spot to it instead. */
  select(selection: Selection, { extend = false }: { extend?: boolean } = {}): void {
    const current = this.selection
    this.selection =
      extend && current
        ? { ...selection, anchor: current.anchor ?? { measure: current.measure, beat: current.beat } }
        : selection
    this.#typed = null
  }

  selectTrack(track: number): void {
    this.track = track
    this.#reselect()
  }

  setTempo(bpm: number): Promise<void> {
    return this.#send({ type: 'setTempo', bpm })
  }

  /** Executes a keyboard or toolbar command. */
  run(command: Command): Promise<void> {
    switch (command.type) {
      case 'digit':
        return this.#typeFret(command.digit)
      case 'move':
        return this.#move(command.direction)
      case 'extend':
        return this.#extend(command.direction)
      case 'copy':
        return this.copy()
      case 'cut':
        return this.cut()
      case 'paste':
        return this.paste()
      case 'remove':
        return selectedRange(this.selection) ? this.#clearRange() : this.#withTarget((at) => ({ type: 'removeNote', ...at }))
      case 'duration':
        return this.#setDuration(() => ({ value: command.value, dots: 0 }))
      case 'dot':
        return this.#setDuration((beat) => ({ value: beat.duration.value, dots: beat.duration.dots === 0 ? 1 : 0 }))
      case 'tie':
        return this.#withNote((at, note) => ({ type: 'setTied', ...at, tied: !note.tied }))
      case 'technique':
        return this.#withNote((at, note) => ({
          type: 'setTechnique',
          ...at,
          technique: note.technique === command.technique ? null : command.technique,
        }))
      case 'insertMeasure':
        return this.insertMeasure()
      case 'undo':
        return this.undo()
      case 'redo':
        return this.redo()
      case 'save':
        return this.save()
      case 'saveAs':
        return this.saveAs()
      case 'new':
        return this.newDocument()
      case 'open':
        return this.openFile()
      case 'clearSelection':
        this.selection = null
        return Promise.resolve()
      case 'togglePlayback':
      case 'rewind':
        return Promise.resolve()
    }
  }

  insertMeasure(): Promise<void> {
    const selection = this.selection
    const at = (selection?.measure ?? this.#measureCount() - 1) + 1
    return this.#send({ type: 'insertMeasure', at }, () => {
      this.selection = { ...clampSelection({ score: this.document!.score, track: this.track, selection }), measure: at, beat: 0 }
    })
  }

  deleteMeasure(): Promise<void> {
    const index = this.selection?.measure
    if (index === undefined) return Promise.resolve()
    return this.#send({ type: 'deleteMeasure', index })
  }

  /** The time signature of the selected measure (the first one when nothing is selected). */
  get currentTimeSignature(): TimeSignature | null {
    const measures = this.document?.score.tracks[this.track]?.measures
    return measures?.[this.selection?.measure ?? 0]?.time_signature ?? null
  }

  /** Changes the selected measure and the ones after it that share its time signature. */
  setTimeSignature({ numerator, denominator }: TimeSignature): Promise<void> {
    return this.#send({ type: 'setTimeSignature', measure: this.selection?.measure ?? 0, numerator, denominator })
  }

  renameTrack(name: string): Promise<void> {
    return this.#send({ type: 'renameTrack', track: this.track, name })
  }

  setInstrument(instrument: Instrument): Promise<void> {
    return this.#send({ type: 'setInstrument', track: this.track, instrument })
  }

  /** Copies the selected beats (or the selected beat) of this track, all strings. */
  copy(): Promise<void> {
    const beats = this.#selectedBeats()
    if (beats === null) return Promise.resolve()
    this.clipboard = beats
    this.message = { tone: 'info', text: `Copied ${beats.length} ${beats.length === 1 ? 'beat' : 'beats'}.` }
    return Promise.resolve()
  }

  /** Copies the selected beats, then clears them. */
  cut(): Promise<void> {
    return this.copy().then(() => this.#clearRange())
  }

  /** Writes the copied beats from the selected beat on, over what is there. */
  paste(): Promise<void> {
    const at = this.#where()
    if (!at) return Promise.resolve()
    if (!this.clipboard) {
      this.message = { tone: 'info', text: 'Nothing to paste yet: copy some beats first.' }
      return Promise.resolve()
    }
    const start = this.#rangeStart()
    return this.#send(
      { type: 'paste', track: at.track, measure: start.measure, beat: start.beat, beats: structuredClone($state.snapshot(this.clipboard)) },
      () => (this.selection = { ...start, string: at.string }),
    )
  }

  /** Adds a track after the others and shows it. */
  addTrack(instrument: Instrument): Promise<void> {
    const index = this.document?.score.tracks.length ?? 0
    return this.#send({ type: 'addTrack', instrument }, () => {
      this.track = index
      this.selection = FIRST_NOTE
    })
  }

  /** Removes a track, the current one by default. */
  removeTrack(index: number = this.track): Promise<void> {
    return this.#send({ type: 'removeTrack', index }, () => this.#playback.dropTrackMix(index))
  }

  /** Retunes the current track from note names such as "E4 B3 G3 D3 A2 D2". */
  setTuning(text: string): Promise<void> {
    const tuning = parseTuning(text)
    if (tuning === null) {
      this.message = {
        tone: 'error',
        text: 'Write the tuning as note names from the highest string to the lowest, for example "E4 B3 G3 D3 A2 E2".',
      }
      return Promise.resolve()
    }
    return this.#send({ type: 'setTuning', track: this.track, tuning })
  }

  undo(): Promise<void> {
    return this.#enqueue(async () => this.#apply(await this.#api.undo()))
  }

  redo(): Promise<void> {
    return this.#enqueue(async () => this.#apply(await this.#api.redo()))
  }

  save(): Promise<void> {
    if (!this.document) return Promise.resolve()
    if (this.document.path === null) return this.saveAs()
    return this.#enqueue(async () => this.#apply(await this.#api.save(null)))
  }

  async saveAs(): Promise<void> {
    if (!this.document) return
    const path = await this.#prompts.chooseFileToSave(fileNameFor(this.document.score.title))
    if (path === null) return
    await this.#enqueue(async () => this.#apply(await this.#api.save(path)))
  }

  /** Asks where to write the score in another format, and writes it. The score stays as it is. */
  async exportAs(format: ExportFormat): Promise<void> {
    if (!this.document) return
    const extension = format === 'midi' ? 'mid' : 'musicxml'
    const path = await this.#prompts.chooseFileToExport({
      defaultName: fileNameFor(this.document.score.title, extension),
      format,
    })
    if (path === null) return
    await this.#enqueue(async () => {
      const written = await this.#api.exportScore({ path, format })
      this.message = { tone: 'info', text: `Exported to ${written}.` }
    })
  }

  async newDocument(): Promise<void> {
    if (!(await this.canDiscard())) return
    await this.#enqueue(async () =>
      this.#adopt({ document: await this.#api.newDocument(), skippedNotes: 0, notice: null }),
    )
  }

  async openFile(): Promise<void> {
    if (!(await this.canDiscard())) return
    const path = await this.#prompts.chooseFileToOpen()
    if (path !== null) await this.#enqueue(async () => this.#adopt(await this.#api.open(path)))
  }

  /** Opens a known path, for a file dropped on the window. */
  async openPath(path: string): Promise<void> {
    if (!(await this.canDiscard())) return
    await this.#enqueue(async () => this.#adopt(await this.#api.open(path)))
  }

  /** True when there is nothing to lose, or the user agrees to lose it. */
  async canDiscard(): Promise<boolean> {
    return !this.isDirty || (await this.#prompts.confirmDiscard())
  }

  /** Where the selection starts: the first beat of a selected range, else the selected beat. */
  #rangeStart(): { measure: number; beat: number } {
    const range = selectedRange(this.selection)
    return range ? range.from : { measure: this.selection?.measure ?? 0, beat: this.selection?.beat ?? 0 }
  }

  /** The beats of the selection, from the first voice of this track, with the first one untied. */
  #selectedBeats(): Beat[] | null {
    const measures = this.document?.score.tracks[this.track]?.measures
    if (!measures || !this.selection) return null
    const range = selectedRange(this.selection)
    const from = range?.from ?? { measure: this.selection.measure, beat: this.selection.beat }
    const to = range?.to ?? from

    const beats: Beat[] = []
    for (let m = from.measure; m <= to.measure; m++) {
      const inMeasure = measures[m]?.voices[0]?.beats ?? []
      const first = m === from.measure ? from.beat : 0
      const last = m === to.measure ? to.beat : inMeasure.length - 1
      beats.push(...structuredClone($state.snapshot(inMeasure.slice(first, last + 1))))
    }
    // A tie on the first beat points at a note that is not part of the copy.
    beats[0]?.notes.forEach((note) => (note.tied = false))
    return beats
  }

  /** Replaces the selected beats with rests of the same lengths. */
  #clearRange(): Promise<void> {
    const at = this.#where()
    const beats = this.#selectedBeats()
    if (!at || !beats) return Promise.resolve()
    const start = this.#rangeStart()
    const rests = beats.map((beat) => ({ duration: beat.duration, notes: [] }))
    return this.#send({ type: 'paste', track: at.track, ...start, beats: rests }, () => (this.selection = { ...start, string: at.string }))
  }

  #extend(direction: 'left' | 'right'): Promise<void> {
    if (this.document) {
      this.selection = extendSelection({
        score: this.document.score,
        track: this.track,
        selection: this.selection ?? FIRST_NOTE,
        direction,
      })
    }
    this.#typed = null
    return Promise.resolve()
  }

  #measureCount(): number {
    return this.document?.score.tracks[this.track]?.measures.length ?? 0
  }

  #where(): Where | null {
    const { selection, track } = this
    return selection ? { track, ...selection } : null
  }

  #withTarget(build: (at: Where) => Edit): Promise<void> {
    const at = this.#where()
    return at ? this.#send(build(at)) : Promise.resolve()
  }

  /**
   * Looks at the note when the edit's turn comes, not when the key was pressed: a note typed just
   * before may not have come back from the desktop app yet.
   */
  #withNote(build: (at: Where, note: Note) => Edit): Promise<void> {
    const at = this.#where()
    if (!at) return Promise.resolve()
    if (!this.#api.canEdit) return this.#send(build(at, { string: at.string, fret: 0, technique: null, tied: false }))
    return this.#enqueue(async () => {
      const note = this.currentNote
      if (!note) {
        this.message = { tone: 'info', text: 'Select a note first: this applies to a note, not a rest.' }
        return
      }
      await this.#apply(await this.#api.edit(build(at, note)))
    })
  }

  #typeFret(digit: number): Promise<void> {
    const at = this.#where()
    if (!at) return Promise.resolve()

    const cell = `${at.track}:${at.measure}:${at.beat}:${at.string}`
    const now = this.#now()
    const previous = this.#typed?.cell === cell ? this.#typed : null
    const fret = nextFret({ previous, digit, now })
    this.#typed = { fret, at: now, cell }
    return this.#send({ type: 'setFret', ...at, fret })
  }

  #move(direction: 'left' | 'right' | 'up' | 'down'): Promise<void> {
    if (this.document) {
      this.selection = moveSelection({
        score: this.document.score,
        track: this.track,
        selection: this.selection ?? FIRST_NOTE,
        direction,
      })
    }
    this.#typed = null
    return Promise.resolve()
  }

  #setDuration(build: (beat: Beat) => { value: NoteValue; dots: number }): Promise<void> {
    const at = this.#where()
    const beat = this.currentBeat
    if (!at || !beat) return Promise.resolve()
    return this.#send({ type: 'setDuration', track: at.track, measure: at.measure, beat: at.beat, duration: build(beat) })
  }

  /** Sends an edit. `beforeApply` runs once the edit is accepted, to move the selection. */
  #send(edit: Edit, beforeApply?: () => void): Promise<void> {
    if (!this.#api.canEdit) {
      this.message = { tone: 'error', text: 'Editing needs the desktop app.' }
      return Promise.resolve()
    }
    return this.#enqueue(async () => {
      const state = await this.#api.edit(edit)
      beforeApply?.()
      await this.#apply(state)
    })
  }

  #enqueue(task: () => Promise<void>): Promise<void> {
    const next = this.#queue.then(() => this.#guard(task))
    this.#queue = next
    return next
  }

  /** A failed action shows its message and leaves everything else as it was. */
  async #guard(task: () => Promise<void>): Promise<void> {
    try {
      await task()
    } catch (error) {
      this.message = { tone: 'error', text: error instanceof Error ? error.message : String(error) }
    }
  }

  /** A different score: start on its first note and say what the user should know about it. */
  async #adopt(opened: OpenedDocument): Promise<void> {
    this.track = 0
    this.selection = FIRST_NOTE
    this.#typed = null
    this.message = null
    await this.#show(opened.document)

    if (opened.notice) {
      this.message = { tone: 'error', text: opened.notice }
    } else if (opened.skippedNotes > 0) {
      const count = opened.skippedNotes
      this.message = {
        tone: 'info',
        text: `${count} ${count === 1 ? 'note' : 'notes'} could not be placed on the strings and ${count === 1 ? 'was' : 'were'} left out.`,
      }
    }
  }

  /** The same score after a change: keep the selection if it still points at something. */
  async #apply(state: DocumentState): Promise<void> {
    await this.#show(state)
    this.#reselect()
  }

  async #show(state: DocumentState): Promise<void> {
    this.document = state
    await this.#playback.setScore(state.score)
  }

  #reselect(): void {
    if (!this.document) return
    this.track = Math.min(this.track, Math.max(this.document.score.tracks.length - 1, 0))
    this.selection = this.selection
      ? clampSelection({ score: this.document.score, track: this.track, selection: this.selection })
      : null
  }
}
