# P3 — Interactive WASM Playground Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the proven `/playground` smoke-test island into a full in-browser `@napi-rs/image` tool: upload an image, run format-conversion / compress-in-place / resize-rotate-transform entirely client-side in a Web Worker, and see a draggable before/after compare with output size, % savings, a download, and a generated code snippet.

**Architecture:** SSR shell (controls + dropzone, instant paint) → `island: 'load'` hydration → ALL wasm/Worker work deferred into `useEffect` + a typed Web Worker. The worker imports `@napi-rs/image` (the browser build resolves to `@napi-rs/image-wasm32-wasi`) and dispatches a typed `{id, op, bytes}` protocol. `crossOriginIsolated === false` → static showcase fallback. Reuses the P2 `_BeforeAfter` island and the showcase data for the fallback.

**Tech Stack:** Void pages mode (island page), React 19, Tailwind v4, `@napi-rs/image` 1.12.0 (wasm32-wasi browser build), TypeScript ESNext (island import attributes), Playwright e2e (cross-origin-isolated browser).

---

## Verified API facts (from node_modules — do NOT guess; these are the contract)

- `@napi-rs/image` browser build exports the SAME surface as native (`image.wasi-browser.js`). Worker polyfills `globalThis.Buffer` BEFORE importing (already in place).
- **Convert** (on `new Transformer(u8)`, all `Promise<Buffer>`): `webp(q?)`, `webpLossless()`, `avif({quality?,alphaQuality?,speed?,chromaSubsampling?})`, `jpeg(q?)`, `png({compressionType?,filterType?})`, plus `bmp/ico/tiff/tga/farbfeld/pnm`.
- **Compress-in-place** (standalone, `Promise<Buffer>`): `compressJpeg(u8,{quality?,optimizeScans?})` (JPEG input), `losslessCompressPng(u8,{...})` (PNG input), `pngQuantize(u8,{minQuality?,maxQuality?,speed?,posterization?})` (PNG input).
- **Transform** (chainable, return `this`): `resize(w, h?, filter?, fit?)`, `rotate(orientation?)`, `crop(x,y,w,h)`, `grayscale()`, `invert()`, `blur(sigma)`, `brighten(b)`, `adjustContrast(c)`, `huerotate(h)`. Then call an encoder.
- **Metadata**: `new Transformer(u8).metadata(withExif?)` → `Promise<{width,height,format,colorType,orientation?,exif?}>`.
- **Enums** (numeric): `ResizeFilterType { Nearest=0, Triangle=1, CatmullRom=2, Gaussian=3, Lanczos3=4 }`, `ResizeFit { Cover=0, Fill=1, Inside=2 }`, `ChromaSubsampling { Yuv444=0, Yuv422=1, Yuv420=2, Yuv400=3 }`, `Orientation { Horizontal=1, MirrorHorizontal=2, Rotate180=3, MirrorVertical=4, MirrorHorizontalAndRotate270Cw=5, Rotate90Cw=6, MirrorHorizontalAndRotate90Cw=7, Rotate270Cw=8 }`, `CompressionType { Default=0, Fast=1, Best=2 }`.
- All encoders return `Buffer` (a `Uint8Array` subclass). Input decode: JPEG/PNG/WebP/AVIF/TIFF/BMP/ICO/TGA/PNM/farbfeld (NOT GIF; SVG only via `fromSvg`).
- Browser-**displayable** output formats (for the after-preview): `webp`, `jpeg`, `png`, `avif`. Others (`bmp/tiff/farbfeld/pnm/ico/tga`) → download-only, no preview.

## Files

