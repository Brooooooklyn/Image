import { createRequire } from 'node:module'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const __dirname = dirname(fileURLToPath(import.meta.url))
const require = createRequire(import.meta.url)

console.log('[repro] process.arch =', process.arch, '| node', process.version)
const { Transformer } = require(join(__dirname, 'image.wasi.cjs'))
const SVG = readFileSync(join(__dirname, 'input-debian.svg'))

function bbox(raw, width, height) {
  let minX = width, minY = height, maxX = -1, maxY = -1
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (raw[(y * width + x) * 4 + 3] > 10) {
        if (x < minX) minX = x; if (x > maxX) maxX = x
        if (y < minY) minY = y; if (y > maxY) maxY = y
      }
    }
  }
  return { minX, minY, maxX, maxY }
}
let fails = 0
function ok(name, cond, detail) { console.log(`[${cond ? 'PASS' : 'FAIL'}] ${name} ${detail || ''}`); if (!cond) fails++ }
function svg(s) { return Buffer.from(s) }

// Mirror of transformer-svg.spec.mjs — all 10 #159 tests, in file order, one process.
function t1_fills() {
  const png = Transformer.fromSvg(SVG).pngSync()
  const { width, height } = new Transformer(png).metadataSync()
  const raw = Transformer.fromSvg(SVG).rawPixelsSync()
  const b = bbox(raw, width, height)
  ok('T1 fills-canvas', b.maxX > width * 0.6 && b.maxY > height * 0.6, `${width}x${height} maxX=${b.maxX} maxY=${b.maxY}`)
}
function t2_aspect() {
  const s = svg(`<svg width="0.6" height="1" viewBox="0 0 0.6 1" xmlns="http://www.w3.org/2000/svg"><circle cx="0.3" cy="0.5" r="0.3" fill="black"/></svg>`)
  const png = Transformer.fromSvg(s).pngSync()
  const { width, height } = new Transformer(png).metadataSync()
  const raw = Transformer.fromSvg(s).rawPixelsSync()
  const b = bbox(raw, width, height); const aspect = (b.maxX - b.minX + 1) / (b.maxY - b.minY + 1)
  ok('T2 aspect', aspect > 0.9 && aspect < 1.1, `aspect=${aspect.toFixed(3)}`)
}
function t3_degenerate() {
  const s = svg(`<svg width="1e-39" height="1e-39" viewBox="0 0 1 1" xmlns="http://www.w3.org/2000/svg"><rect width="1" height="1" fill="black"/></svg>`)
  try {
    const raw = Transformer.fromSvg(s).rawPixelsSync()
    let has = false; for (let i = 3; i < raw.length; i += 4) { if (raw[i] > 10) { has = true; break } }
    ok('T3 degenerate', has, 'rendered content')
  } catch { ok('T3 degenerate', true, 'errored (acceptable)') }
}
function t4_thin() {
  const s = svg(`<svg width="10" height="4000" viewBox="0 0 10 4000" xmlns="http://www.w3.org/2000/svg"><rect width="10" height="4000" fill="black"/></svg>`)
  const png = Transformer.fromSvg(s).pngSync(); const { width, height } = new Transformer(png).metadataSync()
  ok('T4 thin-bounded', width * height < 5_000_000 && height > width * 100, `${width}x${height}`)
}
function t5_oversized() {
  const s = svg(`<svg width="20000" height="20000" viewBox="0 0 20000 20000" xmlns="http://www.w3.org/2000/svg"><rect width="20000" height="20000" fill="black"/></svg>`)
  let threw = false; try { Transformer.fromSvg(s) } catch { threw = true }
  ok('T5 oversized-guard', threw, `threw=${threw}`)
}
function t6_subpixel_axis() {
  const s = svg(`<svg width="0.5" height="2000" viewBox="0 0 0.5 2000" xmlns="http://www.w3.org/2000/svg"><rect width="0.5" height="2000" fill="black"/></svg>`)
  const png = Transformer.fromSvg(s).pngSync(); const { width, height } = new Transformer(png).metadataSync()
  ok('T6 subpixel-axis', width >= 1 && height === 2000, `${width}x${height}`)
}
function t7_rounding_guard() {
  const s = svg(`<svg width="1.5" height="178955968" viewBox="0 0 1.5 178955968" xmlns="http://www.w3.org/2000/svg"><rect width="1.5" height="178955968" fill="black"/></svg>`)
  let threw = false; try { Transformer.fromSvg(s) } catch { threw = true }
  ok('T7 rounding-guard', threw, `threw=${threw}`)
}
function t8_short_axis() {
  const s = svg(`<svg width="2000" height="0.1" viewBox="0 0 2000 0.1" xmlns="http://www.w3.org/2000/svg"><rect width="2000" height="0.1" fill="black"/></svg>`)
  let threw = false; try { Transformer.fromSvg(s) } catch { threw = true }
  ok('T8 short-axis-errors', threw, `threw=${threw}`)
}
function t9_near1000() {
  const s = svg(`<svg width="999.6" height="999.6" viewBox="0 0 999.6 999.6" xmlns="http://www.w3.org/2000/svg"><rect width="999.6" height="999.6" fill="black"/></svg>`)
  const png = Transformer.fromSvg(s).pngSync(); const { width, height } = new Transformer(png).metadataSync()
  ok('T9 near-1000', width === 1000 && height === 1000, `${width}x${height}`)
}
function t10_demult() {
  const s = svg('<svg width="2" height="2" xmlns="http://www.w3.org/2000/svg"></svg>')
  const px = Transformer.fromSvg(s, 'rgba(10, 20, 30, .8)').rawPixelsSync().slice(0, 4)
  ok('T10 demultiplied', px[3] === 204 && Math.abs(px[0] - 10) <= 1, `[${px[0]},${px[1]},${px[2]},${px[3]}]`)
}

const tests = [t1_fills, t2_aspect, t3_degenerate, t4_thin, t5_oversized, t6_subpixel_axis, t7_rounding_guard, t8_short_axis, t9_near1000, t10_demult]
// Run the whole file 3x to accumulate heap (emnapi only frees on GC) — mimics no-gc CI pressure.
for (let pass = 1; pass <= 3; pass++) {
  console.log(`--- pass ${pass} ---`)
  for (const fn of tests) {
    try { fn() } catch (e) { console.log(`[FAIL] ${fn.name} THREW: ${e.message}`); fails++ }
  }
}
console.log(`\n[repro] DONE — ${fails} failure(s). ${fails > 0 ? 'REPRODUCED x86 corruption ✘' : 'clean (no repro) ✔'}`)
