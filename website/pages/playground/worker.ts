/// <reference lib="webworker" />
import { Buffer } from 'buffer'

// @napi-rs/image's webp() returns a Node Buffer. The emnapi runtime that backs the
// wasm build calls emnapi_create_memory_view, which throws NotSupportBufferError unless
// `globalThis.Buffer` is defined (browsers have no Node Buffer). Install the `buffer`
// polyfill BEFORE the dynamic `import('@napi-rs/image')` below so the napi context picks
// it up as `feature.Buffer` at module-init time.
if (typeof (globalThis as { Buffer?: unknown }).Buffer === 'undefined') {
  ;(globalThis as { Buffer?: unknown }).Buffer = Buffer
}

// A real WebP is a RIFF container: bytes 0-3 = "RIFF", bytes 8-11 = "WEBP".
// Checking this proves the worker produced an actual encoded image rather than
// some non-zero-length placeholder, so the e2e gate's byte assertion is meaningful.
function isWebp(out: Uint8Array): boolean {
  return (
    out.length > 12 &&
    out[0] === 0x52 && // R
    out[1] === 0x49 && // I
    out[2] === 0x46 && // F
    out[3] === 0x46 && // F
    out[8] === 0x57 && // W
    out[9] === 0x45 && // E
    out[10] === 0x42 && // B
    out[11] === 0x50 // P
  )
}

self.onmessage = async (e: MessageEvent<ArrayBuffer>) => {
  try {
    const { Transformer } = await import('@napi-rs/image')
    const out = await new Transformer(new Uint8Array(e.data)).webp(75)
    // `out` is a Node Buffer (Uint8Array subclass); index it directly.
    if (!isWebp(out as unknown as Uint8Array)) {
      throw new Error(`encode did not produce a WebP (got ${out.byteLength} bytes, bad signature)`)
    }
    ;(self as unknown as Worker).postMessage({ ok: true, bytes: out.byteLength })
  } catch (err) {
    ;(self as unknown as Worker).postMessage({ ok: false, error: String(err) })
  }
}
