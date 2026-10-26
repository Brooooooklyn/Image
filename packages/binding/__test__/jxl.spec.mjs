import { promises as fs } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import test from 'ava'

import { JsColorType, Transformer } from '../index.js'

const __DIRNAME = join(fileURLToPath(import.meta.url), '..')
const ROOT_DIR = join(__DIRNAME, '..', '..', '..')
const JXL_DIR = join(__DIRNAME, 'jxl')

// Fixtures, all made with cjxl 0.12 from synthetic 48x32 sources (no third-party images):
//   rgb8-lossless       cjxl rgb8.png -d 0 -e 7 --container=0        (bare FF 0A codestream)
//   rgba8-lossless      cjxl rgba8.png -d 0 -e 7                     (alpha ramp, straight alpha)
//   gray8-lossless      cjxl gray8.png -d 0 -e 7
//   graya8-lossless     cjxl graya8.png -d 0 -e 7                    (gray + alpha)
//   rgb16-lossless      cjxl rgb16.ppm -d 0 -e 7                     (container, 16-bit samples)
//   rgbf32-lossless     cjxl rgbf32.pfm -d 0 -e 7 -x color_space=RGB_D65_SRG_Rel_SRG  (32-bit float)
//   rgb8-lossy          cjxl rgb8.png -d 1.0 -e 7                    (VarDCT / XYB)
//   rgba8-premultiplied cjxl rgba8-premul.png -d 0 -e 7 --premultiply=1  (input already premultiplied:
//                       cjxl stores it as-is and only tags the alpha as associated)
//   display-p3-lossless cjxl p3.ppm -d 0 -e 7 -x color_space=RGB_D65_DCI_Rel_SRG  (P3 enum)
//   display-p3-icc-lossless  same pixels with the 516-byte P3 ICC libjxl writes for the enum above
//   gray8-linear-lossless    cjxl gray8.png -d 0 -e 7 -x color_space=Gra_D65_Rel_Lin
//   graya8-linear-lossless   cjxl graya8.png -d 0 -e 7 -x color_space=Gra_D65_Rel_Lin
//   rgb16-pq-lossless        cjxl rgb16.ppm -d 0 -e 7 -x color_space=RGB_D65_202_Rel_PeQ
//   gray-ramp-pq-lossless    cjxl ramp.ppm -d 0 -e 7 -x color_space=RGB_D65_202_Rel_PeQ
//                            (256x1 16-bit neutral ramp, sample x * 257 at column x)
//   gray-ramp-pq-f32-lossless  cjxl ramp.pfm -d 0 -e 7 -x color_space=RGB_D65_202_Rel_PeQ
//                            (the same ramp as 32-bit float, sample x * 257 / 65535)
//   gray-ramp-pq8-lossless   cjxl ramp8.ppm -d 0 -e 7 -x color_space=RGB_D65_202_Rel_PeQ
//                            (the same ramp at 8 bit, sample x: the same PQ codes)
//   gray16-ramp-pq-lossless  cjxl ramp.pgm -d 0 -e 7 -x color_space=Gra_D65_Rel_PeQ  (16-bit gray)
//   gray-ramp-hlg-lossless   cjxl ramp.ppm -d 0 -e 7 -x color_space=RGB_D65_202_Rel_HLG
//                            (intensity target 1000 nits, cjxl's default for HLG)
//   rgb16-colors-pq1000-lossless  cjxl colors.ppm -d 0 -e 7 -x color_space=RGB_D65_202_Rel_PeQ
//                            --intensity_target=1000  (32x6 16-bit: 6 colours, each with its brightest
//                            channel at 500 * 20^(x/31) nits)
//   rgba8-ramp-pq-lossless   cjxl ramp8-a.pam -d 0 -e 7 -x color_space=RGB_D65_202_Rel_PeQ
//                            (ramp8.ppm plus 8-bit alpha (x * 37 + 11) % 256)
//   graya16-ramp-hlg-lossless  cjxl ramp16-ga.pam -d 0 -e 7 -x color_space=Gra_D65_Rel_HLG
//                            (16-bit gray x * 257 plus alpha (x * 4099) % 65536; target 1000 nits)
//   pq-colors-xyb            cjxl pq-color.ppm -e 7 -x color_space=RGB_D65_202_Rel_PeQ
//   hlg-colors-xyb           cjxl hlg-color.ppm -e 7 -x color_space=RGB_D65_202_Rel_HLG
//                            (1024x13, lossy XYB at the default distance: one row per colour, white,
//                            Rec.2020 R/G/B/C/M/Y, sRGB R/G/B, orange, skin, pastel, each a ramp of
//                            its max-channel code from 0 to 1)
//   dci-p3-lossless          cjxl patches.ppm -d 0 -e 7 -x color_space=RGB_DCI_DCI_Rel_DCI
//                            (DCI white, P3 primaries, DCI transfer = pure 2.6 gamma; 8 flat patches)
//   premul.jxl          128x85 RGBA, cjxl --premultiply=1 (associated alpha)
//   animated-4frames    48x32, 4 frames: red, green, blue, yellow, each with a white bar at x = 12*i
//   orientation/orientation_N.jxl  lossless copies of ../orientation/orientation_N.jpg (header orientation N)
//   orientation6-jpeg-recompressed  cjxl ../orientation/orientation_6.jpg --lossless_jpeg=1 (jbrd + Exif box)
//   bomb.jxl            387 bytes that declare a 12000x12000 image
//   rgbf32-rec2100-linear-lossless   cjxl px.pfm -d 0 -P 0 -e 7 -x color_space=RGB_D65_202_Rel_Lin
//                       (12x1 float, samples outside [0, 1]: red 4, 2, 1 then -0.1, 1.5, 8, ...;
//                       `-P 0` because cjxl fails on negative floats without it)
//   rgbaf32-rec2100-linear-lossless  cjxl px.exr -d 0 -P 0 -e 7 --premultiply=0 (same 12 pixels plus
//                       straight alpha 0.1..1, EXR chromaticities = Rec.2020 primaries, linear)
//   rgbf32-hlg-out-of-range-lossless  cjxl hlg-oor.pfm -d 0 -P 0 -e 7 -x color_space=RGB_D65_202_Rel_HLG
//                       --intensity_target=1000  (12x1 float HLG codes, some outside [0, 1]:
//                       [1,.6,.3], [1.1,.6,.3], [1.2,.6,.3], [1.05]x3, [1.3]x3, [-0.1,.5,.5],
//                       [.5,-.05,.2], [.5]x3, [.75]x3, [1]x3, [0]x3, [.2,.9,.1]; djxl gives them back exactly)
//   rgbf32-pq10k-out-of-range-lossless  cjxl pq-oor.pfm -d 0 -P 0 -e 7 -x color_space=RGB_D65_202_Rel_PeQ
//                       --intensity_target=10000  (12x1 float PQ codes: [.75,.5,.3], [.9,.5,.3], [1,.5,.3],
//                       [1.05,.5,.3], [1.05]x3, [-0.05,.5,.5], [.6,-.02,.4], [.58]x3, [.75]x3, [1]x3,
//                       [0]x3, [.3,.7,.2])
//   gray-ramp-hlg250-xyb / gray-ramp-hlg600-xyb  cjxl ramp.ppm -d 1 -e 7 -x color_space=RGB_D65_202_Rel_HLG
//                       --intensity_target=250 (600)  (the 256x1 16-bit neutral ramp, lossy XYB)
//   pq-blue-y0-lossless 128x16 16-bit lossless PQ, D65, primaries (0.7347, 0.2653), (0.1596, 0.8404),
//                       (0.0366, 0), intensity target 10000; 8 flat 16x16 patches with PQ codes
//                       (.5,.5,.5), (.6,.2,.2), (.2,.6,.2), (.2,.2,.6), (.4,.4,.1), (.7,.7,.7), (.3,.3,.3),
//                       (.58,.45,.30) (x 65535, rounded). cjxl rejects a 0 in a primary, so: cjxl p.ppm
//                       -d 0 --container=0 -x 'color_space=RGB_D65_0.7347;0.2653;0.1596;0.8404;0.0366;0.000001_Rel_PeQ',
//                       then clear bit 565 of the file (the stored blue y goes from 1e-6 to 0; jxlinfo shows
//                       "blue(x=0.036600,y=0.000000)"). jxl-rs and djxl both accept the file.
// `ref/*.png` are `djxl X.jxl ref/X.png` decodes (libjxl's reference decoder);
// `ref/dci-p3-lossless.png` is `djxl --color_space=RGB_D65_SRG_Rel_SRG` (converted to sRGB by libjxl);
// `ref/rgb(a)f32-rec2100-linear-lossless.png` add `--bits_per_sample=16`.
// `ref/pq-colors-xyb.png` is `djxl pq-colors-xyb.jxl ref/pq-colors-xyb.png --display_nits=250
// --color_space=RGB_D65_SRG_Rel_SRG --bits_per_sample=16`; `ref/hlg-colors-xyb.png` is the same with
// `--display_nits=80`. These are XYB files because djxl's `--display_nits` is wrong for lossless
// (non-XYB) PQ/HLG files: it linearizes with the output transfer function (202 nits -> 64976).
// `ref/gray-ramp-hlg250-xyb.png` and `ref/gray-ramp-hlg600-xyb.png` are made like `ref/hlg-colors-xyb.png`.
const fixture = (name) => fs.readFile(join(JXL_DIR, name))

