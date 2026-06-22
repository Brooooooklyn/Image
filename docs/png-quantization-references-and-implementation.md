---
title: "PNG Quantization — Research & Implementation History"
updated: 2026-06-22
purpose: "The research that was consulted, the decisions that were made, and the clean-room CIELAB quantizer that actually shipped (PR #208 + #211)."
status: "history / decision record"
---

# PNG Quantization — Research & Implementation History

This is the record of how `pngQuantize` in this repo came to be: what was read,
what was decided, what was built, and why. It replaces an earlier forward-looking
"how to implement" note. The earlier note recommended wrapping **libimagequant** as
the production route — that path was **deliberately abandoned** (see the pivot below);
keep that correction in mind when reading any older draft.

Source of truth for the implementation claims here:
`packages/binding/src/lab.rs`, `quantize.rs`, `quantize_simd.rs`, `png.rs`.

---

## 0. The pivot — why this is clean-room, not a libimagequant wrapper

```
   Original plan                          What shipped
   ─────────────                          ────────────
   decode → libimagequant → encode   ⇒    decode → OUR clean-room quantizer → encode
   (pngquant's GPL/copyleft engine)        (MIT, from published papers only)
```

`libimagequant`/`pngquant` is the best-known answer to "PNG quantize," and the first
research note pointed at it. It was dropped for one reason: **license**. It is the only
copyleft dependency that would have touched this codebase. The project chose to
implement the quantizer **from first principles** so the whole stack stays MIT.

Hard rule that followed from that choice, and that governs everything below:

> **Clean-room.** The implementation was written from *published algorithm
> descriptions only*. No imagequant / libimagequant / pngquant source code was ever
> read. The Celebi survey (below, footnote 9) independently warns that open-source
> ports "have varying degrees of faithfulness to the original algorithms" — which is
> the same reason to work from primary papers rather than from someone's code.

Proof it stuck: `grep imagequant|pngquant|gpl` over `Cargo.lock` +
`packages/binding/Cargo.toml` → **zero matches**. The only deps are `rgb` (RGBA8),
`image` (decode), `lodepng` (palette+tRNS encode), `oxipng` (lossless recompress) —
none copyleft.

---

## 1. The research that was consulted

Two layers: the **primary literature** (the canonical papers) and the **working notes**
scraped during design (kept in `.firecrawl/`). Each entry below says *what it gave us*
and *what we did with it*.

### 1a. Primary literature

| Source | What it is | Used for |
|---|---|---|
| **Heckbert 1982** — "Color Image Quantization for Frame Buffer Display," SIGGRAPH '82 | The original median-cut paper; the 4-stage decomposition (stats → palette → map → redraw/dither) | The overall pipeline shape |
| **Wu 1991/1992** — variance-based / dynamic-programming splitting (Graphics Gems II; *ACM TOG* 11(4)) | Choose the box split that **minimizes within-box variance (SSE)**, not just population | The median-cut **variant we actually use** |
| **Lloyd 1982 / Linde–Buzo–Gray 1980** | k-means / GLA — iterative centroid refinement; a *local* optimizer | Palette **refinement** after median-cut seeding |
| **Arthur & Vassilvitskii 2007** — k-means++ | D²-weighted seeding | **Empty-cluster reseed** during refinement |
| **Floyd & Steinberg 1976** | 7/3/5/1 error-diffusion dither | The **dither** stage |
| **Porter & Duff 1984** | Premultiplied-alpha compositing | Alpha-aware split **moments** |
| **Gervautz & Purgathofer 1988** — octree | Streaming/bounded-memory alternative | **Considered, not used** (median-cut+k-means gave better quality for our offline case) |
| **Celebi 2023** — "Forty Years of Color Quantization," *Artif. Intell. Rev.* 56 | The modern survey of the whole field | Validation + the reading map below |

### 1b. Working notes (`.firecrawl/`)

> `.firecrawl/` holds raw scrapes kept for provenance. Two are blog articles with real
> technical content; two are mostly bibliography/leads.

