# @napi-rs/image Website Redesign — Phase 1 (Foundation) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up a deployable Void (Vite + Cloudflare) app in `website/`, replacing the Next.js/Nextra setup, with the dark design system, the ported build-time asset pipeline, and a proven WASM playground smoke-test (the R1 gate) — so P2–P5 build on verified ground.

**Architecture:** Void pages-mode app, React 19, `output: "server"` + ISR. Native `@napi-rs/image`/`@napi-rs/canvas` run ONLY in build scripts (`scripts/`), never in Worker code. The playground runs the published WASM build client-side in a Web Worker, behind `/playground`-scoped COOP/COEP headers. Tailwind v4 (CSS config) for chrome; `@void/md` for docs prose (wired in P4).

**Tech Stack:** `void`/`@void/react`/`@void/md` `0.9.3` (lockstep) · `vite@^8` · `react@19` · `tailwindcss`/`@tailwindcss/vite` `4.3.1` · `@napi-rs/image`/`@napi-rs/image-wasm32-wasi` `1.12.0` · `@napi-rs/canvas` (build-time) · `@playwright/test` (R1 gate).

**Companion spec:** `docs/superpowers/specs/2026-06-16-image-site-void-redesign-design.md`. Branch: `redesign-website-void`.

---

## File structure (P1)

```
website/
  package.json            rewritten: Void/React/Tailwind/WASM deps + scripts
  tsconfig.json           module:ESNext (needed for `with { island }`)
  vite.config.ts          [voidPlugin(), voidReact(), voidMarkdown(), tailwindcss(), genAssets()]
  void.json               output:server · revalidate · headers(/playground) · redirects · head
  app.css                 tailwind + dark tokens + @void/md theme-content + --vmd vars
  pages/
    layout.tsx            shared root layout (non-island): <main className="void-md"> wrapper hook later
    index.tsx             minimal landing placeholder (P2 fleshes out)
    index.server.ts       head() + prerender
    playground/
      index.island.tsx    imports _Playground with { island: 'load' }  (R1 smoke-test page)
      _Playground.tsx      useEffect → spawn worker → encode sample → show byte length
      worker.ts           Web Worker: import('@napi-rs/image'), encode webp, postMessage
      index.server.ts     head() only — NO prerender
  scripts/
    build-assets.mjs      orchestrates the three below, idempotent, used by buildStart + CI
    generate-img.mjs      ported from website/generate-img.js (paths fixed, Vercel branch dropped)
    og-image.mjs          ported from website/og-image.js (global fetch)
    changelog.mjs         ported from website/changelog.js → writes pages/changelog/index.md, plain anchors
  e2e/
    playground-smoke.spec.ts   R1 gate: crossOriginIsolated + worker encodes sample > 0 bytes
  playwright.config.ts
  public/img/             existing assets (kept); generated demo assets land here at build
  REMOVED: next.config.js, mdx-components.js, app/, content/, style.css, generate-img.js,
           og-image.js, changelog.js   (originals preserved in git history on main)
```

---

### Task 1: Clear the Next.js/Nextra app from `website/`, keep assets + scripts-to-port

**Files:**
- Delete: `website/next.config.js`, `website/mdx-components.js`, `website/style.css`, `website/app/` (dir), `website/content/` (dir)
- Keep: `website/public/`, `website/generate-img.js`, `website/og-image.js`, `website/changelog.js` (ported in Task 8), `website/package.json` (rewritten in Task 2)

- [ ] **Step 1: Confirm originals are safe in git, then remove Next/Nextra files**

Run:
```bash
cd /Users/brooklyn/workspace/github/Image/website
git ls-files app content next.config.js mdx-components.js style.css | head   # confirm tracked on branch (came from main)
rm -rf app content next.config.js mdx-components.js style.css
```
Expected: the `git ls-files` lists the files (so history preserves them); `rm` removes them from the working tree.

- [ ] **Step 2: Verify what remains**

Run: `ls -1 /Users/brooklyn/workspace/github/Image/website`
Expected: includes `public`, `generate-img.js`, `og-image.js`, `changelog.js`, `package.json`, `.gitignore` — and NO `app`, `content`, `next.config.js`, `mdx-components.js`, `style.css`.