const decode = async (input) => {
  const transformer = new Transformer(input)
  const metadata = await transformer.metadata()
  const raw = await transformer.rawPixels()
  return { ...metadata, raw }
}

// Max per-sample difference between two native-endian 8- or 16-bit sample buffers.
function maxDiff(a, b, sixteen = false) {
  const x = sixteen ? new Uint16Array(a.buffer, a.byteOffset, a.length / 2) : a
  const y = sixteen ? new Uint16Array(b.buffer, b.byteOffset, b.length / 2) : b
  let max = 0
  for (let i = 0; i < x.length; i++) max = Math.max(max, Math.abs(x[i] - y[i]))
  return max
}

// Mean per-sample difference between two native-endian 16-bit sample buffers.
function meanDiff(a, b) {
  const x = new Uint16Array(a.buffer, a.byteOffset, a.length / 2)
  const y = new Uint16Array(b.buffer, b.byteOffset, b.length / 2)
  let sum = 0
  for (let i = 0; i < x.length; i++) sum += Math.abs(x[i] - y[i])
  return sum / x.length
}

const samples16 = (raw) => new Uint16Array(raw.buffer, raw.byteOffset, raw.length / 2)

// Checks a decoded 256x1 neutral ramp: every column >= the one before, and R, G, B within 1.
function assertMonotonicNeutral(t, samples, channels, label) {
  for (let x = 1; x < 256; x++) {
    t.true(samples[x * channels] >= samples[(x - 1) * channels], `${label}: column ${x} drops`)
  }
  if (channels === 3) {
    for (let x = 0; x < 256; x++) {
      const [r, g, b] = samples.subarray(x * 3, x * 3 + 3)
      t.true(Math.abs(r - g) <= 1 && Math.abs(g - b) <= 1, `${label}: column ${x} is tinted (${r}, ${g}, ${b})`)
    }
  }
}

