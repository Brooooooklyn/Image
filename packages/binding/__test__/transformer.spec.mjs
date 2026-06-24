import { promises as fs } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import test from 'ava'
import { decode } from 'blurhash'

import { JsColorType, ResizeFit, Transformer } from '../index.js'

const __DIRNAME = join(fileURLToPath(import.meta.url), '..')
const ROOT_DIR = join(__DIRNAME, '..', '..', '..')

const PNG = await fs.readFile(join(ROOT_DIR, 'un-optimized.png'))
const JPEG = await fs.readFile(join(ROOT_DIR, 'un-optimized.jpg'))
const WITH_EXIF_JPG = await fs.readFile(join(ROOT_DIR, 'with-exif.jpg'))
const SVG = await fs.readFile(join(ROOT_DIR, 'input-debian.svg'))

test('should be able to get metadata from png', async (t) => {
  const decoder = new Transformer(PNG)
  const metadata = await decoder.metadata()
  t.is(metadata.width, 1024)
  t.is(metadata.height, 681)
})

test('should be able to get metadata from jpg', async (t) => {
  const decoder = new Transformer(JPEG)
  const metadata = await decoder.metadata()
  t.is(metadata.width, 1024)
  t.is(metadata.height, 678)
})

test('should be able to get exif from jpg', async (t) => {
  const decoder = new Transformer(WITH_EXIF_JPG)
  const metadata = await decoder.metadata(true)
  t.snapshot(metadata)
  t.is(metadata.orientation, 5)
  t.is(metadata.format, 'jpeg')
})

test('should be able to get metadata from jpg - sync', (t) => {
  const decoder = new Transformer(JPEG)
  const metadata = decoder.metadataSync()
  t.is(metadata.width, 1024)
  t.is(metadata.height, 678)
})

test('should be able to encode into webp', async (t) => {
  const decoder = new Transformer(PNG)
  await t.notThrowsAsync(() => decoder.webp(75))
})

test('should be able to decode from avif', async (t) => {
  const decoder = new Transformer(PNG)
  const AVIF = await decoder.avif({
    speed: 10,
    threads: 1,
  })
  const avifDecoder = new Transformer(AVIF)
  await t.notThrowsAsync(() => avifDecoder.png())
})

test('should be able to decode from webp', async (t) => {
  const decoder = new Transformer(PNG)
  const WEBP = await decoder.webp()
  const webpDecoder = new Transformer(WEBP)
  await t.notThrowsAsync(() => webpDecoder.png())
})

test('should be able to create transformer from raw rgba pixels', async (t) => {
  const pixels = decode('LEHV6nWB2yk8pyo0adR*.7kCMdnj', 32, 32)
  await t.notThrowsAsync(() => Transformer.fromRgbaPixels(pixels, 32, 32).webp())
})

// Regression for https://github.com/Brooooooklyn/Image/issues/158 (part 2):
// raw RGBA pixel input keeps reporting format "png" even though SVG input now reports "svg".
test('raw rgba pixels still report format "png" (#158)', (t) => {
  const pixels = decode('LEHV6nWB2yk8pyo0adR*.7kCMdnj', 32, 32)
  const meta = Transformer.fromRgbaPixels(pixels, 32, 32).metadataSync()
  t.is(meta.format, 'png')
})

test('should be able to create transformer from SVG', (t) => {
  t.notThrows(() => Transformer.fromSvg(SVG).jpegSync())
})

// Regression test for https://github.com/Brooooooklyn/Image/issues/199
// Each fixture stores the inverse of a canonical upright scene tagged with its EXIF
// orientation, so a correct `.rotate()` must reproduce the same upright scene:
//   TL = red, TR = green, BL = blue, BR = white   (canonical 64x48)
const ORIENTATION_DIR = join(__DIRNAME, 'orientation')
const CANONICAL_WIDTH = 64
const CANONICAL_HEIGHT = 48

// Classify a pixel into the canonical corner colors via high/low channels,
// robust to JPEG decode rounding.
function classify(r, g, b) {
  const hi = (v) => v >= 160
  const lo = (v) => v <= 95
  if (hi(r) && lo(g) && lo(b)) return 'red'
  if (lo(r) && hi(g) && lo(b)) return 'green'
  if (lo(r) && lo(g) && hi(b)) return 'blue'
  if (hi(r) && hi(g) && hi(b)) return 'white'
  return `unknown(${r},${g},${b})`
}

for (let orientation = 1; orientation <= 8; orientation++) {
  test(`rotate() honors exif orientation ${orientation} (#199)`, async (t) => {
    const buffer = await fs.readFile(join(ORIENTATION_DIR, `orientation_${orientation}.jpg`))

    // The fixture must carry the orientation we expect to exercise.
    const metadata = await new Transformer(buffer).metadata(true)
    t.is(metadata.orientation, orientation)

    const raw = await new Transformer(buffer).rotate().rawPixels()
    const bpp = raw.length / (CANONICAL_WIDTH * CANONICAL_HEIGHT)
    t.is(bpp, 3, 'output must be the upright canonical size (64x48 RGB)')

    const pixel = (x, y) => {
      const offset = (y * CANONICAL_WIDTH + x) * bpp
      return classify(raw[offset], raw[offset + 1], raw[offset + 2])
    }
    // centers of each quadrant
    t.is(pixel(16, 12), 'red', 'top-left')
    t.is(pixel(48, 12), 'green', 'top-right')
    t.is(pixel(16, 36), 'blue', 'bottom-left')
    t.is(pixel(48, 36), 'white', 'bottom-right')
  })
}