```
website/pages/playground/
  protocol.ts          NEW  shared worker request/response types + enum mirrors (no imports of @napi-rs/image)
  worker.ts            REWRITE  typed dispatcher: metadata | convert | compress | transform
  _engine.ts           NEW  promise-based, id-correlated client around the Worker (spawn, transfer, run)
  _snippet.ts          NEW  pure fn: Op -> @napi-rs/image code string (unit-tested)
  _controls.tsx        NEW  Convert/Compress/Transform control panels (presentational, controlled)
  _Playground.tsx      REWRITE  orchestrator: isolation guard, upload, state machine, warnings, run
  _Result.tsx          NEW  before/after compare + size/savings + download + snippet+copy
  index.island.tsx     KEEP  imports _Playground with { island: 'load' } (already correct)
  index.server.ts      KEEP  prerender = false (already correct)
website/pages/_components/_BeforeAfter.tsx   REUSE (P2 island; accepts before/after URLs)
website/e2e/playground-ops.spec.ts           NEW  convert/compress/transform produce valid output
website/scripts/snippet.test.mjs             NEW  unit test for _snippet (run with node)
```

Keep `worker.ts`'s existing `Buffer` polyfill (lines installing `globalThis.Buffer` before any `import('@napi-rs/image')`) and the `isWebp`-style signature checks are no longer needed (replaced by real ops + e2e).

---

### Task 1: Shared protocol types

**Files:** Create `website/pages/playground/protocol.ts`

- [ ] **Step 1: Write the protocol** (no runtime imports — pure types + small const maps so worker, engine, snippet, and UI all agree)

```ts
// website/pages/playground/protocol.ts
// Numeric mirrors of @napi-rs/image enums (kept here so UI/snippet need no wasm import).
export const ResizeFilter = { Nearest: 0, Triangle: 1, CatmullRom: 2, Gaussian: 3, Lanczos3: 4 } as const
export const ResizeFit = { Cover: 0, Fill: 1, Inside: 2 } as const
export const Chroma = { Yuv444: 0, Yuv422: 1, Yuv420: 2, Yuv400: 3 } as const
export const Orientation = {
  Horizontal: 1, Rotate90Cw: 6, Rotate180: 3, Rotate270Cw: 8,
} as const // the four the UI exposes; 'auto' = use embedded EXIF

export type ConvertFormat = 'webp' | 'webpLossless' | 'avif' | 'jpeg' | 'png'
export type CompressCodec = 'jpeg' | 'pngLossless' | 'pngQuantize'

export type ConvertOp = { kind: 'convert'; format: ConvertFormat; quality: number; chroma: number }
export type CompressOp = { kind: 'compress'; codec: CompressCodec; quality: number; maxQuality: number }
export type TransformOp = {
  kind: 'transform'
  resize: { enabled: boolean; width: number; height: number | null; filter: number; fit: number }
  rotate: number | 'auto' | null // Orientation value, 'auto' (EXIF), or null (none)
  grayscale: boolean
  invert: boolean
  blur: number | null
  encode: { format: ConvertFormat; quality: number }
}
export type MetadataOp = { kind: 'metadata' }
export type Op = ConvertOp | CompressOp | TransformOp | MetadataOp

export type ResultMeta = { width: number; height: number; format: string; orientation?: number }

export type WorkerRequest = { id: number; op: Op; bytes: ArrayBuffer }
export type WorkerOk =
  | { id: number; ok: true; kind: 'metadata'; meta: ResultMeta }
  | { id: number; ok: true; kind: 'convert' | 'compress' | 'transform'; bytes: ArrayBuffer; outFormat: string }
export type WorkerErr = { id: number; ok: false; error: string }
export type WorkerResponse = WorkerOk | WorkerErr

// MIME for the output format (for Blob preview + download). null = not browser-displayable.
export const OUTPUT_MIME: Record<string, string | null> = {
  webp: 'image/webp', webpLossless: 'image/webp', avif: 'image/avif',
  jpeg: 'image/jpeg', png: 'image/png',
  bmp: 'image/bmp', tiff: null, farbfeld: null, pnm: null, ico: 'image/x-icon', tga: null,
}
export const DISPLAYABLE = (fmt: string) => Boolean(OUTPUT_MIME[fmt])
```

- [ ] **Step 2: Typecheck** — `cd website && npx tsc --noEmit -p tsconfig.json` → exit 0.
- [ ] **Step 3: Commit** — `git add website/pages/playground/protocol.ts && git commit -m "feat(playground): shared worker op protocol + enum mirrors"`

