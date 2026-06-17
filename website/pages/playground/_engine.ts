// website/pages/playground/_engine.ts
import type { Op, WorkerResponse, ResultMeta } from './protocol'

export type RunResult =
  | { ok: true; kind: 'metadata'; meta: ResultMeta }
  | { ok: true; kind: 'convert' | 'compress' | 'transform'; bytes: ArrayBuffer; outFormat: string }
  | { ok: false; error: string }

export class PlaygroundEngine {
  private worker: Worker
  private seq = 0
  private pending = new Map<number, (r: WorkerResponse) => void>()

  constructor() {
    this.worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' })
    this.worker.onmessage = (e: MessageEvent<WorkerResponse>) => {
      const resolve = this.pending.get(e.data.id)
      if (resolve) { this.pending.delete(e.data.id); resolve(e.data) }
    }
  }

  run(op: Op, bytes: ArrayBuffer): Promise<RunResult> {
    const id = ++this.seq
    // The worker takes ownership of `bytes` (transferred); callers must pass a copy
    // if they still need the original (the UI keeps the original File/Blob separately).
    return new Promise<RunResult>((resolve) => {
      this.pending.set(id, (r) => resolve(r as RunResult))
      this.worker.postMessage({ id, op, bytes }, [bytes])
    })
  }

  dispose() {
    this.worker.terminate()
    this.pending.clear()
  }
}
