import { promises as fs } from 'node:fs'
import os from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import test from 'ava'

import { JsColorType, Transformer } from '../index.js'

const __DIRNAME = join(fileURLToPath(import.meta.url), '..')
const ROOT_DIR = join(__DIRNAME, '..', '..', '..')

// HEIC delegates to the OS ImageIO HEVC codec, so it works ONLY with the native macOS binding. Gate
// on the host platform (and, off macOS, the loaded binding) — NOT on an env var.
//   - native macOS (darwin)  → onMac: run the real decode/encode tests UNCONDITIONALLY. HEIC MUST
//     work here, so a regression that drops the native `.heic`/`.heicSync` registration (or breaks
//     decode/encode) FAILS these tests instead of silently skipping — that is the whole point of the
//     suite. The explicit API-presence test below makes a missing method a clear, named failure.
//   - non-macOS WITH the method (native linux/windows, or a wasm shipping the macOS-only stub)
//                            → offMac: assert the documented "only ... on macOS" rejection.
//   - non-macOS WITHOUT the method (a pre-HEIC wasm)
//                            → skip; there is nothing to assert and HEIC can never run in the wasm
//                              sandbox. The moment a wasm ships the stub, `offMac` lights up on it.
// `hasHeic` is used ONLY for that off-macOS skip — never to gate macOS, so it can't hide a macOS
// regression. Why not key on NAPI_RS_FORCE_WASI: the loader only forces WASI for the literal
// 'true'/'error' (packages/binding/index.js), yet the CI wasi job sets '1', so the env var does NOT
// reliably indicate which binding loaded — the method's presence does. The wasi CI job runs on Linux,
// so `onMac` never runs against a wasm in CI.
const hasHeic = typeof Transformer.prototype.heicSync === 'function'
const isMac = os.platform() === 'darwin'
const onMac = isMac ? test : test.skip
const offMac = !isMac && hasHeic ? test : test.skip

// 8-bit fixture: `sips -s format heic un-optimized.png --out un-optimized.heic` (1024x681).
const HEIC = await fs.readFile(join(ROOT_DIR, 'un-optimized.heic'))
// Genuine 10-bit fixture (HEVC Main10), generated via ImageIO from a 16-bit Display-P3 source.
const HEIC_10BIT = await fs.readFile(join(ROOT_DIR, 'un-optimized-10bit.heic'))
// 8-bit RGBA PNG source for the encode round-trip (1024x681).
const PNG = await fs.readFile(join(ROOT_DIR, 'un-optimized.png'))

onMac('native macOS exposes the HEIC API surface', (t) => {
  // A native-API regression that drops the `.heic`/`.heicSync` registration must FAIL here (not skip).
  t.is(typeof Transformer.prototype.heic, 'function')
  t.is(typeof Transformer.prototype.heicSync, 'function')
})

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

onMac('decode preserves orientation (not vertically flipped)', (t) => {
  // `un-optimized.heic` was produced from `un-optimized.png`, so the PNG (decoded via the trusted
  // image-rs path) is the upright ground truth. A CoreGraphics bitmap context flip would invert the
  // rows while keeping the dimensions, so dims can't catch it — compare row order against the source.
  const heicRaw = new Transformer(HEIC).rawPixelsSync() // our ImageIO decode (RGBA8, top-down)
  const pngRaw = new Transformer(PNG).rawPixelsSync() // trusted upright reference (RGBA8)
  const W = 1024
  const H = 681
  t.is(heicRaw.length, W * H * 4)
  t.is(pngRaw.length, W * H * 4)
  // Mean abs RGB diff between our decode and the source, comparing same-row vs reversed-row order.
  const meanAbsDiff = (flip) => {
    let sum = 0
    for (let y = 0; y < H; y++) {
      const srcY = flip ? H - 1 - y : y
      for (let x = 0; x < W; x++) {
        const h = (y * W + x) * 4
        const p = (srcY * W + x) * 4
        sum += Math.abs(heicRaw[h] - pngRaw[p])
        sum += Math.abs(heicRaw[h + 1] - pngRaw[p + 1])
        sum += Math.abs(heicRaw[h + 2] - pngRaw[p + 2])
      }
    }
    return sum / (W * H * 3)
  }
  const upright = meanAbsDiff(false)
  const flipped = meanAbsDiff(true)
  // HEVC is lossy so `upright` is small but nonzero; a vertical flip makes `flipped` far larger.
  t.true(upright < flipped, `decode looks vertically flipped (upright=${upright}, flipped=${flipped})`)
  t.true(upright < 10, `decode differs too much from its source image (upright=${upright})`)
})

offMac('heic rejected off macOS', async (t) => {
  await t.throwsAsync(() => new Transformer(HEIC).metadata(), {
    message: /only supported on macOS/,
  })
})

// --- HEIC encode (macOS-only, via CGImageDestination "public.heic") ---

onMac('encodes png -> heic (round-trip)', async (t) => {
  const buf = await new Transformer(PNG).heic()
  t.true(Buffer.isBuffer(buf))
  t.true(buf.length > 0)
  // The encoded bytes must re-decode as a HEIC with the source dimensions.
  const meta = await new Transformer(Buffer.from(buf)).metadata()
  t.is(meta.format, 'heic')
  t.is(meta.width, 1024)
  t.is(meta.height, 681)
})

