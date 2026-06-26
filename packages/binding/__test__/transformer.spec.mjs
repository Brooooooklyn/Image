import { promises as fs } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import test from 'ava'
import { decode } from 'blurhash'

import { BlendMode, Gravity, JsColorType, ResizeFit, Transformer } from '../index.js'

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
  // A staged rotate bakes orientation into the previewed (upright) dims, so the
  // reported orientation is normalized to undefined to match the encoded output
  // (sharp autoOrient / ImageMagick -auto-orient / Pillow exif_transpose).
  t.is(meta.orientation, undefined)
  t.is(expected.orientation, undefined, 'encoded round-trip carries no orientation')
})

// A staged rotate normalizes orientation/EXIF in the metadata() preview: the
// orientation field is dropped (undefined) AND the now-stale `Orientation` EXIF
// key is removed, while the rest of the source EXIF is retained (#158).
test('metadata(true) drops Orientation EXIF key after rotate(), keeps other tags (#158)', async (t) => {
  // Source EXIF (no rotate) carries Orientation + other tags.
  const source = await new Transformer(WITH_EXIF_JPG).metadata(true)
  t.true('Orientation' in source.exif, 'source has an Orientation EXIF key')
  const otherKeys = Object.keys(source.exif).filter((k) => k !== 'Orientation')
  t.true(otherKeys.length > 0, 'source has at least one non-Orientation EXIF key')

  const meta = await new Transformer(WITH_EXIF_JPG).rotate().metadata(true)
  // Orientation normalized away.
  t.is(meta.orientation, undefined)
  // EXIF is still present but the stale Orientation key is gone...
  t.truthy(meta.exif)
  t.false('Orientation' in meta.exif, 'stale Orientation EXIF key dropped')
  // ...and at least one other source EXIF tag is retained.
  for (const k of otherKeys) {
    t.is(meta.exif[k], source.exif[k], `retained source EXIF tag ${k}`)
  }

  // Cross-check: the encoded .rotate().png() round-trip carries no orientation.
  const roundTrip = await roundTripMeta(await new Transformer(WITH_EXIF_JPG).rotate().png())
  t.is(roundTrip.orientation, undefined, 'encoded output has no orientation')
})

test('metadata() reflects grayscale colorType (#158)', async (t) => {
  const expected = await roundTripMeta(await new Transformer(PNG).grayscale().png())
  const meta = await new Transformer(PNG).grayscale().metadata()
  t.is(meta.colorType, expected.colorType)
  t.not(meta.colorType, JsColorType.Rgb8)
})

test('metadata() reflects fastResize dims + Rgba8 colorType (#158)', async (t) => {
  const expected = await roundTripMeta(await new Transformer(PNG).fastResize({ width: 256 }).png())
  const meta = await new Transformer(PNG).fastResize({ width: 256 }).metadata()
  t.is(meta.width, 256)
  t.is(meta.width, expected.width)
  t.is(meta.height, expected.height)
  t.is(meta.colorType, JsColorType.Rgba8)
})