function assertAnchors(t, samples, channels, anchors, label) {
  for (const [x, expected] of Object.entries(anchors)) {
    const actual = samples[Number(x) * channels + (channels === 3 ? 1 : 0)]
    t.true(Math.abs(actual - expected) <= 64, `${label}: column ${x} is ${actual}, expected ${expected}`)
  }
}

test('metadata reports format jxl with the decoded size and color type', async (t) => {
  const cases = [
    ['rgb8-lossless.jxl', 48, 32, JsColorType.Rgb8],
    ['rgba8-lossless.jxl', 48, 32, JsColorType.Rgba8],
    ['gray8-lossless.jxl', 48, 32, JsColorType.L8],
    ['graya8-lossless.jxl', 48, 32, JsColorType.La8],
    ['rgb16-lossless.jxl', 48, 32, JsColorType.Rgb16],
    // Float samples become 16-bit integers (never Rgb32F, which png/ico/pnm cannot encode).
    ['rgbf32-lossless.jxl', 48, 32, JsColorType.Rgb16],
    ['rgb8-lossy.jxl', 48, 32, JsColorType.Rgb8],
    ['animated-4frames.jxl', 48, 32, JsColorType.Rgb8],
  ]
  for (const [name, width, height, colorType] of cases) {
    const metadata = await new Transformer(await fixture(name)).metadata(true)
    t.is(metadata.format, 'jxl', name)
    t.is(metadata.width, width, name)
    t.is(metadata.height, height, name)
    t.is(metadata.colorType, colorType, name)
    // EXIF is not read from JPEG XL boxes, and the orientation is already baked in.
    t.is(metadata.exif, undefined, name)
    t.is(metadata.orientation, undefined, name)
  }
})

test('lossless fixtures decode exactly like djxl', async (t) => {
  for (const name of ['rgb8-lossless', 'rgba8-lossless', 'gray8-lossless', 'graya8-lossless', 'rgb16-lossless']) {
    const jxl = await decode(await fixture(`${name}.jxl`))
    const ref = await decode(await fixture(`ref/${name}.png`))
    t.is(jxl.colorType, ref.colorType, name)
    t.is(jxl.raw.length, ref.raw.length, name)
    t.true(jxl.raw.equals(ref.raw), `${name} must be byte-identical to the djxl decode`)
  }
})

test('lossy fixture is within +-2 of djxl', async (t) => {
  // VarDCT output differs by 1 between SIMD paths (NEON/AVX2/SSE4.2/scalar/simd128), so no exact hash.
  const jxl = await decode(await fixture('rgb8-lossy.jxl'))
  const ref = await decode(await fixture('ref/rgb8-lossy.png'))
  t.is(jxl.raw.length, ref.raw.length)
  t.true(maxDiff(jxl.raw, ref.raw) <= 2)
})

test('float samples decode to full-range 16-bit', async (t) => {
  // The float source stores k/255 for every 8-bit value k of rgb8-lossless, so 16-bit output is k*257.
  const float = await decode(await fixture('rgbf32-lossless.jxl'))
  const rgb8 = await decode(await fixture('rgb8-lossless.jxl'))
  const samples = new Uint16Array(float.raw.buffer, float.raw.byteOffset, float.raw.length / 2)
  t.is(samples.length, rgb8.raw.length)
  let max = 0
  for (let i = 0; i < samples.length; i++) max = Math.max(max, Math.abs(samples[i] - rgb8.raw[i] * 257))
  t.true(max <= 1, `max diff ${max}`)
})

test('float samples outside [0, 1] are clipped only after the sRGB conversion', async (t) => {
  // Rec.2100 primaries mix every channel into the others, so red 4 also lowers green in sRGB.
  // Clipping the source to [0, 1] first (what u16 decoding does) gave 27022 off djxl here, and red
  // 4, 2 and 1 all came out the same. djxl keeps the float range and only clips the final result.
  for (const name of ['rgbf32-rec2100-linear-lossless', 'rgbaf32-rec2100-linear-lossless']) {
    const jxl = await decode(await fixture(`${name}.jxl`))
    const ref = await decode(await fixture(`ref/${name}.png`))
    t.is(jxl.colorType, ref.colorType, name)
    t.is(jxl.raw.length, ref.raw.length, name)
    // moxcms and libjxl differ by up to ~44/65535 (0.2 of an 8-bit level) on these pixels.
    const diff = maxDiff(jxl.raw, ref.raw, true)
    t.true(diff <= 64, `${name}: max diff vs djxl ${diff}`)
    // Green of pixels 0..2 (red 4, 2, 1, green and blue 0.5): distinct, and ordered like djxl's.
    const channels = name.startsWith('rgba') ? 4 : 3
    const samples = new Uint16Array(jxl.raw.buffer, jxl.raw.byteOffset, jxl.raw.length / 2)
    const green = [0, 1, 2].map((x) => samples[x * channels + 1])
    t.true(green[0] < green[1] && green[1] < green[2], `${name}: green ${green}`)
  }
})

