import { promises as fs } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import test from 'ava'

import { losslessCompressPng, pngQuantize, compressJpeg, Transformer } from '../index.js'

const ROOT_DIR = join(fileURLToPath(import.meta.url), '..', '..', '..', '..')

const PNG = await fs.readFile(join(ROOT_DIR, 'un-optimized.png'))
const JPEG = await fs.readFile(join(ROOT_DIR, 'un-optimized.jpg'))

test('should be able to lossless optimize png image', async (t) => {
  if (process.env.NAPI_RS_FORCE_WASI) {
    t.pass()
    return
  }
  const dest = await losslessCompressPng(PNG)
  t.true(dest.length < PNG.length)
})

test('should be able to lossy optimize png image', async (t) => {
  if (process.env.NAPI_RS_FORCE_WASI) {
    t.pass()
    return
  }
  const dest = await pngQuantize(PNG, { speed: 5, maxQuality: 1, minQuality: 1 })
  t.true(dest.length < PNG.length)
})

test('should be able to lossless optimize jpeg image', async (t) => {
  const dest = await compressJpeg(JPEG, { quality: 100 })
  t.true(dest.length < PNG.length)
})

test('should be able to lossy encode webp from png', (t) => {
  t.true(new Transformer(PNG).webpSync(90).length < PNG.length)
})

test('should be able to lossy encode webp from jpeg', (t) => {
  t.true(new Transformer(JPEG).webpSync(90).length < JPEG.length)
})

test('should be able to lossless encode webp from png', (t) => {
  t.true(new Transformer(PNG).webpLosslessSync().length < PNG.length)
})

test('should be able to lossless encode webp from jpeg', (t) => {
  t.notThrows(() => {
    new Transformer(JPEG).webpLosslessSync()
  })
})

test('should be able to encode avif from png', (t) => {
  t.true(new Transformer(PNG).avifSync({
    speed: 10,
    threads: 1,
  }).length < PNG.length)
})

test('should be able to encode avif from jpeg', (t) => {
  t.true(new Transformer(JPEG).avifSync({
    speed: 10,
    threads: 1,
  }).length < JPEG.length)
})
