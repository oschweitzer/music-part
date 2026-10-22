// Each scenario starts from a fresh backend showing the demo score ("Demo in A minor":
// 6 measures; guitar measure 1 is eight eighth notes starting on string 5, fret 0).

import assert from 'node:assert/strict'
import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

import { sleep } from './browser.mjs'

// One quarter note (E4) at 480 ticks per quarter; the same bytes as the Rust tests use.
const ONE_NOTE_MIDI = [
  0x4d, 0x54, 0x68, 0x64, 0, 0, 0, 6, 0, 0, 0, 1, 0x01, 0xe0, 0x4d, 0x54, 0x72, 0x6b, 0, 0, 0, 13,
  0x00, 0x90, 0x40, 0x64, 0x83, 0x60, 0x80, 0x40, 0x00, 0x00, 0xff, 0x2f, 0x00,
]

const TICKS = { Whole: 128, Half: 64, Quarter: 32, Eighth: 16, Sixteenth: 8, ThirtySecond: 4 }
const ticksOf = ({ value, dots }) => TICKS[value] * (2 - 0.5 ** dots)

// ---- helpers -------------------------------------------------------------------------------

const frets = (page) =>
  page.eval(`[...document.querySelectorAll('svg.tab text.fret')].map(t => t.textContent)`)

/** Clicks in the middle of the n-th fret number on the page. */
async function clickFret(page, index) {
  const point = await page.eval(`(() => {
    const r = document.querySelectorAll('svg.tab text.fret')[${index}].getBoundingClientRect()
    return { x: r.x + r.width / 2, y: r.y + r.height / 2 }
  })()`)
  await page.mouseClick(point.x, point.y)
}

/** The selection square, in client coordinates, or null. */
const selection = (page) =>
  page.eval(`(() => {
    const r = document.querySelector('svg.tab .selection')?.getBoundingClientRect()
    return r ? { x: r.x + r.width / 2, y: r.y + r.height / 2 } : null
  })()`)

const text = (page, selector) => page.eval(`document.querySelector(${JSON.stringify(selector)})?.textContent?.trim() ?? null`)
const exists = (page, selector) => page.eval(`Boolean(document.querySelector(${JSON.stringify(selector)}))`)
const isDirty = (page) => exists(page, '.brand .dirty')

async function clickButton(page, label) {
  const found = await page.eval(`(() => {
    const b = [...document.querySelectorAll('button')].find(b => b.textContent.trim() === ${JSON.stringify(label)} || b.getAttribute('aria-label') === ${JSON.stringify(label)})
    b?.click()
    return Boolean(b)
  })()`)
  assert.ok(found, `no button "${label}"`)
  await sleep(120)
}

const document_ = async ({ backend }) => (await backend.request('startup')).document
const beatsOf = (doc, measure) => doc.score.tracks[0].measures[measure].voices[0].beats

const pressTimes = async (page, key, times) => {
  for (let i = 0; i < times; i++) await page.press(key)
}

/** Waits for the UI to have caught up with what the backend holds. */
const settle = () => sleep(150)

/**
 * Reloads the page and waits until `ready` holds on the new one. The marker keeps the wait from
 * being met by the old page before the reload takes effect, which let the next step act on a page
 * being torn down.
 */
async function reload(page, ready, what) {
  await page.eval(`window.__beforeReload = true; location.reload()`)
  await page.waitFor(`!window.__beforeReload && (${ready})`, { what })
}

/** Shows the tracks as rows, runs `steps`, and goes back to tabs so the next scenarios start as usual. */
async function inRows(page, steps) {
  try {
    await clickButton(page, 'Rows')
    await page.waitFor(`document.querySelectorAll('section.row').length > 0`, { what: 'the rows' })
    await steps()
  } finally {
    await page.eval(`localStorage.removeItem('music-part-view')`)
  }
}

/** Clicks in the middle of the first element matching `selector`. */
async function clickOn(page, selector) {
  const point = await page.eval(`(() => {
    const r = document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect()
    return { x: r.x + r.width / 2, y: r.y + r.height / 2 }
  })()`)
  await page.mouseClick(point.x, point.y)
}

const rowLabels = (page) => page.eval(`[...document.querySelectorAll('section.row')].map(r => r.getAttribute('aria-label'))`)
const selectedRow = (page) => page.eval(`document.querySelector('section.row.selected')?.getAttribute('aria-label') ?? null`)

/** Picks the track to edit by clicking its tab, or its row header in the rows view. */
const pickTrack = (page, index) =>
  page.eval(`(() => {
    const tab = document.querySelectorAll('.track-tab .name')[${index}]
    ;(tab ?? document.querySelectorAll('section.row header .name')[${index}]).click()
  })()`)

/** Adds a piano track (the third, after the demo's guitar and bass) and shows it. */
async function withPiano({ page, backend }) {
  await backend.request('edit_score', { edit: { type: 'addTrack', instrument: 'Piano' } })
  await reload(page, `document.querySelector('svg.tab line.string')`, 'the score to appear')
  await pickTrack(page, 2)
  await page.waitFor(`document.querySelector('svg.tab g.staff[data-clef="bass"]')`, { what: 'the grand staff' })
}

/**
 * The client point of a staff position on the first measure of the staff with `clef` (in `root`):
 * `position` steps above its bottom line, at the x of the `beat`th rest or note head of the measure.
 */
const staffPoint = (page, { clef, position, root = 'svg.tab' }) =>
  page.eval(`(() => {
    const staff = document.querySelector(${JSON.stringify(root)} + ' g.staff[data-clef="${clef}"]')
    const lines = [...staff.querySelectorAll('line.staff-line')].map(l => l.getBoundingClientRect().y)
    const bottom = Math.max(...lines)
    const gap = (bottom - Math.min(...lines)) / 4
    const mark = document.querySelector(${JSON.stringify(root)} + ' .staff-notes .staff-rest, ' + ${JSON.stringify(root)} + ' .staff-notes .head')
    const box = mark.getBoundingClientRect()
    return { x: box.x + box.width / 2, y: bottom - ${position} * gap / 2 }
  })()`)

const pianoNotes = async (context, measure = 0) => {
  const doc = await document_(context)
  return doc.score.tracks[2].measures[measure].voices[0].beats.map((b) => b.notes.map((n) => [n.pitch, n.tied === true]))
}

/** Sets how string instruments are written, and forgets it afterwards for the next scenarios. */
async function withFrettedView(page, label, steps) {
  try {
    await clickButton(page, label)
    await steps()
  } finally {
    await page.eval(`localStorage.removeItem('music-part-fretted-view')`)
  }
}

// ---- scenarios -----------------------------------------------------------------------------