---

### Task 2: Worker dispatcher (rewrite worker.ts)

**Files:** Modify `website/pages/playground/worker.ts`

- [ ] **Step 1: Rewrite the worker** — keep the Buffer polyfill at top; dispatch the typed protocol. The output `ArrayBuffer` is transferred back.

```ts
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
  // Copy out of the wasm heap into a standalone, transferable ArrayBuffer.
  return out.buffer.slice(out.byteOffset, out.byteOffset + out.byteLength) as ArrayBuffer
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
  switch (op.codec) {
    case 'jpeg': return mod.compressJpeg(u8, { quality: op.quality })
    case 'pngLossless': return mod.losslessCompressPng(u8)
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
```

- [ ] **Step 2: Typecheck** — `npx tsc --noEmit -p tsconfig.json` → exit 0 (fix any enum/type mismatch).
- [ ] **Step 3: Commit** — `git add website/pages/playground/worker.ts && git commit -m "feat(playground): typed worker dispatcher (metadata/convert/compress/transform)"`

---

### Task 3: Code snippet generator (pure, TDD)

**Files:** Create `website/pages/playground/_snippet.ts`, `website/scripts/snippet.test.mjs`

- [ ] **Step 1: Write the failing test** (`website/scripts/snippet.test.mjs`)

```js
import assert from 'node:assert/strict'
import { snippetFor } from '../pages/playground/_snippet.ts'

// convert webp
assert.match(
  snippetFor({ kind: 'convert', format: 'webp', quality: 75, chroma: 0 }),
  /new Transformer\(input\)\.webp\(75\)/,
)
// avif includes chroma when not default
assert.match(
  snippetFor({ kind: 'convert', format: 'avif', quality: 70, chroma: 2 }),
  /\.avif\(\{ quality: 70, chromaSubsampling: ChromaSubsampling\.Yuv420 \}\)/,
)
// compress png quantize
assert.match(
  snippetFor({ kind: 'compress', codec: 'pngQuantize', quality: 75, maxQuality: 80 }),
  /pngQuantize\(input, \{ maxQuality: 80 \}\)/,
)
// transform: rotate auto + resize + grayscale + encode webp
const t = snippetFor({
  kind: 'transform',
  resize: { enabled: true, width: 800, height: null, filter: 4, fit: 0 },
  rotate: 'auto', grayscale: true, invert: false, blur: null,
  encode: { format: 'webp', quality: 75 },
})
assert.match(t, /\.rotate\(\)/)
assert.match(t, /\.resize\(800, null, ResizeFilterType\.Lanczos3\)/)
assert.match(t, /\.grayscale\(\)/)
assert.match(t, /\.webp\(75\)/)
console.log('snippet.test OK')
```

- [ ] **Step 2: Run it, expect failure** — `cd website && npx oxnode scripts/snippet.test.mjs` → fails (module not found / not a function).

- [ ] **Step 3: Implement `_snippet.ts`** (pure; mirrors the worker op exactly)

