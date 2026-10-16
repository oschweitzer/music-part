// End-to-end tests of the UI: the real Svelte app in headless Chrome, talking to the real Rust
// document code through a headless backend (src-tauri/examples/headless.rs). No window, no audio,
// nothing on the user's desktop.
//
// Usage: pnpm e2e            (needs a Chrome; set CHROME_BIN to choose one)
//        pnpm e2e --keep     (leave the screenshots of passing scenarios in target/e2e)

import { execFileSync, spawn } from 'node:child_process'
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { createInterface } from 'node:readline'
import { fileURLToPath } from 'node:url'

import { launchBrowser, sleep } from './browser.mjs'

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..')
const out = join(root, 'target', 'e2e')
const bridge = readFileSync(join(dirname(fileURLToPath(import.meta.url)), 'tauri-bridge.js'), 'utf8')
const uiPort = 14201

const packsFolder = join(root, 'target', 'e2e-packs')

/**
 * The packs the backend lists: the committed manifests of the bundled packs, and a test pack that
 * declares a new sound. The backend has no audio, so it reads no sample: manifests are enough.
 */
function writePacks() {
  rmSync(packsFolder, { recursive: true, force: true })
  const bundled = join(root, 'assets', 'soundfonts', 'packs')
  for (const id of readdirSync(bundled)) {
    if (!existsSync(join(bundled, id, 'pack.json'))) continue
    mkdirSync(join(packsFolder, id), { recursive: true })
    cpSync(join(bundled, id, 'pack.json'), join(packsFolder, id, 'pack.json'))
  }
  const twelve = {
    format: 1,
    id: 'e2e-twelve-string',
    name: 'Test twelve-string',
    author: 'The end-to-end tests',
    licence: 'CC0-1.0',
    sounds: [{ sound: 'guitar/twelve-string', name: 'Twelve-string', program: 25, file: 'twelve-string.sfz', firstKey: 40, lastKey: 88 }],
  }
  mkdirSync(join(packsFolder, twelve.id), { recursive: true })
  writeFileSync(join(packsFolder, twelve.id, 'pack.json'), JSON.stringify(twelve, null, 2))
}

class Backend {
  /** `hasPacks` false starts the backend without any pack, as on a machine that lacks them. */
  constructor(args = [], { hasPacks = true } = {}) {
    this.waiting = new Map()
    const env = { ...process.env, MUSIC_PART_PACKS: hasPacks ? packsFolder : join(packsFolder, 'none') }
    this.process = spawn(join(root, 'target', 'debug', 'examples', 'headless'), args, { stdio: ['pipe', 'pipe', 'inherit'], env })
    createInterface({ input: this.process.stdout }).on('line', (line) => {
      const answer = JSON.parse(line)
      this.waiting.get(answer.id)?.(answer)
      this.waiting.delete(answer.id)
    })
    this.nextId = 0
  }

  /** Resolves with the command's result, or rejects with its error message. */
  request(cmd, args = {}) {
    const id = ++this.nextId
    return new Promise((resolve, reject) => {
      this.waiting.set(id, (answer) => ('err' in answer ? reject(answer.err) : resolve(answer.ok)))
      this.process.stdin.write(`${JSON.stringify({ id, cmd, args })}\n`)
    })
  }

  stop() {
    this.process.kill()
  }
}

async function startVite() {
  const server = spawn('pnpm', ['--filter', 'ui', 'exec', 'vite', '--host', '127.0.0.1', '--port', String(uiPort), '--strictPort'], {
    cwd: root,
    stdio: 'ignore',
  })
  // pnpm and Vite can take a while to start on a busy machine.
  for (let attempt = 0; attempt < 600; attempt++) {
    try {
      if ((await fetch(`http://127.0.0.1:${uiPort}/`)).ok) return server
    } catch {
      // not listening yet
    }
    await sleep(100)
  }
  server.kill()
  throw new Error('the UI dev server did not start')
}

async function main() {
  rmSync(out, { recursive: true, force: true })
  mkdirSync(out, { recursive: true })
  writePacks()

  console.log('Building the headless backend…')
  execFileSync('cargo', ['build', '-q', '-p', 'music-part-app', '--example', 'headless'], { cwd: root, stdio: 'inherit' })

  const vite = await startVite()
  const page = await launchBrowser()
  await page.addInitScript(bridge)

  const context = { backend: null, page, out, root }
  await page.bind('e2eInvoke', async (payload) => {
    const { id, cmd, args } = JSON.parse(payload)
    try {
      const value = await context.backend.request(cmd, args)
      await page.eval(`window.__e2e_resolve(${id}, true, ${JSON.stringify(value)})`)
    } catch (error) {
      await page.eval(`window.__e2e_resolve(${id}, false, ${JSON.stringify(String(error))})`)
    }
  })

  /** A new backend (the demo score, or `file`; with the test packs unless `hasPacks` is false) and a freshly loaded page. */
  context.fresh = async (file, { hasPacks = true } = {}) => {
    context.backend?.stop()
    context.backend = new Backend(file ? [file] : [], { hasPacks })
    await page.goto(`http://127.0.0.1:${uiPort}/`)
    await page.waitFor(`document.querySelector('svg.tab text.fret, svg.tab line.string, svg.tab line.staff-line')`, { what: 'the score to appear' })
  }

  const { scenarios } = await import('./scenarios.mjs')
  let failed = 0
  for (const [name, run] of scenarios) {
    try {
      await context.fresh()
      await run(context)
      console.log(`  ✓ ${name}`)
    } catch (error) {
      failed++
      console.log(`  ✗ ${name}\n      ${error?.message ?? error}`)
      await page.screenshot(join(out, `failure-${failed}.png`)).catch(() => {})
    }
  }

  context.backend?.stop()
  await page.close()
  vite.kill()
  console.log(failed ? `\n${failed} of ${scenarios.length} scenarios failed (screenshots in ${out})` : `\nAll ${scenarios.length} scenarios passed`)
  process.exit(failed ? 1 : 0)
}

main().catch((error) => {
  console.error(error)
  process.exit(1)
})
