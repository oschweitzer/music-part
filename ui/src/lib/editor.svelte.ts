import type { DocumentApi, DocumentState, Edit, ExportFormat, OpenedDocument } from './document-api'
import { fileNameFor, type FilePrompts } from './files'
import type { Command } from './keymap'
import type { Playback } from './playback.svelte'
import { DEFAULT_PIECE, pieceOfDigit, stepPiece } from './drums'
import { isPercussionTrack, isPitchedTrack, noteKindOf } from './instruments'
import { notePitch, noteRow } from './notes'
import type { Beat, Instrument, Note, NoteValue, SoundId, TimeSignature, TrackClef } from './score-types'
import {
  canGrowPastEnd,
  clampSelection,
  DEFAULT_PITCH,
  extendSelection,
  FIRST_NOTE,
  firstSelection,
  isBeatSelected,
  moveSelection,
  moveToTrack,
  nextFret,
  selectedRange,
  selectedRows,
  type BeatPosition,
  type Selection,
} from './selection'
import { pitchOfLetter } from './staff-layout'
import { parseTuning } from './tuning'

export type Message = { tone: 'error' | 'info'; text: string }

const STAFF_HINT = 'This track is written on a staff: press Enter, Shift and a note name, or double-click the staff to write a note.'

const DRUM_HINT = 'This is a drum track: press 1 to 0 or pick a piece in the toolbar, or double-click the staff, to write a hit.'

/** The selected spot with its track. `row` is a string on a fretted track, a pitch on a pitched one, a drum piece on drums. */
type Where = { track: number; measure: number; beat: number; row: number }

/**
 * Turns what the user does into calls on the document, and keeps what the screen needs: the
 * selected beat and row, the current message, whether there is something to save.
 * The score itself is owned by the desktop app; this class only holds the latest copy it sent.
 */
export class Editor {
  document = $state<DocumentState | null>(null)
  /**
   * The track the selection is in: where keys, paste and the track fields act. With several tracks
   * on screen, selecting in another one moves it there.
   */
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

  /** The note under the selection: on the selected string, or at the selected pitch. */
  get currentNote(): Note | null {
    const row = this.selection?.row
    return this.currentBeat?.notes.find((n) => noteRow(n) === row) ?? null
  }

  /** True when the track of the selection stores pitches: rows are pitches, notes go on a staff. */
  get isPitched(): boolean {
    const track = this.document?.score.tracks[this.track]
    return track !== undefined && isPitchedTrack(track)
  }

  /** True when the track of the selection is a drum kit: rows are pieces, hits go on a percussion staff. */
  get isPercussion(): boolean {
    const track = this.document?.score.tracks[this.track]
    return track !== undefined && isPercussionTrack(track)
  }

