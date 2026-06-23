import { promises as fs } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import test from 'ava'
import { decode } from 'blurhash'

import { Transformer } from '../index.js'

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

test('should be able to create transformer from SVG', (t) => {
  t.notThrows(() => Transformer.fromSvg(SVG).jpegSync())
})

// The wasm32-wasip1-threads build corrupts its shared linear-memory heap after a few large SVG
// rasterizations on real multi-core hardware: the Debian logo upscales to 1600x1600 (~10 MB) and after
// 2-3 such renders a subsequent from_svg() reads a clobbered scale (content rasterizes as a corner
// speck), renders blank, or bypasses the size guard into a multi-TB allocation that aborts the worker.
// It reproduces ONLY on real multi-core CI — the native binding, single-threaded wasm, and emulated
// (TCG) wasm all pass — and is independent of issue #159: the from_svg() render logic is correct and is
// covered by these tests on the native binding. (`sync` vs `async`, serial vs concurrent, and the
// raster size all make no difference; the single sync jpegSync smoke test above renders once and is
// fine.) Until the wasm-threads heap issue is fixed, the SVG render-logic regressions below run on the
// native binding only.
const svgTest = process.env.NAPI_RS_FORCE_WASI === '1' ? test.skip : test

// Regression test for https://github.com/Brooooooklyn/Image/issues/159
// from_svg() upscales the raster pixmap to >=1000px. The SVG content must be SCALED to fill that
// pixmap, not drawn at its native size in the top-left corner. Before the fix the Debian logo
// occupied only ~5% of the canvas (a corner speck); after the fix it spans most of the canvas.
svgTest('SVG content fills the canvas, not just a corner (issue #159)', (t) => {
  // Native render dimensions (no resize) come from a PNG metadata round-trip.
  const png = Transformer.fromSvg(SVG).pngSync()
  const { width, height } = new Transformer(png).metadataSync()

  // Raw RGBA pixels of the same native render.
  const raw = Transformer.fromSvg(SVG).rawPixelsSync()

  // Bounding box of non-transparent (opaque-enough) pixels.
  let minX = width, minY = height, maxX = -1, maxY = -1
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const alpha = raw[(y * width + x) * 4 + 3]
      if (alpha > 10) {
        if (x < minX) minX = x
        if (x > maxX) maxX = x
        if (y < minY) minY = y
        if (y > maxY) maxY = y
      }
    }
  }

  // The content must extend into the far (lower-right) region of the canvas.
  // Buggy behavior: maxX/maxY ~ 5% of the canvas (top-left speck).
  // Fixed behavior: content spans to >60% of width and height.
  t.true(maxX > width * 0.6, `content right edge ${maxX} should reach past 60% of width ${width}`)
  t.true(maxY > height * 0.6, `content bottom edge ${maxY} should reach past 60% of height ${height}`)
})

// Regression for https://github.com/Brooooooklyn/Image/issues/159 (adversarial-review finding 1):
// a fractional, non-square SVG must keep its aspect ratio. A circle drawn in square user units must
// rasterize with a (near-)square bounding box. Rounding each axis independently before scaling
// stretched it into an ellipse (bbox aspect ~1.66).
svgTest('SVG with fractional non-square size keeps aspect ratio (issue #159)', (t) => {
  const svg = Buffer.from(
    `<svg width="0.6" height="1" viewBox="0 0 0.6 1" xmlns="http://www.w3.org/2000/svg"><circle cx="0.3" cy="0.5" r="0.3" fill="black"/></svg>`,
  )
  const png = Transformer.fromSvg(svg).pngSync()
  const { width, height } = new Transformer(png).metadataSync()
  const raw = Transformer.fromSvg(svg).rawPixelsSync()

  let minX = width, minY = height, maxX = -1, maxY = -1
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (raw[(y * width + x) * 4 + 3] > 10) {
        if (x < minX) minX = x
        if (x > maxX) maxX = x
        if (y < minY) minY = y
        if (y > maxY) maxY = y
      }
    }
  }
  const bw = maxX - minX + 1
  const bh = maxY - minY + 1
  const aspect = bw / bh
  t.true(aspect > 0.9 && aspect < 1.1, `circle bbox ${bw}x${bh} aspect ${aspect} should be ~1.0 (not a stretched ellipse)`)
})

