# @napi-rs/image Website Redesign — Phase 2 (Landing Page) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the dark + vivid `@napi-rs/image` landing page on the proven P1 Void foundation — hero, before/after optimization showcase, reconciled format matrix, benchmarks vs sharp, code samples, CTAs — plus site chrome and analytics (path-gated GA + soft-nav page_view).

**Architecture:** Prerendered (`prerender = true`) React page in Void pages mode. Static content + data come from an `index.server.ts` loader; the two interactive bits (copy-to-clipboard, draggable before/after slider) are small client islands. Code samples are SSR-highlighted with Shiki (zero client JS). Real, current asset sizes come from a build-time `showcase-manifest.json` (the old hardcoded numbers were stale). GA is rendered as path-gated JSX in `pages/layout.tsx` (excluded from the COEP-isolated `/playground`).

**Tech Stack:** Void / `@void/react` 0.9.3 · React 19 · Tailwind v4 (P1 tokens) · Shiki (already a transitive dep via `@void/md`) · the P1 build pipeline (`website/scripts/*.mjs`).

**Companion spec:** `docs/superpowers/specs/2026-06-16-image-site-void-redesign-design.md`. **Prep brief:** P2 research workflow `wtenlbteu` (format matrix, showcase assets, benchmarks, GA mechanism — all verified). Branch: `redesign-website-void` (continues from P1).