```ts
// website/pages/playground/_snippet.ts
import type { Op, ConvertFormat } from './protocol'

const FILTER_NAME = ['Nearest', 'Triangle', 'CatmullRom', 'Gaussian', 'Lanczos3']
const CHROMA_NAME = ['Yuv444', 'Yuv422', 'Yuv420', 'Yuv400']
const ORI_NAME: Record<number, string> = { 1: 'Horizontal', 6: 'Rotate90Cw', 3: 'Rotate180', 8: 'Rotate270Cw' }

function encodeCall(format: ConvertFormat, quality: number): string {
  switch (format) {
    case 'webp': return `.webp(${quality})`
    case 'webpLossless': return `.webpLossless()`
    case 'avif': return `.avif({ quality: ${quality} })`
    case 'jpeg': return `.jpeg(${quality})`
    case 'png': return `.png()`
  }
}

export function snippetFor(op: Op): string {
  if (op.kind === 'metadata') return `await new Transformer(input).metadata(true)`
  if (op.kind === 'convert') {
    if (op.format === 'avif')
      return `await new Transformer(input).avif({ quality: ${op.quality}, chromaSubsampling: ChromaSubsampling.${CHROMA_NAME[op.chroma]} })`
    return `await new Transformer(input)${encodeCall(op.format, op.quality)}`
  }
  if (op.kind === 'compress') {
    if (op.codec === 'jpeg') return `await compressJpeg(input, { quality: ${op.quality} })`
    if (op.codec === 'pngLossless') return `await losslessCompressPng(input)`
    return `await pngQuantize(input, { maxQuality: ${op.maxQuality} })`
  }
  // transform
  const parts: string[] = ['new Transformer(input)']
  if (op.rotate === 'auto') parts.push(`.rotate()`)
  else if (typeof op.rotate === 'number') parts.push(`.rotate(Orientation.${ORI_NAME[op.rotate] ?? 'Horizontal'})`)
  if (op.resize.enabled)
    parts.push(`.resize(${op.resize.width}, ${op.resize.height ?? 'null'}, ResizeFilterType.${FILTER_NAME[op.resize.filter]})`)
  if (op.grayscale) parts.push(`.grayscale()`)
  if (op.invert) parts.push(`.invert()`)
  if (op.blur != null) parts.push(`.blur(${op.blur})`)
  parts.push(encodeCall(op.encode.format, op.encode.quality))
  return `await ${parts.join('')}`
}
```

- [ ] **Step 4: Run the test, expect pass** — `npx oxnode scripts/snippet.test.mjs` → `snippet.test OK`.
- [ ] **Step 5: Commit** — `git add website/pages/playground/_snippet.ts website/scripts/snippet.test.mjs && git commit -m "feat(playground): code-snippet generator (TDD)"`

---

### Task 4: Engine client (worker wrapper)

**Files:** Create `website/pages/playground/_engine.ts`

- [ ] **Step 1: Write the engine** — spawns the module worker, correlates responses by `id`, transfers input bytes. The worker URL uses `new URL('./worker.ts', import.meta.url)` with `{ type: 'module' }` (Vite bundles it; proven in P1).

```ts
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
```

- [ ] **Step 2: Typecheck** — `npx tsc --noEmit -p tsconfig.json` → exit 0.
- [ ] **Step 3: Commit** — `git add website/pages/playground/_engine.ts && git commit -m "feat(playground): promise-based worker engine client"`

---

### Task 5: Capability controls

**Files:** Create `website/pages/playground/_controls.tsx`

- [ ] **Step 1: Build three controlled control-panels** — `ConvertControls`, `CompressControls`, `TransformControls`. Each receives its slice of state + an `onChange`. Presentational only (no wasm). Use the protocol types. Dark Tailwind styling consistent with the site (`bg-white/5`, `border-white/10`, `text-(--color-muted)`, accent `--color-accent`).

Requirements per panel (wire to the `Op` shapes in `protocol.ts`):
- **ConvertControls** (`ConvertOp`): format `<select>` (WebP / WebP lossless / AVIF / JPEG / PNG); a quality `<input type="range" min=1 max=100>` shown for webp/avif/jpeg (hidden for webpLossless/png); a chroma `<select>` (Yuv444/422/420) shown only for avif. Use the `Chroma`/`ResizeFilter` consts where needed.
- **CompressControls** (`CompressOp`, given the detected input `format: string`): if input is `jpeg` → codec fixed to `jpeg` + quality range. If input is `png` → codec `<select>` (Lossless / Quantize); quality/maxQuality range for quantize. If input is neither → render a muted note: "Compress-in-place supports JPEG and PNG inputs. Use Convert for other formats." and disable run (signal via an `onValidityChange` or a returned `disabled` boolean prop).
- **TransformControls** (`TransformOp`): a resize toggle + width number + optional height number + filter `<select>` (Lanczos3 default); a rotate `<select>` (None / Auto (EXIF) / 90° / 180° / 270° → maps to `null` / `'auto'` / `6` / `3` / `8`); checkboxes grayscale + invert; a blur range (0 = off → `null`); and an output-format `<select>` + quality range (reuse the convert encode shape).

