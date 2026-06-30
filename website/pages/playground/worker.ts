/// <reference lib="webworker" />
//
// DEPLOY REQUIREMENT — this module is bundled to a hashed `assets/worker-*.js` and spawned from the
// COEP:require-corp /playground document. A dedicated worker created by a require-corp document is
// itself blocked unless its OWN response carries `Cross-Origin-Embedder-Policy: require-corp`. That
// header is set on `/assets/*` in void.json. BUT hashed-asset cache keys are unversioned and survive
// deploys (CF edge caches per content-encoding variant), so a no-COEP response cached before the
// header existed keeps being served to browsers even after the rule is added — the worker then spawns
// with `ONERROR` and the playground hangs on "Loading image metadata…". The only reliable fix is to
// mint a NEW asset hash so the URL has no stale cache history. The BUILD_TAG below changes the emitted
// bytes (a comment alone is stripped by minification and does NOT change the hash). Bump it whenever
// the COEP/serving story changes and a clean asset URL is needed.
import { Buffer } from 'buffer'
import type { WorkerRequest, WorkerResponse, ConvertOp, CompressOp, TransformOp } from './protocol'

// Names the worker thread (visible in devtools) AND, as a live side effect on `self`, survives
// minification — so editing BUILD_TAG mints a fresh `assets/worker-*.js` hash. See header note.
const BUILD_TAG = 'napi-image-engine@coep-2026-06-16'
;(self as { name?: string }).name = BUILD_TAG

// @napi-rs/image's encoders return a Node Buffer; the emnapi runtime needs globalThis.Buffer
// defined BEFORE the dynamic import('@napi-rs/image'), or it throws NotSupportBufferError.
if (typeof (globalThis as { Buffer?: unknown }).Buffer === 'undefined') {
  ;(globalThis as { Buffer?: unknown }).Buffer = Buffer
}

type Mod = typeof import('@napi-rs/image')

// SVG is XML text, so it has no binary magic bytes: the generic `new Transformer(bytes)` path
// routes through the wasm's `image::guess_format`, which has no SVG signature and throws
// "Guess format from input image failed". The binding decodes SVG only through the dedicated
// `Transformer.fromSvg`, which rasterizes via resvg. Sniff the bytes here and pick that path.
//
// Detection: every raster format the binding handles starts with binary magic bytes (never '<'),
// so first require the markup to open with '<' (after an optional UTF-8 BOM and leading
// whitespace), then look for an <svg> root element. The element name may carry an XML namespace
// prefix (e.g. `<s:svg xmlns:s="http://www.w3.org/2000/svg">`), which usvg/fromSvg accepts, so
// match an optional `prefix:` and require the name to be exactly `svg` (the trailing [\s/>] rules
// out longer names like `<svgfoo>`). A plain `<svg ...>`, an `<?xml?>`/`<!DOCTYPE>` prolog, or a
// BOM-prefixed document all reach this test.
const SVG_ROOT = /<(?:[A-Za-z_][\w.-]*:)?svg[\s/>]/i
function looksLikeSvg(u8: Uint8Array): boolean {
  let i = 0
  if (u8[0] === 0xef && u8[1] === 0xbb && u8[2] === 0xbf) i = 3 // skip UTF-8 BOM
  while (i < u8.length && (u8[i] === 0x09 || u8[i] === 0x0a || u8[i] === 0x0d || u8[i] === 0x20)) i++
  if (u8[i] !== 0x3c /* '<' */) return false
  const head = new TextDecoder('utf-8', { fatal: false }).decode(u8.subarray(i, i + 65536))
  return SVG_ROOT.test(head)
}

// Build the base Transformer, decoding SVG inputs through `fromSvg` and everything else through
// the raster constructor. Both yield a Transformer whose metadata()/encoders/transforms work the
// same, so callers stay format-agnostic.
function makeTransformer(mod: Mod, u8: Uint8Array) {
  return looksLikeSvg(u8) ? mod.Transformer.fromSvg(u8) : new mod.Transformer(u8)
}

