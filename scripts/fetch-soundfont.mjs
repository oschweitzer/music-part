// Downloads the SoundFont used for playback into assets/soundfonts/ and verifies its checksum.
// Usage: pnpm fetch:soundfont [--force]
// To change the SoundFont, update COMMIT, SHA256 and docs/soundfonts.md together.

import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const COMMIT = '97049183643d5fc5a9322a69c5b09efb667c6c3a'
const SHA256 = '9575028c7a1f589f5770fccc8cff2734566af40cd26ed836944e9a5152688cfe'
const URL = `https://raw.githubusercontent.com/mrbumpy409/GeneralUser-GS/${COMMIT}/GeneralUser-GS.sf2`

const targetDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'assets', 'soundfonts')
const target = join(targetDir, 'GeneralUser-GS.sf2')

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex')
}

if (existsSync(target) && !process.argv.includes('--force')) {
  if (sha256(readFileSync(target)) === SHA256) {
    console.log('GeneralUser-GS.sf2 is already present and valid.')
    process.exit(0)
  }
  console.log('The existing file does not match the expected checksum, downloading again.')
}

console.log(`Downloading ${URL}`)
const response = await fetch(URL)
if (!response.ok) {
  console.error(`Download failed: HTTP ${response.status}`)
  process.exit(1)
}

const bytes = Buffer.from(await response.arrayBuffer())
const actual = sha256(bytes)
if (actual !== SHA256) {
  console.error(`Checksum mismatch.\n  expected ${SHA256}\n  got      ${actual}\nThe file was not saved.`)
  process.exit(1)
}

mkdirSync(targetDir, { recursive: true })
const temporary = `${target}.part`
writeFileSync(temporary, bytes)
rmSync(target, { force: true })
renameSync(temporary, target)
console.log(`Saved ${target} (${(bytes.length / 1024 / 1024).toFixed(1)} MB)`)
