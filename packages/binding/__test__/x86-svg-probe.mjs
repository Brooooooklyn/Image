// Diagnostic probe for the x86 wasi SVG corruption (issue #159 follow-up). NOT a test.
// Compares what `../index.js` (the loader the test suite uses) resolves vs. the fresh local
// `../image.wasi.cjs`, to detect whether CI has been testing a STALE PUBLISHED npm binary instead of
// the freshly-built wasm.
import { createRequire } from 'node:module'
import { promises as fs } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const __DIRNAME = join(fileURLToPath(import.meta.url), '..')
const ROOT_DIR = join(__DIRNAME, '..', '..', '..')
const SVG = await fs.readFile(join(ROOT_DIR, 'input-debian.svg'))

console.log(`[probe] node ${process.version} arch=${process.arch} FORCE_WASI=${process.env.NAPI_RS_FORCE_WASI ?? '(unset)'}`)

// Is a PUBLISHED npm binding installed (which the loader may prefer over the fresh local build)?
for (const pkg of ['@napi-rs/image-wasm32-wasi', `@napi-rs/image-linux-x64-gnu`, '@napi-rs/image-linux-x64-musl']) {
  try {
    console.log(`[probe] resolved ${pkg} -> ${require.resolve(pkg)}`)
  } catch {
    console.log(`[probe] ${pkg} NOT installed`)
  }
}

function bboxMaxX(raw, width, height) {
  let maxX = -1
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) if (raw[(y * width + x) * 4 + 3] > 10 && x > maxX) maxX = x
  return maxX
}

function check(label, Transformer) {
  console.log(`\n[probe] === binding via ${label} ===`)
  console.log(`[probe] typeof fromSvgDebug = ${typeof Transformer.fromSvgDebug}`)
  if (typeof Transformer.fromSvgDebug === 'function') {
    try { console.log('[probe] TRACE:', Transformer.fromSvgDebug(SVG)) } catch (e) { console.log('[probe] TRACE threw:', e.message) }
  }
  try {
    const png = Transformer.fromSvg(SVG).pngSync()
    const { width, height } = new Transformer(png).metadataSync()
    const raw = Transformer.fromSvg(SVG).rawPixelsSync()
    const mx = bboxMaxX(raw, width, height)
    console.log(`[probe] debian render: pixmap ${width}x${height} maxX=${mx} -> ${mx > width * 0.6 ? 'FILLS (correct)' : 'CORNER-SPECK (corrupt)'}`)
  } catch (e) {
    console.log('[probe] render threw:', e.message)
  }
}

// A) What the test suite actually loads.
try { check('../index.js (what `yarn test` uses)', (await import('../index.js')).Transformer) }
catch (e) { console.log('[probe] index.js load failed:', e.message) }

// B) The freshly-built local wasm, loaded directly (bypassing the loader's stale-package preference).
try { check('../image.wasi.cjs (fresh local wasm)', require('../image.wasi.cjs').Transformer) }
catch (e) { console.log('[probe] image.wasi.cjs load failed:', e.message) }

console.log('\n[probe] done')
