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
    'a MIDI file opens under the name of the file, and one without playable notes is refused',
    async (context) => {
      const { page, out } = context
      const good = join(out, 'one-note.mid')
      writeFileSync(good, Buffer.from(ONE_NOTE_MIDI))
      await page.eval(`window.__e2e.answers.open.push(${JSON.stringify(good)})`)
      await clickButton(page, 'Open')
      await settle()
      assert.match(await text(page, '.brand h1'), /one-note/)
      assert.deepEqual(await frets(page), ['0'])

      const silent = Buffer.from(ONE_NOTE_MIDI)
      silent[23] = 0x99 // percussion channel
      silent[27] = 0x89
      const bad = join(out, 'drums.mid')
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
      await clickButton(page, 'Electric bass')
      assert.equal((await document_(context)).score.tracks.length, 3)
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
      assert.equal(await page.eval(`document.querySelectorAll('.context-menu hr').length`), 1)

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
    'Delete on a rest is refused while another track plays there, and says why',
    async (context) => {
      const { page } = context
      const before = (await document_(context)).score.tracks.map((t) => t.measures.length)
      await clickFret(page, 0)
      await page.press('Delete') // the note goes, leaving a rest
      await settle()
      await page.press('Delete') // the rest cannot go: the bass plays on that beat
      await settle()
      assert.match(await text(page, '.notice span'), /another track/)
      assert.deepEqual((await document_(context)).score.tracks.map((t) => t.measures.length), before)
      assert.equal(beatsOf(await document_(context), 0).length, 8, 'the rest is still there')
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
<part-list><score-part id="P1"><part-name>Lead</part-name></score-part></part-list>
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
]