Keep each panel a small focused component; export all three. No commit-blocking visual perfection — the visual gate (Task 9) refines styling.

- [ ] **Step 2: Typecheck** — exit 0.
- [ ] **Step 3: Commit** — `git add website/pages/playground/_controls.tsx && git commit -m "feat(playground): convert/compress/transform control panels"`

---

### Task 6: Result panel (compare + size + download + snippet)

**Files:** Create `website/pages/playground/_Result.tsx`

- [ ] **Step 1: Build `_Result`** — props: `{ originalUrl: string; originalBytes: number; result: { url: string | null; bytes: number; outFormat: string }; op: Op }`.
  - If `result.url` (displayable output): render the P2 `_BeforeAfter` (`import BeforeAfter from '../_components/_BeforeAfter'`) with `before={originalUrl}` `after={result.url}` `beforeLabel="original"` `afterLabel={result.outFormat}`. If `result.url` is null (non-displayable format like tiff): render a muted "Preview not available for this format — download to view." with just the size.
  - Size row: `originalBytes` → `result.bytes` (format with a `kb()` helper), and a `−{pct}%` savings in accent (compute `Math.round((1 - result.bytes/originalBytes) * 100)`; if negative show `+N%` in muted, since some ops grow the file).
  - Download button: an `<a download>` pointing at `result.url` (when present) or a freshly built blob URL; filename `output.<ext>` where ext derives from `outFormat` (webpLossless → webp).
  - Code snippet: `import { snippetFor } from './_snippet'`; render `snippetFor(op)` inside a dark `<pre>` with a copy button (reuse the pattern from P2 `_CopyButton`, or inline a tiny copy handler — this island already ships JS, so inline is fine).

- [ ] **Step 2: Typecheck** — exit 0.
- [ ] **Step 3: Commit** — `git add website/pages/playground/_Result.tsx && git commit -m "feat(playground): result panel — compare, savings, download, snippet"`

---

### Task 7: Orchestrator (rewrite _Playground.tsx)

**Files:** Modify `website/pages/playground/_Playground.tsx`

- [ ] **Step 1: Rewrite the island** with this control flow:
  1. **Isolation guard (first):** `if (!self.crossOriginIsolated) → render <StaticFallback/>` (Task 8 below covers the fallback; for this task render a simple message + a link to `/#showcase` and the GitHub repo). Gate on a mounted state to avoid SSR/CSR mismatch: render the interactive UI only after `useEffect` confirms `crossOriginIsolated` (SSR renders the shell + a neutral "loading playground…").
  2. **Engine:** create a `PlaygroundEngine` in a `useRef`, lazily on first run (or on mount inside the isolation-confirmed effect); `dispose()` on unmount.
  3. **Upload:** a dropzone + `<input type="file" accept="image/*">` + a "Use sample image" button that fetches `/img/un-optimized.png`. On file chosen: read `ArrayBuffer`, keep the original bytes + an `originalUrl = URL.createObjectURL(file)` (revoke on replace). Immediately run a `metadata` op (pass a COPY of the bytes — see note) to display input `width×height`, `format`, and to drive CompressControls' format-specific UI. Reject undecodable inputs (worker returns `ok:false`) with an inline error.
  4. **Capability tabs:** Convert | Compress | Transform. Each shows its control panel (Task 5). One shared `op` state per tab; the active tab determines which `Op` is built.
  5. **Run:** on "Run", pass a COPY of the original bytes to `engine.run(op, copy)` (the engine TRANSFERS the buffer, so always slice a fresh copy: `original.slice(0)`). On `ok:true` build `result.url` via `new Blob([bytes], { type: OUTPUT_MIME[outFormat] })` → `URL.createObjectURL` (only if `DISPLAYABLE(outFormat)`, else url=null) and render `<_Result/>`. Revoke previous result URLs.
  6. **Status + warnings:** a `data-testid="pg-status"` element with `data-status` of `idle|loading|running|done|error` (the e2e relies on this, mirroring P1). Non-blocking warnings: if `width*height > 4_000_000` or input bytes > 5MB → show "Large image — encoding may be slow or memory-heavy." If a coarse mobile check (`matchMedia('(pointer: coarse)')` or width < 768) → show "Running heavy WASM on mobile may be slow." Both are warnings, not blocks (D4).
  7. Keep `data-testid="pg-bytes"` (output byte count) and `data-testid="pg-error"` for the e2e.

  **Buffer-copy note (critical):** the engine transfers the input `ArrayBuffer` to the worker, which neuters it on the main thread. The UI must keep the pristine original (File/Blob or a retained `ArrayBuffer`) and pass `original.slice(0)` (a fresh copy) to EACH `run()` call, so repeated runs and the metadata pre-flight don't operate on a detached buffer.

