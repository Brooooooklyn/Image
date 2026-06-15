/// <reference lib="webworker" />
self.onmessage = async (e: MessageEvent<ArrayBuffer>) => {
  try {
    const { Transformer } = await import('@napi-rs/image')
    const out = await new Transformer(new Uint8Array(e.data)).webp(75)
    ;(self as unknown as Worker).postMessage({ ok: true, bytes: out.byteLength })
  } catch (err) {
    ;(self as unknown as Worker).postMessage({ ok: false, error: String(err) })
  }
}