- [ ] **Step 3: Commit**

```bash
cd /Users/brooklyn/workspace/github/Image
git add -A website
git commit -m "chore(website): remove Next.js/Nextra app, keep assets + build scripts for Void migration"
```

---

### Task 2: Rewrite `website/package.json` with Void/React/Tailwind/WASM deps + scripts

**Files:**
- Modify: `website/package.json`

- [ ] **Step 1: Write the new package.json**

```json
{
  "name": "website",
  "private": true,
  "version": "0.0.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview",
    "void:prepare": "void prepare",
    "deploy": "void deploy",
    "test:e2e": "playwright test"
  },
  "dependencies": {
    "@napi-rs/image": "1.12.0",
    "@napi-rs/image-wasm32-wasi": "1.12.0",
    "@void/md": "0.9.3",
    "@void/react": "0.9.3",
    "react": "^19.2.1",
    "react-dom": "^19.2.1",
    "void": "0.9.3"
  },
  "devDependencies": {
    "@napi-rs/canvas": "^1.0.0",
    "@playwright/test": "^1.50.0",
    "@tailwindcss/vite": "4.3.1",
    "@types/node": "^24.10.1",
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "chalk": "^5.6.2",
    "tailwindcss": "4.3.1",
    "typescript": "^6.0.0",
    "vite": "^8.0.0"
  }
}
```

> Note: `node-fetch` is intentionally dropped — Node 24 has global `fetch` (used by the ported scripts in Task 8). `@napi-rs/image-wasm32-wasi` is an EXPLICIT dependency (its `cpu:['wasm32']` means npm may skip it as a transitive dep).

- [ ] **Step 2: Install from the repo root (yarn workspaces)**

Run:
```bash
cd /Users/brooklyn/workspace/github/Image
yarn install
```
Expected: install completes; `ls website/node_modules/void website/node_modules/@void/react website/node_modules/@napi-rs/image-wasm32-wasi` (or root `node_modules`) all resolve. Run `node -e "console.log(require.resolve('@napi-rs/image-wasm32-wasi/package.json'))"` from `website/` → prints a path (proves the wasm pkg installed despite `cpu` constraint).

- [ ] **Step 3: Commit**

```bash
cd /Users/brooklyn/workspace/github/Image
git add website/package.json yarn.lock
git commit -m "chore(website): Void + React + Tailwind + WASM dependencies"
```

---

### Task 3: Add `website/tsconfig.json` (ESNext modules for import attributes)

**Files:**
- Create: `website/tsconfig.json`

- [ ] **Step 1: Write tsconfig.json**

```json
{
  "compilerOptions": {
    "target": "ESNext",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "lib": ["ESNext", "DOM", "DOM.Iterable", "WebWorker"],
    "jsx": "react-jsx",
    "strict": true,
    "skipLibCheck": true,
    "types": ["vite/client", "node"],
    "verbatimModuleSyntax": true,
    "noEmit": true
  },
  "include": ["pages", "scripts", "e2e", "vite.config.ts", "void.json", ".void"]
}
```

> `"module": "ESNext"` is required so `import X from './x' with { island: 'load' }` import attributes parse.

- [ ] **Step 2: Verify it parses**

Run: `cd website && npx tsc --noEmit -p tsconfig.json` (expected: may error on not-yet-created files; that is fine. It must NOT error with "Unexpected token" on import attributes once Task 9 files exist — re-checked there).
Expected now: tsc runs (errors only about missing `pages/*` files, acceptable at this step).

- [ ] **Step 3: Commit**

```bash
git add website/tsconfig.json && git commit -m "chore(website): tsconfig with ESNext modules for island import attributes"
```

---

### Task 4: Add `website/vite.config.ts` with verified plugin order + asset-gen `buildStart`

**Files:**
- Create: `website/vite.config.ts`

- [ ] **Step 1: Write vite.config.ts**

