// Injected into the page before the UI loads: it stands in for the Tauri window.
// Commands go to the Rust document backend through `window.e2eInvoke`; dialogs are answered from
// queues the tests fill in `window.__e2e.answers`.
;(() => {
  const AUDIO_COMMANDS = ['play', 'pause', 'stop', 'seek', 'set_audible']
  const callbacks = new Map()
  const pending = new Map()
  let next = 1

  window.__e2e = {
    answers: { open: [], save: [], ask: [] },
    asked: [],
    titles: [],
    // The audio commands the UI sent: there is no sound card here, the backend only holds the document.
    audio: [],
  }

  // Called by the test runner with the backend's answer.
  window.__e2e_resolve = (id, ok, value) => {
    const request = pending.get(id)
    pending.delete(id)
    if (request) (ok ? request.resolve : request.reject)(value)
  }

  const plugin = (command, args) => {
    switch (command) {
      case 'plugin:event|listen':
        return Promise.resolve(next++)
      case 'plugin:dialog|open':
        return Promise.resolve(window.__e2e.answers.open.shift() ?? null)
      case 'plugin:dialog|save':
        return Promise.resolve(window.__e2e.answers.save.shift() ?? null)
      case 'plugin:dialog|message': {
        // `ask` is a message with two buttons, and returns the label of the one pressed.
        window.__e2e.asked.push(args.message)
        const yes = window.__e2e.answers.ask.shift() ?? false
        // With custom labels the plugin sends { OkCancelCustom: [ok, cancel] }.
        const labels = args.buttons?.OkCancelCustom
        const accept = labels ? labels[0] : 'Yes'
        const refuse = labels ? labels[1] : 'No'
        return Promise.resolve(yes ? accept : refuse)
      }
      case 'plugin:window|set_title':
        window.__e2e.titles.push(args.value)
        return Promise.resolve()
      default:
        return Promise.resolve(null)
    }
  }

  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: 'main' }, currentWebview: { windowLabel: 'main', label: 'main' } },
    transformCallback(callback) {
      const id = next++
      callbacks.set(id, callback)
      return id
    },
    unregisterCallback: (id) => callbacks.delete(id),
    convertFileSrc: (path) => path,
    invoke(command, args) {
      if (command.startsWith('plugin:')) return plugin(command, args)
      if (AUDIO_COMMANDS.includes(command)) {
        window.__e2e.audio.push({ command, args: args ?? {} })
        return Promise.resolve(null)
      }
      return new Promise((resolve, reject) => {
        const id = next++
        pending.set(id, { resolve, reject })
        window.e2eInvoke(JSON.stringify({ id, cmd: command, args: args ?? {} }))
      })
    },
  }
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} }
})()