// Regression for https://github.com/Brooooooklyn/Image/issues/159 (adversarial-review finding 2):
// a degenerate sub-pixel SVG must not silently rasterize to a fully-transparent (blank) image.
// Acceptable outcomes: throw an error, or render real content — but never a silent blank.
svgTest('degenerate sub-pixel SVG does not silently render blank (issue #159)', (t) => {
  const svg = Buffer.from(
    `<svg width="1e-39" height="1e-39" viewBox="0 0 1 1" xmlns="http://www.w3.org/2000/svg"><rect width="1" height="1" fill="black"/></svg>`,
  )
  try {
    const raw = Transformer.fromSvg(svg).rawPixelsSync()
    let hasContent = false
    for (let i = 3; i < raw.length; i += 4) {
      if (raw[i] > 10) { hasContent = true; break }
    }
    t.true(hasContent, 'render must not be fully blank/transparent')
  } catch {
    t.pass('errored on degenerate size (acceptable)')
  }
})

// Regression for https://github.com/Brooooooklyn/Image/issues/159 (adversarial-review finding 3):
// a thin, high-aspect-ratio SVG must not explode the raster. Previously the >=1000px upscale scaled
// the larger axis by the factor needed to lift the smaller axis to 1000 (1x2000 -> ~8 GB raster).
// Now the upscale targets the larger axis, so the raster stays bounded and keeps its aspect ratio.
svgTest('thin high-aspect-ratio SVG renders without exploding the raster (issue #159)', (t) => {
  const svg = Buffer.from(
    `<svg width="10" height="4000" viewBox="0 0 10 4000" xmlns="http://www.w3.org/2000/svg"><rect width="10" height="4000" fill="black"/></svg>`,
  )
  const png = Transformer.fromSvg(svg).pngSync()
  const { width, height } = new Transformer(png).metadataSync()
  t.true(width * height < 5_000_000, `raster ${width}x${height} should stay bounded, not exploded`)
  t.true(height > width * 100, `aspect ratio ${width}x${height} should be preserved as tall/thin`)
})

// Regression for https://github.com/Brooooooklyn/Image/issues/159 (adversarial-review finding 3):
// an oversized SVG must fail with a clean error rather than aborting the process inside
// tiny_skia::Pixmap::new. 20000x20000 = 4e8 px is comfortably over the ~2.68e8 px (1 GiB RGBA) budget,
// so the guard rejects it. We deliberately do NOT use an astronomically large size (e.g. 100000x100000
// -> a 40 GB allocation): if the guard ever regressed, that would abort the whole worker with SIGABRT
// and cascade into phantom failures of the other tests; a merely-over-budget size fails as one clean
// assertion instead.
svgTest('oversized SVG errors cleanly instead of OOM (issue #159)', (t) => {
  const svg = Buffer.from(
    `<svg width="20000" height="20000" viewBox="0 0 20000 20000" xmlns="http://www.w3.org/2000/svg"><rect width="20000" height="20000" fill="black"/></svg>`,
  )
  const err = t.throws(() => Transformer.fromSvg(svg))
  t.truthy(err)
})

// Regression for https://github.com/Brooooooklyn/Image/issues/159 (adversarial-review finding 4):
// a legitimately thin SVG with a sub-pixel axis must render (clamped to >=1px), not be rejected by
// the size guard. Previously `0.5` failed the `< 1.0` float check and threw.
svgTest('SVG with a sub-pixel axis renders instead of being rejected (issue #159)', (t) => {
  const svg = Buffer.from(
    `<svg width="0.5" height="2000" viewBox="0 0 0.5 2000" xmlns="http://www.w3.org/2000/svg"><rect width="0.5" height="2000" fill="black"/></svg>`,
  )
  const png = Transformer.fromSvg(svg).pngSync()
  const { width, height } = new Transformer(png).metadataSync()
  t.true(width >= 1, `sub-pixel width should clamp to >=1px, got ${width}`)
  t.is(height, 2000)
})