## Defaults taken on the brief's open questions (change freely)
- Benchmarks: ship the existing **M1 Max / macOS 12.3.1** numbers with the honest hardware caption; re-running on current hardware is a **P5/launch** task.
- GA property: reuse **`G-50ZQKJLY5K`** (the current site's ID), `send_page_view:false` + manual `page_view`.
- Format matrix: **drop the OpenEXR row**; HDR & DDS shown **decode-only**; WebP/AVIF shown **bidirectional**; omit unverified per-color-type output detail strings.
- Showcase: **lead with the AVIF/WebP −90%+ rows**; the −5% `compressJpeg()` lossless row lives only in the full table; heavy 1.2M "before" PNG is **lazy-loaded + CSS-sized** (no separate proxy).
- `/playground` keeps site chrome via path-gating only (no structural layout change).

## Visual-iteration note
For visual components (Hero, Showcase, Gallery, etc.) this plan fixes the **content, data, props, structure, and verification** exactly, and gives representative JSX, but the implementer has latitude on fine Tailwind styling to hit "dark chrome + vivid imagery" — verified by screenshot, not by pixel-exact plan code.

---

## File structure (P2)

```
website/
  scripts/
    build-assets.mjs        + emit public/showcase-manifest.json (real byte sizes) after generating images
    showcase-manifest.mjs   NEW — stats the generated files, writes the manifest
  pages/
    layout.tsx              UPDATE — chrome (header/nav/footer slot) + path-gated GA + page_view hook
    index.tsx               REWRITE — assembles the landing sections
    index.server.ts         UPDATE — loader (showcase data + SSR-highlighted code) + head + prerender
    _components/            NEW — landing components (underscore = not a route)
      Hero.tsx
      HeroCodeSample.tsx
      InstallCommand.tsx          (+ _CopyButton.tsx island)
      OptimizationShowcase.tsx
      _BeforeAfter.tsx            island — draggable compare slider
      FormatMatrix.tsx
      Benchmarks.tsx
      FilterGallery.tsx
      CodeSample.tsx
      CtaBand.tsx
      Footer.tsx
    _data/                 NEW — typed content/data modules
      showcase.ts          the 8 optimization rows + filter-gallery list (reads manifest sizes)
      formats.ts           the reconciled matrix
      benchmarks.ts        the two benchmark tables + caption
      samples.ts           the code-sample source strings
  lib/
    highlight.ts           NEW — Shiki SSR highlighter (build/SSR only)
  types/
    globals.d.ts           NEW — Window.gtag / dataLayer
```

---

### Task 1: Build-time showcase manifest (real sizes, no stale numbers)

**Files:**
- Create: `website/scripts/showcase-manifest.mjs`
- Modify: `website/scripts/build-assets.mjs`
- Test: manual (build + inspect JSON)

- [ ] **Step 1: Write `scripts/showcase-manifest.mjs`**

```js
// website/scripts/showcase-manifest.mjs
import { stat, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

// All paths are relative to public/. Every entry MUST be produced by the demo scripts.
const FILES = [
  'img/un-optimized.png', 'img/un-optimized.jpg',
  'img/optimized-lossless.png', 'img/optimized-lossy.png',
  'img/optimized-lossless.jpg', 'img/optimized-lossy.jpg',
  'img/optimized-lossless.webp', 'img/optimized-lossy-png.webp',
  'img/optimized-lossless-png.avif', 'img/optimized-lossy-png.avif',
]

export async function generateShowcaseManifest() {
  const pub = join(process.cwd(), 'public')
  const sizes = {}
  for (const f of FILES) {
    sizes[f] = (await stat(join(pub, f))).size // throws if a demo image is missing — good, fail the build
  }
  await writeFile(join(pub, 'showcase-manifest.json'), JSON.stringify(sizes, null, 2))
}

if (import.meta.url === `file://${process.argv[1]}`) await generateShowcaseManifest()
```

- [ ] **Step 2: Call it from the orchestrator (after images exist)**

In `website/scripts/build-assets.mjs`, import and call it LAST:
```js
import { generateShowcaseManifest } from './showcase-manifest.mjs'
// inside generateAssets(), after generateChangelog():
await generateShowcaseManifest()
```

- [ ] **Step 3: Verify**

Run: `cd website && node scripts/build-assets.mjs && cat public/showcase-manifest.json`
Expected: JSON with all 10 keys mapping to positive byte counts (e.g. `img/un-optimized.png` ≈ 1220641, `img/optimized-lossy-png.webp` ≈ 84600). If any `stat` throws, the corresponding demo image isn't being produced — fix that before continuing.

- [ ] **Step 4: Commit**

```bash
git add website/scripts/showcase-manifest.mjs website/scripts/build-assets.mjs
git commit -m "feat(website): emit showcase-manifest.json with real asset sizes at build"
```

> `public/showcase-manifest.json` is a build artifact — add it to `website/.gitignore`.

---

### Task 2: Showcase data module

**Files:**
- Create: `website/pages/_data/showcase.ts`

- [ ] **Step 1: Write the typed data + helpers**

```ts
// website/pages/_data/showcase.ts
import manifest from '../../public/showcase-manifest.json' // build emits this; bundled at SSR

export type ShowcaseRow = {
  label: string          // the API call
  kind: 'Lossless' | 'Lossy'
  before: string         // /img/... path
  after: string
  beforeBytes: number
  afterBytes: number
}

const m = manifest as Record<string, number>
const row = (label: string, kind: ShowcaseRow['kind'], before: string, after: string): ShowcaseRow => ({
  label, kind, before: `/${before}`, after: `/${after}`,
  beforeBytes: m[before], afterBytes: m[after],
})

// Ordered: lead with the biggest wins (brief §2). The compressJpeg() lossless (−5%) is LAST.
export const showcaseRows: ShowcaseRow[] = [
  row('new Transformer(PNG).webp(75)', 'Lossy', 'img/un-optimized.png', 'img/optimized-lossy-png.webp'),
  row('new Transformer(PNG).avif({ quality: 75 })', 'Lossy', 'img/un-optimized.png', 'img/optimized-lossy-png.avif'),
  row('pngQuantize({ maxQuality: 75 })', 'Lossy', 'img/un-optimized.png', 'img/optimized-lossy.png'),
  row('new Transformer(PNG).avif({ quality: 100 })', 'Lossless', 'img/un-optimized.png', 'img/optimized-lossless-png.avif'),
  row('new Transformer(PNG).webpLossless()', 'Lossless', 'img/un-optimized.png', 'img/optimized-lossless.webp'),
  row('losslessCompressPng()', 'Lossless', 'img/un-optimized.png', 'img/optimized-lossless.png'),
  row('compressJpeg(JPEG, { quality: 75 })', 'Lossy', 'img/un-optimized.jpg', 'img/optimized-lossy.jpg'),
  row('compressJpeg()', 'Lossless', 'img/un-optimized.jpg', 'img/optimized-lossless.jpg'),
]

export const pct = (r: ShowcaseRow) => Math.round((1 - r.afterBytes / r.beforeBytes) * 100)
export const kb = (n: number) => `${Math.round(n / 1024)} KB`

// Filter gallery (brief §2): all served from /img, source un-optimized.png unless noted.
export const filterDemos: { label: string; src: string }[] = [
  { label: 'grayscale', src: '/img/grayscale.manipulated.webp' },
  { label: 'invert', src: '/img/invert.manipulated.webp' },
  { label: 'blur', src: '/img/blur.manipulated.webp' },
  { label: 'huerotate', src: '/img/huerotate.manipulated.webp' },
  { label: 'contrast', src: '/img/contrast.manipulated.webp' },
  { label: 'brighten', src: '/img/brighten.manipulated.webp' },
  { label: 'crop', src: '/img/crop.manipulated.webp' },
]
```

- [ ] **Step 2: Verify the JSON import type-checks**

Run: `cd website && npx tsc --noEmit -p tsconfig.json`
Expected: no error on the `manifest` import. If tsc complains about importing JSON, add `"resolveJsonModule": true` to `tsconfig.json` `compilerOptions` and re-run.

- [ ] **Step 3: Commit**

```bash
git add website/pages/_data/showcase.ts website/tsconfig.json
git commit -m "feat(website): showcase data module backed by the build manifest"
```

---

### Task 3: Format matrix data + component

**Files:**
- Create: `website/pages/_data/formats.ts`, `website/pages/_components/FormatMatrix.tsx`

- [ ] **Step 1: Write the reconciled matrix data (brief §1)**

```ts
// website/pages/_data/formats.ts
export type Support = 'yes' | 'no'
export type FormatRow = { format: string; decode: Support; encode: Support; note?: string }

// Verified against packages/binding/index.d.ts + src/transformer.rs + Cargo.toml (brief §1).
export const formatRows: FormatRow[] = [
  { format: 'JPEG', decode: 'yes', encode: 'yes' },
  { format: 'PNG', decode: 'yes', encode: 'yes' },
  { format: 'WebP', decode: 'yes', encode: 'yes' },
  { format: 'AVIF', decode: 'yes', encode: 'yes' },
  { format: 'TIFF', decode: 'yes', encode: 'yes' },
  { format: 'BMP', decode: 'yes', encode: 'yes' },
  { format: 'ICO', decode: 'yes', encode: 'yes' },
  { format: 'TGA', decode: 'yes', encode: 'yes' },
  { format: 'PNM', decode: 'yes', encode: 'yes' },
  { format: 'farbfeld', decode: 'yes', encode: 'yes' },
  { format: 'RawPixels (RGBA8)', decode: 'yes', encode: 'yes' },
  { format: 'SVG', decode: 'yes', encode: 'no', note: 'input only' },
  { format: 'DDS (DXT1/3/5)', decode: 'yes', encode: 'no', note: 'decode only' },
  { format: 'HDR (Radiance)', decode: 'yes', encode: 'no', note: 'decode only' },
]

export const matrixCaption =
  'WebP and AVIF are fully bidirectional — decode and encode. HDR and DDS are decode-only.'
```

- [ ] **Step 2: Write `FormatMatrix.tsx`** (static, dark, the bidirectional fact is the headline)

```tsx
// website/pages/_components/FormatMatrix.tsx
import { formatRows, matrixCaption } from '../_data/formats'
const Cell = ({ s, note }: { s: 'yes' | 'no'; note?: string }) =>
  s === 'yes'
    ? <span className="text-(--color-accent)">✓{note ? ` (${note})` : ''}</span>
    : <span className="text-(--color-muted)">—{note ? ` (${note})` : ''}</span>
export default function FormatMatrix() {
  return (
    <section className="mx-auto max-w-4xl px-6 py-20">
      <h2 className="text-3xl font-bold">Formats</h2>
      <p className="mt-2 text-(--color-muted)">{matrixCaption}</p>
      <table className="mt-8 w-full border-collapse text-sm">
        <thead>
          <tr className="border-b border-white/10 text-left text-(--color-muted)">
            <th className="py-2">Format</th><th className="py-2">Decode</th><th className="py-2">Encode</th>
          </tr>
        </thead>
        <tbody className="font-mono">
          {formatRows.map((r) => (
            <tr key={r.format} className="border-b border-white/5">
              <td className="py-2">{r.format}</td>
              <td className="py-2"><Cell s={r.decode} /></td>
              <td className="py-2"><Cell s={r.encode} note={r.encode === 'no' ? r.note : undefined} /></td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  )
}
```

- [ ] **Step 3: Commit** (rendered/verified in Task 9)

```bash
git add website/pages/_data/formats.ts website/pages/_components/FormatMatrix.tsx
git commit -m "feat(website): reconciled format support matrix"
```

---

### Task 4: Benchmarks data + component

**Files:**
- Create: `website/pages/_data/benchmarks.ts`, `website/pages/_components/Benchmarks.tsx`

- [ ] **Step 1: Write the data (brief §3, verbatim numbers + caption)**

```ts
// website/pages/_data/benchmarks.ts
export type Bench = { suite: string; napi: number; sharp: number } // ops/s
export const benchDefault: Bench[] = [
  { suite: 'WebP', napi: 202, sharp: 169 },
  { suite: 'AVIF', napi: 26, sharp: 24 },
]
export const benchThreadpool: Bench[] = [
  { suite: 'WebP', napi: 431, sharp: 238 },
  { suite: 'AVIF', napi: 36, sharp: 32 },
]
export const benchCaption =
  'Apple M1 Max · macOS 12.3.1 · node bench/bench.mjs. Pipeline: rotate → resize(225) → encode.'
```

- [ ] **Step 2: Write `Benchmarks.tsx`** — two labelled groups, each suite a horizontal bar pair (napi accent, sharp muted), values labelled, plus the caption. Scale bar widths to the max value in the group. Keep it CSS-only (no chart lib).

```tsx
// website/pages/_components/Benchmarks.tsx
import { benchDefault, benchThreadpool, benchCaption, type Bench } from '../_data/benchmarks'
function Bars({ title, data }: { title: string; data: Bench[] }) {
  const max = Math.max(...data.flatMap((d) => [d.napi, d.sharp]))
  return (
    <div className="rounded-lg border border-white/10 p-6">
      <h3 className="font-mono text-sm text-(--color-muted)">{title}</h3>
      <div className="mt-4 space-y-4">
        {data.map((d) => (
          <div key={d.suite}>
            <div className="mb-1 text-sm">{d.suite}</div>
            <div className="flex items-center gap-2">
              <div className="h-3 rounded bg-(--color-accent)" style={{ width: `${(d.napi / max) * 100}%` }} />
              <span className="text-xs">@napi-rs/image {d.napi}</span>
            </div>
            <div className="mt-1 flex items-center gap-2">
              <div className="h-3 rounded bg-white/20" style={{ width: `${(d.sharp / max) * 100}%` }} />
              <span className="text-xs text-(--color-muted)">sharp {d.sharp}</span>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
export default function Benchmarks() {
  return (
    <section className="mx-auto max-w-4xl px-6 py-20">
      <h2 className="text-3xl font-bold">Faster than sharp</h2>
      <div className="mt-8 grid gap-6 md:grid-cols-2">
        <Bars title="default" data={benchDefault} />
        <Bars title="UV_THREADPOOL_SIZE=10" data={benchThreadpool} />
      </div>
      <p className="mt-4 text-xs text-(--color-muted)">{benchCaption}</p>
    </section>
  )
}
```

- [ ] **Step 3: Commit**

```bash
git add website/pages/_data/benchmarks.ts website/pages/_components/Benchmarks.tsx
git commit -m "feat(website): benchmarks-vs-sharp section"
```

---

### Task 5: Shiki SSR highlighter + code-sample components

**Files:**
- Create: `website/lib/highlight.ts`, `website/pages/_data/samples.ts`, `website/pages/_components/HeroCodeSample.tsx`, `website/pages/_components/CodeSample.tsx`

- [ ] **Step 1: Write the code-sample sources (brief §3, verified-compiling)**

```ts
// website/pages/_data/samples.ts
export const heroSample = `import { Transformer, ChromaSubsampling } from '@napi-rs/image'

const webp = await new Transformer(input).rotate().resize(225).webp(75)
const avif = await new Transformer(input).rotate().resize(225)
  .avif({ quality: 70, chromaSubsampling: ChromaSubsampling.Yuv420 })`

export const fullSample = `import { readFileSync, writeFileSync } from 'node:fs'
import { Transformer, losslessCompressPng, ResizeFilterType, ChromaSubsampling } from '@napi-rs/image'

const PNG = readFileSync('./input.png')

writeFileSync('out.png', await losslessCompressPng(PNG))
writeFileSync('out.webp', await new Transformer(PNG).resize(800, null, ResizeFilterType.Lanczos3).webp(75))
writeFileSync('out.avif', await new Transformer(PNG).resize(800, null, ResizeFilterType.Lanczos3)
  .avif({ quality: 75, chromaSubsampling: ChromaSubsampling.Yuv420 }))`
```

- [ ] **Step 2: Write the Shiki highlighter (SSR/build only)**

```ts
// website/lib/highlight.ts
import { codeToHtml } from 'shiki'
// Shiki ships with @void/md (transitive). If `import 'shiki'` fails to resolve, add shiki to
// website/package.json devDependencies at the version @void/md depends on (check its package.json).
export function highlight(code: string, lang = 'ts') {
  return codeToHtml(code, { lang, theme: 'github-dark' })
}
```

- [ ] **Step 3: Write the components** (they receive pre-highlighted HTML as a prop from the loader — zero client JS)

```tsx
// website/pages/_components/HeroCodeSample.tsx
export default function HeroCodeSample({ html }: { html: string }) {
  return <div className="overflow-x-auto rounded-lg border border-white/10 p-4 text-sm [&_pre]:bg-transparent!"
    dangerouslySetInnerHTML={{ __html: html }} />
}
```
```tsx
// website/pages/_components/CodeSample.tsx
export default function CodeSample({ html }: { html: string }) {
  return (
    <section className="mx-auto max-w-3xl px-6 py-20">
      <h2 className="text-3xl font-bold">Three formats, one pipeline</h2>
      <div className="mt-8 overflow-x-auto rounded-lg border border-white/10 p-4 text-sm [&_pre]:bg-transparent!"
        dangerouslySetInnerHTML={{ __html: html }} />
    </section>
  )
}
```

- [ ] **Step 4: Verify Shiki resolves**

Run: `cd website && node -e "import('shiki').then(m=>m.codeToHtml('const a=1','{lang:\'ts\',theme:\'github-dark\'}'.length?{lang:'ts',theme:'github-dark'}:{})).then(h=>console.log(h.slice(0,40)))"`
Expected: prints an HTML `<pre ...` fragment. If `shiki` doesn't resolve, add it to devDependencies (matching `@void/md`'s version) and `yarn install`.

- [ ] **Step 5: Commit**

```bash
git add website/lib/highlight.ts website/pages/_data/samples.ts website/pages/_components/HeroCodeSample.tsx website/pages/_components/CodeSample.tsx
git commit -m "feat(website): Shiki SSR code highlighting + sample components"
```

---

### Task 6: Hero + Install (with copy island) + badges

**Files:**
- Create: `website/pages/_components/Hero.tsx`, `website/pages/_components/InstallCommand.tsx`, `website/pages/_components/_CopyButton.tsx`

- [ ] **Step 1: Write the copy-button island** (client interactivity)

```tsx
// website/pages/_components/_CopyButton.tsx
import { useState } from 'react'
export default function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)
  return (
    <button
      onClick={() => { navigator.clipboard.writeText(text).then(() => { setCopied(true); setTimeout(() => setCopied(false), 1500) }) }}
      className="rounded border border-white/15 px-2 py-1 text-xs text-(--color-muted) hover:text-(--color-fg)"
      aria-label="Copy to clipboard"
    >{copied ? 'Copied' : 'Copy'}</button>
  )
}
```

- [ ] **Step 2: Write `InstallCommand.tsx`** (imports the island with `{ island: 'visible' }`)

```tsx
// website/pages/_components/InstallCommand.tsx
import CopyButton from './_CopyButton' with { island: 'visible' }
const CMD = 'npm install @napi-rs/image'
export default function InstallCommand() {
  return (
    <div className="mx-auto mt-8 flex max-w-md items-center justify-between gap-3 rounded-lg border border-white/10 bg-white/5 px-4 py-3 font-mono text-sm">
      <code>{CMD}</code>
      <CopyButton text={CMD} />
    </div>
  )
}
```

- [ ] **Step 3: Write `Hero.tsx`** — big headline ("Fast image processing, in Rust"), one-line subhead, the install command, two CTAs (`/playground` accent-filled, `/docs` outline), badges row (npm/install-size/downloads from brief §3), and the hero code sample slot (passed as a prop `codeHtml`). Use the accent oklch for the primary CTA + a subtle radial glow. Representative structure:

```tsx
// website/pages/_components/Hero.tsx
import InstallCommand from './InstallCommand'
import HeroCodeSample from './HeroCodeSample'
const badges = [
  { src: 'https://img.shields.io/npm/v/@napi-rs/image.svg', href: 'https://www.npmjs.com/package/@napi-rs/image', alt: 'npm version' },
  { src: 'https://packagephobia.com/badge?p=@napi-rs/image', href: 'https://packagephobia.com/result?p=@napi-rs/image', alt: 'install size' },
  { src: 'https://img.shields.io/npm/dm/@napi-rs/image.svg', href: 'https://npmcharts.com/compare/@napi-rs/image?minimal=true', alt: 'downloads' },
]
export default function Hero({ codeHtml }: { codeHtml: string }) {
  return (
    <section className="relative mx-auto max-w-5xl px-6 pt-24 pb-16 text-center">
      <h1 className="text-5xl font-bold tracking-tight md:text-6xl">
        Fast image processing, <span className="text-(--color-accent)">in Rust</span>
      </h1>
      <p className="mx-auto mt-5 max-w-2xl text-lg text-(--color-muted)">
        Encode, compress, resize and convert images — JPEG, PNG, WebP, AVIF and more — with a native Node addon that beats sharp.
      </p>
      <div className="mt-8 flex items-center justify-center gap-4">
        <a href="/playground" className="rounded-lg bg-(--color-accent) px-5 py-2.5 font-medium text-black">Try the playground</a>
        <a href="/docs" className="rounded-lg border border-white/15 px-5 py-2.5 font-medium">Read the docs</a>
      </div>
      <div className="mt-6 flex items-center justify-center gap-3">
        {badges.map((b) => <a key={b.alt} href={b.href}><img src={b.src} alt={b.alt} loading="lazy" /></a>)}
      </div>
      <InstallCommand />
      <div className="mx-auto mt-12 max-w-2xl text-left"><HeroCodeSample html={codeHtml} /></div>
    </section>
  )
}
```

- [ ] **Step 4: Commit**

```bash
git add website/pages/_components/Hero.tsx website/pages/_components/InstallCommand.tsx website/pages/_components/_CopyButton.tsx
git commit -m "feat(website): hero, install command (copy island), badges"
```

---

### Task 7: Before/after slider island + Optimization showcase + Filter gallery

**Files:**
- Create: `website/pages/_components/_BeforeAfter.tsx`, `website/pages/_components/OptimizationShowcase.tsx`, `website/pages/_components/FilterGallery.tsx`

- [ ] **Step 1: Write the `_BeforeAfter` slider island** — a draggable clip-reveal compare of two same-dimension images. Props: `before`, `after`, `beforeLabel`, `afterLabel`. Pointer-driven; keyboard-accessible (`role="slider"`, arrow keys). Heavy images `loading="lazy"`.

```tsx
// website/pages/_components/_BeforeAfter.tsx
import { useRef, useState } from 'react'
export default function BeforeAfter({ before, after, beforeLabel, afterLabel }:
  { before: string; after: string; beforeLabel?: string; afterLabel?: string }) {
  const [pos, setPos] = useState(50)
  const ref = useRef<HTMLDivElement>(null)
  const move = (clientX: number) => {
    const el = ref.current; if (!el) return
    const r = el.getBoundingClientRect()
    setPos(Math.min(100, Math.max(0, ((clientX - r.left) / r.width) * 100)))
  }
  return (
    <div ref={ref} className="relative aspect-[3/2] w-full select-none overflow-hidden rounded-lg border border-white/10"
      onPointerMove={(e) => e.buttons === 1 && move(e.clientX)} onPointerDown={(e) => move(e.clientX)}>
      <img src={after} alt={afterLabel ?? 'after'} className="absolute inset-0 h-full w-full object-cover" loading="lazy" />
      <div className="absolute inset-0 overflow-hidden" style={{ width: `${pos}%` }}>
        <img src={before} alt={beforeLabel ?? 'before'} className="absolute inset-0 h-full w-full max-w-none object-cover"
          style={{ width: ref.current?.clientWidth }} loading="lazy" />
      </div>
      <div role="slider" aria-valuenow={Math.round(pos)} aria-valuemin={0} aria-valuemax={100} tabIndex={0}
        onKeyDown={(e) => { if (e.key === 'ArrowLeft') setPos((p) => Math.max(0, p - 2)); if (e.key === 'ArrowRight') setPos((p) => Math.min(100, p + 2)) }}
        className="absolute top-0 bottom-0 w-0.5 cursor-ew-resize bg-(--color-accent)" style={{ left: `${pos}%` }} />
    </div>
  )
}
```

- [ ] **Step 2: Write `OptimizationShowcase.tsx`** — the vivid centerpiece. A featured `_BeforeAfter` for the top row (webp −93%), then a responsive grid of the remaining rows each showing the API label, the before/after sizes, the `%` reduction (accent), and a `_BeforeAfter`. Import the island once: `import BeforeAfter from './_BeforeAfter' with { island: 'visible' }`. Use `showcaseRows`, `pct`, `kb` from `_data/showcase`.

- [ ] **Step 3: Write `FilterGallery.tsx`** — a simple responsive grid over `filterDemos`, each a `loading="lazy"` `<img>` captioned with the filter name (monospace). Static, no island.

- [ ] **Step 4: Commit**

```bash
git add website/pages/_components/_BeforeAfter.tsx website/pages/_components/OptimizationShowcase.tsx website/pages/_components/FilterGallery.tsx
git commit -m "feat(website): before/after compare island, optimization showcase, filter gallery"
```

---

### Task 8: Layout chrome + path-gated GA + soft-nav page_view

**Files:**
- Modify: `website/pages/layout.tsx`
- Create: `website/types/globals.d.ts`, `website/pages/_components/CtaBand.tsx`, `website/pages/_components/Footer.tsx`

- [ ] **Step 1: Window typings**

```ts
// website/types/globals.d.ts
declare global { interface Window { dataLayer: unknown[]; gtag: (...args: unknown[]) => void } }
export {}
```
Ensure `types/` is in `tsconfig.json` `include` (add it if missing).

- [ ] **Step 2: Update `pages/layout.tsx`** — add the header/nav + footer slot, and the GA mechanism from brief §4 (verified). VERIFY `useRouter` is exported from `@void/react` 0.9.3 and returns `{ path, ... }` (read node_modules/@void/react if unsure); adjust if the accessor differs.

```tsx
// website/pages/layout.tsx
import { useEffect, type ReactNode } from 'react'
import { useRouter } from '@void/react'
import '../app.css'
const GA_ID = 'G-50ZQKJLY5K'
export default function Layout({ children }: { children: ReactNode }) {
  const router = useRouter()
  const path = router.path
  const analyticsEnabled = !path.startsWith('/playground')
  useEffect(() => {
    if (!analyticsEnabled || typeof window === 'undefined' || typeof window.gtag !== 'function') return
    window.gtag('event', 'page_view', {
      page_path: window.location.pathname + window.location.search,
      page_location: window.location.href, page_title: document.title,
    })
  }, [path, analyticsEnabled])
  return (
    <div className="min-h-screen">
      {analyticsEnabled && (
        <>
          <script async src={`https://www.googletagmanager.com/gtag/js?id=${GA_ID}`} />
          <script dangerouslySetInnerHTML={{ __html:
            `window.dataLayer=window.dataLayer||[];function gtag(){dataLayer.push(arguments);}gtag('js',new Date());gtag('config','${GA_ID}',{send_page_view:false});` }} />
        </>
      )}
      <header className="flex items-center justify-between border-b border-white/10 px-6 py-4">
        <a href="/" className="font-mono font-bold">@napi-rs/image</a>
        <nav className="flex gap-4 text-sm text-(--color-muted)">
          <a href="/playground" className="hover:text-(--color-fg)">Playground</a>
          <a href="/docs" className="hover:text-(--color-fg)">Docs</a>
          <a href="/changelog" className="hover:text-(--color-fg)">Changelog</a>
          <a href="https://github.com/Brooooooklyn/Image" className="hover:text-(--color-fg)">GitHub</a>
        </nav>
      </header>
      <main>{children}</main>
    </div>
  )
}
```

> If `useRouter()` during SSR throws or `path` is undefined on a non-island page, guard with `router?.path ?? ''`. The `/playground` island layout is static-only and never hydrates, so the `useEffect` never runs there — but the `analyticsEnabled` gate also keeps the gtag `<script>` out of its SSR HTML.

- [ ] **Step 3: Write `CtaBand.tsx` and `Footer.tsx`** — CtaBand: accent-forward band with "Try it in your browser" → `/playground` and "Read the docs" → `/docs`. Footer: repo/npm/discord links + the badges, restrained dark, `border-t border-white/10`.

- [ ] **Step 4: Commit**

```bash
git add website/pages/layout.tsx website/types/globals.d.ts website/pages/_components/CtaBand.tsx website/pages/_components/Footer.tsx website/tsconfig.json
git commit -m "feat(website): site chrome + path-gated GA + soft-nav page_view"
```

---

### Task 9: Assemble the landing page + loader + render gate

**Files:**
- Modify: `website/pages/index.tsx`, `website/pages/index.server.ts`

- [ ] **Step 1: Loader provides data + SSR-highlighted code**

```ts
// website/pages/index.server.ts
import { defineHead } from 'void'
import { highlight } from '../lib/highlight'
import { heroSample, fullSample } from './_data/samples'
export const prerender = true
export async function loader() {
  return { heroHtml: await highlight(heroSample), fullHtml: await highlight(fullSample) }
}
export const head = defineHead(() => ({
  title: 'Fast image processing in Rust',
  meta: [
    { name: 'description', content: 'Encode, compress, resize and convert JPEG/PNG/WebP/AVIF with a native Node addon faster than sharp.' },
    { property: 'og:image', content: '/img/og.png' },
    { property: 'og:title', content: '@napi-rs/image' },
    { name: 'twitter:card', content: 'summary_large_image' },
  ],
}))
```
> VERIFY the loader→props wiring for `@void/react` 0.9.3 (how `loader`'s return reaches the page component — props arg, a `usePage()`/`useLoaderData()` hook, etc.) by reading the docs/`node_modules`. Adjust Step 2's prop access to match.

- [ ] **Step 2: Assemble `index.tsx`** in section order (brief §5), passing `heroHtml`/`fullHtml` down:

```tsx
// website/pages/index.tsx
import Hero from './_components/Hero'
import OptimizationShowcase from './_components/OptimizationShowcase'
import FormatMatrix from './_components/FormatMatrix'
import Benchmarks from './_components/Benchmarks'
import FilterGallery from './_components/FilterGallery'
import CodeSample from './_components/CodeSample'
import CtaBand from './_components/CtaBand'
import Footer from './_components/Footer'
export default function Home({ heroHtml, fullHtml }: { heroHtml: string; fullHtml: string }) {
  return (
    <>
      <Hero codeHtml={heroHtml} />
      <OptimizationShowcase />
      <Benchmarks />
      <FormatMatrix />
      <FilterGallery />
      <CodeSample html={fullHtml} />
      <CtaBand />
      <Footer />
    </>
  )
}
```

- [ ] **Step 3: Build + SSR render gate**

Run: `cd website && npm run build && npm run dev` then `curl -s http://localhost:5173/ | grep -oE "Fast image processing|Faster than sharp|Formats"`
Expected: all three strings appear in SSR HTML (hero + benchmarks + matrix render server-side). Confirm `npm run build` exits 0 and the page prerendered. Stop dev.

- [ ] **Step 4: Visual + image-serving check (screenshot)**

Use the run/verify tooling (or Playwright) to load `/` and screenshot at desktop + mobile widths. Confirm: dark theme, the before/after showcase images actually load (not 404 — this exercises the symlinked `un-optimized.png`), the slider drags, the install Copy button works, badges render, nav works. Fix any broken image paths or layout issues. Capture a screenshot as evidence.

- [ ] **Step 5: Commit**

```bash
git add website/pages/index.tsx website/pages/index.server.ts
git commit -m "feat(website): assemble landing page with loader-provided data + SSR code"
```

---

### Task 10: Polish — exclude heavy asset, lazy-load, a11y, responsive

**Files:**
- Modify: `website/.gitignore` (already ignores artifacts), `website/scripts/generate-img.mjs` or a copy step, components as needed

- [ ] **Step 1: Keep the 13MB nasa source out of the deploy.** Confirm whether `public/img/nasa-4928x3279.png` (a 13MB symlink) is copied into `dist/client`. Run `cd website && npm run build && ls -la dist/client/img/nasa-4928x3279.png 2>/dev/null`. If present, exclude it: it's only an input for the resize benchmark, not a shipped asset — move the symlink out of `public/img` (the demo scripts read it from the repo root, not from public), or add a Vite build step to delete it from `dist/client/img`. Re-build and confirm it's gone from `dist/client`.

- [ ] **Step 2: Verify all heavy images are `loading="lazy"`** (showcase, gallery, badges) and the hero is above-the-fold only. Grep the components for `<img` and confirm.

- [ ] **Step 3: A11y + responsive pass.** Headings in order (one `h1` in Hero, `h2` per section), the slider is keyboard-operable, color contrast of `--color-muted` on `--color-bg` is adequate, and the layout holds at 360px / 768px / 1280px. Fix issues. Re-screenshot mobile + desktop.

- [ ] **Step 4: Full build + commit**

Run: `cd website && npm run build` → exit 0, `dist/client` has no 13MB nasa png.
```bash
git add -A website
git commit -m "chore(website): landing polish — drop heavy asset, lazy-load, a11y/responsive"
```

---

## Self-Review

**Spec coverage (P2 portion of §12):** hero ✓(T6) · format-support matrix reconciled ✓(T3) · benchmarks ✓(T4) · before/after showcases ✓(T7) · code samples ✓(T5,T6,T9) · CTAs → playground/docs ✓(T6,T8) · OG ✓(T9 head) · GA + page_view hook ✓(T8). Build-manifest for accurate sizes ✓(T1,T2). All present.

**Placeholder scan:** Content is concrete (real numbers, real matrix, verified code samples from the brief). The only deliberate latitude is fine Tailwind styling on visual components (per the Visual-iteration note) + three flagged VERIFY points against @void/react 0.9.3 (loader→props wiring T9, `useRouter` shape T8, Shiki resolution T5) — these are real API-shape checks, surfaced as steps, not hand-waving.

**Type consistency:** `ShowcaseRow`/`pct`/`kb` used consistently (T2→T7). `BeforeAfter` props `{before,after,beforeLabel?,afterLabel?}` match between T7 island and its consumers. `highlight()` returns string → consumed as `html`/`codeHtml`/`heroHtml`/`fullHtml` props (T5,T6,T9). `FormatRow`/`Bench` data shapes match their components.

**Dependencies between tasks:** T1→T2 (manifest before data module). T5 highlighter before T9 loader. T8 layout independent. T9 assembles T3–T8. T10 polish last. Each task commits independently; the site keeps building.

---

## After P2

With landing live, proceed to **P3 (full playground UI)** — it builds directly on the proven P1 worker (Task 9 of P1) and the `_BeforeAfter` island from P2 Task 7 (reuse for the playground's before/after compare). Then **P4 (docs)** and **P5 (polish/launch — incl. the optional benchmark re-run)**.