- [ ] **Step 2: Typecheck** — exit 0.
- [ ] **Step 3: Commit** — `git add website/pages/playground/_Playground.tsx && git commit -m "feat(playground): interactive orchestrator — upload, tabs, run, warnings"`

---

### Task 8: Static fallback (no cross-origin isolation)

**Files:** Modify `website/pages/playground/_Playground.tsx` (extract `StaticFallback`), optionally a small `_StaticFallback.tsx`

- [ ] **Step 1: Build the fallback** shown when `crossOriginIsolated === false`: a short explanation ("Your browser can't enable the cross-origin isolation this in-browser demo needs (SharedArrayBuffer)."), then reuse the P2 optimization showcase data to render a couple of static before/after comparisons (`import { showcaseRows, pct, kb } from '../_data/showcase'` + the `_BeforeAfter` island) so the page is still valuable, plus a link to install/use the library locally and the docs. Keep it dark/consistent.
- [ ] **Step 2: Typecheck** — exit 0.
- [ ] **Step 3: Commit** — `git add -A website/pages/playground && git commit -m "feat(playground): static showcase fallback when not cross-origin isolated"`

---

### Task 9: e2e — real ops in an isolated browser

**Files:** Create `website/e2e/playground-ops.spec.ts` (keep the existing `playground-smoke.spec.ts` or supersede it — see Step 1)

- [ ] **Step 1: Write the e2e** — drive the real UI in the cross-origin-isolated browser. Reuse the harness from `playground-smoke.spec.ts` (console/pageerror logging; poll `self.crossOriginIsolated === true`). Then: programmatically load the sample image (click "Use sample image"), and for each capability run an op and assert a valid, smaller-or-valid output:

```ts
import { test, expect } from '@playwright/test'

test.describe('playground operations', () => {
  test.beforeEach(async ({ page }) => {
    page.on('console', (m) => console.log(`[browser:${m.type()}] ${m.text()}`))
    page.on('pageerror', (e) => console.log(`[pageerror] ${e.message}`))
    await page.goto('/playground')
    await expect.poll(() => page.evaluate(() => self.crossOriginIsolated), { timeout: 30_000 }).toBe(true)
  })

  test('convert sample to WebP produces a smaller file', async ({ page }) => {
    await page.getByRole('button', { name: /use sample/i }).click()
    await expect(page.getByTestId('pg-status')).toHaveAttribute('data-status', 'idle', { timeout: 60_000 })
    // Convert tab is default; format defaults to webp. Run.
    await page.getByRole('button', { name: /^run$/i }).click()
    await expect(page.getByTestId('pg-status')).toHaveAttribute('data-status', 'done', { timeout: 120_000 })
    const bytes = Number((await page.getByTestId('pg-bytes').innerText()).match(/(\d+)/)?.[1] ?? 0)
    expect(bytes).toBeGreaterThan(0)
  })
})
```
Add at least one Compress and one Transform assertion in the same style (select the tab, run, assert `done` + `pg-bytes > 0`). Use accessible names/test-ids that the Task-5/7 UI actually exposes — keep the UI's button labels/test-ids in sync with this test (adjust either side to match; the test is the contract).