- **`forty-years-survey.md`** — Celebi 2023 (scrape is the abstract + 69 footnotes +
  bibliography; the article body is paywalled). High value as a **decision map**:
  - footnote 14: **CIELAB** is the CIE-recommended approximately-uniform space — validates
    quantizing in Lab rather than sRGB.
  - footnote 17: CIELAB is *usually* kept in float "to avoid precision loss"; **integer is
    faster but lower-precision.** Our pure-integer Lab is the documented speed side of this
    exact tradeoff — precision loss is the named cost we accept.
  - footnote 18: Lab MSE is computed with either **CIE76 (Euclidean)** or **CIEDE2000.**
    Our integer-Euclidean metric is effectively **CIE76** — good, but a known step below
    CIEDE2000 (the lever to pull if quality ever regresses).
  - footnotes 49/66: k-means is NP-hard and only a **local** optimizer → it needs the
    median-cut seed; footnote 9: open-source ports are unfaithful → **work from papers.**

- **`hyab-kmeans.md`** — Pekka Väänänen blog on **HyAB** distance (Abasi/Fairchild 2019):
  `|ΔL| + √(Δa²+Δb²)`. Evaluated and **rejected** for now:
  - HyAB needs a **different centroid update** — L* by *median*, a*/b* by *mean* (sum of
    abs-diff is minimized by the median). Adopting it without that change makes the two
    k-means steps optimize different objectives. Too much surface for a marginal,
    large-difference-only gain.
  - The lasting takeaway we *did* keep: **"dithering dominates final quality."** A decent
    quantizer + Floyd-Steinberg beats a fancier quantizer with no dither. So dither got
    first-class effort.

- **`median-cut-lab.md`** — Väänänen, "median cut in CIELAB." The single most
  **load-bearing** note, because of one counter-intuitive finding:
  - Running median-cut **splits** in CIELAB does **not** beat sRGB. The perceptual win is
    almost entirely in the **pixel-mapping** (nearest-color) step, not the axis-aligned
    splitting. CIELAB's L* is *under-weighted* for splitting.
  - **This directly shaped our design:** we **split in RGBA8** (cheap, and just as good per
    this finding) and spend the perceptual budget where it pays — the **CIELAB
    nearest-color metric** used for k-means assignment, dither, and final remap.

- **`survey-search.md`** — a web-search result list (titles/URLs only). Useful as a
  clean-room reading list (Celebi survey ×4, JOSA-A median-cut retrospective, an
  "optimized k-means" arXiv, CQ100 benchmark, CIELAB papers). It also independently
  endorses CIELAB ("among the best" — CQ100). No algorithm bodies; orientation only.

---

## 2. Decisions, and the research behind each

