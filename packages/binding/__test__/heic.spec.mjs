import { promises as fs } from 'node:fs'
import os from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import test from 'ava'

import { JsColorType, Transformer } from '../index.js'

const __DIRNAME = join(fileURLToPath(import.meta.url), '..')
const ROOT_DIR = join(__DIRNAME, '..', '..', '..')

// HEIC decode is macOS-only (it delegates to the OS ImageIO HEVC decoder; we ship no codec).
// Gate so the same `yarn test` passes on Linux/Windows CI too.
const onMac = os.platform() === 'darwin' ? test : test.skip
const offMac = os.platform() !== 'darwin' ? test : test.skip

// 8-bit fixture: `sips -s format heic un-optimized.png --out un-optimized.heic` (1024x681).
const HEIC = await fs.readFile(join(ROOT_DIR, 'un-optimized.heic'))
// Genuine 10-bit fixture (HEVC Main10), generated via ImageIO from a 16-bit Display-P3 source.
const HEIC_10BIT = await fs.readFile(join(ROOT_DIR, 'un-optimized-10bit.heic'))

onMac('decodes heic metadata', async (t) => {
  const metadata = await new Transformer(HEIC).metadata()
  t.is(metadata.format, 'heic')
  t.is(metadata.width, 1024)
  t.is(metadata.height, 681)
  // `colorType` is the numeric `JsColorType` enum; the 8-bit decode yields RGBA8.
  t.is(metadata.colorType, JsColorType.Rgba8)
})

onMac('decodes heic to png/jpeg/webp', async (t) => {
  const png = await new Transformer(HEIC).png()
  t.true(Buffer.isBuffer(png))
  t.true(png.length > 0)

  const jpeg = await new Transformer(HEIC).jpeg(80)
  t.true(Buffer.isBuffer(jpeg))
  t.true(jpeg.length > 0)

  const webp = await new Transformer(HEIC).webp(80)
  t.true(Buffer.isBuffer(webp))
  t.true(webp.length > 0)
})

onMac('10-bit heic decodes to rgba16', async (t) => {
  const metadata = await new Transformer(HEIC_10BIT).metadata()
  t.is(metadata.format, 'heic')
  t.is(metadata.width, 256)
  t.is(metadata.height, 256)
  // `kCGImagePropertyDepth` reports 10 (> 8), so the 16-bit branch yields RGBA16.
  t.is(metadata.colorType, JsColorType.Rgba16)
})

onMac('10-bit heic re-encodes to png', async (t) => {
  // Exercises the rgba16 decode -> encode round-trip.
  const png = await new Transformer(HEIC_10BIT).png()
  t.true(Buffer.isBuffer(png))
  t.true(png.length > 0)
})

offMac('heic rejected off macOS', async (t) => {
  await t.throwsAsync(() => new Transformer(HEIC).metadata(), {
    message: /only supported on macOS/,
  })
})