// Each orientation fixture stores the inverse of a canonical upright scene tagged with its header
// orientation (same scene as ../orientation/*.jpg):  TL = red, TR = green, BL = blue, BR = white (64x48).
function classify(r, g, b) {
  const hi = (v) => v >= 160
  const lo = (v) => v <= 95
  if (hi(r) && lo(g) && lo(b)) return 'red'
  if (lo(r) && hi(g) && lo(b)) return 'green'
  if (lo(r) && lo(g) && hi(b)) return 'blue'
  if (hi(r) && hi(g) && hi(b)) return 'white'
  return `unknown(${r},${g},${b})`
}

function assertUpright(t, raw, label) {
  const bpp = raw.length / (64 * 48)
  t.is(bpp, 3, `${label}: output must be the upright canonical size (64x48 RGB)`)
  const pixel = (x, y) => {
    const offset = (y * 64 + x) * bpp
    return classify(raw[offset], raw[offset + 1], raw[offset + 2])
  }
  t.is(pixel(16, 12), 'red', `${label} top-left`)
  t.is(pixel(48, 12), 'green', `${label} top-right`)
  t.is(pixel(16, 36), 'blue', `${label} bottom-left`)
  t.is(pixel(48, 36), 'white', `${label} bottom-right`)
}

for (let orientation = 1; orientation <= 8; orientation++) {
  test(`header orientation ${orientation} decodes upright and rotate() does not rotate again`, async (t) => {
    const buffer = await fixture(join('orientation', `orientation_${orientation}.jxl`))
    const metadata = await new Transformer(buffer).metadata(true)
    t.is(metadata.width, 64)
    t.is(metadata.height, 48)
    // The decoder already applied it, so there is nothing left for `.rotate()` to apply.
    t.is(metadata.orientation, undefined)
    assertUpright(t, await new Transformer(buffer).rawPixels(), 'decode')
    assertUpright(t, await new Transformer(buffer).rotate().rawPixels(), 'rotate()')
  })
}

test('recompressed JPEG with orientation 6 and an Exif box decodes upright once', async (t) => {
  // The file has orientation 6 in the codestream header AND in its (brotli) Exif box.
  const buffer = await fixture('orientation6-jpeg-recompressed.jxl')
  const metadata = await new Transformer(buffer).metadata(true)
  t.is(metadata.width, 64)
  t.is(metadata.height, 48)
  t.is(metadata.orientation, undefined)
  assertUpright(t, await new Transformer(buffer).rotate().rawPixels(), 'jbrd')
})

test('animated file decodes the first frame only', async (t) => {
  const { width, height, raw } = await decode(await fixture('animated-4frames.jxl'))
  t.is(width, 48)
  t.is(height, 32)
  const pixel = (x, y) => [...raw.subarray((y * 48 + x) * 3, (y * 48 + x) * 3 + 3)]
  // Frame 0: red with the white bar at x = 0..11. Frame 1 would be green with the bar at 12..23.
  t.deepEqual(pixel(6, 16), [255, 255, 255])
  t.deepEqual(pixel(18, 16), [255, 0, 0])
  t.deepEqual(pixel(40, 30), [255, 0, 0])
})

test('premultiplied alpha is returned as straight alpha', async (t) => {
  // libjxl/djxl keep associated alpha as-is: pixel (73,20) is [34,14,0,147]. Straight is [59,25,0,147].
  const premul = await decode(await fixture('premul.jxl'))
  t.is(premul.colorType, JsColorType.Rgba8)
  const offset = (20 * 128 + 73) * 4
  t.deepEqual([...premul.raw.subarray(offset, offset + 4)], [59, 25, 0, 147])

  // A second file, stored with associated alpha. djxl outputs it as stored (premultiplied), so the
  // expected straight color is derived here as round(premultiplied * 255 / alpha).
  const assoc = await decode(await fixture('rgba8-premultiplied.jxl'))
  const stored = await decode(await fixture('ref/rgba8-premultiplied.png'))
  const straight = await decode(await fixture('ref/rgba8-lossless.png'))
  t.is(assoc.raw.length, stored.raw.length)
  let max = 0
  let maxVsSource = 0
  for (let i = 0; i < assoc.raw.length; i += 4) {
    const alpha = stored.raw[i + 3]
    t.is(assoc.raw[i + 3], alpha)
    if (alpha === 0) continue
    for (let c = 0; c < 3; c++) {
      const expected = Math.min(255, Math.round((stored.raw[i + c] * 255) / alpha))
      max = Math.max(max, Math.abs(assoc.raw[i + c] - expected))
      // Where alpha is high, 8-bit premultiplication kept enough precision to get the source back.
      if (alpha >= 128) maxVsSource = Math.max(maxVsSource, Math.abs(assoc.raw[i + c] - straight.raw[i + c]))
    }
  }
  t.true(max <= 1, `max diff vs derived straight ${max}`)
  t.true(maxVsSource <= 2, `max diff vs straight source ${maxVsSource}`)
})

