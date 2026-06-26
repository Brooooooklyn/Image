import { promises as fs } from 'node:fs'
import os from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import test from 'ava'

import { JsColorType, Transformer } from '../index.js'

const __DIRNAME = join(fileURLToPath(import.meta.url), '..')
const ROOT_DIR = join(__DIRNAME, '..', '..', '..')

// 8-bit fixture: `sips -s format heic un-optimized.png --out un-optimized.heic` (1024x681).
const HEIC = await fs.readFile(join(ROOT_DIR, 'un-optimized.heic'))
// Genuine 10-bit fixture (HEVC Main10), generated via ImageIO from a 16-bit Display-P3 source.
const HEIC_10BIT = await fs.readFile(join(ROOT_DIR, 'un-optimized-10bit.heic'))
// Rotated fixture: `heif-enc --rotate-cw 90 un-optimized.png` -> 1024x681 HEVC HEIC carrying a HEIF
// container `irot` (90deg CW) property, NO EXIF orientation. macOS ImageIO surfaces irot as
// orientation=6 (pixels un-baked, 1024x681); WIC BAKES irot into the decode (-> upright 681x1024).
const HEIC_ROT90 = await fs.readFile(join(ROOT_DIR, 'rot90-irot.heic'))
// 8-bit RGBA PNG source for the encode round-trip (1024x681).
const PNG = await fs.readFile(join(ROOT_DIR, 'un-optimized.png'))

const hasHeic = typeof Transformer.prototype.heicSync === 'function'
const isMac = os.platform() === 'darwin'
const isWindows = os.platform() === 'win32'

// Runtime probe: does THIS host actually have the OS HEIC codec? macOS always does (ImageIO); Windows
// needs the Store HEVC/HEIF extensions (absent on Server/CI runners). Probe by decoding the committed
// fixture — succeeds only when the codec is present; a codec-missing host throws cleanly.
const codecInstalled = await (async () => {
  if (isMac) return true
  if (!isWindows || !hasHeic) return false
  try {
    await new Transformer(HEIC).metadata()
    return true
  } catch {
    return false
  }
})()

const onMac = isMac ? test : test.skip                                          // macOS-only behaviors (10-bit, alpha-survives)
const onWinCodec = isWindows && codecInstalled ? test : test.skip              // Windows-only behaviors (alpha flattened, 10-bit rejected)
const onCodec = codecInstalled ? test : test.skip                              // shared real round-trips (mac OR win-with-codec)
const onWinNoCodec = isWindows && hasHeic && !codecInstalled ? test : test.skip // CI Windows: codec-missing rejection
const offHeic = !isMac && !isWindows && hasHeic ? test : test.skip             // linux/wasm stub: platform rejection
const onWindows = isWindows ? test : test.skip                                 // Windows API-surface (codec-independent)

onMac('native macOS exposes the HEIC API surface', (t) => {
  // A native-API regression that drops the `.heic`/`.heicSync` registration must FAIL here (not skip).
  t.is(typeof Transformer.prototype.heic, 'function')
  t.is(typeof Transformer.prototype.heicSync, 'function')
})

