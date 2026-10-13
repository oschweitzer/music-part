// A minimal Chrome DevTools Protocol client: just what the end-to-end tests need.

import { spawn } from 'node:child_process'
import { existsSync, readdirSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

/** Chrome from $CHROME_BIN, else Playwright's headless shell, else a Chrome or Chromium on the PATH. */
export function findChrome() {
  if (process.env.CHROME_BIN) return process.env.CHROME_BIN

  const cache = join(homedir(), '.cache', 'ms-playwright')
  if (existsSync(cache)) {
    const shells = readdirSync(cache).filter((d) => d.startsWith('chromium_headless_shell')).sort()
    for (const dir of shells.reverse()) {
      const bin = join(cache, dir, 'chrome-headless-shell-linux64', 'chrome-headless-shell')
      if (existsSync(bin)) return bin
    }
  }
  return 'google-chrome'
}

const KEYS = {
  Enter: { code: 'Enter', vk: 13 },
  Escape: { code: 'Escape', vk: 27 },
  Backspace: { code: 'Backspace', vk: 8 },
  Delete: { code: 'Delete', vk: 46 },
  ArrowLeft: { code: 'ArrowLeft', vk: 37 },
  ArrowUp: { code: 'ArrowUp', vk: 38 },
  ArrowRight: { code: 'ArrowRight', vk: 39 },
  ArrowDown: { code: 'ArrowDown', vk: 40 },
  ' ': { code: 'Space', vk: 32 },
}

export async function launchBrowser({ width = 1200, height = 800, port = 9444 } = {}) {
  const process_ = spawn(
    findChrome(),
    ['--no-sandbox', '--disable-gpu', `--remote-debugging-port=${port}`, `--window-size=${width},${height}`, 'about:blank'],
    { stdio: 'ignore' },
  )

  let targets
  for (let attempt = 0; attempt < 80 && !targets; attempt++) {
    try {
      targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json()
    } catch {
      await sleep(100)
    }
  }
  if (!targets) throw new Error('Chrome did not start')

  const socket = new WebSocket(targets.find((t) => t.type === 'page').webSocketDebuggerUrl)
  await new Promise((resolve, reject) => {
    socket.onopen = resolve
    socket.onerror = () => reject(new Error('cannot connect to Chrome'))
  })

  let nextId = 0
  const pending = new Map()
  const bindingHandlers = new Map()

  socket.onmessage = (message) => {
    const data = JSON.parse(message.data)
    if (data.id !== undefined) {
      pending.get(data.id)?.(data)
      pending.delete(data.id)
    } else if (data.method === 'Runtime.bindingCalled') {
      bindingHandlers.get(data.params.name)?.(data.params.payload)
    }
  }

  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = ++nextId
      pending.set(id, (data) => (data.error ? reject(new Error(`${method}: ${data.error.message}`)) : resolve(data.result)))
      socket.send(JSON.stringify({ id, method, params }))
    })

  await send('Page.enable')
  await send('Runtime.enable')
  await send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })

  const page = {
    send,

    /** Runs `expression` in the page and returns its value (JSON-serializable). */
    async eval(expression) {
      const { result, exceptionDetails } = await send('Runtime.evaluate', {
        expression,
        returnByValue: true,
        awaitPromise: true,
      })
      if (exceptionDetails) throw new Error(`in page: ${exceptionDetails.exception?.description ?? exceptionDetails.text}`)
      return result.value
    },

    /** Calls `handler(payload)` when the page calls `window[name](payload)`. */
    async bind(name, handler) {
      bindingHandlers.set(name, handler)
      await send('Runtime.addBinding', { name })
    },

    async addInitScript(source) {
      await send('Page.addScriptToEvaluateOnNewDocument', { source })
    },

    async goto(url) {
      await send('Page.navigate', { url })
      await sleep(300)
    },

    async waitFor(expression, { timeout = 5000, what = expression } = {}) {
      const deadline = Date.now() + timeout
      while (Date.now() < deadline) {
        if (await page.eval(`Boolean(${expression})`)) return
        await sleep(40)
      }
      throw new Error(`timed out waiting for: ${what}`)
    },

    async mouseClick(x, y) {
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y })
      await send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', clickCount: 1 })
      await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', clickCount: 1 })
    },

    /** Presses a key. `modifiers`: any of 'ctrl', 'shift', 'alt', 'meta'. */
    async press(key, modifiers = []) {
      const mask = (modifiers.includes('alt') ? 1 : 0) | (modifiers.includes('ctrl') ? 2 : 0) | (modifiers.includes('meta') ? 4 : 0) | (modifiers.includes('shift') ? 8 : 0)
      const known = KEYS[key]
      const isChar = key.length === 1 && !known
      const code = known?.code ?? (/\d/.test(key) ? `Digit${key}` : `Key${key.toUpperCase()}`)
      const vk = known?.vk ?? key.toUpperCase().charCodeAt(0)
      const base = { key, code, windowsVirtualKeyCode: vk, modifiers: mask }
      await send('Input.dispatchKeyEvent', { type: isChar && !mask ? 'keyDown' : 'rawKeyDown', ...base, text: isChar && !mask ? key : undefined })
      await send('Input.dispatchKeyEvent', { type: 'keyUp', ...base })
    },

    async screenshot(path) {
      const { data } = await send('Page.captureScreenshot', { format: 'png' })
      writeFileSync(path, Buffer.from(data, 'base64'))
    },

    async close() {
      socket.close()
      process_.kill()
    },
  }
  return page
}

export { sleep }
