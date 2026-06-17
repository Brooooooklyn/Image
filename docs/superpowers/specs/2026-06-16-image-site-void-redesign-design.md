# Design: `@napi-rs/image` website redesign on Void

- **Date:** 2026-06-16
- **Status:** Draft for review
- **Author:** brooklyn (with Claude Code)
- **Scope:** Full redesign + platform migration of the `@napi-rs/image` marketing/docs site (`website/`) from Next.js 16 + Nextra 4 to Void (Vite + Cloudflare), keeping the domain `image.napi.rs`.

All technical claims below were verified against the real Void source/docs (`void`/`@void/react`/`@void/md` `0.9.3`), the published `@napi-rs/image@1.12.0` + `@napi-rs/image-wasm32-wasi@1.12.0` packages, and `tailwindcss@4.3.1` by a parallel validation pass on 2026-06-15/16. Citations are in the companion research output, not repeated here.

---

## 1. Goals & non-goals

**Goals**
1. A bold, dark, image-forward **product site** (not just docs): landing page, interactive playground, rewritten docs, changelog.
2. Move the platform to **Void** and deploy via `void deploy` to the managed Cloudflare platform, custom domain `image.napi.rs`.
3. Ship an **in-browser interactive playground** powered by the existing `@napi-rs/image` WASM build — encode/compress/resize entirely client-side.
4. Preserve SEO: keep existing URLs, GA, OG/social cards, favicons.

**Non-goals (YAGNI)**
- No database, auth, KV, R2, queues, or cron — the site is content + client-side compute only.
- No server-side image processing (the native addon cannot run on Cloudflare Workers; it runs only at build time).
- No light theme at launch (dark is the brand; a toggle is deferred — design tokens leave the door open).
- No account/save/share features in the playground at launch.

---

## 2. Decisions locked during brainstorming

