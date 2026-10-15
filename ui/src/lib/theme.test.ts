import { afterEach, describe, expect, it, vi } from 'vitest'
import { applyTheme, nextTheme, readTheme, saveTheme } from './theme'

afterEach(() => vi.unstubAllGlobals())

describe('nextTheme', () => {
  it('should go round system, light, dark', () => {
    expect(nextTheme('system')).toBe('light')
    expect(nextTheme('light')).toBe('dark')
    expect(nextTheme('dark')).toBe('system')
  })
})

describe('the saved theme', () => {
  const stubStorage = (store: Record<string, string>) =>
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => store[key] ?? null,
      setItem: (key: string, value: string) => void (store[key] = value),
    })

  it('should come back as it was saved', () => {
    stubStorage({})
    saveTheme('dark')
    expect(readTheme()).toBe('dark')
  })

  it('should be the system theme when nothing, or something unknown, was saved', () => {
    stubStorage({})
    expect(readTheme()).toBe('system')
    stubStorage({ 'music-part-theme': 'sepia' })
    expect(readTheme()).toBe('system')
  })

  it('should survive a storage that throws', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('blocked')
      },
      setItem: () => {
        throw new Error('blocked')
      },
    })
    expect(readTheme()).toBe('system')
    expect(() => saveTheme('light')).not.toThrow()
  })
})

describe('applyTheme', () => {
  it('should force a theme with an attribute and clear it for the system one', () => {
    const attributes = new Map<string, string>()
    const root = {
      setAttribute: (name: string, value: string) => void attributes.set(name, value),
      removeAttribute: (name: string) => void attributes.delete(name),
    }
    applyTheme({ theme: 'light', root })
    expect(attributes.get('data-theme')).toBe('light')
    applyTheme({ theme: 'system', root })
    expect(attributes.has('data-theme')).toBe(false)
  })
})