// Display P3 -> sRGB, computed here without moxcms: sRGB curve -> linear, the linear-light P3-D65 to
// sRGB matrix (both D65, so no chromatic adaptation), then the sRGB curve again, clipped to 0..255.
const P3_TO_SRGB = [
  [1.2249401, -0.2249404, 0.0],
  [-0.0420569, 1.0420571, 0.0],
  [-0.0196376, -0.0786361, 1.0982735],
]
const toLinear = (v) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4)
const fromLinear = (v) => (v <= 0.0031308 ? v * 12.92 : 1.055 * v ** (1 / 2.4) - 0.055)
function p3ToSrgb(rgb) {
  const linear = rgb.map((v) => toLinear(v / 255))
  return P3_TO_SRGB.map((row) => {
    const v = row[0] * linear[0] + row[1] * linear[1] + row[2] * linear[2]
    return Math.round(Math.min(1, Math.max(0, fromLinear(Math.min(1, Math.max(0, v))))) * 255)
  })
}
// The 8 flat 12x16 patches of p3.ppm (4 columns x 2 rows), in Display P3.
const P3_PATCHES = [
  [200, 100, 50],
  [50, 150, 200],
  [100, 200, 100],
  [180, 180, 180],
  [230, 60, 90],
  [60, 90, 230],
  [128, 64, 192],
  [20, 220, 160],
]

for (const name of ['display-p3-lossless.jxl', 'display-p3-icc-lossless.jxl']) {
  test(`${name} is converted to sRGB`, async (t) => {
    const { colorType, raw } = await decode(await fixture(name))
    t.is(colorType, JsColorType.Rgb8)
    P3_PATCHES.forEach((p3, i) => {
      const x = (i % 4) * 12 + 6
      const y = Math.floor(i / 4) * 16 + 8
      const actual = [...raw.subarray((y * 48 + x) * 3, (y * 48 + x) * 3 + 3)]
      const expected = p3ToSrgb(p3)
      const diff = Math.max(...actual.map((v, c) => Math.abs(v - expected[c])))
      t.true(diff <= 2, `patch ${i} P3 ${p3}: got ${actual}, expected ${expected}`)
    })
    // A neutral gray is the same in both spaces; a saturated patch must have moved.
    t.notDeepEqual(p3ToSrgb(P3_PATCHES[0]), P3_PATCHES[0])
  })
}

test('gray with a linear transfer curve is converted to sRGB as RGB', async (t) => {
  // Same samples as gray8-lossless, tagged linear (cjxl -x color_space=Gra_D65_Rel_Lin).
  const linear = await decode(await fixture('gray8-linear-lossless.jxl'))
  const source = await decode(await fixture('gray8-lossless.jxl'))
  t.is(linear.colorType, JsColorType.Rgb8)
  let max = 0
  for (let i = 0; i < source.raw.length; i++) {
    const expected = Math.round(fromLinear(source.raw[i] / 255) * 255)
    for (let c = 0; c < 3; c++) max = Math.max(max, Math.abs(linear.raw[i * 3 + c] - expected))
  }
  t.true(max <= 2, `max diff ${max}`)

  // With alpha: gray+alpha becomes RGBA and the alpha channel passes through the conversion untouched.
  const linearAlpha = await decode(await fixture('graya8-linear-lossless.jxl'))
  const sourceAlpha = await decode(await fixture('ref/graya8-lossless.png'))
  t.is(linearAlpha.colorType, JsColorType.Rgba8)
  max = 0
  for (let i = 0; i < sourceAlpha.raw.length / 2; i++) {
    t.is(linearAlpha.raw[i * 4 + 3], sourceAlpha.raw[i * 2 + 1])
    const expected = Math.round(fromLinear(sourceAlpha.raw[i * 2] / 255) * 255)
    for (let c = 0; c < 3; c++) max = Math.max(max, Math.abs(linearAlpha.raw[i * 4 + c] - expected))
  }
  t.true(max <= 2, `max diff with alpha ${max}`)
})

test('DCI-P3 (pure 2.6 gamma) input is converted to sRGB', async (t) => {
  // DCI white point, P3 primaries and the DCI transfer curve, which is a pure 2.6 gamma. Neutral
  // patches stay neutral (relative intent maps DCI white to D65 white), so they are checked against
  // plain maths: v^2.6 -> sRGB curve. A ~9% too bright curve would give 118 for 128 instead of 113.
  const { colorType, raw } = await decode(await fixture('dci-p3-lossless.jxl'))
  const ref = await decode(await fixture('ref/dci-p3-lossless.png'))
  t.is(colorType, JsColorType.Rgb8)
  t.is(raw.length, ref.raw.length)
  const patches = [32, 64, 128, 200]
  patches.forEach((v, i) => {
    const offset = (8 * 48 + i * 12 + 6) * 3
    const expected = Math.round(fromLinear((v / 255) ** 2.6) * 255)
    const actual = [...raw.subarray(offset, offset + 3)]
    t.true(
      Math.max(...actual.map((c) => Math.abs(c - expected))) <= 1,
      `gray ${v}: got ${actual}, expected ${expected}`,
    )
  })
  // The whole image, including the 4 saturated patches, matches libjxl's own sRGB conversion.
  const diff = maxDiff(raw, ref.raw)
  t.true(diff <= 2, `max diff vs djxl ${diff}`)
})

