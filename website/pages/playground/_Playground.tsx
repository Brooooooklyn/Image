import { useEffect, useState } from 'react'
type Result = { status: 'idle' | 'running' | 'done' | 'error'; bytes?: number; error?: string }
export default function Playground() {
  const [r, setR] = useState<Result>({ status: 'idle' })
  useEffect(() => {
    if (!self.crossOriginIsolated) {
      setR({ status: 'error', error: 'not cross-origin isolated' })
      return
    }
    setR({ status: 'running' })
    const worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' })
    worker.onmessage = (e: MessageEvent<{ ok: boolean; bytes?: number; error?: string }>) => {
      setR(e.data.ok ? { status: 'done', bytes: e.data.bytes } : { status: 'error', error: e.data.error })
      worker.terminate()
    }
    fetch('/img/un-optimized.png')
      .then((res) => res.arrayBuffer())
      .then((buf) => worker.postMessage(buf, [buf]))
      .catch((err) => setR({ status: 'error', error: String(err) }))
    return () => worker.terminate()
  }, [])
  return (
    <div data-testid="pg-status" data-status={r.status} className="px-6 py-12 font-mono">
      <p>crossOriginIsolated: {String(typeof self !== 'undefined' && self.crossOriginIsolated)}</p>
      {r.status === 'done' && <p data-testid="pg-bytes">Encoded WebP: {r.bytes} bytes</p>}
      {r.status === 'error' && <p data-testid="pg-error">Error: {r.error}</p>}
      {r.status === 'running' && <p>Encoding…</p>}
    </div>
  )
}
