# @napi-rs/image Website — Editorial/Technical Redesign

**Date:** 2026-06-17
**Status:** Design (awaiting review)
**Target:** `website/` (Vite + Void app, live at https://image.napi.rs)

## Goal

Rebuild the entire **visual layer** of the site so it reads as a crafted, modern, editorial/technical product page — letting the benchmarks and live demos *be* the design — without changing the content or the WASM playground engine.

## Direction (locked with user)

| Decision | Choice |
|---|---|
| Aesthetic | Technical / editorial (swc.rs / oxc / Bun lineage): typography-led, content-forward, substance-as-style |
| Scope | Full visual redesign; keep content + playground engine |
| Typography | Space Grotesk (display) + Inter (body) + JetBrains Mono (signature) |
| Color | Refined dark, warm **rust** accent |
| Hero | Statement headline + proof strip + multi-PM install switcher |

## 1. Type system

```
Space Grotesk  500/600/700   display & headings — tight tracking, large editorial sizes
Inter          400/500/600   body / UI text
JetBrains Mono 400/500       SIGNATURE: numbered eyebrows, labels, code, tabular numerals
```

- Self-hosted `woff2`, `font-display: swap`, `<link rel=preload>` the hero weights (Space Grotesk 600, Inter 400). Subset to latin if practical.
- Fluid scale via `clamp()`:
  - `display-xl` clamp(2.5rem, 6vw, 4.5rem), Space Grotesk 600, tracking −0.02em
  - `display-lg` clamp(2rem, 4.5vw, 3rem)
  - `h2` (section) clamp(1.75rem, 3vw, 2.5rem)
  - body 1rem / line-height 1.6 (Inter)
  - eyebrow 0.78rem mono, uppercase, tracking 0.12em
- Benchmark/stat numerals: JetBrains Mono with `font-variant-numeric: tabular-nums`.

## 2. Color & surface tokens

Defined in `website/app.css` `@theme`. Starting values (refined during build):

```
--color-bg            #0b0a09         warm near-black (vs today's cold #0a0a0f)
--color-surface-1     #131210         panels/cards
--color-surface-2     #1b1916         insets / hover
--color-border        rgb(255 250 245 / .08)
--color-border-strong rgb(255 250 245 / .14)
--color-fg            #f4f1ec         warm off-white
--color-muted         #a8a298         warm gray (passes AA on bg/surfaces)
--color-faint         #6f6a61         de-emphasised (use only on dark bg, AA-checked)
--color-accent        oklch(68% .16 52)   rust/amber
--color-accent-strong oklch(73% .17 52)
--color-accent-muted  oklch(68% .16 52 / .16)   fills/chips
--color-accent-glow   oklch(70% .18 52 / .22)   hero radial glow
--radius-sm/md/lg     6 / 10 / 16px
```

- Benchmark bars: **rust** for @napi-rs/image, neutral muted for competitors (monochrome + one accent — editorial restraint; no rainbow).
- Texture: faint SVG grain overlay (very low opacity) + a soft rust radial glow behind the hero. Hairline (`--color-border`) section dividers.
- **Accessibility:** every text/!bg pair checked for WCAG AA (the earlier dropdown bug is the lesson — no faint-on-faint).

## 3. Layout & rhythm

- Content width ~1140px, generous gutters; full-bleed background accents.
- Every section follows one rhythm:
  `mono numbered eyebrow (01 — BENCHMARK) → Space Grotesk headline → Inter subhead → content`
- Consistent spacing scale; hairline dividers between sections; large vertical rhythm.

## 4. Hero — statement + proof strip

- **Statement:** large Space Grotesk headline (e.g., "Fast image processing in Rust.") with one rust-accented phrase; mono kicker eyebrow above; concise Inter subhead.
- **Install switcher:** tabs `vp · pnpm · yarn · bun · npm`; selected tab renders the command in a mono code chip with a copy button. Commands:
  - npm `npm i @napi-rs/image`
  - pnpm `pnpm add @napi-rs/image`
  - yarn `yarn add @napi-rs/image`
  - bun `bun add @napi-rs/image`
  - vp `vp add @napi-rs/image`
  - Default tab: `vp`. Selected PM persists to `localStorage`.
- **CTAs:** primary "Open playground" → `/playground`; secondary "Docs" / GitHub.
- **Proof strip:** compact row pairing one big mono stat ("N× faster than sharp", count-up) with a small before/after thumbnail or mini bar.
- **Background:** rust radial glow + faint grain + hairline baseline.

## 5. Sections (reworked, substance-as-style)

```
01 BENCHMARK   "Faster than sharp" — the data-viz centerpiece: animated horizontal
               bars (rust = napi-rs, muted = sharp/others), huge mono "N× faster"
               count-up on scroll-in.
02 COMPRESSION "See the bytes disappear" — before/after image slider; mono byte
               counters tick down (e.g. 2.4 MB → 180 KB); format toggle.
03 FORMATS     support matrix — refined table: mono headers, hairline grid, crisp
               ✓/✗, formats × capabilities (encode/decode/lossless).
04 FILTERS     built-in filters — gallery of cleaner demo cards, mono labels.
05 PIPELINE    "One pipeline" — polished shiki code block, rust-accent highlights,
               copy button.
CTA            "Try it in your browser" — editorial hand-off into /playground.
```

## 6. Motion (restrained, purposeful)

- Scroll reveals via IntersectionObserver: fade + translateY 8–12px, stagger 60–80ms.
- Benchmark bars grow + numbers count-up (rAF) when scrolled into view.
- Before/after slider drag; install-tab + copy interactions.
- Hover: border brightens + 1–2px lift; accent focus rings.
- `prefers-reduced-motion: reduce` → no transforms/count-ups; render final state.

## 7. Components / primitives (new)

Under `website/pages/_components/` (or existing components dir), SSR-safe; interactive ones are Void islands/client components:

```
SectionHeader      eyebrow# + title + subhead
Stat / CountUp     mono numeral, animates on scroll-in
BenchmarkChart     bars + stat (island)
BeforeAfterSlider  image compare + byte counters (island)
InstallSwitcher    PM tabs + mono command + copy + localStorage (island)
CodeBlock          shiki, copy button
Chip / Badge       mono pill
Button             primary / secondary / ghost
Reveal             scroll-reveal wrapper (island)
```

## 8. Scope boundaries

**In:** landing page full redesign; shared design system (tokens, self-hosted fonts, primitives); global chrome (header/nav/footer); propagate the new tokens/type to docs + playground **chrome** (nav, type, surfaces) for consistency.

**Out / unchanged behavior:** the playground WASM engine + control logic (restyle chrome only); the GA-isolation + COEP behavior; the existing content/copy (light edits only). Docs/playground page **bodies** inherit the new tokens automatically with targeted polish — not a from-scratch rework (YAGNI; the landing is the priority).

## 9. Constraints (must honor)

- **Performance:** the site sells speed, so it must *be* fast — self-host + preload + subset fonts, no layout shift from fonts (CLS≈0), keep JS lean (islands only where needed).
- **Void SSR:** every section server-renders; interactivity via islands. No runtime WASM in SSR — the playground stays an island ([[void-ssr-workerd-no-runtime-wasm]]). Don't gate per-route SSR output on `useRouter().path` ([[void-island-pages-no-routercontext-ssr]]).
- **Cross-origin isolation** on `/playground` (COOP/COEP) preserved; `/assets/*` COEP rule preserved.
- **GA isolation** rules preserved (no GA in isolated SSR HTML; GA injected client-side on landing).
- **e2e gate:** the 4 Playwright tests must still pass; update selectors if structure changes.
- **@void/md** docs theme wrapper preserved where docs render markdown ([[void-md-prose-theme-setup]]).

## 10. Success criteria

- Landing reads unmistakably as a crafted editorial/technical Rust site: type identity, rust accent, layered depth, purposeful motion.
- All sections rebuilt per §5; hero per §4.
- Fonts self-hosted + preloaded; no font CLS; Lighthouse perf stays strong.
- e2e green; playground still cross-origin isolated and encodes the sample.
- Deployed to image.napi.rs.