test('HDR (Rec.2100 PQ) input is tone mapped to 16-bit sRGB', async (t) => {
  // cjxl rgb16.ppm -d 0 -e 7 -x color_space=RGB_D65_202_Rel_PeQ.
  const metadata = await new Transformer(await fixture('rgb16-pq-lossless.jxl')).metadata()
  t.is(metadata.format, 'jxl')
  t.is(metadata.colorType, JsColorType.Rgb16)
  t.is(metadata.width, 48)
  t.is(metadata.height, 32)

  // A neutral PQ ramp, tone mapped like libjxl: Rec. 2408 from 10000 nits down to 250 nits.
  const { colorType, width, raw } = await decode(await fixture('gray-ramp-pq-lossless.jxl'))
  t.is(colorType, JsColorType.Rgb16)
  t.is(width, 256)
  const samples = samples16(raw)
  t.is(samples[0], 0)
  assertMonotonicNeutral(t, samples, 3, 'PQ ramp')
  // libjxl, in two steps: `djxl --display_nits=250` with PQ output, then the PQ curve (x 40), the
  // Rec.2020 -> sRGB matrix and the sRGB curve. Max difference over the whole ramp: 34.
  // Columns 148, 161, 191, 223, 235, 255 are 202, 328, 975, 3085, 4773 and 10000 nits; 202 nits
  // (SDR reference white) comes out at about 74%.
  assertAnchors(t, samples, 3, { 148: 48753, 161: 53716, 191: 61591, 223: 65029, 235: 65411, 255: 65535 }, 'PQ ramp')
})

test('float and 8-bit PQ input give the same pixels as 16-bit PQ input', async (t) => {
  const int16 = await decode(await fixture('gray-ramp-pq-lossless.jxl'))
  // The same ramp stored as 32-bit float: decoded as u16 (its PQ codes are in [0, 1]).
  const float = await decode(await fixture('gray-ramp-pq-f32-lossless.jxl'))
  t.is(float.colorType, JsColorType.Rgb16)
  t.is(maxDiff(float.raw, int16.raw, true), 0)
  // The same PQ codes at 8 bit: tone mapped at 16 bit, then rounded to 8 bit.
  const int8 = await decode(await fixture('gray-ramp-pq8-lossless.jxl'))
  t.is(int8.colorType, JsColorType.Rgb8)
  const expected = Buffer.from(samples16(int16.raw).map((v) => Math.floor((v * 255 + 32767) / 65535)))
  t.is(maxDiff(int8.raw, expected), 0)
})

test('HDR (Rec.2100 HLG) input is tone mapped to 16-bit sRGB', async (t) => {
  // Intensity target 1000 nits: the HLG OOTF for 1000 nits, then down to 80 nits, like libjxl.
  const { colorType, raw } = await decode(await fixture('gray-ramp-hlg-lossless.jxl'))
  t.is(colorType, JsColorType.Rgb16)
  const samples = samples16(raw)
  assertMonotonicNeutral(t, samples, 3, 'HLG ramp')
  // These agree within 1 with `djxl --display_nits=80` on an XYB copy of this ramp.
  assertAnchors(t, samples, 3, { 64: 14933, 128: 26125, 191: 40295, 224: 51669, 255: 65535 }, 'HLG ramp')
})

test('PQ colours far above a 1000-nit intensity target keep their colour', async (t) => {
  // 6 rows (white, orange, yellow, cyan, skin, pastel) x 32 columns, the brightest channel at 500 to
  // 10000 nits. With a 1000-nit target, columns 24-31 (5084+ nits) are all above the source peak:
  // Rec. 2408 brings each to 250 nits, so a row ends in one colour, not white. A gain table that
  // stopped at 4x the target turned the coloured rows white there (off by up to 5904).
  // Anchors: a float64 port of libjxl's tone mapping (Rec. 2408, gamut map in sRGB); measured max
  // difference over the whole grid 4.
  const { colorType, width, raw } = await decode(await fixture('rgb16-colors-pq1000-lossless.jxl'))
  t.is(colorType, JsColorType.Rgb16)
  t.is(width, 32)
  const samples = samples16(raw)
  const anchors = [
    [65535, 65535, 65535],
    [65535, 61495, 59631],
    [65535, 65361, 62222],
    [61832, 65535, 65322],
    [65535, 63565, 63057],
    [64603, 65205, 65535],
  ]
  anchors.forEach((expected, row) => {
    for (let x = 24; x < 32; x++) {
      const offset = (row * 32 + x) * 3
      const actual = [...samples.subarray(offset, offset + 3)]
      t.true(
        actual.every((v, c) => Math.abs(v - expected[c]) <= 8),
        `row ${row} column ${x}: got ${actual}, expected ${expected}`,
      )
    }
  })
})

test('PQ/HLG input with alpha keeps its alpha and tone maps the colour like without it', async (t) => {
  // rgba8-ramp-pq-lossless: the 8-bit ramp of gray-ramp-pq8-lossless plus alpha (x * 37 + 11) % 256.
  const rgba = await decode(await fixture('rgba8-ramp-pq-lossless.jxl'))
  const rgb = await decode(await fixture('gray-ramp-pq8-lossless.jxl'))
  t.is(rgba.colorType, JsColorType.Rgba8)
  for (let x = 0; x < 256; x++) {
    t.is(rgba.raw[x * 4 + 3], (x * 37 + 11) % 256, `alpha at column ${x}`)
    t.deepEqual(
      [...rgba.raw.subarray(x * 4, x * 4 + 3)],
      [...rgb.raw.subarray(x * 3, x * 3 + 3)],
      `colour at column ${x}`,
    )
  }
  // graya16-ramp-hlg-lossless: 16-bit gray HLG ramp x * 257 plus alpha (x * 4099) % 65536.
  const graya = await decode(await fixture('graya16-ramp-hlg-lossless.jxl'))
  t.is(graya.colorType, JsColorType.La16)
  const gray = samples16(graya.raw)
  const hlg = samples16((await decode(await fixture('gray-ramp-hlg-lossless.jxl'))).raw)
  for (let x = 0; x < 256; x++) {
    t.is(gray[x * 2 + 1], (x * 4099) % 65536, `alpha at column ${x}`)
    t.true(Math.abs(gray[x * 2] - hlg[x * 3 + 1]) <= 2, `gray at column ${x}: ${gray[x * 2]} vs ${hlg[x * 3 + 1]}`)
  }
})

