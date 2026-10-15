/** The colour scheme: follow the system, or force the light or the dark one. */
export type Theme = 'system' | 'light' | 'dark'

const STORAGE_KEY = 'music-part-theme'
const THEMES: readonly Theme[] = ['system', 'light', 'dark']

export const themeLabel: Record<Theme, string> = { system: 'System', light: 'Light', dark: 'Dark' }

/** The theme the toggle moves to: system, then light, then dark, then system again. */
export function nextTheme(theme: Theme): Theme {
  return THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length] ?? 'system'
}

/** The saved choice. Storage can be missing or refuse (private windows), so anything odd means "system". */
export function readTheme(): Theme {
  try {
    const saved = globalThis.localStorage?.getItem(STORAGE_KEY)
    return THEMES.find((theme) => theme === saved) ?? 'system'
  } catch {
    return 'system'
  }
}

export function saveTheme(theme: Theme): void {
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, theme)
  } catch {
    // The choice just does not survive a restart.
  }
}

/** Marks the page with the forced theme; with "system" the attribute is absent and the CSS follows the OS. */
export function applyTheme({ theme, root }: { theme: Theme; root: Pick<Element, 'setAttribute' | 'removeAttribute'> }): void {
  if (theme === 'system') root.removeAttribute('data-theme')
  else root.setAttribute('data-theme', theme)
}
