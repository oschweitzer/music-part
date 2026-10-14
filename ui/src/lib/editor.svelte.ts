import type { DocumentApi, DocumentState, Edit, OpenedDocument } from './document-api'
import { fileNameFor, type FilePrompts } from './files'
import type { Command } from './keymap'
import type { Playback } from './playback.svelte'
import type { Beat, Instrument, Note, NoteValue, TimeSignature } from './score-types'
import { clampSelection, FIRST_NOTE, moveSelection, nextFret, type Selection } from './selection'
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

  select(selection: Selection): void {
    this.selection = selection
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
      case 'remove':
        return this.#withTarget((at) => ({ type: 'removeNote', ...at }))
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

  /** Adds a track after the others and shows it. */
  addTrack(instrument: Instrument): Promise<void> {
    const index = this.document?.score.tracks.length ?? 0
    return this.#send({ type: 'addTrack', instrument }, () => {
      this.track = index
      this.selection = FIRST_NOTE
    })
  }

  removeTrack(): Promise<void> {
    const index = this.track
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