onWindows('native Windows exposes the HEIC API surface', (t) => {
  // Runs on EVERY Windows build regardless of whether the OS HEVC codec is installed: the native
  // binding must always register `.heic`/`.heicSync`. A regression that drops them must FAIL here,
  // not silently skip every codec-gated Windows test. (Mirrors the macOS API-surface guard.)
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

// Adversarial-review finding F1 (#158): gating EXIF/orientation behind `with_exif`
// must NOT drop HEIC orientation, which the ImageIO decoder sets unconditionally
// (independent of rexif). `metadata(false)` must keep it non-null.
onMac('heic metadata(false) preserves decoder orientation (#158, F1)', async (t) => {
  const metadata = await new Transformer(HEIC).metadata()
  t.not(metadata.orientation, undefined)
  t.not(metadata.orientation, null)
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

onCodec('decode preserves orientation (not vertically flipped)', (t) => {
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

// HEIF container orientation (irot/imir). The two OS decoders honor it differently but both yield an
// upright displayed image: macOS returns the orientation tag for the pipeline to apply (pixels stay at
// coded dims); Windows WIC BAKES it into the decoded pixels. (#221 review: orientation is NOT ignored
// on Windows for container-stored rotation — WIC applies it, just not via a returned tag.)
onMac('heic irot orientation surfaces as a tag (macOS, pixels un-baked)', async (t) => {
  // `rot90-irot.heic` stores a 90deg-CW rotation in the HEIF container `irot` property (no EXIF tag).
  // ImageIO reports it via kCGImagePropertyOrientation=6 and leaves the pixels at coded dims 1024x681.
  const m = await new Transformer(HEIC_ROT90).metadata()
  t.is(m.format, 'heic')
  t.is(m.width, 1024)
  t.is(m.height, 681)
  t.is(m.orientation, 6)
})

onWinCodec('heic irot orientation is baked by WIC (Windows, upright dims)', async (t) => {
  // WIC applies the container `irot` during decode, so the same 90deg-CW fixture comes back already
  // upright with swapped dimensions (681x1024) and no residual orientation tag — proving orientation is
  // honored on Windows. A regression that ignored it would report the coded 1024x681 instead.
  const m = await new Transformer(HEIC_ROT90).metadata()
  t.is(m.format, 'heic')
  t.is(m.width, 681)
  t.is(m.height, 1024)
  t.is(m.orientation ?? null, null)
})

onWinNoCodec('heic decode rejected without the OS codec', async (t) => {
  await t.throwsAsync(() => new Transformer(HEIC).metadata(), { message: /codec.*not installed/i })
})

offHeic('heic decode rejected off macOS/Windows', async (t) => {
  await t.throwsAsync(() => new Transformer(HEIC).metadata(), {
    message: /only supported on macOS and Windows/,
  })
})

// --- HEIC encode (macOS-only, via CGImageDestination "public.heic") ---

onCodec('encodes png -> heic (round-trip)', async (t) => {
  const buf = await new Transformer(PNG).heic()
  t.true(Buffer.isBuffer(buf))
  t.true(buf.length > 0)
  // The encoded bytes must re-decode as a HEIC with the source dimensions.
  const meta = await new Transformer(Buffer.from(buf)).metadata()
  t.is(meta.format, 'heic')
  t.is(meta.width, 1024)
  t.is(meta.height, 681)
})

onCodec('heic encode preserves channel order (no R/B swap)', async (t) => {
  // A solid asymmetric color (r > g > b) must survive encode->decode WITHOUT a channel swap. Guards both
  // backends' RGBA handling — notably Windows WIC, where the HEIF encoder negotiates 32bppRGBA to an
  // opaque BGR format and `WriteSource` performs the conversion (a swap would surface here as b > r).
  // HEVC is lossy, so assert the channel ordering plus loose magnitude bounds rather than exact values.
  const WIDTH = 32
  const HEIGHT = 32
  const pixels = Buffer.alloc(WIDTH * HEIGHT * 4)
  for (let i = 0; i < WIDTH * HEIGHT; i++) {
    pixels[i * 4] = 210 // R (max)
    pixels[i * 4 + 1] = 90 // G (mid)
    pixels[i * 4 + 2] = 40 // B (min)
    pixels[i * 4 + 3] = 255
  }
  const buf = Transformer.fromRgbaPixels(pixels, WIDTH, HEIGHT).heicSync()
  const raw = new Transformer(Buffer.from(buf)).rawPixelsSync()
  const c = (Math.floor(HEIGHT / 2) * WIDTH + Math.floor(WIDTH / 2)) * 4
  const [r, g, b] = [raw[c], raw[c + 1], raw[c + 2]]
  t.true(r > g && g > b, `channel order not preserved (got r=${r} g=${g} b=${b}, expected r>g>b)`)
  t.true(r > 150, `red should stay dominant, got ${r}`)
  t.true(b < 110, `blue should stay the minimum, got ${b}`)
})

onCodec('heic max quality (quality 100)', async (t) => {
  // `quality: 100` is valid on both backends but maps differently: macOS ImageIO CLAMPS its
  // compression quality to 0.9 (no truly-lossless mode; compression 1.0 engages a near-lossless path
  // the OS software HEVC encoder rejects without a hardware media engine), while Windows WIC maps it
  // straight to 1.0 with no clamp. Either way it must not throw and must round-trip; we assert only
  // format/dims, never fidelity (a small residual remains regardless).
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

// --- Windows-specific behaviors (alpha flattened, 10-bit rejected) ---

onWinCodec('heic rejects bitDepth:10 on Windows', async (t) => {
  await t.throwsAsync(() => new Transformer(HEIC_10BIT).heic({ bitDepth: 10 }), {
    message: /10-bit.*not supported on Windows/,
  })
  t.throws(() => new Transformer(HEIC_10BIT).heicSync({ bitDepth: 10 }), {
    message: /10-bit.*not supported on Windows/,
  })
})

onWinCodec('encodes transparent rgba -> heic (alpha flattened to opaque)', async (t) => {
  // Windows WIC HEVC encode is opaque-only; a transparent source must still encode successfully and
  // round-trip, but alpha comes back fully opaque (flattened) — the documented Windows behavior.
  const WIDTH = 32
  const HEIGHT = 32
  const pixels = Buffer.alloc(WIDTH * HEIGHT * 4)
  for (let y = 0; y < HEIGHT; y++) {
    for (let x = 0; x < WIDTH; x++) {
      const i = (y * WIDTH + x) * 4
      pixels[i] = 200
      pixels[i + 1] = 100
      pixels[i + 2] = 50
      pixels[i + 3] = Math.round((x / (WIDTH - 1)) * 255)
    }
  }
  const buf = Transformer.fromRgbaPixels(pixels, WIDTH, HEIGHT).heicSync()
  t.true(Buffer.isBuffer(buf))
  t.true(buf.length > 0)
  const meta = await new Transformer(Buffer.from(buf)).metadata()
  t.is(meta.format, 'heic')
  t.is(meta.width, WIDTH)
  t.is(meta.height, HEIGHT)
  const raw = new Transformer(Buffer.from(buf)).rawPixelsSync()
  t.is(raw.length, WIDTH * HEIGHT * 4)
  const midRow = Math.floor(HEIGHT / 2)
  const leftAlpha = raw[(midRow * WIDTH + 0) * 4 + 3]
  // Flattened: even the originally-transparent left edge is opaque.
  t.true(leftAlpha > 200, `alpha should be flattened to opaque, got ${leftAlpha}`)
})

onWinCodec('decodes wide-gamut (Display-P3) heic into sRGB', (t) => {
  // `un-optimized-10bit.heic` carries a Display-P3 ICC profile. macOS renders HEIC into an sRGB color
  // space, so Windows must color-transform P3 -> sRGB (IWICColorTransform) rather than return raw P3
  // mislabeled as sRGB. At the saturated-green pixel (32,128) a correct sRGB conversion collapses red
  // toward 0 (raw P3 leaves it ~31). VM-measured: sRGB=[0,129,74] vs raw-P3=[31,127,79]. Windows decode
  // is 8-bit RGBA8 (WIC normalizes), so this is Windows-only.
  const raw = new Transformer(HEIC_10BIT).rawPixelsSync()
  const W = 256
  t.is(raw.length, W * 256 * 4)
  const i = (128 * W + 32) * 4
  const r = raw[i]
  const g = raw[i + 1]
  t.true(r < 16, `P3->sRGB should drive red toward 0 at the green pixel, got r=${r}`)
  t.true(g > 100, `green channel should stay saturated, got g=${g}`)
  t.true(r < g, `expected a green pixel (r < g), got r=${r} g=${g}`)
})

// --- Codec-missing (CI Windows) + off-platform rejection ---

onWinNoCodec('heic encode rejected without the OS codec', async (t) => {
  // Encode failure mode without the codec is unverified (surfaces at Commit); assert it throws.
  await t.throwsAsync(() => new Transformer(PNG).heic())
})

offHeic('heic encode rejected off macOS/Windows', async (t) => {
  await t.throwsAsync(() => new Transformer(PNG).heic(), {
    message: /only available on macOS and Windows/,
  })
})
