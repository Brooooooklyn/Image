import { promises as fs } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import test from 'ava'

import { losslessCompressPng, pngQuantize, compressJpeg, Transformer } from '../index.js'

const ROOT_DIR = join(fileURLToPath(import.meta.url), '..', '..', '..', '..')

const PNG = await fs.readFile(join(ROOT_DIR, 'un-optimized.png'))
const JPEG = await fs.readFile(join(ROOT_DIR, 'un-optimized.jpg'))
const GAMA = await fs.readFile(join(ROOT_DIR, 'image-with-gama.png'))

test('should be able to lossy optimize png image which has gama chunk', async (t) => {
  if (process.env.NAPI_RS_FORCE_WASI) {
    t.pass()
    return
  }
  const dest = await pngQuantize(GAMA, { speed: 5, maxQuality: 1, minQuality: 1 })
  t.true(dest.length < PNG.length)
})

// Read width/height straight from the PNG IHDR (big-endian u32 at byte 16/20).
// Deliberately avoids a native image dependency (e.g. sharp) so this spec always
// imports, even on a runtime where that optional native module cannot load.
function pngDimensions(buf) {
  const SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]
  for (let i = 0; i < SIGNATURE.length; i++) {
    if (buf[i] !== SIGNATURE[i]) {
      throw new Error('not a PNG')
    }
  }
  return { width: buf.readUInt32BE(16), height: buf.readUInt32BE(20) }
}

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

test('pngQuantize roundtrip preserves dimensions and shrinks file', async (t) => {
  if (process.env.NAPI_RS_FORCE_WASI) {
    t.pass()
    return
  }
  const { width, height } = pngDimensions(PNG)
  const dest = await pngQuantize(PNG, { maxQuality: 75 })
  const out = pngDimensions(dest)
  t.is(out.width, width)
  t.is(out.height, height)
  t.true(dest.length < PNG.length)
  // P2 size-lock: the quantized PNG is losslessly recompressed by the in-repo oxipng
  // pass. The bare lodepng encode of this fixture is 262053 bytes; recompression brings
  // it to ~250061. Asserting < 255000 locks the oxipng pass in: drop it and this fails
  // (262053 > 255000). Loose enough to survive minor quantizer drift, tight enough to
  // catch the recompression being skipped.
  t.true(dest.length < 255000, `expected recompressed < 255000, got ${dest.length}`)
})

test('pngQuantize shrinks the file at both low and high quality', async (t) => {
  if (process.env.NAPI_RS_FORCE_WASI) {
    t.pass()
    return
  }
  // Both a small and a large palette must produce a smaller PNG than the input.
  // Final byte size is an emergent property of DEFLATE over the index stream, so
  // we do NOT assert an ordering between the two — only that both shrink.
  const low = await pngQuantize(PNG, { maxQuality: 20, minQuality: 0 })
  const high = await pngQuantize(PNG, { maxQuality: 99, minQuality: 0 })
  t.true(low.length < PNG.length)
  t.true(high.length < PNG.length)
})

test('pngQuantize with default options does not throw on a real photo', async (t) => {
  if (process.env.NAPI_RS_FORCE_WASI) {
    t.pass()
    return
  }
  // Regression guard: the default min_quality=70 gate must NOT reject an ordinary
  // photographic image. The quality metric is calibrated so a default 256-color
  // reduction clears 70 with margin.
  await t.notThrowsAsync(() => pngQuantize(PNG))
  const dest = await pngQuantize(PNG)
  t.true(dest.length < PNG.length)
})

test('pngQuantize honors posterization without throwing at default quality', async (t) => {
  if (process.env.NAPI_RS_FORCE_WASI) {
    t.pass()
    return
  }
  // posterization is an explicit lossy pre-step; it must not count against the
  // min_quality gate, so this must not throw at the default min_quality=70.
  await t.notThrowsAsync(() => pngQuantize(PNG, { posterization: 4 }))
})

test('pngQuantize throws when min quality is unreachable', async (t) => {
  if (process.env.NAPI_RS_FORCE_WASI) {
    t.pass()
    return
  }
  // A true-color photo cannot reach quality 100 in a 256-color palette, so the
  // min_quality gate must throw.
  await t.throwsAsync(() => pngQuantize(PNG, { minQuality: 100, maxQuality: 100 }))
})

test('pngQuantize rejects out-of-range options', async (t) => {
  if (process.env.NAPI_RS_FORCE_WASI) {
    t.pass()
    return
  }
  // Parity with the previous imagequant-backed API: invalid speed/quality bounds
  // must throw rather than being silently clamped.
  await t.throwsAsync(() => pngQuantize(PNG, { speed: 0 }))
  await t.throwsAsync(() => pngQuantize(PNG, { speed: 11 }))
  await t.throwsAsync(() => pngQuantize(PNG, { maxQuality: 101 }))
  await t.throwsAsync(() => pngQuantize(PNG, { minQuality: 200 }))
  await t.throwsAsync(() => pngQuantize(PNG, { minQuality: 80, maxQuality: 50 }))
})

test('pngQuantize rejects a non-PNG (JPEG) input', async (t) => {
  if (process.env.NAPI_RS_FORCE_WASI) {
    t.pass()
    return
  }
  // pngQuantize is a PNG optimizer: a non-PNG buffer (here a JPEG) must be
  // rejected at decode rather than silently transcoded to a quantized PNG,
  // which would break the same-format contract callers rely on (MIME/extension
  // stay PNG). Guards against the decoder regressing to format-sniffing.
  //
  // Assert the error IDENTITY, not merely that it throws: code `InvalidArg`
  // plus the PNG-only decode message. This makes the test a canary — it fails
  // if the suite ever loads a stale prebuilt addon (whose decode path throws a
  // different "Read png info failed" message) instead of this freshly built one.
  await t.throwsAsync(() => pngQuantize(JPEG), {
    code: 'InvalidArg',
    message: /Decode png failed/,
  })
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
  t.true(
    new Transformer(PNG).avifSync({
      speed: 10,
      threads: 1,
    }).length < PNG.length,
  )
})

test('should be able to encode avif from jpeg', (t) => {
  t.true(
    new Transformer(JPEG).avifSync({
      speed: 10,
      threads: 1,
    }).length < JPEG.length,
  )
})