// Calling `metadata()` (which does not parse EXIF) before `.rotate()` on the same
// Transformer must not poison the cache and turn `.rotate()` into a no-op (#199).
test('rotate() still applies exif orientation after metadata() (#199)', async (t) => {
  const buffer = await fs.readFile(join(ORIENTATION_DIR, 'orientation_6.jpg'))
  const transformer = new Transformer(buffer)
  await transformer.metadata() // no EXIF requested
  const raw = await transformer.rotate().rawPixels()
  const bpp = raw.length / (CANONICAL_WIDTH * CANONICAL_HEIGHT)
  t.is(bpp, 3, 'output must be the upright canonical size (64x48 RGB)')
  const pixel = (x, y) => {
    const offset = (y * CANONICAL_WIDTH + x) * bpp
    return classify(raw[offset], raw[offset + 1], raw[offset + 2])
  }
  t.is(pixel(16, 12), 'red', 'top-left')
  t.is(pixel(48, 12), 'green', 'top-right')
  t.is(pixel(16, 36), 'blue', 'bottom-left')
  t.is(pixel(48, 36), 'white', 'bottom-right')
})

// --- metadata() must reflect pending transforms (issue #158) ---

// Re-decode an encoded buffer to learn what the real output dims/colorType are.
const roundTripMeta = async (buffer) => new Transformer(buffer).metadata()

test('metadata() reflects pending resize (async + sync, #158)', async (t) => {
  const expected = await roundTripMeta(await new Transformer(PNG).resize(256).png())

  const meta = await new Transformer(PNG).resize(256).metadata()
  t.is(meta.width, 256)
  t.is(meta.width, expected.width)
  t.is(meta.height, expected.height)

  const metaSync = new Transformer(PNG).resize(256).metadataSync()
  t.is(metaSync.width, 256)
  t.is(metaSync.width, expected.width)
  t.is(metaSync.height, expected.height)
})

test('metadata() reflects resize Cover (exact dims, #158)', async (t) => {
  const meta = await new Transformer(PNG).resize({ width: 200, height: 100, fit: ResizeFit.Cover }).metadata()
  t.is(meta.width, 200)
  t.is(meta.height, 100)
})

test('metadata() reflects resize Inside (aspect-clamped, #158)', async (t) => {
  // 1024x681 clamped Inside a 200x100 box keeps aspect -> NOT 200x100.
  const expected = await roundTripMeta(
    await new Transformer(PNG).resize({ width: 200, height: 100, fit: ResizeFit.Inside }).png(),
  )
  const meta = await new Transformer(PNG).resize({ width: 200, height: 100, fit: ResizeFit.Inside }).metadata()
  // Aspect-clamped: 1024x681 is wider than 200x100, so width is the limiting
  // dimension and shrinks below 200 (NOT a forced 200x100).
  t.not(meta.width, 200)
  t.is(meta.width, expected.width)
  t.is(meta.height, expected.height)
})

test('metadata() reflects crop (#158)', async (t) => {
  const meta = await new Transformer(PNG).crop(0, 0, 100, 50).metadata()
  t.is(meta.width, 100)
  t.is(meta.height, 50)
})

test('metadata() reflects out-of-bounds crop, clamped (#158)', async (t) => {
  // crop_imm clamps the rect to the image bounds.
  const expected = await roundTripMeta(await new Transformer(PNG).crop(1000, 600, 500, 500).png())
  const meta = await new Transformer(PNG).crop(1000, 600, 500, 500).metadata()
  t.is(meta.width, expected.width)
  t.is(meta.height, expected.height)
})

test('metadata() reflects rotate() exif orientation swap (#158)', async (t) => {
  const buffer = await fs.readFile(join(ORIENTATION_DIR, 'orientation_6.jpg'))
  const expected = await roundTripMeta(await new Transformer(buffer).rotate().png())
  const meta = await new Transformer(buffer).rotate().metadata(true)
  t.is(meta.width, expected.width)
  t.is(meta.height, expected.height)
  // orientation field is still reported
  t.is(meta.orientation, 6)
})

test('metadata() reflects grayscale colorType (#158)', async (t) => {
  const expected = await roundTripMeta(await new Transformer(PNG).grayscale().png())
  const meta = await new Transformer(PNG).grayscale().metadata()
  t.is(meta.colorType, expected.colorType)
  t.not(meta.colorType, JsColorType.Rgb8)
})

test('metadata() reflects fastResize dims + Rgba8 colorType (#158)', async (t) => {
  const expected = await roundTripMeta(
    await new Transformer(PNG).fastResize({ width: 256 }).png(),
  )
  const meta = await new Transformer(PNG).fastResize({ width: 256 }).metadata()
  t.is(meta.width, 256)
  t.is(meta.width, expected.width)
  t.is(meta.height, expected.height)
  t.is(meta.colorType, JsColorType.Rgba8)
})

test('metadata() reflects chained rotate().resize().crop() in order (#158)', async (t) => {
  const buffer = await fs.readFile(join(ORIENTATION_DIR, 'orientation_6.jpg'))
  const expected = await roundTripMeta(
    await new Transformer(buffer).rotate().resize(300).crop(10, 10, 100, 80).png(),
  )
  const meta = await new Transformer(buffer).rotate().resize(300).crop(10, 10, 100, 80).metadata(true)
  t.is(meta.width, expected.width)
  t.is(meta.height, expected.height)
})

// Guard: metadata() must compute on a CLONE and never mutate the shared cache,
// so a subsequent encode applies the transform exactly once (#158).
test('metadata() does not mutate cache; encode applies transform once (#158)', async (t) => {
  const transformer = new Transformer(PNG)
  transformer.resize(256)
  const meta = await transformer.metadata()
  t.is(meta.width, 256)
  const encoded = await transformer.png()
  const after = await roundTripMeta(encoded)
  t.is(after.width, 256, 'resize applied exactly once, not re-resized')
})