onMac('heic max quality (quality 100)', async (t) => {
  // ImageIO HEIC has no truly-lossless mode; the encoder CLAMPS its compression quality to 0.9 (see
  // encode_heic), because compression 1.0 engages a near-lossless path the OS *software* HEVC encoder
  // rejects on hosts without a hardware media engine. So `quality: 100` maps to 0.9 deterministically
  // on every host (no runtime fallback) — this must not throw and must round-trip; we assert only
  // format/dims, never fidelity (a ~1-3/255 residual remains regardless).
  const buf = new Transformer(PNG).heicSync({ quality: 100 })
  t.true(Buffer.isBuffer(buf))
  t.true(buf.length > 0)
  const meta = await new Transformer(Buffer.from(buf)).metadata()
  t.is(meta.format, 'heic')
  t.is(meta.width, 1024)
  t.is(meta.height, 681)
})

onMac('heic 10-bit round-trip', async (t) => {
  // Decode the committed 10-bit HEIC (-> RGBA16 source), re-encode at 10-bit, and confirm the
  // re-decoded output is still RGBA16 (`kCGImagePropertyDepth` > 8). 10-bit HEIC comes purely from
  // feeding a 16-bpc CGImage on the encode side (ImageIO infers HEVC Main10; no explicit depth
  // property), so this RGBA16 re-decode assertion is the proof the encoded file is really >8-bit.
  const buf = new Transformer(HEIC_10BIT).heicSync({ bitDepth: 10 })
  t.true(Buffer.isBuffer(buf))
  t.true(buf.length > 0)
  const meta = await new Transformer(Buffer.from(buf)).metadata()
  t.is(meta.format, 'heic')
  // `colorType` is the numeric `JsColorType` enum; a 10-bit decode yields RGBA16.
  t.is(meta.colorType, JsColorType.Rgba16)
})

onMac('encodes transparent rgba -> heic (alpha round-trip)', async (t) => {
  // The committed PNG fixture is fully opaque (alpha plane all 255), so genuine transparency was
  // never exercised. Build a 32x32 RGBA source with a horizontal alpha gradient (left transparent,
  // right opaque, RGB constant) so we can prove CGImageDestinationFinalize doesn't choke on real
  // alpha and that alpha survives the HEVC round-trip.
  const WIDTH = 32
  const HEIGHT = 32
  const pixels = Buffer.alloc(WIDTH * HEIGHT * 4)
  for (let y = 0; y < HEIGHT; y++) {
    for (let x = 0; x < WIDTH; x++) {
      const i = (y * WIDTH + x) * 4
      pixels[i] = 200 // R (constant)
      pixels[i + 1] = 100 // G (constant)
      pixels[i + 2] = 50 // B (constant)
      // alpha ramps 0 (left, transparent) -> 255 (right, opaque)
      pixels[i + 3] = Math.round((x / (WIDTH - 1)) * 255)
    }
  }

  // `fromRgbaPixels` is a static factory (see transformer.spec.mjs). Encode via both code paths.
  const syncBuf = Transformer.fromRgbaPixels(pixels, WIDTH, HEIGHT).heicSync()
  t.true(Buffer.isBuffer(syncBuf))
  // A non-empty buffer alone proves CGImageDestinationFinalize did NOT fail on RGBA with real alpha.
  t.true(syncBuf.length > 0)

  const asyncBuf = await Transformer.fromRgbaPixels(pixels, WIDTH, HEIGHT).heic({ quality: 90 })
  t.true(Buffer.isBuffer(asyncBuf))
  t.true(asyncBuf.length > 0)

  // Re-decode and confirm format/dims/colorType.
  const meta = await new Transformer(Buffer.from(syncBuf)).metadata()
  t.is(meta.format, 'heic')
  t.is(meta.width, WIDTH)
  t.is(meta.height, HEIGHT)
  t.is(meta.colorType, JsColorType.Rgba8)

  // Re-decode the raw RGBA8 pixels and assert alpha survived the round-trip. HEVC is lossy, so use
  // tolerant bounds: the left edge must stay clearly transparent, the right edge clearly opaque.
  const raw = new Transformer(Buffer.from(syncBuf)).rawPixelsSync()
  t.is(raw.length, WIDTH * HEIGHT * 4)
  const midRow = Math.floor(HEIGHT / 2)
  const leftAlpha = raw[(midRow * WIDTH + 0) * 4 + 3]
  const rightAlpha = raw[(midRow * WIDTH + (WIDTH - 1)) * 4 + 3]
  t.true(leftAlpha < 128, `left-edge alpha should be transparent, got ${leftAlpha}`)
  t.true(rightAlpha > 200, `right-edge alpha should be opaque, got ${rightAlpha}`)
  t.true(rightAlpha > leftAlpha, `alpha gradient direction lost (left=${leftAlpha}, right=${rightAlpha})`)
})

offMac('heic encode rejected off macOS', async (t) => {
  await t.throwsAsync(() => new Transformer(PNG).heic(), {
    message: /only available on macOS/,
  })
})