function toArrayBuffer(out: Uint8Array): ArrayBuffer {
  // Copy into a FRESH regular ArrayBuffer. The wasm runs with threads, so its
  // Memory is `shared: true` and HEAPU8.buffer is a SharedArrayBuffer; if the
  // returned Buffer views that heap, `out.buffer.slice()` would yield another
  // SharedArrayBuffer — which postMessage cannot TRANSFER. Building a new
  // ArrayBuffer and copying the bytes is transferable regardless of the source.
  const ab = new ArrayBuffer(out.byteLength)
  new Uint8Array(ab).set(out)
  return ab
}

async function runConvert(mod: Mod, u8: Uint8Array, op: ConvertOp): Promise<Uint8Array> {
  const t = makeTransformer(mod, u8)
  switch (op.format) {
    case 'webp': return t.webp(op.quality)
    case 'webpLossless': return t.webpLossless()
    case 'avif': return t.avif({ quality: op.quality, chromaSubsampling: op.chroma })
    case 'jpeg': return t.jpeg(op.quality)
    case 'png': return t.png()
  }
}

async function runCompress(mod: Mod, u8: Uint8Array, op: CompressOp): Promise<Uint8Array> {
  // Use the ASYNC variants. oxipng (`oxipng/parallel`) fans out
  // via rayon, which spawns wasi pthreads. emnapi runs these async tasks on its
  // async-work worker pool, and emnapi >= 1.9.0 (@emnapi/wasi-threads >= 1.2.0)
  // routes `spawn-thread` requests from those async-work workers correctly, so
  // the rayon spawn no longer deadlocks. (Pre-1.9.0 dropped that message and the
  // call hung forever — both sync and async.) The async path keeps the worker's
  // event loop free to service the thread-creation round-trip.
  switch (op.codec) {
    case 'jpeg': return mod.compressJpeg(u8, { quality: op.quality })
    case 'pngLossless': return mod.losslessCompressPng(Buffer.from(u8))
    case 'pngQuantize': return mod.pngQuantize(u8, { maxQuality: op.maxQuality })
  }
}

async function runTransform(mod: Mod, u8: Uint8Array, op: TransformOp): Promise<Uint8Array> {
  let t = makeTransformer(mod, u8)
  if (op.rotate === 'auto') t = t.rotate()
  else if (typeof op.rotate === 'number') t = t.rotate(op.rotate)
  if (op.resize.enabled) t = t.resize(op.resize.width, op.resize.height, op.resize.filter, op.resize.fit)
  if (op.grayscale) t = t.grayscale()
  if (op.invert) t = t.invert()
  if (op.blur != null) t = t.blur(op.blur)
  switch (op.encode.format) {
    case 'webp': return t.webp(op.encode.quality)
    case 'webpLossless': return t.webpLossless()
    case 'avif': return t.avif({ quality: op.encode.quality })
    case 'jpeg': return t.jpeg(op.encode.quality)
    case 'png': return t.png()
  }
}

self.onmessage = async (e: MessageEvent<WorkerRequest>) => {
  const { id, op, bytes } = e.data
  const post = (msg: WorkerResponse, transfer: Transferable[] = []) =>
    (self as unknown as Worker).postMessage(msg, transfer)
  try {
    const mod: Mod = await import('@napi-rs/image')
    const u8 = new Uint8Array(bytes)
    if (op.kind === 'metadata') {
      const m = await makeTransformer(mod, u8).metadata(true)
      post({ id, ok: true, kind: 'metadata', meta: { width: m.width, height: m.height, format: m.format, orientation: m.orientation } })
      return
    }
    const out =
      op.kind === 'convert' ? await runConvert(mod, u8, op)
      : op.kind === 'compress' ? await runCompress(mod, u8, op)
      : await runTransform(mod, u8, op)
    const outFormat = op.kind === 'transform' ? op.encode.format : op.kind === 'convert' ? op.format : op.codec === 'jpeg' ? 'jpeg' : 'png'
    const ab = toArrayBuffer(out as unknown as Uint8Array)
    post({ id, ok: true, kind: op.kind, bytes: ab, outFormat }, [ab])
  } catch (err) {
    post({ id, ok: false, error: err instanceof Error ? err.message : String(err) })
  }
}