test('gray PQ input stays gray and is tone mapped like RGB', async (t) => {
  // jxl-rs cannot build a usable ICC for gray PQ: through it, 99.9% of the pixels came out white.
  const gray = await decode(await fixture('gray16-ramp-pq-lossless.jxl'))
  t.is(gray.colorType, JsColorType.L16)
  const samples = samples16(gray.raw)
  const rgb = samples16((await decode(await fixture('gray-ramp-pq-lossless.jxl'))).raw)
  assertMonotonicNeutral(t, samples, 1, 'gray PQ ramp')
  t.true(samples[1] < 1000, `column 1 is ${samples[1]}`)
  let max = 0
  for (let x = 0; x < 256; x++) max = Math.max(max, Math.abs(samples[x] - rgb[x * 3 + 1]))
  t.true(max <= 2, `max diff vs the RGB ramp ${max}`)
})

test('float PQ/HLG samples outside [0, 1] are tone mapped before any clipping, like libjxl', async (t) => {
  // libjxl linearizes, tone maps and gamut maps the signed, unclipped values and clips only when it
  // writes integers. Clipping the codes first made [1, .6, .3], [1.1, .6, .3] and [1.2, .6, .3] the
  // same colour. Expected: the float64 port of libjxl's tone mapping on the unclipped codes.
  const cases = [
    [
      'rgbf32-hlg-out-of-range-lossless.jxl',
      [
        [65535, 33550, 32941],
        [65535, 42612, 43263],
        [65535, 52123, 52718],
        [65535, 65535, 65535],
        [65535, 65535, 65535],
        [0, 25845, 25100],
        [31544, 0, 13065],
        [26046, 26046, 26046],
        [40369, 40369, 40369],
        [65535, 65535, 65535],
        [0, 0, 0],
        [0, 53841, 29742],
      ],
    ],
    [
      'rgbf32-pq10k-out-of-range-lossless.jxl',
      [
        [65535, 43630, 43670],
        [65535, 51755, 52496],
        [65535, 54010, 54720],
        [65535, 54432, 55138],
        [65535, 65535, 65535],
        [0, 39574, 38483],
        [65535, 11863, 29261],
        [48710, 48710, 48710],
        [61635, 61635, 61635],
        [65535, 65535, 65535],
        [0, 0, 0],
        [0, 64162, 35944],
      ],
    ],
  ]
  for (const [name, expected] of cases) {
    const { colorType, raw } = await decode(await fixture(name))
    t.is(colorType, JsColorType.Rgb16)
    const samples = samples16(raw)
    expected.forEach((pixel, x) => {
      const actual = [...samples.subarray(x * 3, x * 3 + 3)]
      t.true(
        actual.every((v, c) => Math.abs(v - pixel[c]) <= 3),
        `${name} pixel ${x}: got ${actual}, expected ${pixel}`,
      )
    })
  }
})

test('XYB HLG at 250 and 600 nits matches djxl (jxl-rs skips its inverse OOTF there)', async (t) => {
  // For XYB output jxl-rs skips the inverse HLG OOTF when |exponent| < 0.1 (libjxl: 0.01), i.e. for
  // targets in about [161, 282] and [321, 602] nits. Applying the forward OOTF anyway made mid-gray
  // 26856 (250 nits) and 23528 (600 nits) where djxl has 26151 and 26173.
  for (const it of [250, 600]) {
    const jxl = await decode(await fixture(`gray-ramp-hlg${it}-xyb.jxl`))
    const ref = await decode(await fixture(`ref/gray-ramp-hlg${it}-xyb.png`))
    t.is(jxl.colorType, JsColorType.Rgb16)
    const max = maxDiff(jxl.raw, ref.raw, true)
    t.true(max <= 3, `${it} nits: max diff vs djxl ${max}`)
  }
})

test('custom primaries with y = 0 are tone mapped, not black', async (t) => {
  // A primary may have y = 0 (only the white point's y must be above 0). Dividing by it made every
  // pixel NaN, which came out black. Expected: the float64 port of libjxl (`PrimariesToXYZ`).
  const { colorType, width, raw } = await decode(await fixture('pq-blue-y0-lossless.jxl'))
  t.is(colorType, JsColorType.Rgb16)
  t.is(width, 128)
  const samples = samples16(raw)
  const expected = [
    [39307, 39307, 39307],
    [65535, 15222, 26130],
    [0, 54164, 30319],
    [0, 6486, 13942],
    [29904, 25869, 0],
    [58921, 58921, 58921],
    [14506, 14506, 14506],
    [65535, 26804, 21521],
  ]
  expected.forEach((pixel, patch) => {
    const offset = (8 * 128 + patch * 16 + 8) * 3
    const actual = [...samples.subarray(offset, offset + 3)]
    t.true(
      actual.every((v, c) => Math.abs(v - pixel[c]) <= 2),
      `patch ${patch}: got ${actual}, expected ${pixel}`,
    )
  })
})