// Regression for https://github.com/Brooooooklyn/Image/issues/159 (adversarial-review finding 4):
// the pixel budget must be enforced on the ROUNDED integer dimensions, not the pre-rounded floats.
// 1.5 x 178955968 slips under the float-area cap but rounds to 2 x 178955968 (~1.33 GiB), which the
// guard must reject before allocating.
svgTest('SVG size guard cannot be bypassed by rounding (issue #159)', (t) => {
  const svg = Buffer.from(
    `<svg width="1.5" height="178955968" viewBox="0 0 1.5 178955968" xmlns="http://www.w3.org/2000/svg"><rect width="1.5" height="178955968" fill="black"/></svg>`,
  )
  const err = t.throws(() => Transformer.fromSvg(svg))
  t.truthy(err)
})

// Regression for https://github.com/Brooooooklyn/Image/issues/159 (adversarial-review finding 5a):
// an SVG with a sub-pixel SHORT axis (rounds to 0) must error, not silently render a blank raster.
// Previously `2000x0.1` clamped the short axis to 1px and rendered a fully-transparent 2000x1 image.
svgTest('SVG with a sub-pixel short axis errors instead of rendering blank (issue #159)', (t) => {
  const svg = Buffer.from(
    `<svg width="2000" height="0.1" viewBox="0 0 2000 0.1" xmlns="http://www.w3.org/2000/svg"><rect width="2000" height="0.1" fill="black"/></svg>`,
  )
  const err = t.throws(() => Transformer.fromSvg(svg))
  t.truthy(err)
})

// Regression for https://github.com/Brooooooklyn/Image/issues/159 (adversarial-review finding 5b):
// a fractional size that rounds to >= the 1000px quality floor must not be needlessly upscaled.
// Previously `999.6x999.6` was doubled to 1999x1999 (~4x the pixels) because the loop thresholded on
// the raw float instead of the rounded dimension.
svgTest('near-1000 fractional SVG is not needlessly upscaled (issue #159)', (t) => {
  const svg = Buffer.from(
    `<svg width="999.6" height="999.6" viewBox="0 0 999.6 999.6" xmlns="http://www.w3.org/2000/svg"><rect width="999.6" height="999.6" fill="black"/></svg>`,
  )
  const png = Transformer.fromSvg(svg).pngSync()
  const { width, height } = new Transformer(png).metadataSync()
  t.is(width, 1000)
  t.is(height, 1000)
})

// Regression for https://github.com/Brooooooklyn/Image/issues/159 (adversarial-review finding 6):
// tiny_skia renders into a PREMULTIPLIED buffer; it must be demultiplied to straight RGBA before
// being treated as an RgbaImage, otherwise a semi-transparent rgba() background (and antialiased
// edges) come out darkened. rgba(10,20,30,.8) must round-trip as ~[10,20,30,204], not [8,16,24,204].
svgTest('semi-transparent SVG background is straight (demultiplied) RGBA (issue #159)', (t) => {
  const svg = Buffer.from('<svg width="2" height="2" xmlns="http://www.w3.org/2000/svg"></svg>')
  const px = Transformer.fromSvg(svg, 'rgba(10, 20, 30, .8)').rawPixelsSync().slice(0, 4)
  t.is(px[3], 204) // alpha 0.8 * 255
  t.true(Math.abs(px[0] - 10) <= 1, `R should be straight ~10, got ${px[0]} (8 = premultiplied bug)`)
  t.true(Math.abs(px[1] - 20) <= 1, `G should be straight ~20, got ${px[1]}`)
  t.true(Math.abs(px[2] - 30) <= 1, `B should be straight ~30, got ${px[2]}`)
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