  /** True when the rows of the track are the keys of its notes on a staff (pitches or drum pieces), not strings. */
  get #hasStaffRows(): boolean {
    return this.isPitched || this.isPercussion
  }

  /** What the rows of the current track are, to tell when a change of track changes their meaning. */
  get #rowKind(): string | null {
    const track = this.document?.score.tracks[this.track]
    return track ? noteKindOf(track.instrument) : null
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

  /**
   * Selects a spot, in `track` when given; with `extend`, selects the beats from the current spot to
   * it instead. A range stays in one track, so extending into another track selects the spot alone.
   */
  select(selection: Selection, { extend = false, track = this.track }: { extend?: boolean; track?: number } = {}): void {
    const current = track === this.track ? this.selection : null
    this.track = track
    this.selection =
      extend && current
        ? {
            ...selection,
            anchor: current.anchor ?? { measure: current.measure, beat: current.beat },
            ...(current.anchorRow === undefined ? {} : { anchorRow: current.anchorRow }),
          }
        : selection
    this.#typed = null
  }

  /**
   * Selects the rectangle dragged from one spot to another: the beats between, and the strings (or
   * pitches) between. With `byRows` false, whole beats. A drag stays in the track it started in.
   */
  dragSelect({ from, to, track = this.track, byRows = true }: { from: Selection; to: Selection; track?: number; byRows?: boolean }): void {
    this.track = track
    const anchor = { measure: from.measure, beat: from.beat }
    this.selection = byRows ? { ...to, anchor, anchorRow: from.row } : { ...to, anchor }
    this.#typed = null
  }

  selectTrack(track: number): void {
    const rows = this.#rowKind
    this.track = track
    // A string number means nothing as a pitch or a drum piece, and the other way round.
    if (this.document && this.selection && rows !== this.#rowKind) {
      this.selection = { ...firstSelection({ score: this.document.score, track }), measure: this.selection.measure }
    }
    this.#reselect()
  }

  /**
   * Writes a note at a pitch on a beat (a double-click on a staff) and selects it. On a fretted
   * track it goes on a free string, and the selection follows it there.
   */
  enterPitch({ track = this.track, measure, beat, pitch }: { track?: number; measure: number; beat: number; pitch: number }): Promise<void> {
    if (!this.#api.canEdit) return this.#send({ type: 'addPitch', track, measure, beat, pitch })
    return this.#enqueue(async () => {
      const state = await this.#api.edit({ type: 'addPitch', track, measure, beat, pitch })
      const target = state.score.tracks[track]
      const note = target?.measures[measure]?.voices[0]?.beats[beat]?.notes.find((n) => notePitch({ track: target, note: n }) === pitch)
      this.track = track
      this.selection = { measure, beat, row: note ? noteRow(note) : (this.selection?.row ?? 1) }
      this.#typed = null
      await this.#apply(state)
    })
  }

  /**
   * Strikes a drum piece on the selected beat, or takes it away when the beat has it already, and
   * selects it: the palette of the toolbar and the digit keys. Decided when its turn comes, so a
   * hit written just before is seen.
   */
  togglePiece(piece: number): Promise<void> {
    if (!this.#api.canEdit) {
      this.message = { tone: 'error', text: 'Editing needs the desktop app.' }
      return Promise.resolve()
    }
    return this.#enqueue(async () => {
      const at = this.#where()
      if (!at || !this.isPercussion) return
      const { row: _row, ...spot } = at
      const hasPiece = this.currentBeat?.notes.some((n) => noteRow(n) === piece) ?? false
      const state = await this.#api.edit({ type: hasPiece ? 'removePitch' : 'addPitch', ...spot, pitch: piece })
      this.selection = { measure: at.measure, beat: at.beat, row: piece }
      this.#typed = null
      await this.#apply(state)
    })
  }

  setTempo(bpm: number): Promise<void> {
    return this.#send({ type: 'setTempo', bpm })
  }

  /** Executes a keyboard or toolbar command. */
  run(command: Command): Promise<void> {
    switch (command.type) {
      case 'digit':
        return this.#typeFret(command.digit)
      case 'enterNote':
        return this.#enterSelectedPitch(() => this.selection?.row ?? null)
      case 'noteName':
        if (this.isPercussion) {
          this.message = { tone: 'info', text: DRUM_HINT }
          return Promise.resolve()
        }
        return this.#enterSelectedPitch(() => pitchOfLetter({ letter: command.letter, near: this.selection?.row ?? DEFAULT_PITCH }))
      case 'move':
        return this.#move(command.direction)
      case 'extend':
        return this.#extend(command.direction)
      case 'switchTrack':
        return this.#switchTrack(command.step)
      case 'copy':
        return this.copy()
      case 'cut':
        return this.cut()
      case 'paste':
        return this.paste()
      case 'remove':
        return this.#remove()
      case 'duration':
        return this.#setDuration(() => ({ value: command.value, dots: 0 }))
      case 'dot':
        return this.#setDuration((beat) => ({ value: beat.duration.value, dots: beat.duration.dots === 0 ? 1 : 0 }))
      case 'tie':
        return this.#withNote(({ row, ...at }, note) =>
          this.#hasStaffRows ? { type: 'setPitchTied', ...at, pitch: row, tied: !note.tied } : { type: 'setTied', ...at, string: row, tied: !note.tied },
        )
      case 'technique':
        if (this.#hasStaffRows) {
          this.message = { tone: 'info', text: 'Techniques are for notes played on strings.' }
          return Promise.resolve()
        }
        return this.#withNote(({ row, ...at }, note) => ({
          type: 'setTechnique',
          ...at,
          string: row,
          technique: note.technique === command.technique ? null : command.technique,
        }))
      case 'insertMeasure':
        return this.insertMeasure()
      case 'deleteMeasure':
        return this.deleteMeasure()
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

  /** The measures the selection touches: a range of beats covers every measure it passes through. */
  get selectedMeasures(): { from: number; count: number } | null {
    if (!this.selection) return null
    const range = selectedRange(this.selection)
    const from = range?.from.measure ?? this.selection.measure
    const to = range?.to.measure ?? from
    return { from, count: to - from + 1 }
  }

  /** A score keeps at least one measure, so the selection cannot cover all of them. */
  get canDeleteMeasures(): boolean {
    const measures = this.selectedMeasures
    return measures !== null && measures.count < this.#measureCount()
  }

  /** Inserts an empty measure after the selection (before it with `before`) and moves onto it. */
  insertMeasure({ before = false }: { before?: boolean } = {}): Promise<void> {
    const measures = this.selectedMeasures
    const at = measures ? (before ? measures.from : measures.from + measures.count) : this.#measureCount()
    return this.#send({ type: 'insertMeasure', at }, () => {
      this.selection = { measure: at, beat: 0, row: this.selection?.row ?? 1 }
    })
  }

  /** Deletes the selected measure, or every measure a range of beats touches, in every track. */
  deleteMeasure(): Promise<void> {
    const measures = this.selectedMeasures
    if (!measures) return Promise.resolve()
    return this.#send({ type: 'deleteMeasure', index: measures.from, count: measures.count }, () => {
      // The range is gone; the selection falls on the measure that took its place (or the last).
      this.selection = { measure: measures.from, beat: 0, row: this.selection?.row ?? 1 }
    })
  }

  /** Right-clicking inside the selection keeps it (the menu acts on all of it); elsewhere it selects the spot. */
  selectForMenu(target: Selection, { track = this.track }: { track?: number } = {}): void {
    const isInside = track === this.track && isBeatSelected({ selection: this.selection, measure: target.measure, beat: target.beat })
    if (!isInside) this.select(target, { track })
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

  /** Changes the sound of the current track, among the sounds of its instrument. */
  setSound(sound: SoundId): Promise<void> {
    return this.#send({ type: 'setSound', track: this.track, sound })
  }

  /** Changes the clef of the current track, a voice: the notes keep their pitches. */
  setClef(clef: TrackClef): Promise<void> {
    return this.#send({ type: 'setClef', track: this.track, clef })
  }

  /** Copies the selected beats (or the selected beat) of this track, all their notes. */
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
      () => (this.selection = { ...start, row: at.row }),
    )
  }

  /** Adds a track after the others and shows it, with the default sound of its instrument unless given one. */
  addTrack(instrument: Instrument, sound?: SoundId): Promise<void> {
    const index = this.document?.score.tracks.length ?? 0
    return this.#send(sound ? { type: 'addTrack', instrument, sound } : { type: 'addTrack', instrument }, () => {
      this.track = index
      const kind = noteKindOf(instrument)
      this.selection = kind === 'pitched' ? { ...FIRST_NOTE, row: DEFAULT_PITCH } : kind === 'percussion' ? { ...FIRST_NOTE, row: DEFAULT_PIECE } : FIRST_NOTE
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

  /** The beats of this track from one spot to another, first voice, across barlines. */
  #beatsBetween(from: BeatPosition, to: BeatPosition): Beat[] {
    const measures = this.document?.score.tracks[this.track]?.measures ?? []
    const beats: Beat[] = []
    for (let m = from.measure; m <= to.measure; m++) {
      const inMeasure = measures[m]?.voices[0]?.beats ?? []
      const first = m === from.measure ? from.beat : 0
      const last = m === to.measure ? to.beat : inMeasure.length - 1
      beats.push(...inMeasure.slice(first, last + 1))
    }
    return beats
  }

  /** The selected beats as a copy, with the first one untied; only the selected rows of a rectangle. */
  #selectedBeats(): Beat[] | null {
    if (!this.selection) return null
    const range = selectedRange(this.selection)
    const from = range?.from ?? { measure: this.selection.measure, beat: this.selection.beat }
    const rows = selectedRows(this.selection)

    const beats: Beat[] = structuredClone($state.snapshot(this.#beatsBetween(from, range?.to ?? from)))
    if (rows) {
      for (const beat of beats) beat.notes = beat.notes.filter((n) => noteRow(n) >= rows.first && noteRow(n) <= rows.last)
    }
    // A tie on the first beat points at a note that is not part of the copy.
    beats[0]?.notes.forEach((note) => (note.tied = false))
    return beats
  }

  /** Empties the selected beats, or only the selected rows of them; the rhythm stays. */
  #clearRange(): Promise<void> {
    const selection = this.selection
    if (!selection) return Promise.resolve()
    const spot = { measure: selection.measure, beat: selection.beat }
    const range = selectedRange(selection) ?? { from: spot, to: spot }
    return this.#send({ type: 'clearNotes', track: this.track, from: range.from, to: range.to, ...this.#rowSpans(selection) })
  }

  /** The rows of a rectangle as the strings or the pitches to clear. */
  #rowSpans(selection: Selection): { strings: { first: number; last: number } | null; pitches: { low: number; high: number } | null } {
    const rows = selectedRows(selection)
    if (!rows) return { strings: null, pitches: null }
    return this.#hasStaffRows ? { strings: null, pitches: { low: rows.first, high: rows.last } } : { strings: rows, pitches: null }
  }

  /**
   * Delete. It empties notes first; on rests it takes them out of the timeline, like deleting
   * a character of text (see `Edit::RemoveRests`). Decided when its turn comes, so a note typed
   * just before is seen.
   */
  #remove(): Promise<void> {
    if (!this.#api.canEdit) {
      this.message = { tone: 'error', text: 'Editing needs the desktop app.' }
      return Promise.resolve()
    }
    return this.#enqueue(async () => {
      const selection = this.selection
      if (!selection) return
      const spot = { measure: selection.measure, beat: selection.beat }
      const range = selectedRange(selection)
      const from = range?.from ?? spot
      const to = range?.to ?? spot
      const beats = this.#beatsBetween(from, to)
      const isAllRests = beats.length > 0 && beats.every((beat) => beat.notes.length === 0)

      if (isAllRests) {
        const state = await this.#api.edit({ type: 'removeRests', track: this.track, from, to })
        // What followed the rests now sits where they were.
        this.selection = { ...from, row: selection.row }
        await this.#apply(state)
      } else if (range) {
        await this.#apply(await this.#api.edit({ type: 'clearNotes', track: this.track, from, to, ...this.#rowSpans(selection) }))
      } else if (this.#hasStaffRows) {
        await this.#apply(await this.#api.edit({ type: 'removePitch', track: this.track, ...from, pitch: selection.row }))
      } else {
        await this.#apply(await this.#api.edit({ type: 'removeNote', track: this.track, ...from, string: selection.row }))
      }
    })
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

  #switchTrack(step: -1 | 1): Promise<void> {
    if (this.document) {
      const moved = moveToTrack({ score: this.document.score, track: this.track, selection: this.selection ?? FIRST_NOTE, step })
      this.track = moved.track
      this.selection = moved.selection
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
    if (!this.#api.canEdit) return this.#send(build(at, { pitch: at.row, technique: null, tied: false }))
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
    if (this.isPercussion) {
      const piece = pieceOfDigit(digit)
      return piece === null ? Promise.resolve() : this.togglePiece(piece)
    }
    if (this.isPitched) {
      this.message = { tone: 'info', text: STAFF_HINT }
      return Promise.resolve()
    }

    const cell = `${at.track}:${at.measure}:${at.beat}:${at.row}`
    const now = this.#now()
    const previous = this.#typed?.cell === cell ? this.#typed : null
    const fret = nextFret({ previous, digit, now })
    this.#typed = { fret, at: now, cell }
    const { row, ...beat } = at
    return this.#send({ type: 'setFret', ...beat, string: row, fret })
  }

  /** Writes a note at a pitch (a drum piece on drums) on the selected beat of a staff-only track, and selects it. */
  #enterSelectedPitch(pitchOf: () => number | null): Promise<void> {
    const at = this.#where()
    if (!at) return Promise.resolve()
    const pitch = pitchOf()
    if (!this.#hasStaffRows || pitch === null) return Promise.resolve()
    return this.enterPitch({ track: at.track, measure: at.measure, beat: at.beat, pitch })
  }

  /**
   * Up and down on a note of a pitched track move it a semitone (the selection follows it), on a
   * hit of a drum track to the next piece up or down the staff that the beat does not have; on an
   * empty spot they move the selection a step of the staff. Elsewhere the arrows move the selection.
   */
  #move(direction: 'left' | 'right' | 'up' | 'down'): Promise<void> {
    const at = this.#where()
    if (at && this.isPercussion && (direction === 'up' || direction === 'down') && this.currentNote) {
      const taken = this.currentBeat?.notes.map(noteRow) ?? []
      const to = stepPiece({ piece: at.row, direction: direction === 'up' ? 1 : -1, taken })
      if (to === null) return Promise.resolve()
      const { row, ...beat } = at
      this.#typed = null
      return this.#send({ type: 'movePitch', ...beat, pitch: row, to }, () => {
        if (this.selection) this.selection = { ...this.selection, row: to }
      })
    }
    if (at && this.isPitched && (direction === 'up' || direction === 'down') && this.currentNote) {
      const to = at.row + (direction === 'up' ? 1 : -1)
      if (to < 0 || to > 127) return Promise.resolve()
      const { row, ...beat } = at
      this.#typed = null
      return this.#send({ type: 'movePitch', ...beat, pitch: row, to }, () => {
        if (this.selection) this.selection = { ...this.selection, row: to }
      })
    }
    if (this.document) {
      const selection = this.selection ?? FIRST_NOTE
      // Past the last note the right arrow opens the next measure, as writing on does.
      if (direction === 'right' && this.canEdit && canGrowPastEnd({ score: this.document.score, track: this.track, selection })) {
        this.#typed = null
        return this.#growScore()
      }
      this.selection = moveSelection({ score: this.document.score, track: this.track, selection, direction })
    }
    this.#typed = null
    return Promise.resolve()
  }

  /**
   * Adds an empty measure at the end and selects its first beat. Decided when its turn comes, so a
   * key held down adds one measure: the presses behind it find the new, empty measure and stop.
   */
  #growScore(): Promise<void> {
    return this.#enqueue(async () => {
      const document = this.document
      const selection = this.selection
      if (!document || !selection || !canGrowPastEnd({ score: document.score, track: this.track, selection })) return
      const at = document.score.tracks[this.track]?.measures.length ?? 0
      const state = await this.#api.edit({ type: 'insertMeasure', at })
      this.selection = { measure: at, beat: 0, row: selection.row }
      this.#typed = null
      await this.#apply(state)
    })
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
    this.selection = firstSelection({ score: opened.document.score, track: 0 })
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