for (const [name, maxMean, maxMax] of [
  ['pq-colors-xyb', 64, 6000],
  ['hlg-colors-xyb', 32, 6000],
]) {
  test(`${name}: saturated HDR colours are close to djxl`, async (t) => {
    // 13 colours (Rec.2020 primaries and secondaries, sRGB primaries, ...) from black to peak. Out of
    // sRGB colours are desaturated towards gray in linear sRGB, as djxl does with sRGB output for an
    // XYB file; djxl works on the float XYB output and we round codes in [0, 1] to 16 bits, so the
    // two differ most on the brightest saturated pixels. jxl-rs writes HLG codes outside [0, 1] here
    // (-0.074 to 1.003); clipping them before the tone mapping gave HLG mean 70 / max 8911.
    // Measured: PQ mean 45 / max 5272, HLG mean 15 / max 4060.
    const jxl = await decode(await fixture(`${name}.jxl`))
    const ref = await decode(await fixture(`ref/${name}.png`))
    t.is(jxl.colorType, JsColorType.Rgb16)
    t.is(jxl.raw.length, ref.raw.length)
    const mean = meanDiff(jxl.raw, ref.raw)
    const max = maxDiff(jxl.raw, ref.raw, true)
    t.true(mean <= maxMean, `mean diff vs djxl ${mean}`)
    t.true(max <= maxMax, `max diff vs djxl ${max}`)
  })
}

test('jxl input encodes to png, webp, jpeg and avif', async (t) => {
  const input = await fixture('rgba8-lossless.jxl')
  const source = await decode(input)

  // PNG is lossless: the round trip must give back the same pixels.
  const png = await new Transformer(input).png()
  const fromPng = await decode(png)
  t.is(fromPng.format, 'png')
  t.true(fromPng.raw.equals(source.raw))

  const outputs = [
    ['webp', await new Transformer(input).webpLossless()],
    ['jpeg', await new Transformer(input).jpeg(90)],
    ['avif', await new Transformer(input).avif()],
  ]
  for (const [format, buffer] of outputs) {
    const metadata = await new Transformer(buffer).metadata()
    t.is(metadata.format, format)
    t.is(metadata.width, 48)
    t.is(metadata.height, 32)
  }
  // 16-bit input keeps its depth in PNG.
  const png16 = await new Transformer(await fixture('rgb16-lossless.jxl')).png()
  t.is((await new Transformer(png16).metadata()).colorType, JsColorType.Rgb16)
})

test('overlay() accepts a jxl image on top', async (t) => {
  // 64x48 base (an opaque PNG), the opaque 48x32 rgb8 jxl drawn on top at (8, 8).
  const base = await new Transformer(await fixture(join('orientation', 'orientation_1.jxl'))).png()
  const top = await fixture('rgb8-lossless.jxl')
  const topRaw = (await decode(top)).raw
  const raw = await new Transformer(base).overlay(top, 8, 8).rawPixels()
  const bpp = raw.length / (64 * 48)
  for (const [x, y] of [
    [0, 0],
    [20, 10],
    [47, 31],
  ]) {
    const offset = ((y + 8) * 64 + (x + 8)) * bpp
    const expected = [...topRaw.subarray((y * 48 + x) * 3, (y * 48 + x) * 3 + 3)]
    t.deepEqual([...raw.subarray(offset, offset + 3)], expected, `top pixel (${x}, ${y})`)
  }
  // Outside the overlay the base (red top-left quadrant) is untouched.
  t.deepEqual([...raw.subarray(0, 3)], [...(await decode(base)).raw.subarray(0, 3)])
})

test('truncated jxl input rejects', async (t) => {
  for (const name of ['rgb8-lossless.jxl', 'rgb16-lossless.jxl', 'rgb8-lossy.jxl']) {
    const buffer = await fixture(name)
    // Every cut keeps the full signature (12 bytes for the container), so the input is still
    // detected as JPEG XL and must fail inside the JXL decoder.
    for (const length of [20, Math.floor(buffer.length / 2), buffer.length - 1]) {
      await t.throwsAsync(
        () => new Transformer(buffer.subarray(0, length)).metadata(),
        { message: /^Decode jxl image failed/ },
        `${name} cut to ${length} bytes`,
      )
    }
  }
})

test('a decompression bomb rejects fast without allocating', async (t) => {
  // 387 bytes that declare 12000x12000: rejected from the header, before any pixel buffer exists.
  const bomb = await fixture('bomb.jxl')
  const start = performance.now()
  await t.throwsAsync(() => new Transformer(bomb).metadata(), {
    code: 'InvalidArg',
    message: /^Decode jxl image failed: .*too large/,
  })
  t.true(performance.now() - start < 2000, 'must not decode the 12000x12000 image')
})

test('non-jxl input is unaffected', async (t) => {
  const png = await fs.readFile(join(ROOT_DIR, 'un-optimized.png'))
  t.is((await new Transformer(png).metadata()).format, 'png')
  // Bytes that are neither a JXL signature nor a known format keep the existing error.
  await t.throwsAsync(() => new Transformer(Buffer.from([0xff, 0x0b, 0, 1, 2, 3, 4, 5])).metadata(), {
    message: /^Guess format from input image failed/,
  })
})