export const scenarios = [
  [
    'shows the demo with its tracks and the file buttons enabled',
    async ({ page }) => {
      assert.match(await text(page, '.brand h1'), /Demo in A minor/)
      assert.equal(await page.eval(`document.querySelectorAll('.track-tab').length`), 2)
      assert.equal(await page.eval(`[...document.querySelectorAll('.file-button')].every(b => !b.disabled)`), true)
      assert.equal(await isDirty(page), false)
      assert.equal((await frets(page))[0], '0')
    },
  ],

  [
    'selects the note that is clicked, and the string that is clicked on an empty spot',
    async ({ page }) => {
      await clickFret(page, 0)
      const first = await selection(page)
      assert.ok(first, 'a selection appears')

      // The fret number sits exactly in the middle of the selection square.
      const fretCenter = await page.eval(`(() => { const r = document.querySelector('svg.tab text.fret').getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 } })()`)
      assert.ok(Math.abs(first.x - fretCenter.x) < 2 && Math.abs(first.y - fretCenter.y) < 3)

      // Same beat, on the string 4 lines below: nothing is written there, but it can be selected.
      const gap = await page.eval(`(() => { const ys = [...document.querySelectorAll('svg.tab line.string')].slice(0, 2).map(l => l.getBoundingClientRect().y); return ys[1] - ys[0] })()`)
      await page.mouseClick(first.x, first.y - gap)
      const above = await selection(page)
      assert.ok(Math.abs(above.x - first.x) < 2)
      assert.ok(Math.abs(above.y - (first.y - gap)) < 2)
    },
  ],

  [
    'typing a digit sets the fret and marks the score as unsaved',
    async (context) => {
      const { page } = context
      await clickFret(page, 0)
      await page.press('7')
      await settle()

      assert.equal((await frets(page))[0], '7')
      assert.equal(await isDirty(page), true)
      const note = beatsOf(await document_(context), 0)[0].notes[0]
      assert.deepEqual([note.string, note.fret], [5, 7])
      assert.equal(await page.eval(`!document.querySelector('[aria-label=Undo]').disabled`), true)
    },
  ],

  [
    'two digits typed quickly make one fret number',
    async (context) => {
      const { page } = context
      await clickFret(page, 0)
      await page.press('1')
      await page.press('2')
      await settle()

      assert.equal((await frets(page))[0], '12')
      assert.equal(beatsOf(await document_(context), 0)[0].notes[0].fret, 12)
    },
  ],

  [
    'a second digit typed too late starts a new number instead of joining the first',
    async (context) => {
      const { page } = context
      await clickFret(page, 0)
      await page.press('2')
      await sleep(1000) // too late to join the next digit
      await page.press('9')
      await settle()

      assert.equal((await frets(page))[0], '9')
      assert.equal(await text(page, '.notice'), null)
    },
  ],

  [
    'undo and redo restore the note and the unsaved mark',
    async ({ page }) => {
      await clickFret(page, 0)
      await page.press('7')
      await settle()
      assert.equal((await frets(page))[0], '7')

      await page.press('z', ['ctrl'])
      await settle()
      assert.equal((await frets(page))[0], '0')
      assert.equal(await isDirty(page), false, 'back to the opened state is clean')

      await page.press('z', ['ctrl', 'shift'])
      await settle()
      assert.equal((await frets(page))[0], '7')
      assert.equal(await isDirty(page), true)
    },
  ],

  [
    'the arrow keys move the selection along beats and strings',
    async ({ page }) => {
      await clickFret(page, 0)
      const start = await selection(page)
      const gap = await page.eval(`(() => { const ys = [...document.querySelectorAll('svg.tab line.string')].slice(0, 2).map(l => l.getBoundingClientRect().y); return ys[1] - ys[0] })()`)

      await page.press('ArrowRight')
      const right = await selection(page)
      assert.ok(right.x > start.x && Math.abs(right.y - start.y) < 1)

      await page.press('ArrowDown')
      const down = await selection(page)
      assert.ok(Math.abs(down.y - (start.y + gap)) < 1, `moved ${down.y - start.y}, expected ${gap}`)

      await page.press('ArrowUp')
      await page.press('ArrowLeft')
      const back = await selection(page)
      assert.ok(Math.abs(back.x - start.x) < 1 && Math.abs(back.y - start.y) < 1)
    },
  ],

  [
    'delete removes the note and leaves the beat as a rest',
    async (context) => {
      const { page } = context
      const before = (await frets(page)).length
      await clickFret(page, 0)
      await page.press('Delete')
      await settle()

      assert.equal((await frets(page)).length, before - 1)
      const beat = beatsOf(await document_(context), 0)[0]
      assert.deepEqual(beat.notes, [])
      assert.equal(ticksOf(beat.duration), 16, 'the beat keeps its length')
    },
  ],

  [
    'a duration key reshapes the measure and it stays full',
    async (context) => {
      const { page } = context
      await clickFret(page, 0)
      await page.press('q')
      await settle()

      const beats = beatsOf(await document_(context), 0)
      assert.equal(beats[0].duration.value, 'Quarter')
      assert.equal(beats.length, 7, 'the quarter swallowed the next eighth note')
      assert.equal(beats.reduce((sum, b) => sum + ticksOf(b.duration), 0), 128)
      assert.equal(await page.eval(`document.querySelector('[title^="Quarter note"]').getAttribute('aria-pressed')`), 'true')
      await page.screenshot(join(context.out, 'duration-changed.png'))
    },
  ],

  [
    'lengthening a note past the end of its measure explains why it cannot',
    async (context) => {
      const { page } = context
      // The last beat of measure 4 is a half note: a whole note cannot fit after it.
      await pressTimes(page, 'ArrowRight', 22)
      await page.press('w')
      await settle()

      assert.match(await text(page, '.notice span'), /not enough room/)
      assert.equal(await isDirty(page), false)
      await clickButton(page, 'Dismiss')
      assert.equal(await exists(page, '.notice'), false)
    },
  ],

  [
    'the measure buttons insert and delete measures for every track',
    async (context) => {
      const { page } = context
      const count = async () => (await document_(context)).score.tracks.map((t) => t.measures.length)
      assert.deepEqual(await count(), [6, 6])

      await clickFret(page, 0)
      await clickButton(page, '+ Measure')
      assert.deepEqual(await count(), [7, 7])
      assert.equal(await isDirty(page), true)

      await clickButton(page, '− Measure')
      assert.deepEqual(await count(), [6, 6])
    },
  ],

  [
    'the tempo field changes the tempo and the length of the piece',
    async (context) => {
      const { page } = context
      await page.eval(`(() => { const i = document.querySelector('.tempo input'); i.value = '75'; i.dispatchEvent(new Event('change', { bubbles: true })) })()`)
      await settle()

      assert.equal((await document_(context)).score.tempo_bpm, 75)
      // 6 measures of 4 beats at 75 bpm last 19.2 seconds.
      assert.match(await text(page, '.clock'), /0:19$/)
    },
  ],

  [
    'saving writes a score file, remembers it and clears the unsaved mark',
    async (context) => {
      const { page, out } = context
      await clickFret(page, 0)
      await page.press('7')
      await settle()

      const target = join(out, 'saved-song')
      await page.eval(`window.__e2e.answers.save.push(${JSON.stringify(target)})`)
      await page.press('s', ['ctrl'])
      await settle()

      const file = `${target}.mpart`
      assert.ok(existsSync(file), 'the file exists, with the extension added')
      const saved = JSON.parse(readFileSync(file, 'utf8'))
      assert.deepEqual([saved.format, saved.version], ['music-part', 1])
      assert.equal(saved.score.tracks[0].measures[0].voices[0].beats[0].notes[0].fret, 7)

      assert.equal(await isDirty(page), false)
      assert.equal((await document_(context)).path, file)

      // Saving again goes to the same file without asking.
      await page.press('8')
      await settle()
      await page.press('s', ['ctrl'])
      await settle()
      assert.equal(JSON.parse(readFileSync(file, 'utf8')).score.tracks[0].measures[0].voices[0].beats[0].notes[0].fret, 8)
    },
  ],

  [
    'the window title shows the score and whether it has unsaved changes',
    async ({ page }) => {
      await settle()
      assert.equal(await page.eval(`window.__e2e.titles.at(-1)`), 'Demo in A minor — Music Part')
      await clickFret(page, 0)
      await page.press('3')
      await settle()
      assert.equal(await page.eval(`window.__e2e.titles.at(-1)`), '• Demo in A minor — Music Part')
    },
  ],

  [
    'opening a saved score brings back its edits',
    async (context) => {
      const { page, backend, out } = context
      await backend.request('edit_score', { edit: { type: 'setFret', track: 0, measure: 0, beat: 0, string: 5, fret: 9 } })
      const file = join(out, 'reopen.mpart')
      await backend.request('save_file', { path: file })

      await context.fresh()
      assert.equal((await frets(page))[0], '0', 'a new backend shows the demo again')

      await page.eval(`window.__e2e.answers.open.push(${JSON.stringify(file)})`)
      await clickButton(page, 'Open')
      await settle()
      assert.equal((await frets(page))[0], '9')
      assert.equal(await isDirty(page), false)
    },
  ],

  [
    'unsaved changes are not lost without asking',
    async (context) => {
      const { page } = context
      await clickFret(page, 0)
      await page.press('7')
      await settle()

      await page.eval(`window.__e2e.answers.ask.push(false)`)
      await clickButton(page, 'New')
      await settle()
      assert.equal(await page.eval(`window.__e2e.asked.length`), 1)
      assert.match(await page.eval(`window.__e2e.asked[0]`), /unsaved/i)
      assert.match(await text(page, '.brand h1'), /Demo in A minor/, 'declining keeps the score')
      assert.equal(await isDirty(page), true)

      await page.eval(`window.__e2e.answers.ask.push(true)`)
      await clickButton(page, 'New')
      await settle()
      assert.match(await text(page, '.brand h1'), /Untitled/)
      assert.equal(await isDirty(page), false)
    },
  ],

  [
    'a new score is blank and can be written in right away',
    async (context) => {
      const { page } = context
      await clickButton(page, 'New')
      await settle()
      assert.deepEqual(await frets(page), [])

      await page.press('5')
      await page.press('ArrowRight')
      await settle()
      assert.deepEqual(await frets(page), ['5'])
      assert.equal(beatsOf(await document_(context), 0)[0].notes[0].string, 1)
    },
  ],

  [
    'a file that cannot be opened is explained and the score stays',
    async ({ page }) => {
      await page.eval(`window.__e2e.answers.open.push('/nonexistent/lost.mpart')`)
      await clickButton(page, 'Open')
      await settle()

      assert.match(await text(page, '.notice span'), /lost\.mpart/)
      assert.match(await text(page, '.brand h1'), /Demo in A minor/)
    },
  ],

  [
    'a MIDI file opens under the name of the file, drums as a drum track, and one without notes is refused',
    async (context) => {
      const { page, out } = context
      const good = join(out, 'one-note.mid')
      writeFileSync(good, Buffer.from(ONE_NOTE_MIDI))
      await page.eval(`window.__e2e.answers.open.push(${JSON.stringify(good)})`)
      await clickButton(page, 'Open')
      await settle()
      assert.match(await text(page, '.brand h1'), /one-note/)
      assert.deepEqual(await frets(page), ['0'])

      const drums = Buffer.from(ONE_NOTE_MIDI)
      drums[23] = 0x99 // percussion channel: E4 is the low conga
      drums[28] = 0x89
      const drumFile = join(out, 'drums.mid')
      writeFileSync(drumFile, drums)
      await page.eval(`window.__e2e.answers.open.push(${JSON.stringify(drumFile)})`)
      await clickButton(page, 'Open')
      await settle()
      assert.match(await text(page, '.brand h1'), /drums/)
      assert.ok(await exists(page, 'svg.tab g.staff[data-clef="percussion"]'), 'a drum track')

      const silent = Buffer.from(ONE_NOTE_MIDI)
      silent[25] = 0 // a note-on of velocity 0 is a note-off
      const bad = join(out, 'silent.mid')
      writeFileSync(bad, silent)
      await page.eval(`window.__e2e.answers.open.push(${JSON.stringify(bad)})`)
      await clickButton(page, 'Open')
      await settle()
      assert.match(await text(page, '.notice span'), /no notes/)
    },
  ],

  [
    'the app started on a file opens that file',
    async (context) => {
      const { page, backend, out } = context
      const file = join(out, 'startup.mpart')
      await backend.request('save_file', { path: file })
      await context.fresh(file)

      assert.match(await text(page, '.brand h1'), /Demo in A minor/)
      assert.equal(await isDirty(page), false)
      await context.fresh(join(out, 'missing.mpart')) // not a file that exists: ignored
      assert.match(await text(page, '.brand h1'), /Demo in A minor/)
    },
  ],

  [
    'L ties the note to the one before it and a letter sets a technique',
    async (context) => {
      const { page } = context
      await clickFret(page, 0)
      await page.press('ArrowRight') // beat 1 holds a note on string 4; string 5 is free
      await page.press('3')
      await page.press('l')
      await settle()

      let note = beatsOf(await document_(context), 0)[1].notes.find((n) => n.string === 5)
      assert.deepEqual([note.tied, note.fret], [true, 0], 'the tie copies the fret of the note it continues')
      assert.equal(await exists(page, 'svg.tab path.tie'), true)

      await page.press('b')
      await settle()
      note = beatsOf(await document_(context), 0)[1].notes.find((n) => n.string === 5)
      assert.equal(note.technique, 'Bend')
      await page.press('b') // the same key again clears it
      await settle()
      note = beatsOf(await document_(context), 0)[1].notes.find((n) => n.string === 5)
      assert.equal(note.technique, null)
    },
  ],

  [
    'a tie on a string with no note before it is refused with a message',
    async (context) => {
      const { page } = context
      await clickFret(page, 1) // beat 1, string 4: the beat before has its note on string 5
      await page.press('l')
      await settle()

      assert.match(await text(page, '.notice span'), /no note to tie/)
      assert.equal(await isDirty(page), false)
    },
  ],

  [
    'a track can be added, retuned and removed',
    async (context) => {
      const { page } = context
      await page.eval(`document.querySelector('.add-track summary').click()`)
      await clickButton(page, 'Electric bass: Finger')
      assert.equal((await document_(context)).score.tracks.length, 3)
      assert.equal((await document_(context)).score.tracks[2].sound, 'bass/finger')
      assert.equal(await page.eval(`document.querySelector('.track-tab.active').textContent.includes('Bass')`), true)

      await page.eval(`(() => { const i = document.querySelector('.tuning input'); i.value = 'G2 D2 A1 D1'; i.dispatchEvent(new Event('change', { bubbles: true })) })()`)
      await settle()
      assert.deepEqual((await document_(context)).score.tracks[2].tuning, [43, 38, 33, 26])
      assert.equal(await page.eval(`document.querySelector('.tuning input').value`), 'G2 D2 A1 D1')

      // A tuning that is not note names is explained and leaves the track as it was.
      await page.eval(`(() => { const i = document.querySelector('.tuning input'); i.value = 'banana'; i.dispatchEvent(new Event('change', { bubbles: true })) })()`)
      await settle()
      assert.match(await text(page, '.notice span'), /note names/)
      assert.equal(await page.eval(`document.querySelector('.tuning input').value`), 'G2 D2 A1 D1')

      // The cross on the tab of the track.
      await page.eval(`document.querySelector('.track-tab.active .drop').click()`)
      await settle()
      assert.equal((await document_(context)).score.tracks.length, 2)
      assert.equal(await page.eval(`document.querySelectorAll('.track-tab').length`), 2)
    },
  ],

  [
    'mute and solo buttons tell the engine which tracks sound',
    async ({ page }) => {
      const lastMix = () =>
        page.eval(`window.__e2e.audio.filter(c => c.command === 'set_audible').at(-1)?.args.audible`)
      assert.deepEqual(await lastMix(), [true, true], 'everything sounds when the score loads')

      await clickButton(page, 'Mute Guitar')
      assert.deepEqual(await lastMix(), [false, true])
      assert.equal(await page.eval(`document.querySelector('[aria-label="Mute Guitar"]').getAttribute('aria-pressed')`), 'true')

      await clickButton(page, 'Solo Guitar')
      assert.deepEqual(await lastMix(), [true, false], 'a soloed track plays alone')
      await clickButton(page, 'Solo Guitar')
      await clickButton(page, 'Mute Guitar')
      assert.deepEqual(await lastMix(), [true, true])
    },
  ],

  [
    'clicking above a measure moves playback to its start and Home goes back to the beginning',
    async ({ page }) => {
      const seeks = () => page.eval(`window.__e2e.audio.filter(c => c.command === 'seek').map(c => c.args.tick)`)
      const selectedBefore = await selection(page)
      const point = await page.eval(`(() => {
        const r = document.querySelectorAll('svg.tab .seek-strip')[2].getBoundingClientRect()
        return { x: r.x + r.width / 2, y: r.y + r.height / 2 }
      })()`)
      await page.mouseClick(point.x, point.y)
      await settle()

      assert.deepEqual(await seeks(), [2 * 128], 'measure 3 starts after two whole-note measures')
      assert.equal(await exists(page, '.notice'), false)
      assert.deepEqual(await selection(page), selectedBefore, 'the click selected no note')

      await page.press('Home')
      await settle()
      assert.deepEqual(await seeks(), [2 * 128, 0])
    },
  ],

  [
    'the time signature field reshapes the selected measure and the ones that share it',
    async (context) => {
      const { page } = context
      await clickFret(page, 0)
      await page.eval(`(() => { const i = document.querySelector('.time-signature input'); i.value = '3'; i.dispatchEvent(new Event('change', { bubbles: true })) })()`)
      await settle()

      const doc = await document_(context)
      const signatures = doc.score.tracks[0].measures.map((m) => `${m.time_signature.numerator}/${m.time_signature.denominator}`)
      assert.deepEqual(signatures, Array(6).fill('3/4'), 'all six 4/4 measures share it, so all change')
      assert.deepEqual(doc.score.tracks[1].measures.map((m) => m.time_signature.numerator), Array(6).fill(3), 'the bass follows')
      for (const m of doc.score.tracks[0].measures) {
        assert.equal(m.voices[0].beats.reduce((sum, b) => sum + ticksOf(b.duration), 0), 96)
      }
      // 3/4 of 6 measures at 100 bpm: 18 beats, 10.8 s.
      assert.match(await text(page, '.clock'), /0:10$/)
    },
  ],

  [
    'a track can be renamed and given another instrument',
    async (context) => {
      const { page } = context
      await page.eval(`(() => { const i = document.querySelector('input[aria-label="Name of this track"]'); i.value = '  Lead  '; i.dispatchEvent(new Event('change', { bubbles: true })) })()`)
      await settle()
      assert.equal((await document_(context)).score.tracks[0].name, 'Lead')
      assert.match(await text(page, '.track-tab.active .name'), /^Lead/)

      await page.eval(`(() => { const i = document.querySelector('input[aria-label="Name of this track"]'); i.value = '   '; i.dispatchEvent(new Event('change', { bubbles: true })) })()`)
      await settle()
      assert.match(await text(page, '.notice span'), /name must be/)
      assert.equal(await page.eval(`document.querySelector('input[aria-label="Name of this track"]').value`), 'Lead')

      await page.eval(`(() => { const s = document.querySelector('select[aria-label="Instrument of this track"]'); s.value = 'Violin'; s.dispatchEvent(new Event('change', { bubbles: true })) })()`)
      await settle()
      assert.equal((await document_(context)).score.tracks[0].instrument, 'Violin')
      assert.equal((await document_(context)).score.tracks[0].sound, 'violin/standard')
    },
  ],

  [
    'a track can be given another sound of its instrument, and the menu adds a track with a sound',
    async (context) => {
      const { page } = context
      const soundSelect = `document.querySelector('select[aria-label="Sound of this track"]')`
      const choose = async (label, value) => {
        await page.eval(`(() => { const s = document.querySelector('select[aria-label="${label}"]'); s.value = ${JSON.stringify(value)}; s.dispatchEvent(new Event('change', { bubbles: true })) })()`)
        await settle()
      }
      const options = () => page.eval(`[...${soundSelect}.options].map(o => o.value).join(' ')`)

      // The nylon guitar of the demo offers the sounds of an acoustic guitar only.
      assert.equal(await page.eval(`${soundSelect}.value`), 'guitar/nylon')
      // The test pack adds a sound of its own after the built-in ones.
      assert.equal(await options(), 'guitar/nylon guitar/steel guitar/twelve-string')
      await choose('Sound of this track', 'guitar/steel')
      assert.equal((await document_(context)).score.tracks[0].sound, 'guitar/steel')
      assert.equal(await isDirty(page), true)

      // Another instrument brings its own sounds, starting from its default one.
      await choose('Instrument of this track', 'ElectricGuitar')
      assert.equal(await page.eval(`${soundSelect}.value`), 'electric-guitar/clean')
      assert.equal(await options(), 'electric-guitar/clean electric-guitar/muted electric-guitar/overdrive electric-guitar/distortion')
      await choose('Sound of this track', 'electric-guitar/distortion')
      assert.equal((await document_(context)).score.tracks[0].sound, 'electric-guitar/distortion')

      await page.press('z', ['ctrl'])
      await settle()
      assert.equal((await document_(context)).score.tracks[0].sound, 'electric-guitar/clean')
      assert.equal(await page.eval(`${soundSelect}.value`), 'electric-guitar/clean')

      // The add-track menu names each sound with its instrument.
      await page.eval(`document.querySelector('.add-track summary').click()`)
      await clickButton(page, 'Banjo: Standard')
      const banjo = (await document_(context)).score.tracks[2]
      assert.equal(banjo.sound, 'banjo/standard')
      assert.deepEqual(banjo.tuning, [62, 59, 55, 50, 67])
      assert.equal(await page.eval(`document.querySelector('.tuning input').value`), 'D4 B3 G3 D3 G4')
    },
  ],

  [
    'shift and the arrows select several beats, then copy and paste write them elsewhere',
    async (context) => {
      const { page } = context
      const before = beatsOf(await document_(context), 0)
      await clickFret(page, 0)
      await page.press('ArrowRight', ['shift'])
      await page.press('c', ['ctrl'])
      assert.equal(await page.eval(`document.querySelectorAll('svg.tab .beat-highlight').length`), 2, 'two beats are highlighted')

      await page.press('ArrowRight') // leaves the range: the selection is on beat 2
      await page.press('v', ['ctrl'])
      await settle()

      const after = beatsOf(await document_(context), 0)
      const frets = (beats) => beats.map((b) => b.notes.map((n) => [n.string, n.fret]))
      assert.deepEqual(frets(after.slice(2, 4)), frets(before.slice(0, 2)), 'beats 0-1 were pasted on 2-3')
      assert.deepEqual(frets(after.slice(0, 2)), frets(before.slice(0, 2)), 'the source is untouched')
      assert.equal(await isDirty(page), true)
    },
  ],

  [
    'cut and Delete clear a range of beats, and the range can be pasted back',
    async (context) => {
      const { page } = context
      const before = beatsOf(await document_(context), 0)
      await clickFret(page, 0)
      await page.press('ArrowRight', ['shift'])
      await page.press('ArrowRight', ['shift'])
      await page.press('x', ['ctrl'])
      await settle()

      let now = beatsOf(await document_(context), 0)
      assert.ok(now.slice(0, 3).every((b) => b.notes.length === 0), 'three beats were emptied')
      assert.equal(now.length, before.length, 'the rhythm is kept')

      await page.press('v', ['ctrl'])
      await settle()
      now = beatsOf(await document_(context), 0)
      assert.deepEqual(now.slice(0, 3).map((b) => b.notes), before.slice(0, 3).map((b) => b.notes), 'pasting restores them')

      await page.press('ArrowRight', ['shift'])
      await page.press('Delete')
      await settle()
      assert.ok(beatsOf(await document_(context), 0).slice(0, 2).every((b) => b.notes.length === 0))
    },
  ],

  [
    'dragging the mouse over several beats selects them, and Delete clears them',
    async (context) => {
      const { page } = context
      const before = beatsOf(await document_(context), 0)
      const centerOf = (index) =>
        page.eval(`(() => {
          const r = document.querySelectorAll('svg.tab text.fret')[${index}].getBoundingClientRect()
          return { x: r.x + r.width / 2, y: r.y + r.height / 2 }
        })()`)
      await page.mouseDrag(await centerOf(0), await centerOf(2))

      const highlighted = await page.eval(`document.querySelectorAll('svg.tab .beat-highlight').length`)
      assert.ok(highlighted >= 2, `the dragged beats are highlighted (${highlighted})`)

      await page.press('Delete')
      await settle()
      const after = beatsOf(await document_(context), 0)
      assert.ok(after.slice(0, highlighted).every((b) => b.notes.length === 0), 'the dragged beats were emptied')
      assert.deepEqual(after[highlighted]?.notes, before[highlighted]?.notes, 'the next beat is untouched')
    },
  ],

  [
    'a right-click menu deletes the measures of the selected beats, and undo brings them back',
    async (context) => {
      const { page } = context
      const measureCounts = async () => (await document_(context)).score.tracks.map((t) => t.measures.length)
      const before = await measureCounts()
      const firstMeasureBeats = beatsOf(await document_(context), 0).length

      // From the first beat to the first beat of the next measure: two measures.
      await clickFret(page, 0)
      for (let i = 0; i < firstMeasureBeats; i++) await page.press('ArrowRight', ['shift'])

      const point = await page.eval(`(() => {
        const r = document.querySelector('svg.tab .beat-highlight').getBoundingClientRect()
        return { x: r.x + r.width / 2, y: r.y + r.height / 2 }
      })()`)
      await page.mouseRightClick(point.x, point.y)
      await page.waitFor(`document.querySelector('.context-menu')`, { what: 'the menu opens' })
      // Clipboard, then the bend of the note under the click, then the measures.
      assert.equal(await page.eval(`document.querySelectorAll('.context-menu hr').length`), 2)

      // Escape closes the menu and keeps the selection.
      await page.press('Escape')
      assert.equal(await page.eval(`Boolean(document.querySelector('.context-menu'))`), false)
      assert.ok((await page.eval(`document.querySelectorAll('svg.tab .beat-highlight').length`)) >= 2, 'the range is still selected')

      await page.mouseRightClick(point.x, point.y)
      await page.waitFor(`document.querySelector('.context-menu')`)
      await clickButton(page, 'Delete 2 measures')
      await settle()
      assert.deepEqual(await measureCounts(), before.map((n) => n - 2))
      assert.equal(await page.eval(`Boolean(document.querySelector('.context-menu'))`), false, 'choosing an item closes the menu')

      await page.press('z', ['ctrl'])
      await settle()
      assert.deepEqual(await measureCounts(), before)
    },
  ],

  [
    'Ctrl+Delete deletes the selected measure, and the menu inserts one before it',
    async (context) => {
      const { page } = context
      const count = async () => (await document_(context)).score.tracks[0].measures.length
      const before = await count()

      await clickFret(page, 0)
      await page.press('Delete', ['ctrl'])
      await settle()
      assert.equal(await count(), before - 1)

      await clickFret(page, 0)
      const point = await page.eval(`(() => {
        const r = document.querySelector('svg.tab text.fret').getBoundingClientRect()
        return { x: r.x + r.width / 2, y: r.y + r.height / 2 }
      })()`)
      await page.mouseRightClick(point.x, point.y)
      await page.waitFor(`document.querySelector('.context-menu')`)
      await clickButton(page, 'Insert measure before')
      await settle()
      assert.equal(await count(), before)
    },
  ],

  [
    'a bend gets its amount from the bend menu, from + and -, and from the right-click menu',
    async (context) => {
      const { page } = context
      const bendOf = async () => beatsOf(await document_(context), 0)[1].notes.find((n) => n.string === 4).technique
      const label = () => text(page, 'svg.tab text.technique')
      const clickMatching = async (selector, start) => {
        const found = await page.eval(`(() => {
          const b = [...document.querySelectorAll(${JSON.stringify(selector)})].find(b => b.textContent.trim().startsWith(${JSON.stringify(start)}))
          b?.click()
          return Boolean(b)
        })()`)
        assert.ok(found, `no ${selector} starting with "${start}"`)
        await settle()
      }

      await clickFret(page, 0)
      await page.press('ArrowRight')
      await page.press('ArrowUp') // the note of string 4 in beat 1
      await page.eval(`document.querySelector('.bend-menu summary').click()`)
      await page.screenshot(join(context.out, 'bend-menu.png'))
      await clickMatching('.bend-menu .menu button', '1/4')
      assert.equal(await bendOf(), 'BendQuarter')
      assert.equal(await label(), '¼', 'the tab shows the amount')

      await page.press('+')
      await settle()
      assert.equal(await bendOf(), 'BendHalf')
      assert.equal(await label(), '½')
      await page.press('+')
      await page.press('+') // already full: stays
      await settle()
      assert.equal(await bendOf(), 'Bend')
      assert.equal(await label(), 'full')
      await page.press('-')
      await settle()
      assert.equal(await bendOf(), 'BendHalf')

      await page.press('b') // B clears a bend of any amount
      await settle()
      assert.equal(await bendOf(), null)
      await page.press('+') // nothing to step on a plain note
      await settle()
      assert.equal(await bendOf(), null)

      const rightClickNote = async () => {
        const point = await selection(page)
        await page.mouseRightClick(point.x, point.y)
        await page.waitFor(`document.querySelector('.context-menu')`)
      }
      await rightClickNote()
      await clickMatching('.context-menu button', 'Bend')
      await page.screenshot(join(context.out, 'bend-context-menu.png'))
      await clickMatching('.context-menu.submenu button', '1/2')
      assert.equal(await bendOf(), 'BendHalf')

      await rightClickNote()
      await clickMatching('.context-menu button', 'Remove the bend')
      assert.equal(await bendOf(), null)
    },
  ],

  [
    'the right arrow at the end of the score adds one empty measure, and no more while it is empty',
    async (context) => {
      const { page } = context
      const count = async () => (await document_(context)).score.tracks.map((t) => t.measures.length)
      const before = await count()

      await clickFret(page, 0)
      // More presses than there are beats: the first one past the last note adds the measure.
      for (let i = 0; i < 40; i++) await page.press('ArrowRight')
      await settle()
      assert.deepEqual(await count(), before.map((n) => n + 1), 'every track gets the measure, once')
    },
  ],

  [
    'dragging over part of a chord selects those strings, and Delete clears only them',
    async (context) => {
      const { page } = context
      // The six notes of the chord in the third measure, from the highest string to the lowest.
      const chord = await page.eval(`(() => {
        const frets = [...document.querySelectorAll('svg.tab text.fret')]
        const byX = new Map()
        for (const f of frets) byX.set(f.getAttribute('x'), [...(byX.get(f.getAttribute('x')) ?? []), f])
        const column = [...byX.values()].find((c) => c.length === 6)
        const points = column.map((f) => { const r = f.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 } })
        return points.sort((a, b) => a.y - b.y)
      })()`)
      assert.equal(beatsOf(await document_(context), 2)[0].notes.length, 6)

      await page.mouseDrag(chord[1], chord[3]) // strings 2 to 4
      const heights = await page.eval(`[...document.querySelectorAll('svg.tab .beat-highlight')].map((r) => Number(r.getAttribute('height')))`)
      assert.equal(heights.length, 1, 'one beat is highlighted')
      assert.ok(heights[0] < 60, `the highlight covers only three strings (${heights[0]})`)

      await page.press('Delete')
      await settle()
      assert.deepEqual(
        beatsOf(await document_(context), 2)[0].notes.map((n) => n.string),
        [6, 5, 1],
        'only strings 2, 3 and 4 were cleared',
      )
    },
  ],

  [
    'Delete on a rest takes it out of the timeline, and removes the measure when it is the last rest',
    async (context) => {
      const { page } = context
      await clickButton(page, 'New')
      await settle()
      const measures = async () => (await document_(context)).score.tracks[0].measures.length
      const before = await measures()

      // The new score is a row of whole-measure rests: Delete on one removes that measure.
      await page.press('Delete')
      await settle()
      assert.equal(await measures(), before - 1)

      // A quarter rest in a 4/4 measure: the measure becomes 3/4.
      await page.press('q')
      await settle()
      await page.press('Delete')
      await settle()
      const first = (await document_(context)).score.tracks[0].measures[0]
      assert.deepEqual(first.time_signature, { numerator: 3, denominator: 4 })
      assert.deepEqual(first.voices[0].beats.map((b) => b.duration), [{ value: 'Half', dots: 1 }], 'what is left is a dotted half')
    },
  ],

  [
    'Delete on a rest takes it out of that track only, even where another track plays, and Ctrl+Z brings it back',
    async (context) => {
      const { page } = context
      const signatures = async () => (await document_(context)).score.tracks.map((t) => t.measures[0].time_signature)
      const before = await signatures()

      await clickFret(page, 0)
      await page.press('Delete') // the note goes, leaving a rest
      await settle()
      await page.press('Delete') // the rest leaves the guitar's timeline; the bass plays on that beat
      await settle()
      const after = await signatures()
      assert.deepEqual(after[0], { numerator: 7, denominator: 8 }, 'the guitar measure is one eighth shorter')
      assert.deepEqual(after[1], before[1], 'the bass keeps its measure')
      assert.equal(await exists(page, '.notice'), false, 'nothing is refused')
      assert.ok(beatsOf(await document_(context), 0).length < 8, 'the rest is gone')

      await page.press('z', ['ctrl'])
      await settle()
      assert.deepEqual(await signatures(), before)
    },
  ],

  [
    'the theme button goes round system, light and dark, and the choice is kept',
    async (context) => {
      const { page } = context
      const state = () =>
        page.eval(`({
          attribute: document.documentElement.getAttribute('data-theme'),
          background: getComputedStyle(document.body).backgroundColor,
          saved: localStorage.getItem('music-part-theme'),
        })`)
      const click = () => page.eval(`document.querySelector('.theme-button').click()`)

      assert.equal((await state()).attribute, null, 'it follows the system at first')

      await click()
      await settle()
      assert.deepEqual(await state(), { attribute: 'light', background: 'rgb(244, 244, 246)', saved: 'light' })

      await click()
      await settle()
      assert.deepEqual(await state(), { attribute: 'dark', background: 'rgb(19, 19, 24)', saved: 'dark' })

      await click()
      await settle()
      assert.equal((await state()).attribute, null)
      assert.equal((await state()).saved, 'system')
    },
  ],

  [
    'a MusicXML file opens with its title, tempo, tuning and tied notes',
    async (context) => {
      const { page, out } = context
      const xml = `<?xml version="1.0"?>
<score-partwise><work><work-title>XML tune</work-title></work>
<part-list><score-part id="P1"><part-name>Lead guitar</part-name></score-part></part-list>
<part id="P1">
<measure number="1"><attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
<direction><sound tempo="88"/></direction>
<note><pitch><step>E</step><octave>4</octave></pitch><duration>4</duration><tie type="start"/></note>
<note><pitch><step>E</step><octave>4</octave></pitch><duration>4</duration><tie type="stop"/></note></measure>
<measure number="2"><note><pitch><step>G</step><octave>3</octave></pitch><duration>8</duration></note></measure>
</part></score-partwise>`
      const file = join(out, 'tune.musicxml')
      writeFileSync(file, xml)
      await page.eval(`window.__e2e.answers.open.push(${JSON.stringify(file)})`)
      await clickButton(page, 'Open')
      await settle()

      assert.match(await text(page, '.brand h1'), /XML tune/)
      assert.match(await text(page, '.track-tab.active .name'), /^Lead/)
      const doc = await document_(context)
      assert.equal(doc.score.tempo_bpm, 88)
      assert.equal(doc.path, null, 'an import is a new unsaved score')
      const beats = beatsOf(doc, 0)
      assert.deepEqual(beats.map((b) => b.notes[0]?.tied), [false, true])
      assert.equal((await frets(page)).length, 3, 'three notes are drawn')
    },
  ],

  [
    'the Export menu writes a MIDI and a MusicXML file and leaves the score as it was',
    async (context) => {
      const { page, out } = context
      const exportAs = async (label, target) => {
        await page.eval(`window.__e2e.answers.save.push(${JSON.stringify(target)})`)
        await page.eval(`document.querySelector('.export-menu summary').click()`)
        await clickButton(page, label)
        await settle()
      }

      await exportAs('MIDI file (.mid)', join(out, 'exported-song'))
      assert.ok(existsSync(join(out, 'exported-song.mid')), 'the extension was added')
      assert.equal(readFileSync(join(out, 'exported-song.mid')).subarray(0, 4).toString(), 'MThd')
      assert.match(await text(page, '.notice span'), /Exported to .*exported-song\.mid/)

      await exportAs('MusicXML (.musicxml)', join(out, 'exported-song.musicxml'))
      const xml = readFileSync(join(out, 'exported-song.musicxml'), 'utf8')
      assert.match(xml, /<score-partwise/)
      assert.match(xml, /Demo in A minor/)

      assert.equal(await isDirty(page), false, 'exporting is not saving')
      assert.equal((await document_(context)).path, null)
    },
  ],

  [
    'the view switch shows every track in a row, and the choice is kept',
    async (context) => {
      const { page } = context
      await inRows(page, async () => {
        assert.deepEqual(await rowLabels(page), ['Track 1: Guitar', 'Track 2: Bass'])
        assert.equal(await exists(page, '.track-tab'), false, 'the row headers replace the tabs')
        assert.equal(await exists(page, '.track-row'), false, 'there is no line of tabs to leave empty')
        assert.ok(await exists(page, '.instrument-row .add-track'), '+ Track moves to the settings line')
        assert.equal(await page.eval(`document.querySelectorAll('section.row svg.tab').length`), 2)
        assert.equal(await page.eval(`localStorage.getItem('music-part-view')`), 'rows')

        // Measure 3 starts at the same x in both rows: they share their barlines.
        const thirdMeasureStarts = await page.eval(`[...document.querySelectorAll('section.row svg.tab')].map(svg => {
          const bars = [...svg.querySelectorAll('line.bar')]
          return Math.round(bars[2].getBoundingClientRect().x)
        })`)
        assert.equal(thirdMeasureStarts[0], thirdMeasureStarts[1])

        // Moving playback to measure 3 shows one cursor across both rows, on its first beat.
        const strip = await page.eval(`(() => {
          const r = document.querySelectorAll('section.row:nth-of-type(2) svg.tab .seek-strip')[2].getBoundingClientRect()
          return { x: r.x + r.width / 2, y: r.y + r.height / 2 }
        })()`)
        await page.mouseClick(strip.x, strip.y)
        await page.waitFor(`document.querySelector('.playhead .line')`, { what: 'the playback cursor' })
        const cursor = await page.eval(`(() => {
          const line = document.querySelector('.playhead .line').getBoundingClientRect()
          const rows = [...document.querySelectorAll('section.row')].map(r => r.getBoundingClientRect())
          return { x: line.x + line.width / 2, top: line.top, bottom: line.bottom, rowsTop: rows[0].top, rowsBottom: rows.at(-1).bottom }
        })()`)
        assert.ok(cursor.top <= cursor.rowsTop + 1 && cursor.bottom >= cursor.rowsBottom - 1, 'the cursor crosses every row')
        const offset = cursor.x - thirdMeasureStarts[0]
        assert.ok(offset > 0 && offset < 40, `the cursor is on the first beat of measure 3 (${offset} px after its barline)`)

        await page.screenshot(join(context.out, 'rows-mode.png'))

        await context.fresh()
        assert.equal(await page.eval(`document.querySelectorAll('section.row').length`), 2, 'still rows after a restart')
        assert.equal(await page.eval(`document.querySelector('.view-switch [aria-pressed=true]').textContent.trim()`), 'Rows')

        await clickButton(page, 'Tabs')
        assert.equal(await exists(page, 'section.row'), false)
        assert.equal(await page.eval(`document.querySelectorAll('.track-tab').length`), 2)
      })
    },
  ],

  [
    'clicking in the second row selects there, and typing writes in that track',
    async (context) => {
      const { page } = context
      await inRows(page, async () => {
        assert.equal(await selectedRow(page), 'Track 1: Guitar')
        await clickOn(page, 'section.row:nth-of-type(2) svg.tab text.fret')
        assert.equal(await selectedRow(page), 'Track 2: Bass')
        assert.equal(await page.eval(`document.querySelectorAll('svg.tab .selection').length`), 1, 'one selection, in the clicked row')

        await page.press('9')
        await settle()
        const doc = await document_(context)
        assert.deepEqual(doc.score.tracks[1].measures[0].voices[0].beats[0].notes.map((n) => n.fret), [9])
        assert.equal(doc.score.tracks[0].measures[0].voices[0].beats[0].notes[0].fret, 0, 'the guitar is untouched')

        // Alt+Up and Alt+Down move the selection to the track above and below.
        await page.press('ArrowUp', ['alt'])
        assert.equal(await selectedRow(page), 'Track 1: Guitar')
        await page.press('ArrowDown', ['alt'])
        assert.equal(await selectedRow(page), 'Track 2: Bass')
      })
    },
  ],

  [
    'a drag in a row selects in that row only, and paste writes into the selected track',
    async (context) => {
      const { page } = context
      await inRows(page, async () => {
        const points = await page.eval(`(() => {
          const frets = [...document.querySelectorAll('section.row:nth-of-type(1) svg.tab text.fret')]
          const center = (t) => { const r = t.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 } }
          const bass = document.querySelector('section.row:nth-of-type(2) svg.tab').getBoundingClientRect()
          return { from: center(frets[0]), next: center(frets[1]), bassY: bass.y + bass.height / 2 }
        })()`)
        // Drag along the guitar to the next beat, then down into the bass row, and let go there.
        const mouse = (type, { x, y }, buttons) =>
          page.send('Input.dispatchMouseEvent', { type, x, y, button: 'left', buttons, clickCount: 1 })
        const { from, next, bassY } = points
        await mouse('mouseMoved', from, 0)
        await mouse('mousePressed', from, 1)
        await mouse('mouseMoved', { x: next.x, y: from.y }, 1)
        await mouse('mouseMoved', { x: next.x, y: bassY }, 1)
        await mouse('mouseReleased', { x: next.x, y: bassY }, 0)
        await settle()
        assert.equal(await selectedRow(page), 'Track 1: Guitar', 'the drag stays in the row it started in')
        assert.equal(await page.eval(`document.querySelectorAll('section.row:nth-of-type(1) .beat-highlight').length`), 2)
        assert.equal(await page.eval(`document.querySelectorAll('section.row:nth-of-type(2) .beat-highlight').length`), 0)

        // Two guitar eighths pasted on the first bass quarter.
        await page.press('c', ['ctrl'])
        await clickOn(page, 'section.row:nth-of-type(2) svg.tab text.fret')
        await page.press('v', ['ctrl'])
        await settle()
        const doc = await document_(context)
        const bass = doc.score.tracks[1].measures[0].voices[0].beats
        assert.deepEqual(bass.slice(0, 2).map((b) => b.duration.value), ['Eighth', 'Eighth'])
        assert.equal(beatsOf(doc, 0).length, 8, 'the guitar is untouched')
      })
    },
  ],

  [
    'mute, solo and fold from a row header',
    async ({ page }) => {
      await inRows(page, async () => {
        const lastMix = () => page.eval(`window.__e2e.audio.filter(c => c.command === 'set_audible').at(-1)?.args.audible`)
        await clickButton(page, 'Mute Bass')
        assert.deepEqual(await lastMix(), [true, false])
        await clickButton(page, 'Solo Bass')
        assert.deepEqual(await lastMix(), [false, true], 'a soloed track plays alone, muted or not')
        await clickButton(page, 'Solo Bass')
        await clickButton(page, 'Mute Bass')
        assert.deepEqual(await lastMix(), [true, true])

        await clickButton(page, 'Fold Guitar')
        assert.equal(await page.eval(`document.querySelectorAll('section.row svg.tab').length`), 1, 'a folded row is only its header')
        await clickButton(page, 'Unfold Guitar')
        assert.equal(await page.eval(`document.querySelectorAll('section.row svg.tab').length`), 2)
      })
    },
  ],

  [
    'a tab or a row header picks the track to edit, and the instrument row follows it',
    async ({ page }) => {
      const pick = (index) => pickTrack(page, index)
      await pick(1)
      await settle()
      assert.match(await text(page, '.track-tab.active .name'), /^Bass/)
      assert.equal(await page.eval(`document.querySelector('input[aria-label="Name of this track"]').value`), 'Bass')
      assert.equal(await text(page, '.instrument-row .owner'), 'Bass', 'the instrument row names the track it applies to')

      await inRows(page, async () => {
        await pick(0)
        await settle()
        assert.equal(await selectedRow(page), 'Track 1: Guitar')
      })
    },
  ],

  [
    'with many tracks only the rows on screen are drawn, and Alt+Down scrolls to the next track',
    async (context) => {
      const { page, backend } = context
      for (let i = 0; i < 22; i++) await backend.request('edit_score', { edit: { type: 'addTrack', instrument: 'Guitar' } })
      await reload(page, `document.querySelector('svg.tab line.string')`, 'the reloaded score to appear')

      await inRows(page, async () => {
        const drawn = await page.eval(`document.querySelectorAll('section.row').length`)
        assert.ok(drawn > 0 && drawn < 24, `${drawn} of 24 rows are drawn`)

        for (let i = 0; i < 23; i++) await page.press('ArrowDown', ['alt'])
        await settle()
        assert.match(await selectedRow(page), /^Track 24: /)
        const isInView = await page.eval(`(() => {
          const row = document.querySelector('section.row.selected').getBoundingClientRect()
          const view = document.querySelector('.scroller').getBoundingClientRect()
          return row.top >= view.top - 1 && row.bottom <= view.bottom + 1
        })()`)
        assert.ok(isInView, 'the selected row was scrolled into view')
      })
    },
  ],

  [
    'the Sounds panel lists the packs with their licence and says where the track sound comes from',
    async ({ page }) => {
      const panelText = () => text(page, '.sounds-panel')
      await page.eval(`document.querySelector('.sounds-menu summary').click()`)
      await page.waitFor(`document.querySelectorAll('.sounds-panel .pack').length > 0`, { what: 'the packs' })

      const packs = await page.eval(`[...document.querySelectorAll('.sounds-panel .pack .name')].map(n => n.textContent.trim())`)
      assert.ok(packs.some((name) => name.startsWith('Spanish classical guitar')), packs.join(', '))
      assert.ok(packs.every((name) => name.endsWith('CC0-1.0')), 'each pack shows its licence')
      assert.match(await text(page, '.sounds-panel .current'), /Nylon strings: Spanish classical guitar \(CC0-1\.0\)/)
      assert.match(await panelText(), /only known in the desktop app/)

      // Another track: the panel follows the selection.
      await page.eval(`document.querySelectorAll('.track-tab .name')[1].click()`)
      await settle()
      assert.match(await text(page, '.sounds-panel .current'), /Electric bass: Finger: Finger Bass YR/)

      // A press elsewhere closes it.
      await clickOn(page, 'svg.tab')
      assert.equal(await page.eval(`document.querySelector('.sounds-menu').open`), false)
    },
  ],

  [
    'a sound a pack declares is offered, keeps its fallback program, and shows as missing without the pack',
    async (context) => {
      const { page, out } = context
      const soundSelect = `document.querySelector('select[aria-label="Sound of this track"]')`
      const selectedText = () => page.eval(`${soundSelect}.selectedOptions[0].textContent.trim()`)
      await page.waitFor(`[...${soundSelect}.options].some(o => o.value === 'guitar/twelve-string')`, { what: 'the pack sound in the menu' })
      assert.equal(await page.eval(`[...${soundSelect}.options].find(o => o.value === 'guitar/twelve-string').textContent.trim()`), 'Twelve-string')

      await page.eval(`(() => { const s = ${soundSelect}; s.value = 'guitar/twelve-string'; s.dispatchEvent(new Event('change', { bubbles: true })) })()`)
      await settle()
      const track = (await document_(context)).score.tracks[0]
      assert.equal(track.sound, 'guitar/twelve-string')
      assert.equal(track.fallback_program, 25, 'the track records the program the pack declared')

      // The Sounds panel marks the pack's sound as new, and names it for the track.
      await page.eval(`document.querySelector('.sounds-menu summary').click()`)
      await page.waitFor(`document.querySelector('.sounds-panel .pack-sound .new')`, { what: 'a new sound in the panel' })
      assert.match(await text(page, '.sounds-panel .current'), /Acoustic guitar: Twelve-string: Test twelve-string \(CC0-1\.0\)/)
      await clickOn(page, 'svg.tab')

      // The "+ Track" menu offers it too.
      assert.ok(await exists(page, '.add-track button[aria-label="Acoustic guitar: Twelve-string"]'), 'the add-track menu lists the pack sound')

      // Opened where the pack is not installed, the score still loads and says what plays instead.
      const file = join(out, 'twelve-string.mpart')
      await context.backend.request('save_file', { path: file })
      await context.fresh(file, { hasPacks: false })
      assert.equal(await page.eval(`${soundSelect}.value`), 'guitar/twelve-string')
      assert.equal(await selectedText(), 'twelve-string (pack missing), plays General MIDI program 25')
      await page.eval(`document.querySelector('.sounds-menu summary').click()`)
      await page.waitFor(`document.querySelector('.sounds-panel .current')`, { what: 'the current sound' })
      assert.match(await text(page, '.sounds-panel .current'), /twelve-string \(pack missing\): General MIDI \(pack missing\)/)
      assert.match(await text(page, '.sounds-panel .current'), /General MIDI program 25 \(Acoustic guitar: Steel strings\)/)
      await clickOn(page, 'svg.tab')

      // Another sound can be chosen, and the missing one no longer shows.
      await page.eval(`(() => { const s = ${soundSelect}; s.value = 'guitar/steel'; s.dispatchEvent(new Event('change', { bubbles: true })) })()`)
      await settle()
      const steel = (await document_(context)).score.tracks[0]
      assert.equal(steel.sound, 'guitar/steel')
      assert.equal(steel.fallback_program, undefined)
      assert.equal(await page.eval(`[...${soundSelect}.options].map(o => o.value).join(' ')`), 'guitar/nylon guitar/steel')
    },
  ],

  [
    'writes a guitar track on a staff, alone or above its tablature, and remembers the choice',
    async ({ page }) => {
      await withFrettedView(page, 'Staff', async () => {
        assert.equal(await exists(page, 'svg.tab line.string'), false, 'no tablature')
        assert.ok(await exists(page, 'svg.tab g.staff[data-clef="treble8vb"] line.staff-line'), 'a treble staff, an octave down')
        assert.ok((await page.eval(`document.querySelectorAll('svg.tab .staff-notes .head').length`)) > 10, 'the notes have heads')

        await clickButton(page, 'Both')
        const order = await page.eval(`(() => {
          const staff = document.querySelector('svg.tab line.staff-line').getBoundingClientRect().y
          const tab = document.querySelector('svg.tab line.string').getBoundingClientRect().y
          return staff < tab
        })()`)
        assert.equal(order, true, 'the staff is above the tablature')

        await reload(page, `document.querySelector('svg.tab line.staff-line')`, 'the staff after a reload')
        assert.ok(await exists(page, 'svg.tab line.string'), 'both are still shown after a restart')

        // Clicking a note head on the staff selects that note's string on the tablature too.
        await clickOn(page, 'svg.tab .staff-notes .head')
        await settle()
        assert.ok(await exists(page, 'svg.tab .staff-selection'), 'the head is selected')
        assert.ok(await exists(page, 'svg.tab rect.selection:not(.staff-selection)'), 'and its fret on the tablature')

        await clickButton(page, 'Tab')
        assert.equal(await exists(page, 'svg.tab line.staff-line'), false)
      })
    },
  ],

  [
    'writes a guitar note by double-clicking its staff',
    async (context) => {
      const { page } = context
      await withFrettedView(page, 'Both', async () => {
        // A new empty measure after the first one: its whole rest is the first rest of the page.
        await page.press('Enter', ['ctrl'])
        await settle()
        const point = await page.eval(`(() => {
          const rest = document.querySelector('svg.tab .staff-notes .staff-rest').getBoundingClientRect()
          const lines = [...document.querySelectorAll('svg.tab g.staff line.staff-line')].map(l => l.getBoundingClientRect().y)
          const bottom = Math.max(...lines.slice(0, 5))
          const gap = (bottom - Math.min(...lines.slice(0, 5))) / 4
          // The second space from the bottom: A4 written, A3 sounding.
          return { x: rest.x + rest.width / 2, y: bottom - 3 * gap / 2 }
        })()`)
        await page.mouseDoubleClick(point.x, point.y)
        await settle()
        assert.deepEqual(beatsOf(await document_(context), 1)[0].notes, [{ string: 3, fret: 2, technique: null, tied: false }], 'A3 is fret 2 of string 3')
      })
    },
  ],

  [
    'enters, moves, ties and deletes notes on a piano staff with the mouse and the keyboard',
    async (context) => {
      const { page } = context
      await withPiano(context)
      assert.equal(await exists(page, 'svg.tab line.string'), false, 'a piano has no tablature')
      assert.equal(await text(page, 'nav.tracks .track-tab.active .strings'), 'Piano')
      assert.equal(await exists(page, 'input[aria-label="Tuning of this track"]'), false)

      // Middle C: one ledger line below the treble staff.
      const middleC = await staffPoint(page, { clef: 'treble', position: -2 })
      await page.mouseDoubleClick(middleC.x, middleC.y)
      await settle()
      assert.deepEqual((await pianoNotes(context))[0], [[60, false]])
      assert.ok(await exists(page, 'svg.tab .staff-notes .ledger'), 'middle C has its ledger line')

      await page.press('ArrowUp')
      await settle()
      assert.deepEqual((await pianoNotes(context))[0], [[61, false]], 'up moves the note a semitone')
      assert.ok(await exists(page, 'svg.tab .staff-notes path.accidental'), 'C# shows its sharp')

      await page.press('G', ['shift'])
      await settle()
      assert.deepEqual((await pianoNotes(context))[0], [[61, false], [67, false]], 'Shift+G adds the nearest G')

      await page.press('q')
      await settle()
      await page.press('ArrowRight')
      await page.press('Enter')
      await settle()
      await page.press('l')
      await settle()
      const notes = await pianoNotes(context)
      assert.deepEqual(notes[1], [[67, true]], 'Enter writes the selected pitch on the next beat, L ties it')

      await page.press('Delete')
      await settle()
      assert.deepEqual((await pianoNotes(context))[1], [], 'Delete leaves a rest')
      await page.press('1')
      await settle()
      assert.match(await text(page, '.notice'), /staff/, 'digits explain how to write on a staff')
    },
  ],

  [
    'selects and writes on a piano staff in the rows view',
    async (context) => {
      const { page } = context
      await withPiano(context)
      await inRows(page, async () => {
        const root = 'section.row:nth-of-type(3) svg.tab'
        // The F clef staff: its middle line is D3.
        const d3 = await staffPoint(page, { clef: 'bass', position: 4, root })
        await page.mouseClick(d3.x, d3.y)
        await settle()
        assert.match(await selectedRow(page), /^Track 3: Piano/)
        assert.ok(await exists(page, `${root} .ghost-head`), 'an empty spot shows where the note would go')
        await page.press('Enter')
        await settle()
        assert.deepEqual((await pianoNotes(context))[0], [[50, false]])
        assert.ok(await exists(page, `${root} g.staff[data-clef="bass"] ~ .staff-notes .head, ${root} .staff-notes .head`))
      })
    },
  ],

  [
    'writes a drum pattern with the digit keys and the palette, and plays it',
    async (context) => {
      const { page } = context
      await clickButton(page, 'Drums: Standard kit')
      await page.waitFor(`document.querySelector('svg.tab g.staff[data-clef="percussion"]')`, { what: 'the percussion staff' })
      assert.ok(await exists(page, '[aria-label="Drum pieces"]'), 'the palette shows for a drum track')
      // Eight eighths: the hi-hat on each (3), the kick on 1 and 3 (1), the snare on 2 and 4 from
      // the palette.
      for (let i = 0; i < 8; i++) {
        await page.press('e')
        await page.press('3')
        if (i % 4 === 0) await page.press('1')
        await settle()
        if (i % 4 === 2) await clickButton(page, 'Snare')
        await page.press('ArrowRight')
      }
      await settle()
      const doc = await document_(context)
      const drums = doc.score.tracks[2]
      assert.equal(drums.instrument, 'Drums')
      const hits = drums.measures[0].voices[0].beats.map((b) => b.notes.map((n) => n.piece))
      assert.deepEqual(hits, [[36, 42], [42], [38, 42], [42], [36, 42], [42], [38, 42], [42]])
      assert.ok((await page.eval(`document.querySelectorAll('svg.tab .cross-head').length`)) >= 8, 'hi-hats have cross heads')

      // Up on the selected hi-hat of beat 7 moves it a line up, to the crash.
      await page.press('ArrowLeft')
      await page.press('ArrowLeft')
      await settle()
      await page.press('ArrowUp')
      await settle()
      const moved = (await document_(context)).score.tracks[2].measures[0].voices[0].beats[6].notes.map((n) => n.piece)
      assert.deepEqual(moved, [38, 49])

      await page.press(' ')
      await settle()
      assert.ok(await page.eval(`window.__e2e.audio.some(c => c.command === 'play')`), 'playback started')
      await clickButton(page, 'Mute Drums')
      const audible = await page.eval(`window.__e2e.audio.filter(c => c.command === 'set_audible').at(-1)?.args.audible`)
      assert.deepEqual(audible, [true, true, false], 'the drum track can be muted')
    },
  ],

  [
    'draws a drum staff that reads well (drums.png in target/e2e)',
    async (context) => {
      const { page, backend, out } = context
      const beat = (value, pieces, dots = 0) => ({
        duration: { value, dots },
        notes: pieces.map((piece) => ({ piece, technique: null, tied: false })),
      })
      await backend.request('edit_score', { edit: { type: 'addTrack', instrument: 'Drums', sound: 'drums/jazz' } })
      const groove = [
        beat('Eighth', [36, 49]),
        beat('Eighth', [42]),
        beat('Eighth', [38, 42]),
        beat('Eighth', [42]),
        beat('Eighth', [36, 42]),
        beat('Eighth', [36, 46]),
        beat('Eighth', [38, 42]),
        beat('Eighth', [44]),
      ]
      const fill = [
        beat('Sixteenth', [38]),
        beat('Sixteenth', [38]),
        beat('Sixteenth', [48]),
        beat('Sixteenth', [47]),
        beat('Sixteenth', [45]),
        beat('Sixteenth', [43]),
        beat('Sixteenth', [41]),
        beat('Sixteenth', [41]),
        beat('Quarter', [51, 36]),
        beat('Quarter', [53, 44]),
        beat('Quarter', [57, 35]),
        beat('Quarter', [52, 37, 39, 54, 56]),
        beat('Half', [55, 40]),
        beat('Quarter', [62, 63, 64]),
        beat('Quarter', []),
      ]
      await backend.request('edit_score', { edit: { type: 'paste', track: 2, measure: 0, beat: 0, beats: [...groove, ...fill] } })
      await reload(page, `document.querySelector('svg.tab line.string')`, 'the score to appear')
      await pickTrack(page, 2)
      await page.waitFor(`document.querySelector('svg.tab g.staff[data-clef="percussion"]')`, { what: 'the percussion staff' })
      await settle()
      await page.screenshot(join(out, 'drums.png'))
      const heads = await page.eval(`document.querySelectorAll('svg.tab .staff-notes .head').length`)
      assert.ok(heads >= 30, `${heads} note heads drawn`)
    },
  ],

  [
    'draws staves that read well (screenshots in target/e2e)',
    async (context) => {
      const { page, backend, out } = context
      const beat = (value, pitches, tied = false) => ({
        duration: { value, dots: 0 },
        notes: pitches.map((pitch) => ({ pitch, technique: null, tied })),
      })
      const dotted = (value, pitches) => ({ ...beat(value, pitches), duration: { value, dots: 1 } })
      await backend.request('edit_score', { edit: { type: 'addTrack', instrument: 'Piano' } })
      const beats = [
        ...[60, 62, 64, 66, 67, 69, 70, 72].map((p) => beat('Eighth', [p, p - 24])),
        beat('Quarter', [48, 52, 55, 72, 74, 77]),
        dotted('Quarter', [65, 69, 72]),
        beat('Sixteenth', [79]),
        beat('Sixteenth', [81]),
        beat('Quarter', []),
        beat('Half', [36, 84]),
        beat('Half', [36], true),
        beat('Whole', [33, 45, 61, 63]),
      ]
      await backend.request('edit_score', { edit: { type: 'paste', track: 2, measure: 0, beat: 0, beats } })
      await reload(page, `document.querySelector('svg.tab line.string')`, 'the score to appear')
      await withFrettedView(page, 'Both', async () => {
        await settle()
        await page.screenshot(join(out, 'staff.png'))
        await pickTrack(page, 2)
        await page.waitFor(`document.querySelector('svg.tab g.staff[data-clef="bass"]')`, { what: 'the grand staff' })
        await settle()
        await page.screenshot(join(out, 'staff-2.png'))
        const heads = await page.eval(`document.querySelectorAll('svg.tab .staff-notes .head').length`)
        assert.ok(heads >= 30, `${heads} note heads drawn`)
      })
    },
  ],

  [
    'adds an organ on a grand staff and a synthesizer on one treble staff',
    async (context) => {
      const { page } = context
      const clefs = () => page.eval(`[...new Set([...document.querySelectorAll('svg.tab g.staff')].map(g => g.dataset.clef))].join(' ')`)

      await page.eval(`document.querySelector('.add-track summary').click()`)
      await clickButton(page, 'Organ: Church')
      await page.waitFor(`document.querySelector('svg.tab g.staff[data-clef="bass"]')`, { what: 'the grand staff of the organ' })
      const organ = (await document_(context)).score.tracks[2]
      assert.deepEqual([organ.instrument, organ.sound, organ.tuning], ['Organ', 'organ/church', []])
      assert.equal(await clefs(), 'treble bass')
      assert.equal(await exists(page, 'svg.tab line.string'), false, 'an organ has no tablature')
      assert.equal(await exists(page, 'select[aria-label="Clef of this track"]'), false, 'an organ has no clef to choose')

      await page.eval(`document.querySelector('.add-track summary').click()`)
      await clickButton(page, 'Synthesizer: Saw lead')
      await page.waitFor(`document.querySelector('nav.tracks .track-tab.active .strings')?.textContent.trim() === 'Synthesizer'`, { what: 'the synthesizer track' })
      await settle()
      assert.equal(await clefs(), 'treble')
      const middleC = await staffPoint(page, { clef: 'treble', position: -2 })
      await page.mouseDoubleClick(middleC.x, middleC.y)
      await settle()
      const synth = (await document_(context)).score.tracks[3]
      assert.equal(synth.sound, 'synth/saw-lead')
      assert.deepEqual(synth.measures[0].voices[0].beats[0].notes.map((n) => n.pitch), [60])
    },
  ],

  [
    'a voice is written in the clef its singer reads, and the clef never moves its notes',
    async (context) => {
      const { page } = context
      const clefSelect = `document.querySelector('select[aria-label="Clef of this track"]')`
      const chooseClef = async (clef) => {
        await page.eval(`(() => { const s = ${clefSelect}; s.value = '${clef}'; s.dispatchEvent(new Event('change', { bubbles: true })) })()`)
        await settle()
      }
      const voice = async () => (await document_(context)).score.tracks[2]
      const staffClef = () => page.eval(`document.querySelector('svg.tab g.staff')?.dataset.clef ?? null`)

      await page.eval(`document.querySelector('.add-track summary').click()`)
      await clickButton(page, 'Voice: Voice oohs')
      await page.waitFor(`${clefSelect}`, { what: 'the clef menu of the voice' })
      assert.equal(await page.eval(`${clefSelect}.value`), 'treble')
      assert.equal(await staffClef(), 'treble')
      assert.deepEqual([(await voice()).sound, (await voice()).clef], ['voice/oohs', 'treble'])

      // Middle C, one ledger line below the treble staff.
      const middleC = await staffPoint(page, { clef: 'treble', position: -2 })
      await page.mouseDoubleClick(middleC.x, middleC.y)
      await settle()

      await chooseClef('treble8vb')
      assert.equal((await voice()).clef, 'treble8vb')
      await page.waitFor(`document.querySelector('svg.tab g.staff')?.dataset.clef === 'treble8vb'`, { what: 'the tenor clef' })
      await chooseClef('bass')
      assert.equal(await staffClef(), 'bass')
      assert.deepEqual((await voice()).measures[0].voices[0].beats[0].notes.map((n) => n.pitch), [60], 'the note keeps its pitch')
      // In the bass clef, middle C is one ledger line above the staff: position 10.
      const head = await page.eval(`(() => {
        const staff = document.querySelector('svg.tab g.staff[data-clef="bass"]')
        const lines = [...staff.querySelectorAll('line.staff-line')].map(l => l.getBoundingClientRect().y)
        const r = document.querySelector('svg.tab .staff-notes .head').getBoundingClientRect()
        return { top: Math.min(...lines), y: r.y + r.height / 2, gap: (Math.max(...lines) - Math.min(...lines)) / 4 }
      })()`)
      assert.ok(Math.abs(head.top - head.gap - head.y) < 2, `middle C sits on the ledger line above the bass staff (${JSON.stringify(head)})`)

      await page.press('z', ['ctrl'])
      await settle()
      assert.equal((await voice()).clef, 'treble8vb')
      assert.equal(await page.eval(`${clefSelect}.value`), 'treble8vb')

      // Another instrument has no clef to choose.
      await page.eval(`(() => { const s = document.querySelector('select[aria-label="Instrument of this track"]'); s.value = 'Synth'; s.dispatchEvent(new Event('change', { bubbles: true })) })()`)
      await settle()
      assert.equal(await exists(page, 'select[aria-label="Clef of this track"]'), false)
      assert.equal((await voice()).clef, undefined)
    },
  ],

  [
    'draws an organ grand staff and a tenor voice that read well (screenshot in target/e2e)',
    async (context) => {
      const { page, backend, out } = context
      const beat = (value, pitches) => ({ duration: { value, dots: 0 }, notes: pitches.map((pitch) => ({ pitch, technique: null })) })
      const edit = (edit) => backend.request('edit_score', { edit })
      await edit({ type: 'addTrack', instrument: 'Organ', sound: 'organ/church' })
      await edit({ type: 'addTrack', instrument: 'Voice' })
      await edit({ type: 'setClef', track: 3, clef: 'treble8vb' })
      await edit({
        type: 'paste',
        track: 2,
        measure: 0,
        beat: 0,
        beats: [beat('Half', [48, 55, 64, 67, 72]), beat('Half', [43, 53, 62, 67, 71]), beat('Whole', [48, 52, 60, 67, 72])],
      })
      await edit({
        type: 'paste',
        track: 3,
        measure: 0,
        beat: 0,
        beats: [beat('Quarter', [55]), beat('Quarter', [57]), beat('Quarter', [59]), beat('Quarter', [60]), beat('Half', [62]), beat('Half', [55])],
      })
      await reload(page, `document.querySelector('svg.tab line.string')`, 'the score to appear')
      await inRows(page, async () => {
        await settle()
        const clefs = await page.eval(`[...document.querySelectorAll('section.row')].map(r => [...r.querySelectorAll('g.staff')].slice(0, 2).map(g => g.dataset.clef).join('+'))`)
        assert.equal(clefs[2], 'treble+bass', 'the organ has a grand staff')
        assert.equal(clefs[3], 'treble8vb', 'the tenor reads the treble clef an octave down')
        await page.screenshot(join(out, 'keyboards-voice.png'))
      })
    },
  ],
]
