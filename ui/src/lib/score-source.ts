import { invoke } from '@tauri-apps/api/core'
import type { Score } from './score-types'

/** True when running inside the Tauri window rather than a plain browser. */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

/** Loads the demo score from Rust in the app, or from the exported JSON in a browser. */
export async function loadDemoScore(): Promise<Score> {
  if (isTauri()) return invoke<Score>('demo_score')

  const response = await fetch('/demo-score.json')
  if (!response.ok) throw new Error(`Could not load demo score: HTTP ${response.status}`)
  return (await response.json()) as Score
}