```ts
import { defineConfig } from 'vite'
import { voidPlugin } from 'void'
import { voidReact } from '@void/react/plugin'
import { voidMarkdown } from '@void/md/plugin'
import tailwindcss from '@tailwindcss/vite'

let assetsGenerated = false

export default defineConfig({
  plugins: [
    voidPlugin(),
    voidReact(),
    voidMarkdown(), // enforce:'pre', auto-detects React → MUST come after voidReact()
    tailwindcss(),
    {
      // Generates OG + demo images + changelog at build time. Runs under both
      // `npm run build` and `void deploy`'s internal `vite build`.
      name: 'gen-build-assets',
      apply: 'build',
      async buildStart() {
        if (assetsGenerated) return // buildStart fires per-environment (client+server); run once
        assetsGenerated = true
        const { generateAssets } = await import('./scripts/build-assets.mjs')
        await generateAssets()
      },
    },
  ],
})
```

- [ ] **Step 2: Verify config loads (after Task 5's void.json exists this is re-run; for now just typecheck import resolution)**

Run: `cd website && node -e "import('vite').then(()=>console.log('vite ok'))"`
Expected: `vite ok` (just proves vite resolves; full boot verified in Task 7).

- [ ] **Step 3: Commit**

```bash
git add website/vite.config.ts && git commit -m "feat(website): vite config with Void plugin order + build-asset hook"
```

---

### Task 5: Add `website/void.json` (output, ISR, isolation headers, redirects, head)

**Files:**
- Create: `website/void.json`

- [ ] **Step 1: Write void.json**

```jsonc
{
  "$schema": "./node_modules/void/schema.json",
  "output": "server",
  "routing": {
    "revalidate": {
      "/playground": 0,
      "/": 31536000,
      "/docs/*": 31536000,
      "/changelog": 31536000,
      "*": 60
    },
    "headers": {
      "/playground": [
        "Cross-Origin-Opener-Policy: same-origin",
        "Cross-Origin-Embedder-Policy: require-corp"
      ],
      "/playground/*": [
        "Cross-Origin-Opener-Policy: same-origin",
        "Cross-Origin-Embedder-Policy: require-corp"
      ]
    },
    "redirects": [
      { "source": "/docs/", "destination": "/docs", "permanent": true },
      { "source": "/docs/credits/", "destination": "/docs/credits", "permanent": true },
      { "source": "/changelog/", "destination": "/changelog", "permanent": true }
    ]
  },
  "head": {
    "titleTemplate": "%s | @napi-rs/image",
    "htmlAttrs": { "lang": "en" }
  }
}
```

> GA (`G-50ZQKJLY5K`) is intentionally NOT here — it is added in P2 via the shared (non-island) layout so it never loads on the isolated `/playground` route (D5). Revalidate keys are first-match-wins (specific before globs).

- [ ] **Step 2: Validate against the schema**

Run: `cd website && npx void prepare`
Expected: exits 0, generates `.void/` typedefs (`.void/routes.d.ts` etc.). If it reports a schema error in `void.json`, fix the offending key before continuing.

- [ ] **Step 3: Commit**

```bash
git add website/void.json website/.void && git commit -m "feat(website): void.json — server output, ISR, /playground isolation headers, redirects"
```

---

### Task 6: Add `website/app.css` — dark design system + scoped @void/md theme

**Files:**
- Create: `website/app.css`

- [ ] **Step 1: Write app.css**

```css
@import 'tailwindcss';
@import '@void/md/theme-content.css'; /* scoped to .void-md — no global reset (won't fight Tailwind preflight) */

/* dark via class, not OS @media — keeps a future light toggle class-based */
@custom-variant dark (&:where(.dark, .dark *));

@theme {
  --color-bg: #0a0a0f;
  --color-fg: #e7e7ee;
  --color-muted: #9aa0b4;
  --color-accent: oklch(72% 0.23 250);
}

:root {
  color-scheme: dark;
}

html,
body {
  background: var(--color-bg);
  color: var(--color-fg);
  font-family:
    ui-sans-serif, system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif;
}

code,
kbd,
pre {
  font-family: ui-monospace, 'SFMono-Regular', 'JetBrains Mono', Menlo, monospace;
}

/* match @void/md docs prose to the brand accent (used in P4) */
.void-md {
  --vmd-link: var(--color-accent);
}
```

- [ ] **Step 2: Commit (verified when the page boots in Task 7)**

```bash
git add website/app.css && git commit -m "feat(website): dark design-system tokens + scoped @void/md theme"
```

---

### Task 7: Minimal landing page + root layout — boot & build gate

**Files:**
- Create: `website/pages/layout.tsx`
- Create: `website/pages/index.tsx`
- Create: `website/pages/index.server.ts`

- [ ] **Step 1: Write the shared layout**

```tsx
// website/pages/layout.tsx
import type { ReactNode } from 'react'
import '../app.css'

export default function Layout({ children }: { children: ReactNode }) {
  return (
    <div className="min-h-screen">
      <header className="border-b border-white/10 px-6 py-4">
        <a href="/" className="font-mono font-bold">@napi-rs/image</a>
      </header>
      <main>{children}</main>
    </div>
  )
}
```

- [ ] **Step 2: Write the landing placeholder**

```tsx
// website/pages/index.tsx
export default function Home() {
  return (
    <section className="px-6 py-24 text-center">
      <h1 className="text-5xl font-bold tracking-tight">
        Fast image processing, <span className="text-(--color-accent)">in Rust</span>
      </h1>
      <p className="mt-4 text-(--color-muted)">Encode, compress, and transform images. Landing content lands in P2.</p>
      <p className="mt-8">
        <a className="text-(--color-accent) underline" href="/playground">Open the playground →</a>
      </p>
    </section>
  )
}
```

- [ ] **Step 3: Write the server loader (head + prerender)**

```ts
// website/pages/index.server.ts
import { defineHead } from '@void/react'

export const prerender = true

export const head = defineHead(() => ({
  title: 'Fast image processing in Rust',
  meta: [{ name: 'description', content: 'Encode, compress, and transform images with @napi-rs/image.' }],
}))
```

> If `defineHead` is not exported from `@void/react` at 0.9.3, use the markdown/frontmatter or the documented head export shape from `node_modules/@void/react`; verify the exact import in Step 4 and adjust.

- [ ] **Step 4: Boot the dev server and verify the page renders**

Run: `cd website && npm run dev` (then in another shell) `curl -s http://localhost:5173/ | grep -o "Fast image processing"`
Expected: prints `Fast image processing` (SSR HTML contains the heading). Stop the dev server.

- [ ] **Step 5: Verify a production build succeeds**

Run: `cd website && npm run build`
Expected: exits 0; `ls dist/client/index.html` exists; build log shows the `gen-build-assets` hook ran (it will fail until Task 8 creates the scripts — so if doing tasks in order, expect Step 5 to fail on the missing `./scripts/build-assets.mjs` import; that is the signal to do Task 8 next). Once Task 8 is done, re-run and expect exit 0.

- [ ] **Step 6: Commit**

```bash
git add website/pages && git commit -m "feat(website): minimal landing page + root layout (boot gate)"
```

---

### Task 8: Port the build-time asset scripts to `scripts/` (paths fixed, Void-shaped output)

**Files:**
- Create: `website/scripts/build-assets.mjs`
- Create: `website/scripts/generate-img.mjs` (from `website/generate-img.js`)
- Create: `website/scripts/og-image.mjs` (from `website/og-image.js`)
- Create: `website/scripts/changelog.mjs` (from `website/changelog.js`)
- Delete: `website/generate-img.js`, `website/og-image.js`, `website/changelog.js`

- [ ] **Step 1: Write `scripts/og-image.mjs` (global fetch, unchanged drawing logic)**

Copy `website/og-image.js` verbatim EXCEPT: remove `import fetch from 'node-fetch'` (use global `fetch`). Output path stays `public/img/og.png` (D2). Keep the canvas drawing + `pngQuantize` exactly as in the original. Export a function so the orchestrator can call it:

```js
// website/scripts/og-image.mjs
import { promises as fs } from 'node:fs'
import { createCanvas, GlobalFonts, Image } from '@napi-rs/canvas'
import { pngQuantize } from '@napi-rs/image'

const FONT_URL = 'https://github.com/Brooooooklyn/canvas/raw/main/__test__/fonts/iosevka-slab-regular.ttf'

export async function generateOgImage() {
  const canvas = createCanvas(1200, 700)
  const ctx = canvas.getContext('2d')
  ctx.globalCompositeOperation = 'destination-over'
  if (!GlobalFonts.families.some(({ family }) => family === 'Iosevka Slab')) {
    const font = await fetch(FONT_URL, { redirect: 'follow' }).then((r) => r.arrayBuffer())
    GlobalFonts.register(Buffer.from(font))
  }
  ctx.fillStyle = 'white'
  ctx.font = '48px Iosevka Slab'
  ctx.fillText('@napi-rs/image', 80, 100)
  // ... (paste the full Arrow SVG drawImage + ViceCity gradient block from the original og-image.js verbatim) ...
  await fs.mkdir('public/img', { recursive: true })
  await fs.writeFile('public/img/og.png', await pngQuantize(await canvas.encode('png'), { maxQuality: 90, minQuality: 75 }))
}

if (import.meta.url === `file://${process.argv[1]}`) await generateOgImage()
```

> Paste the omitted SVG/gradient block exactly from `website/generate-img.js`'s sibling `og-image.js` (the long `Arrow.src = ...` template and `ctx.drawImage`/gradient/`fillRect`). Do not paraphrase the SVG.

- [ ] **Step 2: Write `scripts/generate-img.mjs` (drop Vercel branch, fix cwd, global fetch)**

```js
// website/scripts/generate-img.mjs
import { execSync } from 'node:child_process'
import { join } from 'node:path'
import { promises as fs } from 'node:fs'
import chalk from 'chalk'

// run from website/ ; repo root is one level up
const REPO_ROOT = join(process.cwd(), '..')
const IMG_DIR = join(process.cwd(), 'public', 'img')

export async function generateDemoImages() {
  await fs.mkdir(IMG_DIR, { recursive: true })
  await fs.writeFile(join(IMG_DIR, 'example.mjs'), await fs.readFile(join(REPO_ROOT, 'example.mjs')))
  await fs.writeFile(join(IMG_DIR, 'sharp.mjs'), await fs.readFile(join(REPO_ROOT, 'sharp.mjs')))
  for (const script of ['example.mjs', 'sharp.mjs', 'manipulate.mjs']) {
    execSync(`node ${script}`, { cwd: IMG_DIR, stdio: 'inherit' })
  }
  console.info(chalk.green('demo images generated'))
}

if (import.meta.url === `file://${process.argv[1]}`) await generateDemoImages()
```

> The Vercel `.node` download branch is removed (R5): the native `@napi-rs/image` must already be built/installed for the CI OS/arch — see Task 10 CI notes. The original ran `node og-image` separately; here the orchestrator (Step 4) calls `generateOgImage()` directly.

- [ ] **Step 3: Write `scripts/changelog.mjs` (write to pages/changelog, plain markdown anchors)**

```js
// website/scripts/changelog.mjs
import { writeFile, mkdir } from 'node:fs/promises'
import { join } from 'node:path'

const PACKAGE = '@napi-rs/image'

export async function generateChangelog() {
  const token = process.env.GITHUB_TOKEN
  const res = await fetch('https://api.github.com/repos/Brooooooklyn/Image/releases?per_page=100', {
    headers: token ? { Authorization: `token ${token}` } : {},
  })
  const releases = await res.json()
  if (!Array.isArray(releases)) throw new Error(`GitHub releases fetch failed: ${JSON.stringify(releases)}`)
  const body = releases
    .filter(({ name }) => name?.startsWith(PACKAGE))
    .map((r) => {
      const md = r.body
        .replace(/&#39;/g, "'")
        .replace(/@([a-zA-Z0-9_-]+)(?=(,| ))/g, '[@$1](https://github.com/$1)')
      return `## [${r.tag_name}](${r.html_url})\n${new Date(r.published_at).toLocaleDateString('en')}\n\n${md}`
    })
    .join('\n\n')
  await mkdir(join(process.cwd(), 'pages', 'changelog'), { recursive: true })
  await writeFile(
    join(process.cwd(), 'pages', 'changelog', 'index.md'),
    `---\ntitle: 'Changelog'\ndescription: '@napi-rs/image changelog.'\n---\n\n# Changelog\n\n${body}\n`,
  )
}

if (import.meta.url === `file://${process.argv[1]}`) await generateChangelog()
```

> Nextra-specific inline `<a className="x:...">` rewrites from the original are replaced with plain markdown — `@void/md` renders standard GFM links. Missing `GITHUB_TOKEN` degrades to an unauthenticated request (low rate limit) rather than crashing the build.

- [ ] **Step 4: Write the orchestrator `scripts/build-assets.mjs`**

```js
// website/scripts/build-assets.mjs
import { generateDemoImages } from './generate-img.mjs'
import { generateOgImage } from './og-image.mjs'
import { generateChangelog } from './changelog.mjs'

export async function generateAssets() {
  await generateDemoImages()
  await generateOgImage()
  await generateChangelog()
}

if (import.meta.url === `file://${process.argv[1]}`) await generateAssets()
```

- [ ] **Step 5: Remove the originals and run the orchestrator standalone**

Run:
```bash
cd website
rm generate-img.js og-image.js changelog.js
node scripts/build-assets.mjs
```
Expected: `public/img/og.png` regenerated, demo images present, and `pages/changelog/index.md` created with `## [v...]` headings. Verify: `ls -la public/img/og.png pages/changelog/index.md`.

- [ ] **Step 6: Verify the build hook runs end-to-end (this also unblocks Task 7 Step 5)**

Run: `cd website && npm run build`
Expected: exits 0; build log shows the asset generation; `ls dist/client/img/og.png` exists.

- [ ] **Step 7: R4 check — confirm generated assets reached the build output**

Run: `cd website && ls dist/client/img/og.png && ls dist/client/img/example.mjs`
Expected: both exist. **If `dist/client/img/og.png` is MISSING** (public/ was copied before `buildStart` wrote it), switch the build to pre-generate: set `"build": "node scripts/build-assets.mjs && vite build"` in `package.json` and remove the `gen-build-assets` plugin from `vite.config.ts` (keep the plugin ONLY if this check passes). Re-run and confirm.

- [ ] **Step 8: Commit**

```bash
cd /Users/brooklyn/workspace/github/Image
git add -A website
git commit -m "feat(website): port OG/demo-image/changelog generation to Void build pipeline"
```

---

### Task 9: R1 GATE — WASM playground smoke-test island (build emits wasm + worker)

**Files:**
- Create: `website/pages/playground/worker.ts`
- Create: `website/pages/playground/_Playground.tsx`
- Create: `website/pages/playground/index.island.tsx`
- Create: `website/pages/playground/index.server.ts`

- [ ] **Step 1: Write the Web Worker that runs the WASM encode**

```ts
// website/pages/playground/worker.ts
/// <reference lib="webworker" />
self.onmessage = async (e: MessageEvent<ArrayBuffer>) => {
  try {
    // dynamic import: the 8.8MB wasm is fetched + instantiated only inside the worker.
    // top-level await in the module means this resolves once the wasm + thread pool are ready.
    const { Transformer } = await import('@napi-rs/image')
    const out = await new Transformer(new Uint8Array(e.data)).webp(75)
    ;(self as unknown as Worker).postMessage({ ok: true, bytes: out.byteLength })
  } catch (err) {
    ;(self as unknown as Worker).postMessage({ ok: false, error: String(err) })
  }
}
```

- [ ] **Step 2: Write the client island that spawns the worker**

```tsx
// website/pages/playground/_Playground.tsx
import { useEffect, useState } from 'react'

type Result = { status: 'idle' | 'isolating' | 'running' | 'done' | 'error'; bytes?: number; error?: string }

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
    // sample input shipped in public/img by the build pipeline
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
      {(r.status === 'running' || r.status === 'isolating') && <p>Encoding…</p>}
    </div>
  )
}
```

- [ ] **Step 3: Write the island page + server head**

```tsx
// website/pages/playground/index.island.tsx
import Playground from './_Playground' with { island: 'load' }

export default function PlaygroundPage() {
  return (
    <section>
      <h1 className="px-6 pt-12 text-3xl font-bold">Playground (smoke test)</h1>
      <Playground />
    </section>
  )
}
```

```ts
// website/pages/playground/index.server.ts
import { defineHead } from '@void/react'
// NO prerender — keep /playground SSR-on-demand so isolation headers apply per request.
export const head = defineHead(() => ({ title: 'Playground' }))
```

> Ensure `public/img/un-optimized.png` exists (it is a repo-root file copied into `public/img` — if the build pipeline does not place it there, copy it: `cp ../un-optimized.png public/img/` and verify). Adjust the `fetch` path if the sample lives elsewhere.

- [ ] **Step 4: Build and verify the wasm + worker assets are emitted (R1a — the core bundling risk)**

Run:
```bash
cd website && npm run build
ls dist/client/assets/*.wasm 2>/dev/null || find dist/client -name '*.wasm'
find dist/client -name '*worker*'
```
Expected: a `.wasm` file (~8.8MB) IS present under `dist/client`, and a worker chunk exists. **If no `.wasm` is emitted**, Vite did not bundle the bare-specifier worker/wasm URLs — apply the napi-rs Vite remedy: add `optimizeDeps: { exclude: ['@napi-rs/image', '@napi-rs/image-wasm32-wasi'] }` and/or `assetsInclude: ['**/*.wasm']` to `vite.config.ts`, and if the `wasi-worker-browser.mjs` bare specifier fails, add a resolve alias to its file path. Re-run until the `.wasm` is emitted. Document whatever was needed.

- [ ] **Step 5: typecheck the import-attribute syntax**

Run: `cd website && npx tsc --noEmit -p tsconfig.json`
Expected: NO "Unexpected token" / import-attribute parse errors (proves Task 3's `module: ESNext` is correct). Type errors unrelated to syntax are acceptable to note.

- [ ] **Step 6: Commit**

```bash
cd /Users/brooklyn/workspace/github/Image
git add -A website
git commit -m "feat(website): WASM playground smoke-test island (R1 gate scaffolding)"
```

---

### Task 10: Deploy pipeline + R1b live gate (cross-origin isolation + real encode)

**Files:**
- Create: `website/playwright.config.ts`
- Create: `website/e2e/playground-smoke.spec.ts`

- [ ] **Step 1: Write the Playwright config**

```ts
// website/playwright.config.ts
import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './e2e',
  timeout: 120_000, // wasm instantiate + encode can be slow on first load
  use: { baseURL: process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173' },
})
```

- [ ] **Step 2: Write the R1b smoke test**

```ts
// website/e2e/playground-smoke.spec.ts
import { test, expect } from '@playwright/test'

test('playground page is cross-origin isolated and the worker encodes the sample', async ({ page }) => {
  await page.goto('/playground')
  // cross-origin isolation must be active (proves COOP/COEP headers reached the document)
  await expect.poll(() => page.evaluate(() => self.crossOriginIsolated)).toBe(true)
  // the worker imports the wasm, encodes, and reports a positive byte count
  await expect(page.getByTestId('pg-status')).toHaveAttribute('data-status', 'done', { timeout: 120_000 })
  const text = await page.getByTestId('pg-bytes').innerText()
  expect(Number(text.match(/(\d+) bytes/)?.[1] ?? 0)).toBeGreaterThan(0)
})
```

- [ ] **Step 3: Install the Playwright browser, then try the test against the local dev server**

Run:
```bash
cd website
npx playwright install chromium
npm run dev   # in shell A
PLAYWRIGHT_BASE_URL=http://localhost:5173 npm run test:e2e   # in shell B
```
Expected: **either** the test passes (the Void dev server applies `void.json routing.headers`, so `crossOriginIsolated` is true) — **or** it fails on the `crossOriginIsolated` poll, which means dev does not apply the headers; in that case proceed to Step 4 and run the test against the deployed URL instead (do not treat a dev-only header miss as an R1 failure).

- [ ] **Step 4: Authenticate + deploy (USER-INTERACTIVE — these need a human)**

> `void auth login` opens a browser OAuth flow and `void deploy` may prompt to create/link a project. Ask the user to run these in the session with the `!` prefix, or run them and pause for the OAuth handoff:

```bash
cd website
void auth login          # user completes OAuth
node scripts/build-assets.mjs   # explicit safety-net asset gen (deploy's vite build also runs buildStart)
void deploy              # creates/links project on first run → prints the deploy URL
```
Expected: `void deploy` prints a `https://<slug>.void.app` URL; `.void/project.json` is written.

- [ ] **Step 5: R1b GATE — run the smoke test against the deployed URL**

Run: `cd website && PLAYWRIGHT_BASE_URL=https://<slug>.void.app npm run test:e2e`
Expected: PASS — `crossOriginIsolated === true` on the deployed `/playground` AND the worker reports `Encoded WebP: <n> bytes` with n > 0. **This passing is the P1 exit gate** (proves R1, R2, R3 end-to-end on real Cloudflare). If `crossOriginIsolated` is false on the deployed URL, the isolation headers are not reaching the SSR document — revisit Task 5 `routing.headers` (try the single-key `/playground*` form or a `public/_headers` mirror) before proceeding.

- [ ] **Step 6: Attach the custom domain (USER-INTERACTIVE)**

> Only after the gate passes. Ask the user to run:
```bash
cd website && void domain add image.napi.rs
```
Expected: prints DNS instructions; once DNS propagates, `https://image.napi.rs/playground` is live and isolated. (DNS change is the user's to make.)

- [ ] **Step 7: Commit**

```bash
cd /Users/brooklyn/workspace/github/Image
git add -A website
git commit -m "test(website): Playwright R1 gate (cross-origin isolation + wasm encode) + deploy pipeline"
```

---

## Self-Review

**Spec coverage (P1 portion of §12):**
- void scaffold/React/Tailwind/@void/md/design tokens → Tasks 2,3,4,6 ✓
- vite.config plugin order → Task 4 ✓
- void.json (output/revalidate/headers/redirects/head) → Task 5 ✓
- port build scripts (buildStart + CI) → Task 8 (+ CI note Task 10 Step 4) ✓
- deploy pipeline green → Task 10 ✓
- GATE R1 (wasm island encodes one image end-to-end) → Task 9 (R1a build-emit) + Task 10 Step 5 (R1b live) ✓
- GA + page_view → deliberately deferred to P2 (noted in Task 5); not a P1 gate ✓

**Placeholder scan:** The only "paste verbatim" directives are for the OG-image SVG/gradient block (Task 8 Step 1) — intentional, because reproducing a 150-line decorative SVG inline is error-prone; the source file (`website/og-image.js`) is the exact reference and still in the tree until Task 8 Step 5. No TBD/TODO/"add error handling" placeholders.

**Type consistency:** worker message contract `{ ok, bytes?, error? }` matches between `worker.ts` (Step 1) and `_Playground.tsx` (Step 2). `generateAssets`/`generateDemoImages`/`generateOgImage`/`generateChangelog` names match between the scripts and `build-assets.mjs` and `vite.config.ts`. `data-testid` values (`pg-status`/`pg-bytes`) match between `_Playground.tsx` and the Playwright test.

**Known API-shape risks flagged inline (verify against installed 0.9.3, adjust if needed):** `defineHead` import (Task 7/9), Vite wasm/worker bundling remedy (Task 9 Step 4), dev-server header application (Task 10 Step 3). These are the spec's R1/R3 gates and are surfaced as decision points, not assumptions.

---

## After P1

Once Task 10 Step 5 passes (R1 proven on real Cloudflare), write the **P2 (Landing) plan** and **P4 (Docs) plan** — both now safe to detail because the scaffold, build pipeline, and wasm integration are verified. P3 (full playground UI) builds directly on the proven worker from Task 9. P5 polish last.
