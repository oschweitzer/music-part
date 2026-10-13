import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vitest/config'

// Tauri expects a fixed dev server port (see src-tauri/tauri.conf.json).
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  test: { include: ['src/**/*.test.ts'] },
})