| Decision | What we chose | Why (grounded in §1) |
|---|---|---|
| **Engine** | Clean-room MIT, from papers | License (drop GPL libimagequant); Celebi fn9 |
| **Color space for *mapping*** | CIELAB | Celebi fn14, CQ100; perceptual win is in mapping (`median-cut-lab`) |
| **Color space for *splitting*** | RGBA8 (not Lab) | `median-cut-lab`: Lab splits don't beat RGB; cheaper |
| **Numeric model** | Integer fixed-point for the **distance metric + opaque argmin** (the dither's error diffusion is deterministic f32, not integer) | **Determinism is sacred** — byte-identical across x86/arm/wasm. Celebi fn17 names integer as the fast/lower-precision side; we take it deliberately |
| **Distance metric** | Squared **CIE76** ΔE (`dl²+da²+db²`), alpha-weighted | Celebi fn18's simpler option; HyAB rejected (needs median centroids); CIEDE2000 is the future lever |
| **Palette init** | **Wu** variance-minimizing median-cut | Wu 1991: split the box that reduces SSE most |
| **Refinement** | Lloyd k-means + k-means++ reseed + **keep-best guard** | Lloyd is local (fn66) & non-monotone under the alpha-weighted metric |
| **Dither** | Floyd-Steinberg, **selective**, serpentine, linear-light | Floyd-Steinberg 1976; "dither dominates" (`hyab-kmeans`) |
| **Alpha** | Reserved transparent slot + premultiplied moments + vanish penalties | Porter-Duff 1984; PNG `tRNS` semantics |

Why integer math is non-negotiable for the metric: integer add/mul are **exact and
associative**, so a sum like `dl²+da²+db²` is the same value regardless of SIMD lane order
or CPU. Float `powf`/`cbrt` are not bit-identical across backends, and float addition is
non-associative — a different reduction order can flip a nearest-color tie and silently
change a pixel's palette index. Keeping the CIELAB conversion + distance integer removes
that entire failure class, which is what later made the SIMD work (§5) safe.

**One honest exception — the default dither path is *not* integer.** `remap_dither` keeps
f32 Floyd-Steinberg error rows and builds each pixel's query color `want` in linear light
(`srgb_to_linear_f` → f32 error add → `linear_to_srgb8`) *before* the integer `rgb_to_lab`
lookup, so those floats do influence the chosen index (and, via diffusion, later pixels').
Output is still **byte-identical across platforms** because that f32 is only plain,
correctly-rounded IEEE-754 add/mul/div + table lookups — no `powf`, no fast-math, no
FMA-contractable `mul_add` — run under a fixed, data-independent serpentine scan. So the
accurate framing is *deterministic everywhere, integer-exact only for the metric and the
opaque argmin* — not "pure-integer everywhere."

---

## 3. The architecture that shipped (PR #208)

```
PNG bytes
  │  decode_rgba8  (image crate; sRGB assumed — gAMA/iCCP intentionally ignored)
  ▼
RGBA8 pixels
  │  build_histogram  → distinct-color histogram (u64 counts)
  │     · canonical_key: every a==0 pixel collapses to ONE exact (0,0,0,0) matte
  │     · posterize RGB by dropping N LSBs (alpha is NEVER posterized)
  ▼
  │  median_cut  (Wu variance cut, in RGBA8, on VISIBLE entries only)
  │     · split the box whose best split cuts total weighted SSE the most
  │     · merit compared as EXACT integer rationals (cmp_ratio, 256-bit) — float-free
  │     · split moments premultiply RGB by alpha (Porter-Duff); centroid = raw weighted mean
  ▼  initial palette (≤ max_colors, minus 1 slot if image has transparency)
  │  kmeans_refine  (Lloyd; iters = max(0, 10 − speed))
  │     · objective & assignment: pdist_lab  (CIELAB, see §4)
  │     · empty clusters → k-means++ D² reseed (deterministic fixed-seed LCG)
  │     · KEEP-BEST guard: adopt a pass only if objective ≤ best (Lloyd is non-monotone here)
  ▼  refined palette
  │  remap  →  indices
  │     · no-dither: remap_nearest, memoized per distinct color
  │     · dither:    remap_dither  (selective serpentine Floyd-Steinberg, §4)
  │     · insert exact (0,0,0,0) at index 0 if transparency; force a==0 pixels onto it
  ▼
  │  canonicalize: sort palette so non-opaque entries are FRONT (shortest tRNS)
  │  lodepng encode (PLTE + tRNS)  →  oxipng StripChunks::Safe recompress (keeps tRNS)
  ▼
PNG output    (never larger than input — indexed when quantizing wins; else the original
               returned verbatim, or an oxipng-reduced color type; see §7)
```

### Public API (`index.d.ts`, frozen)

```ts
pngQuantize(input: Uint8Array, options?, signal?): Promise<Buffer>
pngQuantizeSync(input: Uint8Array, options?): Buffer

interface PngQuantOptions {
  minQuality?: number     // default 70  — error if achieved quality is below this
  maxQuality?: number     // default 99  — maps to max_colors via a QUADRATIC ramp
  speed?: number          // 1–10, default 5 — higher = fewer k-means passes; 10 skips dither
  posterization?: number  // # of LSBs to ignore (retro / 15-bit palettes)
}
```

`maxQuality → max_colors = round(2 + (q/100)²·254)`, clamped 2..=256
(99→~251, 75→~145, 50→~66). If the exact-palette fast path applies (distinct colors ≤
max_colors) it returns a lossless palette at quality 100. If pass 1 scores below
`minQuality` and max_colors < 256, it retries once at 256 colors.

---

## 4. The CIELAB metric (the perceptual core)

All in integer fixed-point — **no `powf`/`cbrt`/`sqrt` on the runtime path** (`lab.rs`).

```
sRGB8 ──[SRGB_TO_LINEAR LUT, Q16]──► linear ──[D65 matrix MAT, Q16]──► XYZ ratios
       (table, no powf)                       (rows pre-divided by Xn/Yn/Zn)
                                                        │
                                                        ▼  f(t)=t^(1/3) via icbrt_u128
                                                        │  (integer cube root: Newton on u128
                                                        │   + exact floor correction — no cbrt)
                                                        ▼
   Lab{l,a,b: i32}, each ×100   (L=5358 means 53.58; ≤0.5 Lab-unit error vs skimage)
```

**Distance** — squared CIE76 (no sqrt; the quantizer only *compares* distances):

```
delta_e76_sq(p,q) = dl² + da² + db²        (dl,da,db widened to i64 before squaring)
                                            ≤ MAX_DELTA_E76_SQ = 669_160_034  (gamut diameter,
                                            pure-blue↔pure-green, pinned by exhaustive sweep)
```

**Full assignment metric** `pdist_lab` (in `quantize.rs`):

```
score = delta_e76_sq · wa/510          ← color term, down-weighted by combined alpha (wa ≤ 510)
      + da² · ALPHA_WEIGHT_LAB(1500)   ← alpha term (a full alpha flip ≈ a maximal ΔE≈100 gap)
      + dim_penalty   (final remap only): DIM_WEIGHT(3000)·Δa²   when entry dimmer
      + vanish_penalty(final remap only): VANISH_WEIGHT(100)·Δa³ when entry dimmer (steep cubic)
```

The dim/vanish penalties keep a **visible** source pixel from being assigned to a
near-invisible same-hue palette entry (and disappearing). They are keyed on the **raw**
source alpha and are **0** for clustering callers and on fully-opaque images, so they
never perturb the palette/objective — a compile-time `const` assertion proves the cubic
meets the anti-vanish guarantee against the gamut diameter.

### Dither, in one box

```
remap_dither: Floyd-Steinberg 7/3/5/1, SERPENTINE (even rows L→R, odd R→L)
  · color error diffuses in LINEAR LIGHT; alpha error in sRGB units
  · SELECTIVE strength = residual-ramp × edge-suppression × 1.0
      residual  → more dither where post-quant error is high (kills banding)
      edge/activity → less dither on hard edges/texture (floor 0.2, no halo)
  · index choice uses FULL inbound error; only OUTBOUND error is scaled (no seam)
  · outbound color scaled by src.a/255 (no invisible matte bleed); alpha not vis-scaled
  · transparent source pixels skipped; deadzone(2.0)+clamp(±192) bound jitter/streaks
```

---

## 5. The SIMD chapter (PR #211) — speed with zero byte change

The hottest loop is the per-pixel **nearest-palette argmin**. It was accelerated with
runtime-detected SIMD under a contract:

> **Determinism is sacred.** Output PNG must be **byte-identical** across x86 / arm / wasm
> and run-to-run. Every SIMD kernel reproduces the scalar argmin **bit-for-bit**, including
> *lowest-index-wins* on ties. Pure perf, zero golden-pin change.

### The opaque fast path (what makes SIMD trivially safe)

When the palette is all-opaque **and** the query is opaque, `pdist_lab` provably collapses:

```
wa=510 ⇒ ·510/510 = ·1   |   alpha term = 0   |   dim = vanish = 0
  ⇒  score == dl² + da² + db²   (no divide, no 64-bit, no branches)
```

In-gamut `≤ 669_160_034`; even out-of-gamut centroids keep each `|Δ| ≤ ~26_000` so
`3·26000² ≈ 2.03e9 < i32::MAX`. The whole scan fits in **i32** lanes — which is exactly
why integer SIMD reaches the same argmin as scalar.

### Five kernels, runtime dispatch

```
                       opaque_argmin(kernel, l[], a[], b[], q)
                                     │  (caller detects ONCE, above the pixel loop)
   ┌─────────────┬──────────────┬────┴────────┬─────────────┬──────────────┐
   ▼             ▼              ▼             ▼             ▼              ▼
 Scalar        NEON          AVX2          SSE4.1       simd128        (other
 (ref, i64)   4×i32         8×i32         4×i32         4×i32          → Scalar)
 every host   aarch64       x86_64        x86_64        wasm32
              (baseline)    if avx2       else if       if cfg
                                          sse4.1        simd128

 x86 order:  avx2 → sse4.1 → scalar     (widest first; is_x86_feature_detected!)
 aarch64:    NEON unconditionally       (ARMv8 baseline — no host can lack it)
 wasm:       compile-time cfg only      (no runtime probe)
```

**Never-panic:** each `*_simd` is `unsafe fn` + `#[target_feature]`, reachable only after a
passing `is_x86_feature_detected!` (x86) / on guaranteed NEON (arm) / when statically
compiled in (wasm). A host without the feature takes scalar → no illegal instruction.
Escape hatch `IMAGE_QUANTIZE_SCALAR=1` forces scalar without a rebuild.

**Determinism mechanics (all kernels):**
- within-lane **strict** compare keeps the lowest index;
- `i32::MAX` sentinel marks lanes that saw no full block (skipped in reduction);
- cross-lane reduce breaks ties with `d < best || (d == best && idx < best)`;
- scalar tail (`n % width`) mirrors the reference with strict `<`.

### The trap that nearly shipped a SIGILL

`is_x86_feature_detected!("avx2")` is **not** purely runtime — std_detect expands it to
`cfg!(target_feature="avx2") || <cpuid probe>`. A static `+avx2` floor in
`.cargo/config.toml` folds it to **compile-time `true`**, so the AVX2 kernel would run
**unconditionally** and **SIGILL** on a pre-AVX2 host. Fix: **keep target-feature floors
OUT of distributed x86_64 builds** (a warning comment lives in `.cargo/config.toml` and in
the module doc). wasm is the deliberate opposite — `+simd128` is a compile-time feature
with no probe, so it can never emit an illegal instruction.

### How it was verified (seeing is believing)

This Apple-Silicon host cannot run x86 AVX2 (Rosetta lacks it), so each path got a *real*
execution proof, not just review:

| Path | Execution proof |
|---|---|
| NEON | native aarch64 `cargo test` |
| SSE4.1 | x86_64 under Rosetta |
| **AVX2** | static musl `+avx2` binary in **amd64 Docker / qemu-TCG** — 2700 sweep checks == scalar |
| wasm simd128 | standalone `wasm32-wasip1 +simd128` under **wasmtime** — 2700 checks == scalar |
| all | `kernel_matches_scalar_<isa>` (palette 1..=300, random + exact-hit), `opaque_fast_path_equals_general`, golden integer pins, run1==run2 sha |

### Results

```
x86 (CodSpeed, PR #211 vs main):  ×2.9 overall, 6/6 improved, 0 regressions
   colors/256 ×4.3 · default ×4.2 · no_dither/256 ×3.9 · max_quality_75 ×3.1 · colors/64 ×2.0 · colors/16 +25%

aarch64 (local Apple Silicon, NEON vs pre-SIMD):
   default −62.7% · colors/256 −62.0% · no_dither/256 −60.4% · max_quality_75 −53.6% · colors/64 −37% · colors/16 −15%
```

(×4+ on the remap-bound cases confirms the 8-wide AVX2 kernel ran, not a 4-wide fallback.)

---

## 6. Deliberately not done (open levers)

| Lever | Status | Reason |
|---|---|---|
| **CIEDE2000** metric | not done | Heavier; CIE76 is "perceptually decent" (Celebi fn18). The first lever if quality regresses |
| **HyAB** distance | evaluated, rejected | Needs median-L* centroid update; marginal, large-difference-only gain |
| **Octree** init | not used | median-cut+k-means wins quality for offline use |
| **AVX-512** kernel | not done | CodSpeed's Valgrind can't execute it (no CI measurement); no local AVX-512 execution proof available (TCG support partial); narrow/shrinking host base; would break the uniform argmin structure. Not worth adding **blind** to a byte-identity guarantee |
| Lab-space **splitting** | not used | `median-cut-lab`: Lab splits don't beat RGB; the perceptual win is in mapping |

---

## 7. Verification & determinism gates (the safety net)

- **Golden pins:** exact integer Lab triples (`golden_exact_integer_outputs`), index-vector
  pins, `max_delta_e76_sq_is_gamut_diameter`, `icbrt_is_floor_cube_root` — fail if any
  platform's integer math diverges.
- **Equivalence:** `opaque_fast_path_equals_general`, `kernel_matches_scalar_<isa>`,
  `neon_matches_scalar_end_to_end`.
- **Determinism:** run1 == run2 SHA; cross-arch byte-identity follows from the integer-exact
  metric/argmin **plus** deterministic IEEE-754 in the dither (no `powf`/FMA/fast-math, fixed
  serpentine scan).
- **Quantizer-specific:** keep-best guard never worsens objective; visible pixel never maps
  to the reserved transparent slot; dithered visible pixel never vanishes; true-population
  (not capped) weights drive centroids.
- **PNG correctness** (of the intermediate lodepng indexed encode): color-type 3, legal
  indexed bit depth, `tRNS ≤ PLTE`, valid CRCs, round-trip decode. **Caveat — not an
  invariant of the final returned bytes:** the oxipng pass runs with color/grayscale/palette
  reduction *enabled* (it may losslessly re-emit a smaller non-indexed type), and the
  never-grow guard returns the **original input verbatim** when quantizing wouldn't shrink it
  (which may itself be truecolor). The actual output guarantee is *a valid, never-larger,
  round-tripping PNG of some legal color type* — not necessarily indexed.

---

## 8. PNG indexed-encoding reference (unchanged facts)

For an indexed output PNG (W3C PNG spec, https://w3c.github.io/png/):

- `IHDR.color_type = 3`; bit depth ∈ {1,2,4,8}; every sample is a palette index.
- `PLTE` required, 1–256 RGB triples; optional `tRNS` carries per-entry alpha;
  trailing opaque `tRNS` entries are omitted (front-loading non-opaque entries → shortest `tRNS`).
- Smallest legal depth: `≤2 colors→1`, `≤4→2`, `≤16→4`, `≤256→8`.

---

## 9. Reading order (clean-room reading list)

1. **Heckbert 1982** — problem decomposition + median-cut.
2. **Wu 1991/1992** — variance-based splitting (the cut we use).
3. **Lloyd/LBG** + **Arthur & Vassilvitskii 2007** — k-means refinement + k-means++ reseed.
4. **Floyd & Steinberg 1976** + **Ulichney 1987** — error diffusion + streak control.
5. **Celebi 2023** survey — the field map; CIELAB validation; CIE76-vs-CIEDE2000 lever.
6. Blogs: Väänänen **median-cut-in-CIELAB** (split-space vs map-space) + **HyAB** (dither dominates).
7. **W3C PNG spec** — correct indexed serialization.

*(`pngquant` / `libimagequant` are intentionally absent — clean-room.)*

---

## Provenance

Clean-room MIT implementation; no copyleft source was read. Shipped in **PR #208**
(clean-room quantizer) and accelerated in **PR #211** (runtime-dispatched SIMD,
byte-identical). Determinism is the invariant that makes the SIMD safe and the output
reproducible across x86 / aarch64 / wasm.