test('metadata() reflects chained rotate().resize().crop() in order (#158)', async (t) => {
  const buffer = await fs.readFile(join(ORIENTATION_DIR, 'orientation_6.jpg'))
  const expected = await roundTripMeta(await new Transformer(buffer).rotate().resize(300).crop(10, 10, 100, 80).png())
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

// --- metadata(false) must NOT leak EXIF/orientation a pending rotate forced us
// to parse (adversarial-review finding F1, HIGH, #158) ---
// A pending `.rotate()` makes compute() parse EXIF (to swap dims), but with
// `with_exif=false` the caller never asked for EXIF/orientation, so they must
// stay suppressed. The dim-swap fix must remain.
test('metadata() (withExif=false) suppresses EXIF/orientation when rotate pending (#158, F1)', async (t) => {
  const buffer = await fs.readFile(join(ORIENTATION_DIR, 'orientation_6.jpg'))
  // Dims still swapped: cross-check against the same value with EXIF requested.
  const withExif = await new Transformer(buffer).rotate().metadata(true)
  const meta = await new Transformer(buffer).rotate().metadata()
  t.is(meta.width, withExif.width)
  t.is(meta.height, withExif.height)
  t.not(meta.width, meta.height, 'dimensions are swapped (portrait -> landscape)')
  // The leak: must NOT surface EXIF/orientation the caller never requested.
  t.is(meta.exif, undefined)
  t.is(meta.orientation, undefined)
})

test('metadata() (withExif=false) suppresses rich EXIF on with-exif.jpg when rotate pending (#158, F1)', async (t) => {
  const meta = await new Transformer(WITH_EXIF_JPG).rotate().metadata()
  t.is(meta.exif, undefined)
  t.is(meta.orientation, undefined)
  // dims still rotated (orientation 5 swaps width/height)
  const withExif = await new Transformer(WITH_EXIF_JPG).rotate().metadata(true)
  t.is(meta.width, withExif.width)
  t.is(meta.height, withExif.height)
})

// Regression guard: metadata(true) is UNCHANGED — EXIF + orientation still present.
test('metadata(true) still returns EXIF + orientation on with-exif.jpg (#158, F1 guard)', async (t) => {
  const meta = await new Transformer(WITH_EXIF_JPG).metadata(true)
  t.is(meta.orientation, 5)
  t.truthy(meta.exif)
  t.true(Object.keys(meta.exif).length > 0)
})

// --- raw_pixels_sync() must apply staged transforms like every other *_sync
// encoder (adversarial-review finding F2, MEDIUM, #158) ---
test('rawPixelsSync() reflects resize, matching async rawPixels() (#158, F2)', async (t) => {
  const sm = new Transformer(PNG).resize(256).metadataSync()
  const channels = 4 // PNG fixture decodes to RGBA8
  const expected = sm.width * sm.height * channels
  const syncLen = new Transformer(PNG).resize(256).rawPixelsSync().length
  const asyncLen = (await new Transformer(PNG).resize(256).rawPixels()).length
  t.is(syncLen, asyncLen, 'sync raw pixels match async raw pixels')
  t.is(syncLen, expected, 'sync raw pixels length == width*height*channels of resized dims')
})

test('rawPixelsSync() reflects crop (#158, F2)', async (t) => {
  const sm = new Transformer(PNG).crop(0, 0, 100, 50).metadataSync()
  const channels = 4
  const expected = sm.width * sm.height * channels
  const syncLen = new Transformer(PNG).crop(0, 0, 100, 50).rawPixelsSync().length
  const asyncLen = (await new Transformer(PNG).crop(0, 0, 100, 50).rawPixels()).length
  t.is(syncLen, asyncLen)
  t.is(syncLen, expected)
})

// Sanity: a no-transform rawPixelsSync() is unchanged (matches the decoded dims).
test('rawPixelsSync() with no transform == decoded width*height*channels (#158, F2)', (t) => {
  const sm = new Transformer(PNG).metadataSync()
  const expected = sm.width * sm.height * 4
  t.is(new Transformer(PNG).rawPixelsSync().length, expected)
})

// --- Transformer reuse must be idempotent: encode must NOT mutate the shared
// cached decode, so the staged transforms apply exactly once on every call
// (adversarial-review round 2; encode on a clone / no cache mutation, #158) ---

// crop: metadata before an encode == metadata after the encode (no double-crop).
test('reuse: crop metadata is stable across an encode (no cache mutation, #158)', (t) => {
  const transformer = new Transformer(PNG).crop(10, 10, 100, 80)
  const before = transformer.metadataSync()
  t.is(before.width, 100)
  t.is(before.height, 80)
  // Encoding must not re-crop the cached decode in place.
  transformer.pngSync()
  const after = transformer.metadataSync()
  t.is(after.width, 100, 'crop applied once, not re-cropped by the prior encode')
  t.is(after.height, 80)
  // rawPixels length stays consistent with 100x80 across calls.
  const channels = 4 // PNG fixture decodes to RGBA8
  t.is(transformer.rawPixelsSync().length, 100 * 80 * channels)
  t.is(transformer.rawPixelsSync().length, 100 * 80 * channels)
})

// two encodes on one instance must produce the same dimensions.
test('reuse: two encodes on one instance produce identical dims (#158)', (t) => {
  const transformer = new Transformer(PNG).crop(10, 10, 100, 80)
  const d1 = new Transformer(transformer.pngSync()).metadataSync()
  const d2 = new Transformer(transformer.pngSync()).metadataSync()
  t.is(d1.width, 100)
  t.is(d1.height, 80)
  t.is(d2.width, d1.width, 'second encode is not double-cropped')
  t.is(d2.height, d1.height)
})

// no-transform encode borrows the cached decode read-only; encoding twice on the
// same instance must yield byte-identical output (borrow path leaves cache intact, PR #218).
test('reuse: no-transform encode borrows cache, two encodes are byte-identical (#158)', (t) => {
  const transformer = new Transformer(PNG)
  const first = transformer.pngSync()
  const second = transformer.pngSync()
  t.true(Buffer.from(first).equals(Buffer.from(second)), 'borrow path leaves the cached decode untouched')
})

// https://github.com/Brooooooklyn/Image/issues/42 — set an image's alpha (opacity).
// `opacity(factor)` multiplies the existing alpha by `factor` (0..1), like CSS opacity.
test('opacity(factor) multiplies the alpha channel (#42)', async (t) => {
  // Fully-opaque 2x2 RGBA (every alpha = 255).
  const opaque = Uint8Array.from([255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255])
  const raw = await Transformer.fromRgbaPixels(opaque, 2, 2).opacity(0.5).rawPixels()
  // Alpha halved (round(255 * 0.5) = 128); RGB untouched.
  t.deepEqual([raw[3], raw[7], raw[11], raw[15]], [128, 128, 128, 128])
  t.is(raw[0], 255)
  t.is(raw[1], 0)
  t.is(raw[2], 0)
})

test('opacity(0) is fully transparent, opacity(1) leaves alpha unchanged (#42)', async (t) => {
  const opaque = Uint8Array.from([10, 20, 30, 255])
  const zero = await Transformer.fromRgbaPixels(opaque, 1, 1).opacity(0).rawPixels()
  t.is(zero[3], 0)
  const one = await Transformer.fromRgbaPixels(opaque, 1, 1).opacity(1).rawPixels()
  t.is(one[3], 255)
})

test('opacity clamps factor above 1 (#42)', async (t) => {
  const opaque = Uint8Array.from([10, 20, 30, 200])
  const raw = await Transformer.fromRgbaPixels(opaque, 1, 1).opacity(2).rawPixels()
  // Clamped to 1.0, so the source alpha is preserved, never overflowing past 255.
  t.is(raw[3], 200)
})

test('opacity scales existing partial alpha (#42)', async (t) => {
  // Source alpha already 100; opacity(0.5) -> round(100 * 0.5) = 50.
  const semi = Uint8Array.from([10, 20, 30, 100])
  const raw = await Transformer.fromRgbaPixels(semi, 1, 1).opacity(0.5).rawPixels()
  t.is(raw[3], 50)
})

test('opacity converts an RGB source to RGBA in metadata (#42)', async (t) => {
  // JPEG decodes to Rgb8 (no alpha); opacity must promote it to Rgba8.
  const meta = await new Transformer(JPEG).opacity(0.5).metadata()
  t.is(meta.colorType, JsColorType.Rgba8)
})

// `adjustContrast` (and other value filters) map every channel including alpha, so
// opacity scales the SOURCE alpha captured before them — otherwise contrast mangles
// the alpha and the `new = old * factor` contract breaks. Regression for the Codex
// review on #42. Both positive and (the harder) negative contrast must hold.
test('opacity scales the source alpha regardless of a staged contrast filter (#42)', async (t) => {
  const opaque = Uint8Array.from([100, 120, 140, 255])
  const hi = await Transformer.fromRgbaPixels(opaque, 1, 1).opacity(0.25).adjustContrast(100).rawPixels()
  t.is(hi[3], 64, 'round(255 * 0.25) survives positive contrast')
  t.not(hi[0], 100, 'contrast still affects the RGB channels')
  // Negative contrast pulls 255 toward 128; opacity must still scale the original 255.
  const lo = await Transformer.fromRgbaPixels(opaque, 1, 1).opacity(0.5).adjustContrast(-50).rawPixels()
  t.is(lo[3], 128, 'round(255 * 0.5) survives negative contrast')
})

// A spatial filter (blur) feathers the alpha channel on purpose, so opacity must
// scale that FEATHERED alpha — not restore the pre-blur hard mask. adjustContrast's
// alpha change is undone right after it runs, so spatial filters always feather the
// true alpha. Regression for the Codex review on #42.
test('opacity keeps a spatial filter (blur) alpha feathering (#42)', async (t) => {
  // 4x1 with a hard alpha edge: [0, 0, 255, 255]. blur softens the boundary.
  const edge = Uint8Array.from([10, 10, 10, 0, 10, 10, 10, 0, 10, 10, 10, 255, 10, 10, 10, 255])
  const raw = await Transformer.fromRgbaPixels(edge, 4, 1).blur(1.2).opacity(0.5).rawPixels()
  const alpha = [raw[3], raw[7], raw[11], raw[15]]
  t.true(alpha[1] > 0, 'blur feathering reaches the transparent side (not a hard 0)')
  t.true(alpha[2] < 128, 'blur feathering softens the opaque side (not a hard 128)')
  t.true(alpha[0] <= alpha[1] && alpha[1] <= alpha[2] && alpha[2] <= alpha[3], 'monotonic gradient')
})

// The hardest case: adjustContrast AND a spatial filter staged together. Contrast
// mangles alpha, but its change is reverted right after it runs, so the LATER blur
// still feathers the true alpha and opacity scales that feathered result. The earlier
// fix (snapshot whenever contrast is present, restore at opacity time) collapsed this
// back to the hard mask. Regression for the Cursor/Codex review on #42 (commit 917d261).
test('opacity keeps blur feathering even when adjustContrast is also staged (#42)', async (t) => {
  const edge = Uint8Array.from([10, 10, 10, 0, 10, 10, 10, 0, 10, 10, 10, 255, 10, 10, 10, 255])
  const raw = await Transformer.fromRgbaPixels(edge, 4, 1).adjustContrast(100).blur(1.2).opacity(0.5).rawPixels()
  const alpha = [raw[3], raw[7], raw[11], raw[15]]
  t.true(alpha[1] > 0, 'blur feathering reaches the transparent side despite contrast')
  t.true(alpha[2] < 128, 'blur feathering softens the opaque side despite contrast')
  t.true(alpha[0] <= alpha[1] && alpha[1] <= alpha[2] && alpha[2] <= alpha[3], 'monotonic gradient despite contrast')
})

// Grayscale (LumaA) path: grayscale() yields a LumaA image, so these exercise the
// alpha-preserving contrast/huerotate helpers on a non-RGB color model in the CI test
// path. adjustContrast must not touch the alpha that opacity scales.
test('opacity on a grayscale (LumaA) source preserves alpha through contrast (#42)', async (t) => {
  const gray = Uint8Array.from([120, 120, 120, 200])
  const raw = await Transformer.fromRgbaPixels(gray, 1, 1).grayscale().adjustContrast(80).opacity(0.5).rawPixels()
  // alpha = round(200 * 0.5) = 100, independent of the staged contrast.
  t.is(raw[3], 100, 'contrast must not change the alpha opacity scales on the LumaA path')
})

// Hue rotation of grayscale has no hue to rotate: the luma must survive (the crate
// emitted garbage by treating luma/alpha as RGB). opacity(1) promotes LumaA -> RGBA so
// rawPixels gives R=G=B=luma.
test('huerotate leaves a grayscale (LumaA) luma unchanged (#42)', async (t) => {
  const gray = Uint8Array.from([120, 120, 120, 255])
  const raw = await Transformer.fromRgbaPixels(gray, 1, 1).grayscale().huerotate(90).opacity(1).rawPixels()
  t.true(Math.abs(raw[0] - 120) <= 1, `grayscale R preserved through huerotate; got ${raw[0]}`)
  t.true(Math.abs(raw[2] - 120) <= 1, `grayscale B preserved through huerotate; got ${raw[2]}`)
  t.is(raw[3], 255, 'alpha preserved')
})

// The #42 "overlap two images without hiding the bottom" use case: fade the TOP
// image with opacity, then composite it over the bottom with overlay. The bottom
// must remain visible through the now semi-transparent top.
test('opacity on the overlay source blends, keeping the bottom visible (#42)', async (t) => {
  const blueOpaque = Uint8Array.from([0, 0, 255, 255])
  const redOpaque = Uint8Array.from([255, 0, 0, 255])
  // Fade the red top to ~50% and encode it for use as an overlay layer.
  const fadedTop = await Transformer.fromRgbaPixels(redOpaque, 1, 1).opacity(0.5).png()
  const raw = await Transformer.fromRgbaPixels(blueOpaque, 1, 1).overlay(fadedTop, 0, 0).rawPixels()
  // Result is a red/blue blend: the top contributes red AND the bottom blue still
  // shows through (neither pure red nor pure blue).
  t.true(raw[0] > 100, 'top red contributes')
  t.true(raw[2] > 100, 'bottom blue is not hidden')
  // Compositing over an opaque bottom stays (essentially) opaque; the image crate's
  // integer "over" blend can round 255 down by 1.
  t.true(raw[3] >= 254, 'composite over an opaque bottom stays opaque')
})

// rotate: metadata before == metadata after a rawPixels()/encode (no double-rotate).
test('reuse: rotate metadata is stable across raw pixels + encode (#158)', async (t) => {
  const buffer = await fs.readFile(join(ORIENTATION_DIR, 'orientation_6.jpg'))
  const transformer = new Transformer(buffer).rotate()
  // Cross-check the upright dims against this instance's own first encode round-trip.
  const upright = new Transformer(transformer.pngSync()).metadataSync()
  t.is(upright.width, CANONICAL_WIDTH)
  t.is(upright.height, CANONICAL_HEIGHT)

  const before = transformer.metadataSync()
  t.is(before.width, upright.width)
  t.is(before.height, upright.height)
  // A raw-pixel read (an encode) must not rotate the cached decode in place.
  transformer.rawPixelsSync()
  const after = transformer.metadataSync()
  t.is(after.width, upright.width, 'rotate applied once, not re-rotated by the prior encode')
  t.is(after.height, upright.height)
})

// composite() with a Multiply blend: two mid-grays multiply to ~quarter brightness.
// 0.502^2 * 255 ≈ 64. Alpha stays fully opaque.
test('composite Multiply halves two mid-grays (#138)', async (t) => {
  const gray = Uint8Array.from([128, 128, 128, 255])
  const topPng = await Transformer.fromRgbaPixels(gray, 1, 1).png()
  const raw = await Transformer.fromRgbaPixels(gray, 1, 1).composite(topPng, { blend: BlendMode.Multiply }).rawPixels()
  t.true(raw[0] >= 63 && raw[0] <= 65, `Multiply of two mid-grays ~= 64; got ${raw[0]}`)
  t.is(raw[3], 255, 'opaque base stays opaque')
})

// composite() with Gravity.SouthEast anchors a small overlay at the bottom-right corner,
// leaving the rest of the base untouched.
test('composite Gravity.SouthEast places the overlay bottom-right (#138)', async (t) => {
  const base = Uint8Array.from(Array.from({ length: 4 * 4 }, () => [0, 0, 0, 255]).flat())
  const top = Uint8Array.from(Array.from({ length: 2 * 2 }, () => [10, 20, 30, 255]).flat())
  const topPng = await Transformer.fromRgbaPixels(top, 2, 2).png()
  const raw = await Transformer.fromRgbaPixels(base, 4, 4).composite(topPng, { gravity: Gravity.SouthEast }).rawPixels()
  // bottom-right pixel (3,3) -> offset (3*4+3)*4 = 60 carries the overlay color.
  t.is(raw[60], 10)
  t.is(raw[61], 20)
  t.is(raw[62], 30)
  t.is(raw[63], 255)
  // top-left pixel (0,0) -> offset 0 is untouched base color.
  t.is(raw[0], 0)
  t.is(raw[1], 0)
  t.is(raw[2], 0)
  t.is(raw[3], 255)
})

// composite() with tile: true repeats the overlay across the whole base.
test('composite tile covers the whole base (#138)', async (t) => {
  const base = Uint8Array.from(Array.from({ length: 4 * 4 }, () => [50, 60, 70, 255]).flat())
  const top = Uint8Array.from(Array.from({ length: 2 * 2 }, () => [10, 20, 30, 255]).flat())
  const topPng = await Transformer.fromRgbaPixels(top, 2, 2).png()
  const raw = await Transformer.fromRgbaPixels(base, 4, 4).composite(topPng, { tile: true }).rawPixels()
  for (let i = 0; i < 16; i++) {
    const o = i * 4
    t.is(raw[o], 10, `pixel ${i} R`)
    t.is(raw[o + 1], 20, `pixel ${i} G`)
    t.is(raw[o + 2], 30, `pixel ${i} B`)
    t.is(raw[o + 3], 255, `pixel ${i} A`)
  }
})

// composite() with BlendMode.DestOver keeps the (opaque) backdrop: Fa = 1 - ab = 0,
// so the base shows through unchanged.
test('composite DestOver keeps the opaque backdrop (#138)', async (t) => {
  const red = Uint8Array.from([255, 0, 0, 255])
  const blue = Uint8Array.from([0, 0, 255, 255])
  const topPng = await Transformer.fromRgbaPixels(blue, 1, 1).png()
  const raw = await Transformer.fromRgbaPixels(red, 1, 1).composite(topPng, { blend: BlendMode.DestOver }).rawPixels()
  t.is(raw[0], 255)
  t.is(raw[1], 0)
  t.is(raw[2], 0)
  t.is(raw[3], 255)
})

// composite() opacity fades the OVERLAY, so a 50%-faded red over blue blends to ~purple.
test('composite opacity fades the overlay (#138)', async (t) => {
  const blue = Uint8Array.from([0, 0, 255, 255])
  const red = Uint8Array.from([255, 0, 0, 255])
  const topPng = await Transformer.fromRgbaPixels(red, 1, 1).png()
  const raw = await Transformer.fromRgbaPixels(blue, 1, 1).composite(topPng, { opacity: 0.5 }).rawPixels()
  t.true(raw[0] >= 127 && raw[0] <= 129, `faded red ~= 128; got ${raw[0]}`)
  t.true(raw[2] >= 127 && raw[2] <= 129, `remaining blue ~= 128; got ${raw[2]}`)
  t.is(raw[3], 255, 'opaque base stays opaque')
})
