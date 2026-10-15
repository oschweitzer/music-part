// Downloads the sounds used for playback into assets/soundfonts/ and verifies their checksums:
// the General MIDI SoundFont, and the samples of the sound packs in assets/soundfonts/packs/.
// Usage: pnpm fetch:soundfont [--force]
// To change a sound, update its entry below and docs/soundfonts.md together.

import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

import SevenZip from '7z-wasm'

const GENERAL_USER_COMMIT = '97049183643d5fc5a9322a69c5b09efb667c6c3a'

/**
 * `archive` entries are .7z files holding the sound bank at `entry`; two checksums are kept, of
 * the download and of the file that comes out, so a changed archive is caught either way.
 */
const ASSETS = [
  {
    name: 'GeneralUser-GS.sf2',
    url: `https://raw.githubusercontent.com/mrbumpy409/GeneralUser-GS/${GENERAL_USER_COMMIT}/GeneralUser-GS.sf2`,
    sha256: '9575028c7a1f589f5770fccc8cff2734566af40cd26ed836944e9a5152688cfe',
  },
  {
    name: 'packs/freepats-nylon-guitar/nylon-guitar.sf2',
    archive: {
      url: 'https://freepats.zenvoid.org/Guitar/SpanishClassicalGuitar/SpanishClassicalGuitar-SF2-20190618.7z',
      sha256: '8c192f1fc640a553199a4a43a17ae12354a6a05e3d6aad79bd43b5d29314ff51',
      entry: 'SpanishClassicalGuitar-SF2-20190618/SpanishClassicalGuitar-20190618.sf2',
    },
    sha256: '0d4c08ea8c1c8924b3829084b0c8778f6d6b906f331e44fde51bc159f76b03b9',
  },
  {
    name: 'packs/freepats-electric-guitar-clean/electric-guitar.sf2',
    archive: {
      url: 'https://github.com/freepats/electric-guitar-FSBS-clean/releases/download/2026-08-07/EGuitarFSBS-clean-bridge-small-SF2-20260807.7z',
      sha256: 'bcea7d4c92a1cc6a26e545a218b34e59037d4bab6cf0fce24396428e6934d69c',
      entry: 'EGuitarFSBS-clean bridge small SF2-20260807/EGuitarFSBS-clean bridge small 20260807.sf2',
    },
    sha256: 'a58803acd8640a06a0a1a277a16ff4cd2610d9eab5fd6793741db4730c12f594',
  },
  {
    name: 'packs/freepats-electric-bass-finger/electric-bass.sf2',
    archive: {
      url: 'https://github.com/freepats/electric-bass-YR/releases/download/2019-09-30/FingerBassYR-SF2-20190930.7z',
      sha256: 'f5b5cfe04435a9e9037d1f97d515dc596535acf26637db9579d724cd757f179e',
      entry: 'FingerBassYR SF2-20190930/FingerBassYR 20190930.sf2',
    },
    sha256: 'bd1332696f61c0cb254ce7f202fb6b945ff46ccb1dcdd62e345a50bde3afb165',
  },
]

const targetDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'assets', 'soundfonts')
const force = process.argv.includes('--force')

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex')
}

async function download(url) {
  console.log(`Downloading ${url}`)
  const response = await fetch(url)
  if (!response.ok) throw new Error(`download failed: HTTP ${response.status} for ${url}`)
  return Buffer.from(await response.arrayBuffer())
}

function checked(bytes, expected, what) {
  const actual = sha256(bytes)
  if (actual !== expected) {
    throw new Error(`checksum mismatch for ${what}\n  expected ${expected}\n  got      ${actual}\nnothing was saved.`)
  }
  return bytes
}

/** Reads one file out of a .7z archive, in memory. */
async function extract(archive, entry) {
  const sevenZip = await SevenZip()
  sevenZip.FS.writeFile('bank.7z', archive)
  sevenZip.callMain(['x', 'bank.7z', '-oout', '-y'])
  try {
    return Buffer.from(sevenZip.FS.readFile(`out/${entry}`))
  } catch {
    throw new Error(`${entry} is not in the archive`)
  }
}

async function fetchAsset(asset) {
  const target = join(targetDir, asset.name)
  if (existsSync(target) && !force) {
    if (sha256(readFileSync(target)) === asset.sha256) {
      console.log(`${asset.name} is already present and valid.`)
      return
    }
    console.log(`${asset.name} does not match the expected checksum, downloading again.`)
  }

  let bytes
  if (asset.archive) {
    const archive = checked(await download(asset.archive.url), asset.archive.sha256, asset.archive.url)
    bytes = await extract(archive, asset.archive.entry)
  } else {
    bytes = await download(asset.url)
  }
  checked(bytes, asset.sha256, asset.name)

  mkdirSync(dirname(target), { recursive: true })
  const temporary = `${target}.part`
  writeFileSync(temporary, bytes)
  rmSync(target, { force: true })
  renameSync(temporary, target)
  console.log(`Saved ${asset.name} (${(bytes.length / 1024 / 1024).toFixed(1)} MB)`)
}

try {
  for (const asset of ASSETS) await fetchAsset(asset)
} catch (error) {
  console.error(error instanceof Error ? error.message : error)
  process.exit(1)
}
