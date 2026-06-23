// Diagnostic probe for the x86-only wasi SVG miscompile (issue #159 follow-up). NOT a test — run
// manually/in CI under the wasm binding with NAPI_IMAGE_SVG_DEBUG=1 to print from_svg's intermediate
// values (scale, target dims, rendered content bbox) on the real x86 runner where the corruption
// happens. Run: NAPI_RS_FORCE_WASI=1 NAPI_IMAGE_SVG_DEBUG=1 node packages/binding/__test__/x86-svg-probe.mjs
import { promises as fs } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { Transformer } from '../index.js'

const __DIRNAME = join(fileURLToPath(import.meta.url), '..')
const ROOT_DIR = join(__DIRNAME, '..', '..', '..')

console.log(`[probe] node ${process.version} arch=${process.arch} FORCE_WASI=${process.env.NAPI_RS_FORCE_WASI ?? '(unset)'}`)

function bbox(raw, width, height) {
  let maxX = -1, maxY = -1
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (raw[(y * width + x) * 4 + 3] > 10) {
        if (x > maxX) maxX = x
        if (y > maxY) maxY = y
      }
    }
  }
  return { maxX, maxY }
}

// Case 1 FIRST (the headline corner-speck) so its logs print before any later SIGABRT.
try {
  const SVG = await fs.readFile(join(ROOT_DIR, 'input-debian.svg'))
  const png = Transformer.fromSvg(SVG).pngSync()
  const { width, height } = new Transformer(png).metadataSync()
  const raw = Transformer.fromSvg(SVG).rawPixelsSync()
  const b = bbox(raw, width, height)
  const verdict = b.maxX > width * 0.6 ? 'FILLS (correct)' : 'CORNER-SPECK (corrupt)'
  console.log(`[probe] debian.svg: pixmap ${width}x${height}  JS bbox maxX=${b.maxX} maxY=${b.maxY}  -> ${verdict}`)
} catch (e) {
  console.log('[probe] debian.svg THREW:', e.message)
}

// Case 2: oversized SVG must throw (size-guard). x86 CI let it through.
try {
  const svg = Buffer.from(`<svg width="20000" height="20000" viewBox="0 0 20000 20000" xmlns="http://www.w3.org/2000/svg"><rect width="20000" height="20000" fill="black"/></svg>`)
  Transformer.fromSvg(svg)
  console.log('[probe] oversized 20000x20000: did NOT throw -> guard BYPASSED (corrupt)')
} catch (e) {
  console.log('[probe] oversized 20000x20000: threw (correct) ->', e.message)
}

// Case 3: sub-pixel axis (0.5 x 2000) — the case that preceded the 5.86 TB SIGABRT on CI.
try {
  const svg = Buffer.from(`<svg width="0.5" height="2000" viewBox="0 0 0.5 2000" xmlns="http://www.w3.org/2000/svg"><rect width="0.5" height="2000" fill="black"/></svg>`)
  const png = Transformer.fromSvg(svg).pngSync()
  const { width, height } = new Transformer(png).metadataSync()
  console.log(`[probe] subpixel 0.5x2000: pixmap ${width}x${height} (expect 1x2000)`)
} catch (e) {
  console.log('[probe] subpixel 0.5x2000 THREW:', e.message)
}

console.log('[probe] done')
