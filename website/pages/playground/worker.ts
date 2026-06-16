/// <reference lib="webworker" />
import { Buffer } from 'buffer'
import type { WorkerRequest, WorkerResponse, ConvertOp, CompressOp, TransformOp } from './protocol'

// @napi-rs/image's encoders return a Node Buffer; the emnapi runtime needs globalThis.Buffer
// defined BEFORE the dynamic import('@napi-rs/image'), or it throws NotSupportBufferError.
if (typeof (globalThis as { Buffer?: unknown }).Buffer === 'undefined') {
  ;(globalThis as { Buffer?: unknown }).Buffer = Buffer
}

type Mod = typeof import('@napi-rs/image')

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
  const t = new mod.Transformer(u8)
  switch (op.format) {
    case 'webp': return t.webp(op.quality)
    case 'webpLossless': return t.webpLossless()
    case 'avif': return t.avif({ quality: op.quality, chromaSubsampling: op.chroma })
    case 'jpeg': return t.jpeg(op.quality)
    case 'png': return t.png()
  }
}

async function runCompress(mod: Mod, u8: Uint8Array, op: CompressOp): Promise<Uint8Array> {
  // Use the ASYNC variants. oxipng (`oxipng/parallel`) and imagequant fan out
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
  let t = new mod.Transformer(u8)
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
      const m = await new mod.Transformer(u8).metadata(true)
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