- [ ] **Step 2: Run** — dev server running (or let Playwright boot it): `cd website && npx playwright test e2e/playground-ops.spec.ts --reporter=line`. Expect all green. If a real op fails (e.g. compress on a PNG sample), fix the UI/worker — do NOT weaken the assertion.
- [ ] **Step 3: Commit** — `git add website/e2e/playground-ops.spec.ts && git commit -m "test(playground): e2e convert/compress/transform in isolated browser"`

---

### Task 10: Build + render + visual gate + polish

**Files:** components as needed

- [ ] **Step 1: Production build** — `cd website && npm run build` → exit 0. Confirm the wasm asset + worker `.mjs` still emit in `dist/client` (R1 invariant): `ls dist/client/assets/*.wasm` shows the ~9MB binary. Confirm the wasm is NOT in the SSR HTML (curl `/playground`, grep should not inline base64 wasm).
- [ ] **Step 2: Visual + interaction gate (controller drives a browser)** — load `/playground`, upload the sample, run each of convert/compress/transform, confirm: isolation badge true, before/after compare renders + drags, output size + % savings shown, snippet matches settings + copy works, download works, the large/mobile warnings appear appropriately. Screenshot desktop + mobile. Fix any breakage.
- [ ] **Step 3: a11y + responsive** — controls are labelled (`<label>`/aria), tabs keyboard-navigable, layout holds at 390/768/1280. The slider already has `role="slider"`. Fix issues.
- [ ] **Step 4: Final build + commit** — `npm run build` exit 0; `git add -A website && git commit -m "chore(playground): build/render gate, a11y + responsive polish"`

---

## Self-Review

**Spec coverage (§6):** SSR shell + island:'load' ✓(T7, index.island already) · deferred wasm in worker ✓(T2,T4,T7) · convert/compress/transform ops ✓(T2 worker, T5 controls) · metadata pre-flight ✓(T7) · before/after + size + % savings ✓(T6) · generated code snippet + copy ✓(T3,T6) · crossOriginIsolated guard → static fallback ✓(T7,T8) · D4 attempt-everywhere + warnings ✓(T7) · isolation headers already scoped to /playground (P1) · GA off on /playground (P2 fix + regression test). All present.

**Placeholder scan:** Worker/engine/snippet/protocol are exact code. Control panels + orchestrator + result are precisely specified against the protocol types (exact Op shapes, exact testids, exact enum maps); visual styling is deliberate latitude refined in T10, same as P2. No hand-waving on the contracts.

**Type consistency:** `Op`/`ConvertOp`/`CompressOp`/`TransformOp`/`WorkerRequest`/`WorkerResponse` defined once in `protocol.ts` and consumed identically by worker (T2), engine (T4), snippet (T3), controls (T5), result (T6), orchestrator (T7). `OUTPUT_MIME`/`DISPLAYABLE` shared. Enum mirrors (`ResizeFilter`/`Chroma`/`Orientation`) match the verified native enum values. `pg-status`/`pg-bytes`/`pg-error` testids consistent between T7 UI and T9 e2e.

**Task dependencies:** T1 protocol first (everyone imports it). T2 worker + T3 snippet + T4 engine independent (all depend on T1). T5 controls + T6 result depend on T1 (+ T3 for result). T7 orchestrator integrates T4/T5/T6. T8 fallback extends T7. T9 e2e needs T7. T10 gate last. Each task commits independently; the playground keeps building.

---

## After P3

With the playground live, proceed to **P4 (docs)** — rewritten split docs (`/docs`, `/docs/transformer`, `/docs/compression`, `/docs/credits`) via `@void/md`, reconciled format matrix, resize-filter comparison, credits — then **P5 (polish/launch — incl. the optional benchmark re-run, favicon links on all routes, deployed-isolation verification, cross-browser/mobile pass)**.
