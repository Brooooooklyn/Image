import { promises as fs } from 'fs'
import { join } from 'path'
import { fileURLToPath } from 'url'

import test from 'ava'

import { losslessCompressPng, compressJpeg, encodeWebp, losslessEncodeWebp, encodeAvif } from '../index.js'

const ROOT_DIR = join(fileURLToPath(import.meta.url), '..', '..', '..', '..')

const PNG = await fs.readFile(join(ROOT_DIR, 'un-optimized.png'))
const JPEG = await fs.readFile(join(ROOT_DIR, 'un-optimized.jpg'))

test('should be able to lossless optimize png image', async (t) => {
  const dest = losslessCompressPng(PNG)
  t.true(dest.length < PNG.length)
})

test('should be able to lossless optimize jpeg image', async (t) => {
  const dest = compressJpeg(JPEG, { quality: 100 })
  t.true(dest.length < PNG.length)
})

test('should be able to lossy encode webp from png', (t) => {
  t.true(encodeWebp(PNG, 90).length < PNG.length)
})

test('should be able to lossy encode webp from jpeg', (t) => {
  t.true(encodeWebp(JPEG, 90).length < JPEG.length)
})

test('should be able to lossless encode webp from png', (t) => {
  t.true(losslessEncodeWebp(PNG).length < PNG.length)
})

test('should be able to lossless encode webp from jpeg', (t) => {
  t.notThrows(() => {
    losslessEncodeWebp(JPEG)
  })
})

test('should be able to encode avif from png', (t) => {
  t.true(encodeAvif(PNG).length < PNG.length)
})

test('should be able to encode avif from jpeg', (t) => {
  t.true(encodeAvif(JPEG).length < JPEG.length)
})