| # | Decision | Choice |
|---|----------|--------|
| Goal | Site ambition | Full product-site rethink (landing + playground + docs) |
| Aesthetic | Look & feel | Dark + vivid imagery (restrained dark chrome, color comes from the images) |
| Playground | Capabilities | Format conversion · compress-in-place · resize/rotate/EXIF · draggable compare + code snippet |
| Deploy | Platform & domain | `void deploy` (managed Cloudflare) + `void domain add image.napi.rs` |
| Docs | Content treatment | Rewrite, **split into sections** |
| Arch | Output mode | `output: "server"` (SSR) + ISR; partial-SSR playground (shell SSR'd, WASM after hydration) |
| Framework | UI | React 19 (continues current stack) |
| D1 | Docs IA | Split: `/docs`, `/docs/transformer`, `/docs/compression`, `/docs/credits` |
| D2 | OG image path | Keep `/img/og.png` |
| D3 | COEP mode | `require-corp` + self-host everything on `/playground` |
| D4 | Playground on mobile | Attempt everywhere with a warning; static-showcase fallback only when cross-origin isolation/SAB is unavailable |
| D5 | GA on `/playground` | Off (COEP blocks `gtag.js`); GA on landing + docs only |
| D6 | GA page_views | Add a soft-nav hook so route changes still log |

---

## 3. Architecture

```
Void + Vite + React 19
output: "server"  →  SSR on Cloudflare Workers, + ISR edge cache (KV-backed)
deploy:           →  void deploy → managed CF platform → void domain add image.napi.rs

RUNTIME SPLIT (the load-bearing boundary):
  Worker (server)   render SSR HTML shells. NO native addon, NO image work, NO wasm.
  Browser (client)  the WASM playground runs here, in a Web Worker, AFTER hydration.
  Node (build only) native @napi-rs/image + @napi-rs/canvas generate OG + demo assets.
```

**Rule (must hold):** `@napi-rs/image` / `@napi-rs/canvas` native packages are imported **only** by build scripts. They must never be imported by anything under `routes/`, `pages/**/*.server.*`, `middleware/`, `crons/`, `queues/` — that would pull a native `.node` into the Worker bundle and break the build/deploy.

### ISR / caching

Content pages get a 1-year revalidate (effectively static until the next deploy, which clears the ISR cache). `/playground` stays SSR-on-demand.

```jsonc
// void.json (excerpt) — first-match-wins, so specific BEFORE globs
"output": "server",
"routing": {
  "revalidate": {
    "/playground": 0,
    "/": 31536000,
    "/docs/*": 31536000,
    "/changelog": 31536000,
    "*": 60
  }
}
```

Content pages additionally `export const prerender = true` (deploy-time edge prerender, so the first visitor isn't cold). `/playground` does **not** prerender.

---

## 4. Site structure & routes

```
/                  Landing      SSR + prerender · ISR 1yr
/playground        Playground   SSR shell → hydrate → WASM (Web Worker) · ISR 0 · isolation headers
/docs              Docs home    overview + getting-started   (markdown)
/docs/transformer  API ref      the Transformer class surface (markdown)
/docs/compression  API ref      losslessCompressPng / pngQuantize / compressJpeg (markdown)
/docs/credits      Credits      license attributions (markdown)
/changelog         Changelog    generated from git at build → markdown

redirects: trailing-slash normalizers only (no content moved). See §10.
```

### File layout

```
website/
  vite.config.ts            voidPlugin() + voidReact() + voidMarkdown() + tailwindcss() + asset-gen buildStart plugin
  void.json                 output, revalidate, headers, redirects, head (GA)
  env.ts                    (likely empty / minimal — no secrets needed at runtime)
  app.css                   tailwind entry + dark-first tokens + @void/md theme import override
  pages/
    index.tsx               Landing (component)
    index.server.ts         loader (benchmark/format data) + head() + prerender
    playground/
      index.island.tsx      page; imports _Playground with { island: 'load' }
      _Playground.tsx       useEffect + dynamic import('@napi-rs/image'); spins Web Worker
      worker.ts             Web Worker: receives bytes+op, runs wasm, returns bytes+meta
      index.server.ts       head() only — NO prerender
    docs/
      layout.island.tsx     docs shell: sidebar (from @void/md/pages) + TOC + search box
      index.md
      transformer.md
      compression.md
      credits.md
    changelog/
      index.md              generated by changelog.js at build
  scripts/                  build-time Node (native addon allowed here)
    generate-img.ts         before/after + format demo assets → public/img
    og-image.ts             OG card → public/img/og.png
    changelog.ts            git log → pages/changelog/index.md
  public/
    img/                    favicons, og.png, generated demo assets, vendored sample inputs
    _headers                (optional mirror of void.json headers; void.json wins)
```

---

## 5. Design system — dark + vivid

```
chrome    near-black background · high-contrast foreground · monospace for code/labels/benchmarks
accent    a single electric accent (oklch) for links, focus, CTAs
color     the VIVID is carried by the IMAGES (before/after, format demos) — UI stays restrained
type      clean sans for prose + monospace for code
styling   Tailwind v4 → landing · playground · all chrome
          @void/md  → docs prose only (scoped), themed via --vmd-* to match accent
code      Shiki dual light/dark built into @void/md (dark default), zero client JS
mode      dark-first (the @custom-variant below opts dark off the OS so a future toggle is class-based)
```

### Tailwind v4 (CSS-only config — no `tailwind.config.js`, no PostCSS)

```css
/* app.css */
@import 'tailwindcss';
@import '@void/md/theme-content.css';          /* scoped to .void-md — does NOT reset/preflight-fight */
@custom-variant dark (&:where(.dark, .dark *)); /* dark via class, not OS @media */

@theme {
  --color-accent: oklch(72% 0.23 250);
}
:root { color-scheme: dark; --bg: #0a0a0f; --fg: #e7e7ee; }
body { background: var(--bg); color: var(--fg); }

/* match @void/md prose to the brand accent */
.void-md { --vmd-link: var(--color-accent); }
```

> Use `@void/md/theme-content.css` (scoped to `.void-md`), **not** `@void/md/theme.css` — the full theme ships a reset that fights Tailwind's preflight. Docs prose is wrapped in `<main className="void-md">`.

---

## 6. Playground engine (the centerpiece)

### Lifecycle

```
SSR        render controls + dropzone + sample-image + empty result/code panes (instant paint, indexable)
hydrate    React island mounts (island: 'load')
defer      ALL wasm/Worker/DOM work is inside useEffect + dynamic import('@napi-rs/image')
           → 8.8MB wasm never enters the SSR HTML or the server bundle
on use     first interaction (or idle) → spawn Web Worker → worker imports @napi-rs/image
           → wasm instantiates (implicit top-level-await; ~250MB shared mem, 4 threads)
worker     receives {op, bytes, opts} → runs:
             convert  : new Transformer(bytes).webp(q) | .avif({quality,chromaSubsampling}) | .png() | jpeg
             compress : losslessCompressPng(bytes) | pngQuantize(bytes,{maxQuality}) | compressJpeg(bytes,{quality})
             transform: new Transformer(bytes).rotate().resize(w,h,ResizeFilterType.Lanczos3)...
           → returns output bytes (Uint8Array → Blob) + metadata() {width,height,format}
UI         draggable before/after slider over decoded images · live output size + % savings
           · generated code snippet reproducing the exact settings (copy button)
```

### Isolation (scoped to `/playground` only)

```jsonc
// void.json routing.headers — two-key form for exact anchoring
"/playground":   ["Cross-Origin-Opener-Policy: same-origin", "Cross-Origin-Embedder-Policy: require-corp"],
"/playground/*": ["Cross-Origin-Opener-Policy: same-origin", "Cross-Origin-Embedder-Policy: require-corp"]
```

- Isolating only the `/playground` document is sufficient; the spawned same-origin Worker inherits the isolated agent cluster. All subresources (wasm, worker `.mjs`, JS chunks, demo images) are **same-origin** → no CORP headers needed.
- Landing + docs stay non-isolated, so GA and any third-party assets there are unaffected.
- Client guard before init: `if (!self.crossOriginIsolated) → render static showcase fallback` (rather than throwing a confusing SAB error).

### D4 behavior — attempt everywhere, warn

```
crossOriginIsolated === true   → run the live WASM playground (desktop AND mobile)
                                  · on mobile or very large inputs: show a non-blocking
                                    "this is heavy — may be slow or run out of memory" warning
                                  · downscale-before-encode option offered for huge inputs
crossOriginIsolated === false  → cannot instantiate SAB → static before/after showcase + explanation
                                  (older browsers without cross-origin isolation support)
```

### Constraints baked in
- Add `@napi-rs/image-wasm32-wasi` as an **explicit** website dependency (its `cpu:['wasm32']` means npm may skip it transitively).
- `tsconfig` `"module": "ESNext"` (required for `with { island }` import attributes).
- Do **not** expose `Transformer.fromSvg` with text (the wasm build is `fs:false`, system fonts won't load → glyph-less text). Vector-only SVG and all raster ops are fine.
- Vite must keep `new URL('@napi-rs/image-wasm32-wasi/wasi-worker-browser.mjs', import.meta.url)` resolvable and emit the `.wasm` asset — **smoke-test early** (highest integration risk, see §11/R1).

---

## 7. Docs system

```
content    rewritten markdown in pages/docs/*.md  (frontmatter: title, description)
render     @void/md → static HTML at build, zero client JS, Shiki dual-theme highlighting, auto-head
shell      pages/docs/layout.island.tsx, hand-built:
             sidebar  ← import pages from '@void/md/pages'  ([{path,title,frontmatter,headings}])
             TOC      ← page.headings
             search   ← headings/title index (see R7: full-text needs more work; launch = headings+title)
prose CSS  @void/md/theme-content.css scoped to .void-md, --vmd-* matched to accent
islands    interactive bits in docs (e.g. the resize-filter comparison) via
             <script>import X from '...' with { island: 'visible' }</script>  (see R6 — may become a static image)
```

IA (D1): `/docs` overview+getting-started, `/docs/transformer`, `/docs/compression`, `/docs/credits`.

**Content truth:** reconcile the two conflicting format matrices (current `index.mdx` says WebP/AVIF decode ✅; `docs/index.mdx` + root README say No) to a single source verified against `packages/binding/index.d.ts` during the rewrite. Carry over: full Transformer API (incl. the currently-undocumented `.overlay()`), format matrix, Node/OS support matrix, the two benchmark blocks, the resize-filter comparison, and ~12 credits license blocks. Vendor the 5 currently-hotlinked `raw.githubusercontent` resize images into `public/img`.

---

## 8. Build pipeline & CI

**Critical:** `void deploy` for a pages app runs a hardcoded `vite build` and **ignores** `package.json "build"` and `void.json inference.build`. So native-asset generation must hook in two ways (belt + suspenders):

```
1. Vite plugin buildStart (apply:'build') in vite.config.ts:
     runs scripts/generate-img.ts → scripts/changelog.ts → scripts/og-image.ts
     → fires under BOTH `npm run build` AND void deploy's internal `vite build`
2. Explicit CI step before void deploy (safety net):
     npm ci → npx void prepare → node scripts/*.ts (asset gen) → npx void deploy
```

`void prepare` generates `.void/*.d.ts` + tsconfig for typechecking without booting Vite (run after install in CI).

**Porting the existing scripts (R5 — handle carefully):**
- `generate-img.js` has a Vercel-only branch (downloads a linux `.node`) and reaches repo-root-relative paths (`../packages/binding`, `../example.mjs`) assuming the monorepo layout. Rewrite paths for the Void build cwd; drop the Vercel branch; ensure a prebuilt native `@napi-rs/image` exists for the CI OS/arch.
- `og-image.js` fetches a remote TTF — ensure build-time network egress (or vendor the font).
- `changelog.js` needs `GITHUB_TOKEN` + egress **at build time** (not a Worker secret). Rewrite its Nextra-specific class/anchor output to plain markdown anchors.
- Smoke-test that `buildStart` fires before Vite copies `public/` → `dist/client` (R4).

---

## 9. Configuration reference (verified snippets)

### `vite.config.ts`

```ts
import { defineConfig } from 'vite'
import { voidPlugin } from 'void'
import { voidReact } from '@void/react/plugin'
import { voidMarkdown } from '@void/md/plugin'
import tailwindcss from '@tailwindcss/vite'
import { generateAssets } from './scripts/build-assets' // wraps the 3 gen scripts

export default defineConfig({
  plugins: [
    voidPlugin(),
    voidReact(),
    voidMarkdown(),       // enforce:'pre', auto-detects React → MUST be after voidReact()
    tailwindcss(),        // self-enforces 'pre', only touches .css → position not load-bearing
    { name: 'gen-assets', apply: 'build', buildStart: () => generateAssets() },
  ],
})
```

### Dependencies to pin

```
void  @void/react  @void/md        → 0.9.3 (lockstep; @void/md peer-pins void exactly)
tailwindcss  @tailwindcss/vite     → 4.3.1   (no tailwind.config.js, no PostCSS)
react  react-dom                   → 19.x
@napi-rs/image                     → 1.12.0
@napi-rs/image-wasm32-wasi         → 1.12.0  (EXPLICIT dep — do not rely on transitive resolution)
@napi-rs/canvas                    → build-time (OG image)
(no @vitejs/plugin-react — voidReact bundles it)
```

### GA (D5/D6)

```
site-wide gtag bootstrap (G-50ZQKJLY5K) via void.json head.script — but NOT on /playground
  (render the snippet conditionally off the isolated route, OR omit from global head and inject
   in the root layout only for non-/playground paths).
soft-nav page_view: a small useRouter hook in the root layout fires gtag('event','page_view') on route change.
```

---

## 10. Content migration & SEO

```
URLs       4 current (/ , /docs , /docs/credits , /changelog) all preserved; /docs/credits kept;
           new /docs/transformer + /docs/compression added; old /docs stays valid (overview).
redirects  trailing-slash normalizers only (current Next.js is trailingSlash:false):
             /docs/ → /docs ; /docs/credits/ → /docs/credits ; /changelog/ → /changelog  (301)
assets     keep /img/og.png (D2), favicons, apple-touch-icon. Re-generate demo assets at build.
           NOTE: current demo images are NOT in git (build outputs); 4 source inputs are symlinks
           to repo-root files outside website/ — fix these paths when porting generate scripts.
meta       OG + Twitter card + favicons ported to void.json head / per-page head().
```

---

## 11. Risks & first-build validation gates

Run these **before trusting the design** (smoke tests, in order):

| ID | Risk | Gate |
|----|------|------|
| R1 | Vite bundling the wasm package inside a Void island (top-level-await + bare-specifier `new URL` worker + `WebAssembly.instantiate`) may need a Vite alias/copy tweak. **Highest integration risk.** | Scaffold a minimal `/playground` island that imports `@napi-rs/image` in a worker and encodes one image. Confirm `vite build` emits the `.wasm` + worker `.mjs` and it runs. |
| R4 | `buildStart` asset-gen vs `public/` copy ordering in a multi-environment build. | Local `vite build`; confirm generated images land in `dist/client/img`. |
| R5 | CI native-addon availability + script path/env assumptions. | Dry-run the asset scripts in the deploy env; confirm native `@napi-rs/image` + `GITHUB_TOKEN` + egress. |
| R2/R8 | Browser/Safari nested module-worker + SAB; live deploy isolation, cold-start, memory — all verified statically only. | Deploy to a preview; test isolation header on `/playground`, run the playground on Chrome/Firefox/Safari + a phone. |
| R6 | `@void/md` may not support Nextra-style inline component imports in `.md`. | Convert the resize-filter comparison to a registered island or a static image if inline import fails. |
| R7 | `@void/md/pages` gives headings/title only (no body text) for search. | Launch with headings+title search; full-text is a follow-up. |
| R3/R9 | Production dispatch worker applying `routing.headers` to SSR HTML was inferred, not byte-verified; checkout was 0.9.0 vs published 0.9.3. | Verify the isolation header is actually present on the deployed `/playground` document response. |

---

## 12. Phasing (implementation plan preview)

```
P1 Foundation   void scaffold in website/ · React · Tailwind v4 · @void/md · design tokens
                · vite.config plugin order · void.json (output/revalidate/headers/redirects/head)
                · port build scripts (buildStart + CI) · deploy pipeline green
                · GATE: R1 wasm smoke-test island encodes one image end-to-end
P2 Landing      hero · format-support matrix (reconciled) · benchmarks · before/after showcases
                · code samples · CTAs → playground & docs · OG + GA + page_view hook
P3 Playground   worker.ts wasm engine · 4 capabilities · dropzone + sample · compare slider
                · live size/savings · code-gen + copy · isolation headers · D4 warn/fallback
P4 Docs         rewrite content split into 4 pages · @void/md · sidebar/TOC/search shell
                · reconcile matrices · vendor hotlinked images · resize-table island-or-static
P5 Polish       changelog generation · redirects · perf (lazy wasm, cache) · a11y · cross-browser · launch
```

Each phase ships independently behind the others; `void deploy` keeps the site live throughout.

---

## 13. Open follow-ups (post-launch, not blocking)

- Light theme toggle (tokens already class-based).
- Full-text docs search (R7).
- Playground share/permalink of settings.
