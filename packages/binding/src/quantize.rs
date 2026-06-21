//! Clean-room PNG color quantizer.
//!
//! This module implements an 8-bit indexed-color quantizer from first
//! principles using only published, patent-free algorithms:
//!
//! * **Median-cut** (Heckbert, 1982) for an initial palette,
//! * **Lloyd / k-means** relaxation to refine the palette, and
//! * **Floyd-Steinberg** error-diffusion dithering for the remap.
//!
//! It depends only on [`rgb::RGBA8`] (already a direct dependency) and the
//! standard library. There are intentionally **no** `napi` types here so the
//! algorithm stays pure, testable, and `wasm`-safe.

use std::collections::HashMap;

use rgb::RGBA8;

use crate::lab::{Lab, MAX_DELTA_E76_SQ, delta_e76_sq, rgb_to_lab};

/// Hard upper bound on an 8-bit indexed palette.
const MAX_PALETTE: usize = 256;

/// Alpha-error weight relative to a single color channel, used by the RGB
/// [`dist2`] metric (quality gate, dither calibration, Wu split).
///
/// Alpha mistakes are far more visible than a small color shift, so the alpha
/// term in the distance metric is scaled up by this factor.
const ALPHA_WEIGHT: i64 = 3;

/// Alpha-error weight for the PERCEPTUAL [`pdist`] metric (color ASSIGNMENT).
///
/// `pdist`'s color term is a CIE76 ΔE² in (×[`crate::lab::LAB_SCALE`])² units, so
/// its magnitude is on a completely different scale than `dist2`'s RGB squared
/// distance — `ALPHA_WEIGHT` (=3) cannot be reused. A "very different colors" gap
/// is ΔE ≈ 100, i.e. `(100 · LAB_SCALE)² = 1e8`; a full alpha flip is `da² = 255² =
/// 65025`. To rank a full alpha flip COMPARABLE to that maximal color gap we need
/// `da² · ALPHA_WEIGHT_LAB ≈ 1e8`, i.e. `ALPHA_WEIGHT_LAB ≈ 1e8 / 65025 ≈ 1538`.
/// We use 1500: it puts a full alpha flip (`255² · 1500 ≈ 9.75e7`) just under a
/// maximal ΔE≈100 color gap, so a large color change is never out-ranked by an
/// alpha flip, yet a full alpha flip still dwarfs any realistic small color change
/// (e.g. ΔE 2 ≈ `(2·100)² = 4e4`). Tuned against the transparency tests (fully
/// -transparent collapse, partial-alpha preservation) and the A/B measurement; see
/// the report. `ALPHA_WEIGHT` (the RGB one, =3) is unchanged and still used by
/// `dist2`.
const ALPHA_WEIGHT_LAB: i64 = 1500;

/// One-sided "visibility-loss" weight for the FINAL REMAP assignment only.
///
/// Added to the assignment score (in [`nearest_lab`]) ONLY when a candidate palette
/// entry is strictly DIMMER than the SOURCE pixel, as `DIM_WEIGHT · (src_a − entry_a)²`;
/// it is ZERO when the entry is as opaque or more. It exists to stop a VISIBLE source
/// pixel being assigned to a non-zero-but-near-invisible SAME-HUE entry and VANISHING:
/// `pdist_lab`'s color term `de · wa/510` is discounted up to ~2× for a low-alpha entry
/// (`wa = query_a + entry_a` shrinks), and `da² · ALPHA_WEIGHT_LAB` alone is too small
/// versus Lab color gaps (~1e8), so an opaque query is otherwise pulled onto a dim entry.
///
/// It is `2 · ALPHA_WEIGHT_LAB` (= 3000), i.e. dimming a pixel costs 3× the symmetric
/// alpha penalty (1500 base + 3000 one-sided) while brightening stays at 1×. Lower bound:
/// flipping the verified repro (opaque green must rank red@255 over green@11) needs
/// `(1500 + DIM_WEIGHT) · 244² > 212.5e6`, i.e. `DIM_WEIGHT > 2070`; 3000 clears it with
/// margin. It is NOT bounded above by any pinned test because the penalty is applied ONLY
/// at the two remap sites (`remap_nearest`, `remap_dither`) — clustering, `kmeans_objective`,
/// the D² reseed, the Wu split, and `quality_score` all pass `guard_src_alpha = 0` and so
/// are byte-identical. Being one-sided and PROPORTIONAL, small dimming stays negligible
/// (e.g. `a=8 → a=3`: `3000·25 = 75_000 ≪` a typical `pdist` gap), so legitimate
/// partial-alpha remaps are not disturbed (no hard cliff). OPAQUE-NEUTRAL by construction:
/// every all-opaque pair has `entry_a == src_a == 255`, so the term is identically 0.
const DIM_WEIGHT: i64 = 2 * ALPHA_WEIGHT_LAB;

/// Visibility-loss penalty for the final remap argmin: how much a candidate palette
/// entry would reduce the SOURCE pixel's visibility. Zero unless the entry is strictly
/// dimmer than the source (and zero when `src_a == 0`, the signal clustering callers use
/// to DISABLE the guard). Flat-additive integer term — no `wa/510` discount, so a dim
/// entry cannot soften its own penalty. Overflow-safe: `≤ DIM_WEIGHT · 255² ≈ 1.95e8`,
/// and `pdist_lab + dim_penalty ≤ ~1.5e9 ≪ i64::MAX`.
#[inline]
fn dim_penalty(src_a: u8, entry_a: u8) -> i64 {
  if src_a > 0 && entry_a < src_a {
    let d = src_a as i64 - entry_a as i64; // 1..=255
    DIM_WEIGHT * d * d
  } else {
    0
  }
}

/// Weight of the smooth anti-vanish penalty [`vanish_penalty`]. CUBIC, so it is highly SELECTIVE:
/// huge at the LARGE drop of a genuine vanish, negligible at the small drop of a translucent remap.
///
/// Sized on PRINCIPLE, not calibrated against one fixture: it must keep an ESSENTIALLY-SOLID source
/// off a clearly-invisible same-hue entry even against the WORST-case hue gap. The worst visible
/// competitor is a full-alpha entry at the gamut's ΔE² diameter [`MAX_DELTA_E76_SQ`] (= 669_160_034,
/// blue↔green), whose `pdist_lab` is at most that value (`wa/510 ≤ 1`, alpha term 0). A same-hue entry
/// at alpha `e` scores `1500·(query_a−e)²` (`pdist` alpha term; colour term 0) `+ 3000·(S−e)²`
/// (`dim_penalty`) `+ W·(S−e)³` (`vanish_penalty`), where `S` is the raw source alpha. The guarantee
/// covers the box `S ≥ 224` (≈88 % — "essentially solid", [`VANISH_GUARD_MIN_SRC`]) and
/// `e ≤ 50` (≤ ~20 % — an unambiguous vanish, [`VANISH_GUARD_MAX_ENTRY`]) for ANY `query_a` (so it
/// survives dither raising `want.a`). The score is linear in `query_a` and minimised at the dither
/// maximum `query_a = 255`, and falls with rising `e` and falling `S`, so the hardest corner is
/// `(S=224, e=50, query_a=255)`: `1500·205² + 3000·174² + W·174³ > 669_160_034` needs `W > 97.8`.
/// `W = 100` clears it (`6.807e8 > 6.692e8`, ~11.5e6 margin), GUARANTEEING (compile-checked just
/// below) that no source ≥88 % opaque lands on a ≤20 %-opacity same-hue entry, whatever the
/// alternative hue, even under dither. Outside that box the smooth penalty still biases toward
/// visibility but the winner is the plain argmin (a soft crossover, not a guarantee). The cube stays
/// tiny for a genuinely TRANSLUCENT remap (drop ~62 scores `100·62³ ≈ 2.4e7`, far below a hue gap), so
/// `pdist_lab` keeps translucent edges on-hue; the closest-hue (red) crossover sits near source alpha
/// ~157, well above the translucent band, so round-7 (`a=100` keeps its hue) holds with margin.
const VANISH_WEIGHT: i64 = 100;

/// Source-alpha FLOOR of the anti-vanish guarantee (≈88 % opacity, "essentially solid"). NOT a code
/// branch — [`nearest_lab`] has no threshold — only the documented, compile-checked SCOPE within which
/// [`VANISH_WEIGHT`] provably keeps a source off a clearly-invisible same-hue entry. Below it the
/// penalty still biases toward visibility, but the winner is the plain argmin and no guarantee is
/// claimed (translucent sources prefer to keep their hue at a lower alpha anyway — see [`vanish_penalty`]).
const VANISH_GUARD_MIN_SRC: i64 = 224;
/// Entry-alpha CEILING of the anti-vanish guarantee (≈20 % opacity): an entry this faint is a clear
/// vanish for an essentially-solid source. Above it the remap is a soft argmin crossover, not a guarantee.
const VANISH_GUARD_MAX_ENTRY: i64 = 50;

/// Compile-time proof that [`VANISH_WEIGHT`] meets the anti-vanish guarantee across its whole scope.
/// WORST CASE (derived): a source at the floor [`VANISH_GUARD_MIN_SRC`] onto a same-hue entry at the
/// ceiling [`VANISH_GUARD_MAX_ENTRY`], with the dither-raised `query_a` at a full 255 (the score is
/// linear in `query_a` and minimised there). Its score `1500·(255−e)² + 3000·(S−e)² + W·(S−e)³` must
/// exceed the worst-hue visible competitor's, whose `pdist_lab` is at most the gamut diameter
/// [`MAX_DELTA_E76_SQ`]. If the weight, the envelope, or the diameter changes so the guarantee fails,
/// the build fails here instead of silently regressing.
const _: () = {
  let s = VANISH_GUARD_MIN_SRC;
  let e = VANISH_GUARD_MAX_ENTRY;
  let dim_score = 1500 * (255 - e).pow(2) + 3000 * (s - e).pow(2) + VANISH_WEIGHT * (s - e).pow(3);
  assert!(dim_score > MAX_DELTA_E76_SQ);
};

/// Smooth final-remap ANTI-VANISH penalty: how strongly to avoid assigning a visible SOURCE pixel to
/// a much-DIMMER entry, as `VANISH_WEIGHT · (src_a − entry_a)³` when the entry is dimmer, else `0`.
///
/// It REPLACES an earlier HARD "visibility floor" exclusion. Every threshold form of that floor
/// (`src_a == 255`; a proportional `2·entry_a < src_a`; a `src_a >= 224` band) created a forced-exclusion
/// CLIFF: a visible source one step below the threshold fell through to a near-invisible entry, or —
/// proportionally — a translucent source was force-flipped onto a brighter WRONG-hue entry. Being a
/// CONTINUOUS score term keyed on the raw SOURCE alpha (`guard_src_alpha`), this has NO hard exclusion
/// threshold: the chosen entry is always the global argmin of `pdist_lab + dim_penalty + vanish_penalty`.
/// (`nearest_lab` is still a discrete argmin, so as source alpha rises the winner can change at a
/// crossover — but at that crossover the two entries are near-equal in SCORE, exactly like an ordinary
/// nearest-neighbour quantizer flipping a pixel that sits between two palette colors; there is no
/// "forbidden better-scoring entry", which is what made the hard floor's cliff a defect rather than
/// just a quantization boundary.)
///
/// WHY it is needed on top of [`dim_penalty`]: an opaque (or near-opaque) pixel that resolves to a
/// near-invisible same-hue entry visually VANISHES, and the gentle quadratic `dim_penalty` cannot
/// always prevent it — in the dither path Floyd–Steinberg can push `want` toward a saturated hue
/// whose colour gap to the only visible alternative out-ranks the quadratic penalty (e.g. a forced
/// 2-colour palette `[green@29, red@255]`: pre-penalty an opaque green maps to `green@29` and
/// disappears). And the gap to overcome depends on the alternative's HUE: a far hue (e.g. only
/// `blue@255` visible, ΔE² up to the [`MAX_DELTA_E76_SQ`] diameter) needs far more push than a near
/// one (`red`). The CUBIC growth, weighted to dominate that worst-case diameter (see
/// [`VANISH_WEIGHT`]), makes this term win wherever the drop is large (the genuine vanish), so the
/// opaque pixel lands on the visible entry whatever its hue — accepting a hue change as the price of
/// staying visible, which for an essentially-solid pixel beats vanishing.
///
/// WHY it does NOT recolour translucent edges: for a genuinely TRANSLUCENT source the drop to a dim
/// same-hue entry is small, so `(src_a − entry_a)³` stays tiny and `pdist_lab` (hue) still decides —
/// the translucent pixel keeps its colour at a lower alpha rather than flipping to a brighter wrong
/// hue. The cube's selectivity is what separates the two regimes without any threshold.
///
/// Keyed on the SOURCE alpha (`guard_src_alpha`), so clustering callers (`guard_src_alpha == 0`)
/// disable it (`0 > entry_a` is false → `0`) and the palette / objective / Wu-split / `quality_score`
/// stay byte-identical. It is also a literal no-op on a fully-opaque image: nothing is dimmer than an
/// `a == 255` source, the term is `0`, and the bundled-photo bytes are preserved. Overflow-safe:
/// `≤ VANISH_WEIGHT · 255³ ≈ 1.66e9`, and `pdist_lab + dim_penalty + vanish_penalty ≲ 2.6e9 ≪ i64::MAX`.
#[inline]
fn vanish_penalty(guard_src_alpha: u8, entry_a: u8) -> i64 {
  if guard_src_alpha > entry_a {
    let drop = guard_src_alpha as i64 - entry_a as i64; // 1..=255
    VANISH_WEIGHT * drop * drop * drop
  } else {
    0
  }
}

/// Tunable parameters for a single quantization run.
///
/// Constructed from the public [`crate::png::PngQuantOptions`] via
/// [`QuantizeConfig::from_options`].
pub struct QuantizeConfig {
  /// Target palette size, clamped to `2..=256`.
  pub max_colors: u16,
  /// Acceptance threshold in `0..=100`; the caller throws below it.
  pub min_quality: u8,
  /// Number of Lloyd/k-means refinement passes.
  pub kmeans_iters: u8,
  /// Whether to apply Floyd-Steinberg dithering on remap.
  pub dither: bool,
  /// Least-significant bits to drop per channel before histogramming.
  pub posterization: u8,
}

impl QuantizeConfig {
  /// Maps the public, JS-facing options onto internal tunables.
  ///
  /// See the module-level mapping table; defaults mirror the previous public
  /// API defaults (min 70 / max 99 / speed 5).
  pub fn from_options(o: &crate::png::PngQuantOptions) -> Self {
    let max_quality = o.max_quality.unwrap_or(99).min(100) as f64;
    // Quadratic ramp: biases toward larger palettes at high quality.
    // 99 -> ~251, 75 -> ~145, 50 -> ~66, 1 -> 2.
    let max_colors = (2.0 + (max_quality / 100.0).powi(2) * 254.0).round();
    let max_colors = (max_colors as i64).clamp(2, MAX_PALETTE as i64) as u16;

    let min_quality = o.min_quality.unwrap_or(70).min(100) as u8;

    let speed = o.speed.unwrap_or(5).clamp(1, 10) as i32;
    // Slower speeds spend more passes refining the palette.
    let kmeans_iters = (10 - speed).max(0) as u8;
    // Only the very fastest speed skips dithering entirely.
    let dither = speed <= 9;

    let posterization = o.posterization.unwrap_or(0).min(7) as u8;

    QuantizeConfig {
      max_colors,
      min_quality,
      kmeans_iters,
      dither,
      posterization,
    }
  }
}

/// Result of quantization: a palette, per-pixel indices into it, and an
/// achieved quality score for the caller's `min_quality` gate.
pub struct QuantizeOutput {
  /// Palette colors, `len() <= max_colors`, sorted deterministically.
  pub palette: Vec<RGBA8>,
  /// One palette index per source pixel; `len() == width * height`.
  pub indices: Vec<u8>,
  /// Achieved quality in `0..=100` (100 only when the result is lossless).
  pub quality: u8,
}

/// Alpha-weighted squared distance between two colors.
///
/// The color (R,G,B) term is scaled by the combined alpha of the two colors so
/// near-transparent pixels do not consume palette slots they cannot show, while
/// the alpha term always counts (weighted by [`ALPHA_WEIGHT`]). For fully-opaque
/// pixels `wa == 510`, so the color term is plain squared-Euclidean distance and
/// the k-means arithmetic-mean update is its exact minimizer. When alpha varies,
/// the per-pair `wa` weighting makes this a perceptual relaxation rather than a
/// strict Lloyd objective — assignment and the unweighted-mean update no longer
/// share one objective, so k-means convergence is approximate (still bounded and
/// deterministic). The same metric is used in median-cut, assignment, and remap.
#[inline]
fn dist2(p: RGBA8, q: RGBA8) -> i64 {
  let wa = p.a as i64 + q.a as i64; // 0..=510
  let dr = p.r as i64 - q.r as i64;
  let dg = p.g as i64 - q.g as i64;
  let db = p.b as i64 - q.b as i64;
  let da = p.a as i64 - q.a as i64;
  let color = (dr * dr + dg * dg + db * db) * wa / 510;
  color + da * da * ALPHA_WEIGHT
}

/// PERCEPTUAL color distance for ASSIGNMENT decisions only (the analogue of
/// [`dist2`], same structure but a CIELAB ΔE color term).
///
/// Mirrors `dist2` exactly except the color term is the squared CIE76 ΔE
/// ([`delta_e76_sq`]) instead of squared RGB Euclidean distance: only the COLOR
/// term gets the `wa/510` visibility weighting (`wa = p.a + q.a ∈ 0..=510`, the
/// same combined-alpha visibility weighting as `dist2`), while the alpha term
/// always counts (weighted by [`ALPHA_WEIGHT_LAB`]). Integer/fixed-point only
/// (`rgb_to_lab` + `delta_e76_sq` are deterministic and float-free), so output
/// stays byte-identical run-to-run and cross-platform.
///
/// This is the CANONICAL, by-value reference form of the perceptual metric. The
/// production hot paths (assignment in [`nearest_lab`] and thus
/// `remap_nearest`/`remap_dither`'s index selection, [`kmeans_objective`], and the
/// k-means++ D² reseed) call the cached [`pdist_lab`] form to avoid recomputing
/// `rgb_to_lab` per query — `pdist_lab` must stay byte-identical to this. NOT used
/// by `quality_score`, the dither residual/activity thresholds, or the Wu split —
/// those stay on `dist2`. Kept `#[cfg(test)]` because production uses the cached
/// form exclusively; the tests pin `pdist_lab == pdist` and the perceptual ordering
/// against this reference.
///
/// OVERFLOW: `de` (`delta_e76_sq`) is `dL²+da²+db²` over components ≤ ~20000 (×100
/// units), so `de ≤ ~1.2e9`; `de * wa` ≤ `1.2e9 * 510 ≈ 6.1e11`, far inside `i64`.
/// The alpha term is `255² · 1500 ≈ 9.75e7`. So `pdist ≤ ~1.3e9` (the `de * wa /
/// 510` divide keeps the color term ≤ ~1.2e9). See [`kmeans_objective`] for the
/// `u128` accumulator bound that this larger metric requires.
#[cfg(test)]
#[inline]
fn pdist(p: RGBA8, q: RGBA8) -> i64 {
  let wa = p.a as i64 + q.a as i64; // 0..=510, same visibility weighting as dist2
  let de = delta_e76_sq(rgb_to_lab(p.r, p.g, p.b), rgb_to_lab(q.r, q.g, q.b));
  let da = p.a as i64 - q.a as i64;
  de * wa / 510 + da * da * ALPHA_WEIGHT_LAB
}

/// Perceptual distance between a query (its precomputed [`Lab`] + alpha) and a
/// palette entry (its precomputed `Lab` + alpha), WITHOUT recomputing either
/// `rgb_to_lab`. This is the hot-loop form of [`pdist`]: the cube-root-heavy
/// `rgb_to_lab` is done ONCE per color by the caller and cached, so the per-pair
/// cost collapses to a cheap `delta_e76_sq` + the alpha term.
///
/// Must stay byte-identical to `pdist(query_rgba, entry_rgba)`: same `wa/510`
/// color weighting, same `ALPHA_WEIGHT_LAB` alpha term.
#[inline]
fn pdist_lab(query_lab: Lab, query_a: u8, entry_lab: Lab, entry_a: u8) -> i64 {
  let wa = query_a as i64 + entry_a as i64; // 0..=510
  let de = delta_e76_sq(query_lab, entry_lab);
  let da = query_a as i64 - entry_a as i64;
  de * wa / 510 + da * da * ALPHA_WEIGHT_LAB
}

/// Precomputes the [`Lab`] of every palette entry so the perceptual hot loops
/// ([`nearest_lab`]) never recompute `rgb_to_lab` per query. Build once where the
/// palette is fixed across many queries (remap, k-means assignment/objective).
#[inline]
fn palette_labs(palette: &[RGBA8]) -> Vec<Lab> {
  palette.iter().map(|p| rgb_to_lab(p.r, p.g, p.b)).collect()
}

/// A distinct color and how many source pixels carry it.
///
/// `count` is `u64` (not `u32`): a single color can cover the entire image, so
/// its population can exceed `u32::MAX` (a >4.3-gigapixel single-color image).
/// Widening here keeps the per-color population from overflowing before the
/// population cap and the `split_reduction` overflow guard can run.
#[derive(Clone, Copy)]
struct ColorCount {
  color: RGBA8,
  count: u64,
}

/// Packs an `RGBA8` into a single `u32` for stable, total ordering.
#[inline]
fn packed(c: RGBA8) -> u32 {
  (c.r as u32) << 24 | (c.g as u32) << 16 | (c.b as u32) << 8 | (c.a as u32)
}

/// Minimal LCG (linear congruential generator) for deterministic D²-weighted
/// sampling inside `kmeans_refine`. Uses Knuth/NR constants; fixed seed gives
/// identical sequences across runs.
struct Lcg(u64);
impl Lcg {
  fn next_u64(&mut self) -> u64 {
    self.0 = self
      .0
      .wrapping_mul(6364136223846793005)
      .wrapping_add(1442695040888963407);
    self.0
  }
}

/// Total-ordering key for the *final* palette that places alpha in the most
/// significant position, so all non-opaque entries cluster at the front of the
/// palette. lodepng writes the `tRNS` chunk as a prefix up to the last
/// non-opaque index, so front-loading transparency yields the shortest possible
/// `tRNS` chunk. Still a total order, so output stays deterministic.
#[inline]
fn alpha_first_key(c: RGBA8) -> u32 {
  (c.a as u32) << 24 | (c.r as u32) << 16 | (c.g as u32) << 8 | (c.b as u32)
}

/// Applies posterization by dropping the `bits` least-significant bits of a
/// channel, exactly as the public API documents. This yields precisely
/// `256 >> bits` evenly-spaced retained levels (e.g. `bits == 4` → 16 levels at
/// 0,16,…,240; `bits == 7` → 2 levels at 0,128). A previous centered-rounding
/// variant produced an off-by-one bucket count (an extra level clamped at 255).
#[inline]
fn posterize_channel(v: u8, bits: u8) -> u8 {
  (v >> bits) << bits
}

#[inline]
fn posterize(c: RGBA8, bits: u8) -> RGBA8 {
  if bits == 0 {
    return c;
  }
  RGBA8 {
    r: posterize_channel(c.r, bits),
    g: posterize_channel(c.g, bits),
    b: posterize_channel(c.b, bits),
    // Alpha is never posterized: doing so would turn fully-transparent (a==0)
    // pixels into faintly-visible ones and break exact transparency.
    a: c.a,
  }
}

/// Canonical histogram/lookup key for a source pixel. Fully-transparent pixels
/// (`a == 0`) collapse to one exact transparent color: their RGB is a
/// "don't care" matte that must not pollute the palette or bleed into visible
/// pixels. Everything else is posterized normally.
#[inline]
fn canonical_key(c: RGBA8, bits: u8) -> RGBA8 {
  if c.a == 0 {
    RGBA8 {
      r: 0,
      g: 0,
      b: 0,
      a: 0,
    }
  } else {
    posterize(c, bits)
  }
}

/// Builds the distinct-color histogram (post-posterization).
///
/// The per-color count is `u64`: a single color may cover every pixel, so its
/// population can exceed `u32::MAX` (a >4.3-gigapixel single-color image would
/// wrap a `u32` counter — silently in release, with a panic in debug — making
/// the dominant color near-zero-weight before the cap/split guards can act).
fn build_histogram(px: &[RGBA8], bits: u8) -> HashMap<RGBA8, u64> {
  let mut hist: HashMap<RGBA8, u64> = HashMap::new();
  for &p in px {
    let key = canonical_key(p, bits);
    *hist.entry(key).or_insert(0) += 1;
  }
  hist
}

/// Unsigned 128×128 → 256-bit product, returned as `(hi, lo)` limbs.
///
/// Schoolbook multiply over four `u64` half-limbs. Used only to compare two
/// exact rationals without overflow (see [`cmp_ratio`]); the products can reach
/// ~187 bits in the Wu split, which exceeds `i128`, so we widen to 256 bits.
#[inline]
fn widen_mul_u128(a: u128, b: u128) -> (u128, u128) {
  let a_lo = a as u64 as u128;
  let a_hi = a >> 64;
  let b_lo = b as u64 as u128;
  let b_hi = b >> 64;
  let ll = a_lo * b_lo;
  let lh = a_lo * b_hi;
  let hl = a_hi * b_lo;
  let hh = a_hi * b_hi;
  let mid = lh.wrapping_add(hl);
  let mid_carry = (lh > mid) as u128; // carry out of lh + hl
  let (lo, c1) = ll.overflowing_add(mid << 64);
  let hi = hh
    .wrapping_add(mid >> 64)
    .wrapping_add(mid_carry << 64)
    .wrapping_add(c1 as u128);
  (hi, lo)
}

/// Exact signed comparison of the rationals `a1/b1` and `a2/b2`.
///
/// Returns [`Ordering`] of `a1/b1` vs `a2/b2`. Denominators `b1`, `b2` must be
/// strictly positive (they always are here: a box's weight is `>= 1`). The
/// cross-products `a1*b2` and `a2*b1` are computed in 256 bits via
/// [`widen_mul_u128`] so the comparison is exact and overflow-free — this is the
/// determinism guarantee for the Wu split (integer-only, no float).
#[inline]
fn cmp_ratio(a1: i128, b1: i128, a2: i128, b2: i128) -> std::cmp::Ordering {
  use std::cmp::Ordering;
  let s1 = a1.signum();
  let s2 = a2.signum();
  if s1 != s2 {
    return s1.cmp(&s2);
  }
  if s1 == 0 {
    return Ordering::Equal; // both numerators zero
  }
  // Same sign: compare magnitudes |a1|*b2 vs |a2|*b1, then flip if both negative.
  let (h1, l1) = widen_mul_u128(a1.unsigned_abs(), b2 as u128);
  let (h2, l2) = widen_mul_u128(a2.unsigned_abs(), b1 as u128);
  let mag = (h1, l1).cmp(&(h2, l2));
  if s1 < 0 { mag.reverse() } else { mag }
}

/// Per-channel moment sums of a set of entries: weight `N`, first moments `S[x]`.
///
/// `N = Σ count`; the RGB first moments are PREMULTIPLIED by alpha
/// (`S[x] = Σ count·x·A/255` for `x ∈ {R,G,B}`, Porter-Duff 1984) while the alpha
/// moment is raw (`S[A] = Σ count·A`). Premultiplying the RGB moments down-weights
/// near-transparent RGB in the Wu split so it APPROXIMATES `dist2`'s `wa/510`
/// visibility weighting — it is NOT an exact match: the squared `term_num` weights
/// equal-alpha RGB variance by ~`(a/255)²` (quadratic) while `dist2` weights it by
/// `a/255` (linear), so the split is a visibility-aware APPROXIMATION, not a strict
/// agreement. Empirically premul lands ~2.7× closer to the dist2-optimal split than
/// raw (alpha-blind) moments, so it is the better merge despite the gap. Opaque
/// pixels (`A == 255`) are unchanged (`x·255/255 == x`), so premul == raw there.
/// The "Term" `A = Σ_x w_x·S[x]²` (with `w = [1,1,1,ALPHA_WEIGHT]`) is the single
/// numerator of `Σ_x w_x·S[x]²/N`, used by the Wu variance-minimizing split. The
/// alpha weight matches `dist2`'s alpha weight. All integer (`i128`) to keep the
/// split decision exact and deterministic.
// P3 (candidate C): exact dist2-consistency needs premultiplying dist2 itself
// (and remap/quality); deferred — see plan.
#[derive(Clone, Copy)]
struct Moments {
  n: i128,
  s: [i128; 4],
}

impl Moments {
  #[inline]
  fn zero() -> Self {
    Moments { n: 0, s: [0; 4] }
  }

  #[inline]
  fn add(&mut self, e: &ColorCount) {
    let w = e.count as i128;
    self.n += w;
    let a = e.color.a as i128;
    // Premultiplied-alpha RGB first moments (Porter-Duff 1984): near-transparent
    // RGB is down-weighted in the Wu split so the split APPROXIMATES dist2's wa/510
    // visibility weighting — premul weights equal-alpha RGB variance by ~(a/255)²
    // (quadratic) vs dist2's linear a/255, so this is a visibility-aware
    // approximation, not an exact match (empirically ~2.7× closer to the
    // dist2-optimal split than raw moments). OPAQUE-NEUTRAL: a==255 -> r*255/255 == r
    // exactly, so opaque images split identically (q75 stays 262053). Premul values
    // <= raw, so the i128 SSE moments are <= the old ones -> overflow strictly safer.
    self.s[0] += w * (e.color.r as i128 * a / 255);
    self.s[1] += w * (e.color.g as i128 * a / 255);
    self.s[2] += w * (e.color.b as i128 * a / 255);
    self.s[3] += w * a;
  }

  #[inline]
  fn sub(&self, other: &Moments) -> Moments {
    Moments {
      n: self.n - other.n,
      s: [
        self.s[0] - other.s[0],
        self.s[1] - other.s[1],
        self.s[2] - other.s[2],
        self.s[3] - other.s[3],
      ],
    }
  }

  /// `A = Σ_x w_x·S[x]²` (with `w = [1,1,1,ALPHA_WEIGHT]`), the single numerator
  /// of this part's `Σ_x w_x·S[x]²/N` term.
  #[inline]
  fn term_num(&self) -> i128 {
    // Wu within-box SSE reduction, alpha-weighted to match `dist2` (ALPHA_WEIGHT,
    // line 25): the split objective must agree with the assignment/remap/quality
    // metric on how much alpha error counts, or small partial-alpha palettes get an
    // alpha-blind split. RGB weights stay 1; alpha is weighted ALPHA_WEIGHT (=3).
    self.s[0] * self.s[0]
      + self.s[1] * self.s[1]
      + self.s[2] * self.s[2]
      + ALPHA_WEIGHT as i128 * (self.s[3] * self.s[3])
  }
}

/// The best variance-minimizing split found for a box: which axis to sort along,
/// the split position (entries `[..pos]` | `[pos..]` in sorted order), and the
/// SSE reduction expressed as the exact rational `red_num / red_den`
/// (`red_den > 0`). `red_num <= 0` means no split reduces SSE.
#[derive(Clone, Copy)]
struct BestSplit {
  axis: usize,
  pos: usize,
  red_num: i128,
  red_den: i128,
}

/// SSE-reduction rational `red_num / red_den` for a candidate split, or `None`
/// when the reduction is non-positive OR forming it in `i128` would overflow.
///
/// `red_num = merit_num·parent_n − parent_num·merit_den` can reach `~2^19·N^4`
/// (`N` = total box weight). The population cap keeps `N` small for concentrated
/// images, but cannot bound `N` when a pathological input has more distinct
/// unit-count colors than the cap (`entries.len() > cap`, i.e. a >100M-pixel
/// image with >67M distinct RGBA values): there `N = entries.len()` and `red_num`
/// would exceed `i128::MAX`. Rather than wrap (release) or panic (debug), we
/// report "no usable split" so the box stops subdividing. For every normal image
/// the products fit `i128` with vast margin, so this is bit-for-bit identical to
/// the previous unchecked formula — the size gate is unchanged.
fn split_reduction(
  merit_num: i128,
  merit_den: i128,
  parent_num: i128,
  parent_n: i128,
) -> Option<(i128, i128)> {
  let red_num = merit_num
    .checked_mul(parent_n)?
    .checked_sub(parent_num.checked_mul(merit_den)?)?;
  if red_num <= 0 {
    return None; // no split reduces SSE (degenerate / single point per axis).
  }
  let red_den = merit_den.checked_mul(parent_n)?;
  Some((red_num, red_den))
}

/// An axis-aligned box of histogram entries used by median-cut. Caches its best
/// Wu split so the box-selection loop is `O(boxes)` per step instead of
/// re-scanning every box.
struct MCBox {
  entries: Vec<ColorCount>,
  /// Smallest `packed` color in the box; a deterministic, box-unique tie-break
  /// for selecting which box to split next (boxes are disjoint entry sets).
  pmin: u32,
  /// Cached best split, or `None` if the box has `< 2` entries or no split
  /// reduces SSE.
  best: Option<BestSplit>,
}

impl MCBox {
  /// Builds a box from entries, computing its `pmin` and best Wu split once.
  fn new(entries: Vec<ColorCount>) -> Self {
    let pmin = entries
      .iter()
      .map(|e| packed(e.color))
      .min()
      .unwrap_or(u32::MAX);
    let best = Self::compute_best_split(&entries);
    MCBox {
      entries,
      pmin,
      best,
    }
  }

  /// Finds the (axis, position) that most reduces total within-box SSE.
  ///
  /// For each axis the entries are stably sorted by `(channel, packed)` and swept
  /// left→right; running left moments give the right side as `total − left`. The
  /// split "merit" is `LeftTerm + RightTerm = A_L/N_L + A_R/N_R`, maximized; the
  /// SSE reduction `merit − ParentTerm` is returned as an exact rational so boxes
  /// can be compared. All comparisons are exact integer ([`cmp_ratio`]); ties
  /// break by lower axis then lower position. Returns `None` when no split with a
  /// positive reduction exists (`< 2` entries, or every candidate has `red <= 0`).
  fn compute_best_split(entries: &[ColorCount]) -> Option<BestSplit> {
    if entries.len() < 2 {
      return None;
    }
    let mut total = Moments::zero();
    for e in entries {
      total.add(e);
    }
    let parent_num = total.term_num();
    let parent_n = total.n;

    // Best merit so far as the rational merit_num / merit_den (both > 0).
    let mut best: Option<(usize, usize, i128, i128)> = None; // (axis, pos, num, den)
    let mut sorted: Vec<ColorCount> = entries.to_vec();
    for axis in 0..4 {
      sorted.sort_by(|x, y| {
        channel_of(x.color, axis)
          .cmp(&channel_of(y.color, axis))
          .then_with(|| packed(x.color).cmp(&packed(y.color)))
      });
      let mut left = Moments::zero();
      for pos in 1..sorted.len() {
        left.add(&sorted[pos - 1]);
        let right = total.sub(&left);
        if right.n <= 0 {
          continue; // shouldn't happen with positive counts, but stay safe.
        }
        // merit = A_L/N_L + A_R/N_R = (A_L*N_R + A_R*N_L) / (N_L*N_R)
        let a_l = left.term_num();
        let a_r = right.term_num();
        let merit_num = a_l * right.n + a_r * left.n;
        let merit_den = left.n * right.n;
        match best {
          Some((_, _, bn, bd)) if cmp_ratio(merit_num, merit_den, bn, bd).is_le() => {}
          _ => best = Some((axis, pos, merit_num, merit_den)),
        }
      }
    }

    let (axis, pos, merit_num, merit_den) = best?;
    // reduction = merit − parent_num/parent_n, over common denom merit_den*parent_n.
    // `split_reduction` forms `red_num`/`red_den` with checked i128 arithmetic and
    // returns `None` on a non-positive reduction OR on overflow, so a pathological
    // box that would overflow simply reports "no usable split" (median_cut skips
    // it) instead of wrapping or panicking.
    let (red_num, red_den) = split_reduction(merit_num, merit_den, parent_num, parent_n)?;
    Some(BestSplit {
      axis,
      pos,
      red_num,
      red_den,
    })
  }

  /// Population-weighted centroid color, rounded to `u8` per channel.
  ///
  /// `true_counts`, when `Some`, maps each member's `packed` color to its TRUE
  /// (pre-cap) population. The Wu split decisions are computed from the capped
  /// `split_entries` (precision/overflow safety), but the final centroid VALUE
  /// must use the TRUE per-color counts under the SAME box membership — capping
  /// is proportional but `cap_entry_weights`'s `.max(1)` floor over-represents a
  /// swarm of rare colors, which can round the centroid to a different `u8`
  /// (true R=51 vs capped R=52) on >67M-px images. Membership is NOT changed: we
  /// only re-weight the members this box already owns (no nearest re-assignment),
  /// so opaque output stays byte-identical. `None` (no cap bit) keeps the capped
  /// counts, which then equal the true counts anyway.
  fn centroid(&self, true_counts: Option<&HashMap<u32, u64>>) -> RGBA8 {
    let mut sr = 0u64;
    let mut sg = 0u64;
    let mut sb = 0u64;
    let mut sa = 0u64;
    let mut n = 0u64;
    for e in &self.entries {
      let c = match true_counts {
        Some(map) => *map.get(&packed(e.color)).unwrap_or(&e.count),
        None => e.count,
      };
      sr += e.color.r as u64 * c;
      sg += e.color.g as u64 * c;
      sb += e.color.b as u64 * c;
      sa += e.color.a as u64 * c;
      n += c;
    }
    if n == 0 {
      return RGBA8::default();
    }
    RGBA8 {
      r: ((sr + n / 2) / n) as u8,
      g: ((sg + n / 2) / n) as u8,
      b: ((sb + n / 2) / n) as u8,
      a: ((sa + n / 2) / n) as u8,
    }
  }
}

#[inline]
fn channel_of(c: RGBA8, ch: usize) -> u8 {
  match ch {
    0 => c.r,
    1 => c.g,
    2 => c.b,
    _ => c.a,
  }
}

/// Wu variance-minimizing median-cut (Xiaolin Wu, *Graphics Gems II*, 1991).
///
/// Keeps the median-cut box-subdivision shape — start with one box of all visible
/// colors, repeatedly split until `max_colors` boxes exist or nothing is
/// splittable — but replaces the split RULE: instead of "widest channel, split at
/// the population median", each box's split is the `(axis, position)` that most
/// reduces total within-box weighted SSE, and the box chosen to split next is the
/// one whose best split reduces SSE the most.
///
/// SSE reduction is evaluated from integer moment sums (`Σw`, `Σw·x`) so the whole
/// decision is exact and deterministic (no float). For a part with weight `N` and
/// first moments `S[x]`, the channel SSE is `Σw·x² − S[x]²/N`; since `Σw·x²` is
/// split-invariant, the reduction depends only on the `S[x]²/N` "terms", compared
/// as exact rationals via [`cmp_ratio`]. The RGB first moments are PREMULTIPLIED
/// by alpha (Porter-Duff, see [`Moments::add`]) so the split's RGB term
/// APPROXIMATES `dist2`'s visibility weighting (premul ≈ `(a/255)²` vs dist2's
/// linear `a/255` — a visibility-aware approximation, ~2.7× closer to optimal than
/// raw moments, not an exact match); opaque pixels (`A == 255`) split identically.
/// Each box caches its best split, so the
/// box-selection scan is `O(boxes)` and only the two new boxes recompute a split.
/// Determinism: axis sort ties break on `packed`; box-selection ties break on the
/// box's smallest `packed` entry; split ties break on lower axis then position.
///
/// `true_counts` (when `Some`) is threaded into [`MCBox::centroid`] so the final
/// per-box centroid VALUE uses the TRUE (pre-cap) per-color populations instead of
/// the capped split copy's counts, while every split DECISION still uses the
/// passed-in (capped) `entries`. `None` when no cap bit, where capped == true.
fn median_cut(
  entries: &[ColorCount],
  max_colors: usize,
  true_counts: Option<&HashMap<u32, u64>>,
) -> Vec<RGBA8> {
  // `entries` are pre-collected and deterministically sorted by the caller, so
  // the seed box order never depends on HashMap iteration order.
  let mut boxes: Vec<MCBox> = vec![MCBox::new(entries.to_vec())];

  while boxes.len() < max_colors {
    // Pick the box whose best split reduces SSE the most (largest red_num/red_den).
    // Tie-break on the box's smallest packed entry so the choice is deterministic;
    // boxes are disjoint, so `pmin` is unique per box.
    let mut target: Option<usize> = None;
    let mut best: Option<BestSplit> = None;
    let mut best_pmin = u32::MAX;
    for (i, b) in boxes.iter().enumerate() {
      let Some(split) = b.best else {
        continue; // unsplittable: < 2 entries or no SSE-reducing split.
      };
      let take = match best {
        None => true,
        Some(cur) => match cmp_ratio(split.red_num, split.red_den, cur.red_num, cur.red_den) {
          std::cmp::Ordering::Greater => true,
          std::cmp::Ordering::Equal => b.pmin < best_pmin,
          std::cmp::Ordering::Less => false,
        },
      };
      if take {
        best = Some(split);
        best_pmin = b.pmin;
        target = Some(i);
      }
    }
    let (Some(idx), Some(split)) = (target, best) else {
      break; // every box is unsplittable; nothing left to do.
    };

    let mut b = boxes.swap_remove(idx);
    // Re-sort along the chosen axis (same order compute_best_split used) and cut
    // at the chosen position, so [..pos] | [pos..] matches the scored split.
    b.entries.sort_by(|x, y| {
      channel_of(x.color, split.axis)
        .cmp(&channel_of(y.color, split.axis))
        .then_with(|| packed(x.color).cmp(&packed(y.color)))
    });
    let right = b.entries.split_off(split.pos);
    let left = b.entries;
    boxes.push(MCBox::new(left));
    boxes.push(MCBox::new(right));
  }

  boxes
    .iter()
    .filter(|b| !b.entries.is_empty())
    .map(|b| b.centroid(true_counts))
    .collect()
}

/// Finds the index of the nearest palette entry to the query (given its
/// precomputed [`Lab`] + alpha) by the PERCEPTUAL [`pdist_lab`] metric, scanning
/// CACHED palette Labs. This is the hot-loop assignment primitive: `rgb_to_lab` is
/// computed ONCE per query by the caller and once per palette entry in
/// `palette_labs`, so the per-call cost is `O(palette · delta_e76_sq)` with no
/// cube roots. Lower index wins ties (`<`), exactly like the old `dist2` `nearest`.
///
/// `guard_src_alpha` is the FINAL-REMAP visibility guard: the raw SOURCE pixel's alpha
/// when this is a real remap of a source pixel (`remap_nearest`, `remap_dither`), or `0`
/// to DISABLE the guard for clustering-context calls (`kmeans_objective`, `kmeans_refine`,
/// the `nearest` wrapper used by the Wu split and `quality_score`). It drives TWO SCORE terms
/// so a visible source pixel cannot be assigned to a much-dimmer entry and VANISH, both added to
/// the `pdist_lab` distance and both keyed on the raw source alpha: the gentle quadratic
/// [`dim_penalty`] (see [`DIM_WEIGHT`]) that biases ANY visible source away from dimmer entries,
/// and the steeper cubic [`vanish_penalty`] (see [`VANISH_WEIGHT`]) that dominates only at the
/// LARGE drop of a genuine vanish (an essentially-solid source onto a near-invisible entry) — so
/// the solid source lands on a visible entry whenever the palette has one, while a genuinely
/// TRANSLUCENT source, whose drop to a dim same-hue entry is small, keeps its HUE instead of being
/// flipped onto a brighter wrong-hue entry. Both are SCORE terms, not exclusions, so there is no
/// hard source-alpha threshold that force-excludes an otherwise-best entry: the winner is always the
/// global argmin, so any change at a crossover is a near-tie between two entries (ordinary
/// nearest-neighbour quantization), not the forced-exclusion cliff every hard-floor form had
/// (`== 255`, proportional, a `>= 224` band; see [`vanish_penalty`]). The guard is kept SEPARATE from `query_a` because in the
/// dither path `query_a` is the dither-adjusted `want.a` (which can drop below the source alpha),
/// whereas the guard must key on the RAW source visibility — exactly like `skip_transparent`. The
/// clustering callers pass `0`, so both terms vanish (`guard_src_alpha > entry_a` and
/// `entry_a < guard_src_alpha` are both false at `0`) and the palette, the keep-best objective, the
/// D² reseed, the Wu split, and `quality_score` are byte-identical; the guard touches ONLY the two
/// final-remap sites. On a fully-opaque image every entry is `a == 255`, nothing is dimmer than an
/// `a == 255` source, both terms are `0`, and output is byte-identical there too.
#[inline]
fn nearest_lab(
  palette_labs: &[Lab],
  palette_alphas: &[u8],
  query_lab: Lab,
  query_a: u8,
  skip_transparent: bool,
  guard_src_alpha: u8,
) -> usize {
  // When `skip_transparent` is set, the reserved fully-transparent palette slot
  // (entry alpha == 0) is excluded from selection. The perceptual metric can rank a
  // saturated color CLOSER to transparent-black than to any real color: `pdist`'s ΔE
  // color term dwarfs the `da²·ALPHA_WEIGHT_LAB` alpha term, and the `wa/510` factor
  // HALVES the color penalty for a transparent comparison (the slot has alpha 0).
  // Without the exclusion a visible pixel could be mapped to the transparent slot and
  // VANISH (e.g. opaque green ranks transparent at 207,749,788 < opaque blue at
  // 669,160,034). RGB `dist2` never needed this — its color and alpha terms share one
  // scale, so the alpha penalty always dominated.
  //
  // `skip_transparent` is the SOURCE pixel's visibility, decided by the CALLER — it is
  // NOT derived from `query_a`. In the dither path `query_a` is the dither-adjusted
  // `want.a`, which can clamp to 0 for a SOURCE-VISIBLE pixel that accumulated negative
  // alpha error; deriving the exclusion from `query_a` would then wrongly re-admit the
  // transparent slot and the visible pixel would vanish. Source `a == 0` pixels are
  // forced onto the transparent slot separately (the override in `quantize_pass`).
  // For opaque images no slot is reserved (no `a == 0` entry exists), so this is a
  // no-op and output stays byte-identical regardless of the flag.
  // TIER 1 — the production path. Exclude ONLY the reserved transparent slot (for visible sources).
  // A visible source is kept off a much-dimmer entry by SCORE — the smooth [`vanish_penalty`] (plus
  // the gentle [`dim_penalty`]) — NOT by a hard exclusion, so the winner is always the global argmin:
  // no forced-exclusion cliff, just the ordinary near-tie a nearest-neighbour quantizer has at any
  // boundary between two palette entries.
  let mut best = usize::MAX;
  let mut best_d = i64::MAX;
  for (i, (&plab, &pa)) in palette_labs.iter().zip(palette_alphas.iter()).enumerate() {
    if skip_transparent && pa == 0 {
      continue;
    }
    let d = pdist_lab(query_lab, query_a, plab, pa)
      + dim_penalty(guard_src_alpha, pa)
      + vanish_penalty(guard_src_alpha, pa);
    if d < best_d {
      best_d = d;
      best = i;
    }
  }
  if best != usize::MAX {
    return best;
  }
  // TIER 2 — fallback for an all-transparent palette (every entry `a == 0`, so Tier 1 skipped them
  // all). Drop the transparent-slot exclusion so the result is always defined (lower index wins ties).
  // With every entry `a == 0` the dimming terms are a constant offset and do not change the argmin.
  // Unreachable in production: `quantize_pass` always clusters visible pixels into ≥ one visible entry.
  let mut best = 0usize;
  let mut best_d = i64::MAX;
  for (i, (&plab, &pa)) in palette_labs.iter().zip(palette_alphas.iter()).enumerate() {
    let d = pdist_lab(query_lab, query_a, plab, pa)
      + dim_penalty(guard_src_alpha, pa)
      + vanish_penalty(guard_src_alpha, pa);
    if d < best_d {
      best_d = d;
      best = i;
    }
  }
  best
}

/// Finds the index of the nearest palette entry to `c` by the PERCEPTUAL metric
/// ([`pdist`]). Thin, COLD wrapper: it recomputes the palette Labs and the query
/// Lab on every call. All production hot loops (remap, k-means
/// assignment/objective/reseed) build the palette `Lab` cache once and call
/// [`nearest_lab`] directly, so this convenience wrapper is `#[cfg(test)]`-only;
/// the tests use it to assert the perceptual nearest choice without threading a
/// cache.
#[cfg(test)]
#[inline]
fn nearest(palette: &[RGBA8], c: RGBA8) -> usize {
  let labs = palette_labs(palette);
  let alphas: Vec<u8> = palette.iter().map(|p| p.a).collect();
  // `nearest` is the clustering/measurement helper (Wu split, `quality_score`). The
  // final-remap visibility guard is DISABLED here (`guard_src_alpha = 0`) so the split
  // and the `min_quality` gate stay byte-identical.
  nearest_lab(&labs, &alphas, rgb_to_lab(c.r, c.g, c.b), c.a, c.a > 0, 0)
}

/// Exact integer k-means objective for `palette` over `entries`, using the
/// PERCEPTUAL assignment metric:
/// `Σ_entries count · pdist(palette[nearest(palette, color)], color)`.
///
/// MUST use the same metric as assignment ([`pdist`]/[`nearest_lab`]): the
/// keep-best guard in [`kmeans_refine`] compares two palettes, and comparing them
/// under a DIFFERENT metric than the one assignment minimizes would let it keep an
/// RGB-better-but-Lab-worse palette. Palette and entry Labs are cached once so the
/// inner scan is cube-root-free.
///
/// Accumulated in `u128` so it is exact and overflow-free. The perceptual metric
/// is larger than the old RGB one — re-derive the bound: `pdist ≤ ~1.3e9` (see
/// [`pdist`]); `count` is a `u64` (`< 2^64`); a histogram has at most one entry per
/// distinct RGBA, `≤ 2^32`. Worst case `Σ count·pdist ≤ 2^32 · 2^64 · 1.3e9 ≈
/// 2^96 · 1.3e9 ≈ 2^127`, which still fits `u128` (`< 2^128`); in practice
/// `Σ count` is the pixel count (`≤ 2^32` for any real image) so the sum is
/// `≤ 2^32 · 1.3e9 ≈ 2^63`, with enormous margin. No other accumulator sums
/// `pdist`; the D² reseed weights (`count · pdist`) are bounded identically and
/// already use `u128`.
fn kmeans_objective(palette: &[RGBA8], entries: &[ColorCount]) -> u128 {
  let labs = palette_labs(palette);
  let alphas: Vec<u8> = palette.iter().map(|p| p.a).collect();
  let mut obj: u128 = 0;
  for e in entries {
    let qlab = rgb_to_lab(e.color.r, e.color.g, e.color.b);
    // Clustering objective: guard disabled (`0`) so the keep-best metric is pure `pdist`.
    let idx = nearest_lab(&labs, &alphas, qlab, e.color.a, e.color.a > 0, 0);
    let d = pdist_lab(qlab, e.color.a, labs[idx], alphas[idx]).max(0) as u128;
    obj += e.count as u128 * d;
  }
  obj
}

/// Lloyd / k-means relaxation over the histogram cells.
///
/// Each pass assigns every distinct color to its nearest palette entry, then
/// recomputes each entry as the population-weighted mean of its assigned cells.
/// Empty clusters are re-seeded with population-weighted D² sampling (k-means++,
/// Arthur & Vassilvitskii 2007): each dead slot is filled by sampling an entry
/// with probability proportional to `count · D²` — its squared residual distance
/// from the nearest current center, scaled by how many pixels carry that color —
/// biasing new seeds toward underrepresented but well-populated gamut regions. A
/// fixed-seed LCG makes the sequence fully deterministic across all runs.
///
/// KEEP-BEST GUARD: the assignment minimizes alpha-weighted `dist2`, but the
/// centroid update is the plain count-weighted RGBA mean, which is NOT that
/// metric's minimizer when alpha varies (the per-pair RGB weight `(a_i+a_c)/510`
/// couples to the center's own alpha). So Lloyd is NOT monotone here: a pass can
/// RAISE the [`kmeans_objective`]. To never return a palette worse than its seed,
/// we snapshot the best palette+objective starting from the INPUT (seed), and
/// after EACH pass (centroid update + any D² reseed) adopt the new palette whenever
/// its objective is `<=` the best seen — DISCARDING only a pass that strictly
/// RAISED the objective; on return `*palette` holds the best. OPAQUE-NEUTRAL: for
/// `a==255` (`wa==510`) `dist2` is plain RGB SSE and the count-weighted mean IS its
/// exact per-assignment minimizer, so every pass is monotone non-increasing and the
/// unguarded code returns the LAST pass — adopting on ties (`<=`) reproduces that
/// byte-for-byte (q75 stays 262053). The compare is exact integer (`u128`), so the
/// decision is fully deterministic — no float, no RNG.
fn kmeans_refine(palette: &mut [RGBA8], entries: &[ColorCount], iters: u8) {
  if palette.is_empty() || entries.is_empty() {
    return;
  }
  let k = palette.len();

  // Cache every entry's Lab ONCE: entries are scanned repeatedly (the assignment
  // pass each iter, plus the D² reseed) and `rgb_to_lab` is the cube-root cost, so
  // computing it per-entry-per-scan would dominate. Alphas are kept alongside for
  // the perceptual `pdist_lab` alpha term.
  let entry_labs: Vec<Lab> = entries
    .iter()
    .map(|e| rgb_to_lab(e.color.r, e.color.g, e.color.b))
    .collect();
  let entry_alphas: Vec<u8> = entries.iter().map(|e| e.color.a).collect();

  // Best-seen palette starts at the seed; the guard never returns worse than this.
  let mut best: Vec<RGBA8> = palette.to_vec();
  let mut best_obj: u128 = kmeans_objective(palette, entries);

  for _ in 0..iters {
    // Cache the palette's Labs for this pass's assignment scan (perceptual
    // nearest), rebuilt each pass because the centroids moved.
    let pal_labs = palette_labs(palette);
    let pal_alphas: Vec<u8> = palette.iter().map(|p| p.a).collect();

    // Accumulators per cluster.
    let mut sr = vec![0u64; k];
    let mut sg = vec![0u64; k];
    let mut sb = vec![0u64; k];
    let mut sa = vec![0u64; k];
    let mut wn = vec![0u64; k];

    for (ei, e) in entries.iter().enumerate() {
      let idx = nearest_lab(
        &pal_labs,
        &pal_alphas,
        entry_labs[ei],
        entry_alphas[ei],
        entry_alphas[ei] > 0,
        // Clustering assignment: guard disabled so centroids/palette stay byte-identical.
        0,
      );
      let c = e.count;
      sr[idx] += e.color.r as u64 * c;
      sg[idx] += e.color.g as u64 * c;
      sb[idx] += e.color.b as u64 * c;
      sa[idx] += e.color.a as u64 * c;
      wn[idx] += c;
    }

    // Recompute centroids; collect empty clusters for re-seeding.
    let mut empty: Vec<usize> = Vec::new();
    for i in 0..k {
      if wn[i] == 0 {
        empty.push(i);
        continue;
      }
      let n = wn[i];
      palette[i] = RGBA8 {
        r: ((sr[i] + n / 2) / n) as u8,
        g: ((sg[i] + n / 2) / n) as u8,
        b: ((sb[i] + n / 2) / n) as u8,
        a: ((sa[i] + n / 2) / n) as u8,
      };
    }

    // Re-seed empty clusters with population-weighted D² sampling (count · D²;
    // k-means++, Arthur & Vassilvitskii 2007). A fixed-seed LCG makes the
    // sequence deterministic across all runs. Multiple dead clusters in one pass
    // get proper D² incremental updates so they spread across the gamut.
    if !empty.is_empty() {
      // The centroids moved in place above, so the assignment-pass `pal_labs` is
      // stale. Rebuild the live centers' Labs for the perceptual D² baseline.
      let live_labs = palette_labs(palette);
      // Initial PERCEPTUAL squared residuals: each entry's `pdist` to its NEAREST
      // CURRENT LIVE center. k-means++ requires D² against the nearest center as
      // it stands NOW (after the in-place centroid recompute), not the center of
      // the cluster the entry was assigned to before that cluster moved. We scan
      // only live clusters (`wn[i] > 0`); dead slots hold stale colors and must
      // not seed their own D². The D² metric MUST match assignment (`pdist`), or
      // the reseed would bias by an inconsistent (RGB) residual. This runs only
      // inside `if !empty.is_empty()` (reseed events are rare) and is O(entries·k)
      // with k ≤ 256 — acceptable. `min_d2` is `u64`: `pdist ≤ ~1.3e9` fits easily.
      let mut min_d2: Vec<u64> = (0..entries.len())
        .map(|ej| {
          let mut best = u64::MAX;
          for i in 0..k {
            if wn[i] > 0 {
              let d = pdist_lab(entry_labs[ej], entry_alphas[ej], live_labs[i], palette[i].a).max(0)
                as u64;
              if d < best {
                best = d;
              }
            }
          }
          // `entries` is non-empty here, so >= 1 cluster is live and `best` is
          // set; the fallback keeps it defined if that ever changes.
          if best == u64::MAX { 0 } else { best }
        })
        .collect();

      let mut lcg = Lcg(0x9E3779B97F4A7C15);

      for &dead in &empty {
        // k-means++ weight is population-weighted (count · D²): each entry is a
        // histogram cell of `count` pixels, so a 10000-pixel color must be far
        // likelier to seed a dead cluster than a 1-pixel outlier at the same
        // residual. Weighting by D² alone would spend empty clusters on rare
        // colors and starve high-population gamut regions.
        let total: u128 = min_d2
          .iter()
          .zip(entries.iter())
          .map(|(&d, e)| e.count as u128 * d as u128)
          .sum();

        let picked_idx = if total == 0 {
          // Every entry already coincides with a center (all D² == 0; counts
          // are >= 1 so this triggers iff every residual is 0): deterministic
          // fallback — pick the entry with the smallest packed color.
          entries
            .iter()
            .enumerate()
            .min_by_key(|(_, e)| packed(e.color))
            .map(|(i, _)| i)
            .unwrap_or(0)
        } else {
          let r = lcg.next_u64() as u128 % total;
          let mut cum = 0u128;
          let mut s = 0usize;
          for (i, &d) in min_d2.iter().enumerate() {
            cum += entries[i].count as u128 * d as u128;
            if cum > r {
              s = i;
              break;
            }
          }
          s
        };

        palette[dead] = entries[picked_idx].color;

        // Incremental PERCEPTUAL D² update: reduce each entry's min_d2 if the
        // just-placed center (an entry color, so its Lab is `entry_labs[picked]`)
        // is nearer under `pdist` — the same metric as the baseline above.
        let picked_lab = entry_labs[picked_idx];
        let picked_a = entry_alphas[picked_idx];
        for ej in 0..entries.len() {
          let new_d =
            pdist_lab(picked_lab, picked_a, entry_labs[ej], entry_alphas[ej]).max(0) as u64;
          if new_d < min_d2[ej] {
            min_d2[ej] = new_d;
          }
        }
      }
    }

    // Keep-best: this pass mutated `palette` in place. Adopt it whenever its
    // objective is <= the best seen; only DISCARD a pass that strictly RAISED the
    // objective (the non-monotone case the guard exists to catch — alpha-varying
    // assignment vs the count-weighted-mean update), keeping the prior best then.
    // `<=` (adopt on ties, keeping the LATEST equal-objective palette) is what
    // preserves OPAQUE-NEUTRALITY: for `a==255` every pass is monotone
    // non-increasing, so the original (unguarded) code returns the LAST pass's
    // palette — `<=` reproduces that byte-for-byte (a strict `<` would instead
    // freeze on the FIRST palette to hit the minimum and could diverge whenever a
    // tied-objective pass still reshapes the palette, e.g. an empty-slot reseed
    // that lowers no objective but fills a dead slot). The compare is exact integer
    // (`u128`), so `<=` stays fully deterministic — no float, no RNG in the decision.
    let obj = kmeans_objective(palette, entries);
    if obj <= best_obj {
      best_obj = obj;
      best.copy_from_slice(palette);
    }
  }

  // Return the best-seen palette (never worse than the seed by construction).
  palette.copy_from_slice(&best);
}

/// Nearest-color remap with a per-color memoization cache (no dithering).
fn remap_nearest(px: &[RGBA8], palette: &[RGBA8], bits: u8) -> Vec<u8> {
  // Palette Labs cached once; the per-color cache only computes `rgb_to_lab` for
  // each DISTINCT canonical key once (perceptual assignment via `nearest_lab`).
  let pal_labs = palette_labs(palette);
  let pal_alphas: Vec<u8> = palette.iter().map(|p| p.a).collect();
  let mut cache: HashMap<RGBA8, u8> = HashMap::new();
  let mut indices = Vec::with_capacity(px.len());
  for &p in px {
    let key = canonical_key(p, bits);
    let idx = *cache.entry(key).or_insert_with(|| {
      nearest_lab(
        &pal_labs,
        &pal_alphas,
        rgb_to_lab(key.r, key.g, key.b),
        key.a,
        // `key` IS the (posterized) source pixel here, so its alpha is the source
        // visibility; a visible source key must not resolve to the transparent slot.
        key.a > 0,
        // FINAL REMAP: enable the visibility guard keyed on the source pixel's alpha so
        // a visible pixel cannot be assigned to a much-dimmer entry and vanish. `key.a`
        // is the (posterized) source alpha; posterize never touches alpha, so it equals
        // the raw source alpha.
        key.a,
      ) as u8
    });
    indices.push(idx);
  }
  indices
}

// ---------------------------------------------------------------------------
// Selective error-diffusion dithering tunables.
//
// Strategy: diffuse error only where it earns its bytes. Two orthogonal,
// per-pixel signals decide a strength in `[0, 1]`:
//
//   * RESIDUAL (primary, self-correcting): the post-quantization squared
//     distance `dist2(want, chosen)` — how badly the palette reproduced the
//     target we actually wanted. Small => clean map (a flat the palette
//     covers) => diffuse nothing => long DEFLATE runs. Large => the palette is
//     about to BAND a gradient => diffuse to smooth it. This is the banding
//     signal itself, so it re-engages dither exactly on pixels that would band.
//
//   * SOURCE ACTIVITY (secondary, suppressor): a direction-independent local
//     gradient of the ORIGINAL source over the symmetric 4-neighborhood. High
//     activity = a hard EDGE or busy TEXTURE, where FS speckle is perceptually
//     invisible but incompressible; we suppress dither there. Low activity =
//     smooth, where banding is visible; we let the residual ramp govern.
//
// Thresholds are in `dist2` units (alpha-weighted squared distance). For a
// fully-opaque pair the color term is plain squared-Euclidean RGB distance, so
// an N-per-channel error on one channel is `N*N`; an even N across R,G,B is
// `3*N*N`. NOTE: the default operating point posterizes with `bits == 0` (a
// no-op), so source activity is driven by genuine photo signal, not by
// posterizer steps — these defaults are sized for raw 8-bit gradients and MUST
// be swept on the real image.
// ---------------------------------------------------------------------------

/// Residual `dist2` below which a pixel mapped cleanly: diffuse nothing, keep
/// flats/edges byte-flat for DEFLATE. ~one 8-bit level on one channel == 64.
/// Sweep: 24.0 ..= 256.0 (higher => flatter/smaller, more banding risk).
const DITHER_RESID_LO: f32 = 0.0;

/// Residual `dist2` at/above which dither runs at full configured strength
/// (gradients the palette cannot follow). Between LO and HI strength ramps
/// linearly. Must satisfy `DITHER_RESID_HI > DITHER_RESID_LO`.
/// Sweep: 256.0 ..= 1200.0 (lower => gradients dither sooner => smoother/bigger).
const DITHER_RESID_HI: f32 = 128.0;

/// Global ceiling on dither strength in `[0, 1]`. Damps all dither energy
/// without retuning the ramp. 1.0 == full FS at maximally-bad pixels.
/// Sweep: 0.55 ..= 1.0 (lower => smaller, more banding risk).
const DITHER_MAX_STRENGTH: f32 = 0.9;

/// Source-activity `dist2` at/above which a pixel is treated as a hard EDGE /
/// busy texture and dither is fully suppressed (strength -> ACT_FLOOR). Below
/// it, the suppressor ramps from no suppression (at 0) to full (here). Measured
/// as the MAX `dist2` over the 4-neighborhood of the posterized source, so a
/// single strong edge fully activates the pixel. ~16-per-channel one-channel
/// step == 256; a multi-channel photo edge is larger.
/// Sweep: 256.0 ..= 4096.0 (lower => suppress dither on more texture => smaller;
/// higher => dither into more textured regions => bigger/safer).
const DITHER_EDGE_ACT: f32 = 2048.0;

/// Strength multiplier retained in maximally-active (edge) regions, in `[0, 1]`.
/// 0.0 == hard edge-stop, but that fully zeroes a smooth HIGH-residual pixel
/// whenever a SINGLE neighbor is a hard edge (activity is the MAX 4-neighbor
/// delta): such a pixel still consumes inbound error at index selection yet
/// propagates none, leaving a 1-px nearest-mapped (solid) column hugging edges
/// inside otherwise-smooth gradients (the "edge halo"). A small positive floor
/// keeps `>= MAX*FLOOR` dither on edge-adjacent high-residual pixels so error
/// feedback survives the transition and the halo never forms; 0.2 is the
/// smallest swept value that breaks the halo while costing ~435 B vs 0.0
/// (the edge gate itself still earns ~3.9 KB vs disabling it at FLOOR=1.0).
/// Sweep: 0.0 ..= 0.5 (higher => more edge dither => bigger/safer).
const DITHER_EDGE_FLOOR: f32 = 0.2;

/// Per-channel HARD dead-zone (0..255 units) applied to residual BEFORE
/// diffusion: magnitudes below this are dropped to exactly 0, magnitudes above
/// pass through UNCHANGED (no soft shrink => no systematic energy loss / color
/// drift). Kills sub-threshold jitter that random-walks into streaks across a
/// nominally-flat run. 0.0 disables.
/// Sweep: 0.0 ..= 6.0.
const DITHER_ERR_DEADZONE: f32 = 2.0;

/// Symmetric clamp (0..255 units) on the ACCUMULATED per-channel error pulled
/// from the diffusion buffer, applied right before quantizing (Ulichney 1987
/// streak guard). Bounds worst-case incompressible bursts and color drift on
/// out-of-gamut patches without touching normal single-step gradient dither.
/// 192 is the minimum near-inert value: it is >= ceil(255/2) = 128, so for ANY
/// palette gap G <= 255 the inbound clamp can still push `want` clear across the
/// decision threshold (dead-band width `max(0, ceil(G/2) - CLAMP)` is 0). Below
/// 128 (e.g. the old 96) a flat near-endpoint tone gets pinned on one side of
/// the threshold and the field COLLAPSES to a flat endpoint instead of
/// dithering. So 192 never clips a single-step endpoint gradient yet still
/// guards against pathological accumulation; 255 would disable the guard
/// entirely (and pass values like 200 straight through).
/// Sweep: 24.0 ..= 255.0 (255 effectively disables it).
const DITHER_ERR_CLAMP: f32 = 192.0;

/// Per-channel HARD dead-zone on residual: drop below threshold, pass through
/// unchanged above. Deterministic pure-f32; identity when the const is 0.0.
#[inline]
fn dither_deadzone(v: f32) -> f32 {
  if DITHER_ERR_DEADZONE <= 0.0 {
    return v;
  }
  if v.abs() < DITHER_ERR_DEADZONE {
    0.0
  } else {
    v
  }
}

/// Symmetric clamp of an accumulated-error value to `[-DITHER_ERR_CLAMP, +..]`.
/// Deterministic; NaN-free for finite inputs (all inputs are bounded f32 sums).
#[inline]
fn dither_clamp_err(v: f32) -> f32 {
  v.clamp(-DITHER_ERR_CLAMP, DITHER_ERR_CLAMP)
}

/// Direction-independent local source activity at `(x, y)`: the MAX `dist2`
/// between the posterized source pixel here and its 4-neighborhood (left, right,
/// up, down) in the ORIGINAL source. Reads ONLY `px[]` (posterized on the fly),
/// never the evolving error rows, so the value is identical for L2R and R2L scan
/// and byte-identical across runs. A fully-transparent neighbor (`a == 0`) is
/// treated as the canonical `(0, 0, 0, 0)`: its don't-care matte RGB is ignored,
/// but the alpha discontinuity at a sprite/cutout silhouette is a real, VISIBLE
/// hard edge — so it must register as one (driving activity high and suppressing
/// dither toward `DITHER_EDGE_FLOOR` at the boundary), exactly like a spatial
/// color edge. Visible neighbors (`a > 0`, including partial alpha) use their
/// real posterized color, so soft anti-aliased edges still get proportional
/// treatment. O(1) per pixel.
#[inline]
fn dither_source_activity(
  px: &[RGBA8],
  width: usize,
  height: usize,
  x: usize,
  y: usize,
  here: RGBA8,
  bits: u8,
) -> f32 {
  let mut act: i64 = 0;
  let consider = |nx: usize, ny: usize, act: &mut i64| {
    let n = px[ny * width + nx];
    // A fully-transparent neighbor marks an ALPHA EDGE (sprite/cutout
    // silhouette), not a smooth interior. Treat it as canonical (0,0,0,0): its
    // don't-care matte RGB is ignored, but the alpha discontinuity drives
    // activity high so dither is suppressed toward DITHER_EDGE_FLOOR at the
    // boundary (no speckled cutout edge). Visible neighbors use their
    // posterized color.
    let nc = if n.a == 0 {
      RGBA8 {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
      }
    } else {
      posterize(n, bits)
    };
    let d = dist2(here, nc);
    if d > *act {
      *act = d;
    }
  };
  if x > 0 {
    consider(x - 1, y, &mut act);
  }
  if x + 1 < width {
    consider(x + 1, y, &mut act);
  }
  if y > 0 {
    consider(x, y - 1, &mut act);
  }
  if y + 1 < height {
    consider(x, y + 1, &mut act);
  }
  act as f32
}

/// Maps a post-quantization residual `dist2` and a source-activity `dist2` to a
/// dither strength in `[0, DITHER_MAX_STRENGTH]`.
///
/// `ramp`  : residual LO..HI -> 0..1 (the self-correcting banding signal).
/// `edge`  : activity 0..EDGE_ACT -> 1..EDGE_FLOOR (edge/texture suppressor).
/// strength = ramp * edge * MAX_STRENGTH.
///
/// Pure f32 over integer-derived `dist2` inputs; no RNG, no f64 in control flow,
/// no map iteration. Identical inputs -> identical bits on every run/process.
#[inline]
fn dither_strength(resid: f32, activity: f32) -> f32 {
  // Residual ramp: clean map -> 0, banding map -> 1.
  let span = DITHER_RESID_HI - DITHER_RESID_LO;
  let ramp = if span <= 0.0 {
    if resid >= DITHER_RESID_HI { 1.0 } else { 0.0 }
  } else {
    ((resid - DITHER_RESID_LO) / span).clamp(0.0, 1.0)
  };
  // Edge suppressor: flat source -> 1.0, hard edge -> DITHER_EDGE_FLOOR.
  let edge = if DITHER_EDGE_ACT <= 0.0 {
    DITHER_EDGE_FLOOR
  } else {
    let t = (activity / DITHER_EDGE_ACT).clamp(0.0, 1.0);
    1.0 - (1.0 - DITHER_EDGE_FLOOR) * t
  };
  (ramp * edge * DITHER_MAX_STRENGTH).clamp(0.0, 1.0)
}

/// Floyd-Steinberg error-diffusion remap with serpentine scanning and selective,
/// residual-aware, edge-gated per-pixel dither strength.
///
/// Standard 7/3/5/1 kernel over `f32` RGBA error rows; alpha is dithered
/// alongside color. Serpentine (boustrophedon) scanning alternates row direction
/// to avoid directional artifacts. Nearest-entry selection uses [`dist2`] on the
/// FULL accumulated inbound error (only the OUTBOUND diffused error is scaled by
/// strength), so upstream gradient energy is always honored at index selection
/// and no bright/dark seam forms where strength drops.
///
/// Selective dithering: pixels that map cleanly (small residual) diffuse little
/// or nothing — keeping flats and edges byte-flat so DEFLATE sees long runs —
/// while pixels with large residual in smooth source regions diffuse to smooth
/// gradients that would otherwise band. Hard edges / busy texture are suppressed
/// (their detail masks any +-1 noise). A hard per-channel dead-zone and a
/// symmetric accumulated-error clamp prevent sub-threshold jitter and
/// single-pixel streaks. Determinism is preserved: the strength signal is pure
/// f32 over integer-`dist2` inputs, and source activity reads only the
/// direction-independent posterized source.
fn remap_dither(px: &[RGBA8], width: usize, height: usize, palette: &[RGBA8], bits: u8) -> Vec<u8> {
  let mut indices = vec![0u8; px.len()];
  if width == 0 || height == 0 {
    return indices;
  }
  // Cache the palette Labs ONCE: the palette is fixed across every pixel, so
  // `rgb_to_lab` runs `palette.len()` times here, not once per pixel. The per-pixel
  // query Lab (for `want`) is still computed per pixel — `want` varies — but that is
  // a single `rgb_to_lab` per visible pixel (the residual stays on RGB `dist2`).
  let pal_labs = palette_labs(palette);
  let pal_alphas: Vec<u8> = palette.iter().map(|p| p.a).collect();
  // Two error rows of (r, g, b, a) deltas in f32 for sub-unit accumulation.
  let mut cur = vec![[0f32; 4]; width];
  let mut next = vec![[0f32; 4]; width];

  for y in 0..height {
    for slot in next.iter_mut() {
      *slot = [0.0; 4];
    }
    let l2r = y % 2 == 0;
    for step in 0..width {
      let x = if l2r { step } else { width - 1 - step };
      let base = y * width + x;
      // Fully-transparent pixels carry a "don't care" matte RGB. Skip them: the
      // transparent-slot override in quantize_pass assigns their index, and
      // diffusing their matte error would tint visible neighbors.
      if px[base].a == 0 {
        continue;
      }
      let src = posterize(px[base], bits);
      // Apply accumulated inbound error, clamped per-channel (Ulichney streak
      // guard) so a single bad pixel can't poison the rest of the run. Index
      // selection always uses this FULL inbound error.
      let e = cur[x];
      let want = RGBA8 {
        r: clamp_u8(src.r as f32 + dither_clamp_err(e[0])),
        g: clamp_u8(src.g as f32 + dither_clamp_err(e[1])),
        b: clamp_u8(src.b as f32 + dither_clamp_err(e[2])),
        a: clamp_u8(src.a as f32 + dither_clamp_err(e[3])),
      };
      // PERCEPTUAL index selection (cached palette Labs; one query `rgb_to_lab`). The
      // exclusion of the transparent slot keys on the RAW SOURCE pixel's visibility
      // (`px[base].a > 0`), NOT on the dither-adjusted `want.a`: a visible source pixel
      // can accumulate negative alpha error so `want.a` clamps to 0, and keying on that
      // would let the pixel vanish into the transparent slot. We key on the same raw
      // source alpha the quantize_pass override uses (`p.a == 0`), so the two agree
      // exactly. Transparent source pixels (`px[base].a == 0`) never reach here — they
      // are `continue`d above — so this flag is always `true` in this call; passing it
      // explicitly documents the invariant and removes any dependence on alpha never
      // being posterized.
      let idx = nearest_lab(
        &pal_labs,
        &pal_alphas,
        rgb_to_lab(want.r, want.g, want.b),
        want.a,
        px[base].a > 0,
        // FINAL REMAP visibility guard, keyed on the RAW source alpha `px[base].a` — NOT
        // the dither-adjusted `want.a`, which can drop below the source: a visible source
        // pixel must not vanish onto a much-dimmer entry even when its `want.a` clamped low.
        px[base].a,
      );
      indices[base] = idx as u8;
      let chosen = palette[idx];

      // Post-quantization residual: how far the chosen entry is from the target
      // we wanted. Small => clean map (keep flat); large => band risk. Stays on
      // RGB `dist2`: the 8 dither consts are calibrated in `dist2` units and the
      // residual is the RGB reproduction-error / banding signal.
      let resid = dist2(want, chosen) as f32;
      // Direction-independent local source activity (4-neighborhood of px[]).
      let activity = dither_source_activity(px, width, height, x, y, src, bits);
      let strength = dither_strength(resid, activity);

      // Outbound error to diffuse: hard dead-zone, then scale by strength. The
      // dead-zone drops sub-threshold jitter; the scale damps where the map is
      // clean / on edges.
      //
      // The THREE color channels are additionally scaled by the source pixel's
      // visibility `vis = src.a / 255` (premultiplied alpha, Porter-Duff 1984):
      // a pixel's visible color contribution is color*alpha, so a near-transparent
      // source must not inject its (invisible) matte RGB into neighbors — including
      // fully-opaque ones. The ALPHA channel is visibility itself and stays
      // UNSCALED. For opaque sources (`src.a == 255`) `vis == 1.0` exactly, so the
      // outbound error — and thus every byte of output — is unchanged.
      let vis = src.a as f32 / 255.0;
      let err = [
        dither_deadzone(want.r as f32 - chosen.r as f32) * strength * vis,
        dither_deadzone(want.g as f32 - chosen.g as f32) * strength * vis,
        dither_deadzone(want.b as f32 - chosen.b as f32) * strength * vis,
        dither_deadzone(want.a as f32 - chosen.a as f32) * strength,
      ];

      // Skip diffusion entirely when nothing would propagate. Keeps both error
      // rows at exactly 0.0 across flat runs (byte-clean flats, no float drift
      // re-triggering near-threshold dither downstream).
      if err[0] != 0.0 || err[1] != 0.0 || err[2] != 0.0 || err[3] != 0.0 {
        // Serpentine neighbor offsets (forward = scan direction).
        let fwd: isize = if l2r { 1 } else { -1 };
        diffuse(&mut cur, x as isize + fwd, 7.0 / 16.0, &err);
        diffuse(&mut next, x as isize - fwd, 3.0 / 16.0, &err);
        diffuse(&mut next, x as isize, 5.0 / 16.0, &err);
        diffuse(&mut next, x as isize + fwd, 1.0 / 16.0, &err);
      }
    }
    std::mem::swap(&mut cur, &mut next);
  }
  indices
}

#[inline]
fn clamp_u8(v: f32) -> u8 {
  v.round().clamp(0.0, 255.0) as u8
}

#[inline]
fn diffuse(row: &mut [[f32; 4]], x: isize, factor: f32, err: &[f32; 4]) {
  if x < 0 || x as usize >= row.len() {
    return;
  }
  let cell = &mut row[x as usize];
  for k in 0..4 {
    cell[k] += err[k] * factor;
  }
}

/// Computes achieved quality in `0..=100` from the alpha-weighted RMSE between
/// the quantizer's *input* pixels and their remapped palette colors.
///
/// Quality measures how faithfully the palette reproduces the image the
/// quantizer was actually asked to represent — i.e. the **posterized** pixels.
/// Posterization is an explicit, user-requested lossy reduction (a no-op when
/// `bits == 0`), so its error is deliberately excluded from the gate: a result
/// that exactly reproduces the posterized image scores 100, and any *further*
/// quantization error drives the score below 100. The mapping is a
/// monotonically-decreasing heuristic for the `min_quality` gate, not a match to
/// any external metric.
///
/// Only *visible* (`a > 0`) pixels are scored. Fully-transparent pixels map to
/// the exact transparent slot (zero error) but are arbitrarily numerous in a
/// sprite/icon with a large transparent canvas; counting them in the denominator
/// would dilute the visible-region error and let a badly-quantized icon pass the
/// gate. An all-transparent image is lossless, so it scores 100.
fn quality_score(px: &[RGBA8], bits: u8, palette: &[RGBA8], indices: &[u8]) -> u8 {
  let mut sum_err = 0f64;
  let mut n = 0u64;
  // Exact-lossless flag: stays true only while every visible pixel's chosen
  // palette color is BYTE-IDENTICAL to its reference. The accumulated `dist2`
  // alone cannot decide losslessness because `dist2` truncates the
  // alpha-weighted RGB term with integer division (`* wa / 510`): for a < 255 a
  // 1-LSB near-opaque RGB difference floors to 0, so a genuinely lossy remap can
  // accumulate zero error and would otherwise report 100. Equality here is exact
  // (no float, no `dist2`), so the score==100 verdict means true byte-identity.
  let mut lossless = true;
  for (i, &p) in px.iter().enumerate() {
    if p.a == 0 {
      continue;
    }
    // Compare against the posterized reference, not the raw source, so the
    // user's explicit posterization is never counted as quantizer error. With
    // `bits == 0` (the default operating point) the reference IS the raw source,
    // and the fast path returns 100 exactly when the output reproduces this same
    // posterized reference byte-for-byte, so 100 keeps a single consistent
    // meaning: "byte-identical to the (posterized) input on every visible pixel".
    let reference = posterize(p, bits);
    let q = palette[indices[i] as usize];
    if q != reference {
      lossless = false;
    }
    sum_err += dist2(reference, q) as f64;
    n += 1;
  }
  if n == 0 {
    return 100;
  }
  let mse = sum_err / n as f64;
  // Only declare a perfect 100 when the output is byte-identical to the
  // reference on every visible pixel. `mse <= 0.0` is necessary but NOT
  // sufficient: `dist2`'s `* wa / 510` truncation can zero out a real
  // near-opaque difference. The exact `lossless` flag closes that gap so the
  // `min_quality == 100` lossless gate rejects any lossy visible remap.
  if mse <= 0.0 && lossless {
    return 100;
  }
  let rmse = mse.sqrt();
  // Denominator calibrated empirically against the bundled photographic test
  // image: an ordinary default 256-color reduction lands ~90 (comfortably above
  // the default min_quality of 70), while heavier reductions fall off smoothly.
  // The `.min(99.0)` below guarantees a lossy result can never report 100, so a
  // `min_quality` of 100 always rejects any lossy output.
  let score = 100.0 / (1.0 + rmse / 64.0);
  let score = score.min(99.0);
  score.round().clamp(0.0, 99.0) as u8
}

/// Sorts the palette by packed RGBA and rewrites indices to match, so output
/// bytes are identical across runs regardless of `HashMap` iteration order.
fn canonicalize(palette: Vec<RGBA8>, indices: &mut [u8]) -> Vec<RGBA8> {
  let n = palette.len();
  let mut order: Vec<usize> = (0..n).collect();
  order.sort_by_key(|&i| alpha_first_key(palette[i]));
  // old index -> new index
  let mut remap = vec![0u8; n];
  let mut sorted = Vec::with_capacity(n);
  for (new_i, &old_i) in order.iter().enumerate() {
    remap[old_i] = new_i as u8;
    sorted.push(palette[old_i]);
  }
  for idx in indices.iter_mut() {
    *idx = remap[*idx as usize];
  }
  sorted
}

/// Proportionally pre-scales the histogram entry weights down to roughly `cap`
/// total, so Wu's exact-integer SSE moments stay precise on huge images.
///
/// `compute_best_split` forms the reduction numerator `red_num ~ 2^19 · N^4`
/// (`N = Σ count`, the total weight) directly in `i128` — the i256 widening in
/// [`cmp_ratio`] only protects the cross-products inside `cmp_ratio`, not these
/// operands. This cap is NOT the overflow guarantee: it cannot bound `N` for a
/// pathological input whose distinct-color count already exceeds `cap` (the
/// `.max(1)` floor keeps every entry present, so the post-cap total is
/// `max(cap, entries.len())`, not `~cap`). The HARD overflow guard lives in
/// [`split_reduction`], whose checked arithmetic returns `None` on overflow and
/// needs no entry-count assumption.
///
/// What the cap buys: for a huge but CONCENTRATED-weight image (few distinct
/// colors, each carrying enormous counts) it scales `N` down proportionally so
/// the split decision is computed in a precise integer regime instead of forcing
/// every box's `red_num` into [`split_reduction`]'s no-split overflow fallback.
/// Scaling is proportional, so the variance-minimizing decision is preserved up
/// to integer rounding; it only triggers for >~67M-weight inputs, and for any
/// normal image it is a no-op.
fn cap_entry_weights(entries: &mut [ColorCount], cap: u128) {
  let total: u128 = entries.iter().map(|e| e.count as u128).sum();
  if total > cap {
    for e in entries.iter_mut() {
      // The `.max(1)` keeps every visible color present (weight >= 1); it can
      // push the post-scaling total slightly above `cap` (by at most `len`),
      // which the bound above already accounts for.
      e.count = ((e.count as u128 * cap / total).max(1)) as u64;
    }
  }
}

/// Shared, pass-invariant inputs for [`quantize_pass`].
struct PassInput<'a> {
  px: &'a [RGBA8],
  width: usize,
  height: usize,
  hist: &'a HashMap<RGBA8, u64>,
  has_transparent: bool,
  bits: u8,
}

/// Runs the full quantization pipeline for a fixed `max_colors`, returning the
/// palette, indices, and achieved quality. Internal helper; the public entry
/// point [`quantize_rgba`] wraps this with the retry policy.
fn quantize_pass(
  input: &PassInput,
  max_colors: usize,
  kmeans_iters: u8,
  dither: bool,
) -> QuantizeOutput {
  let PassInput {
    px,
    width,
    height,
    hist,
    has_transparent,
    bits,
  } = *input;
  // Reserve one exact fully-transparent slot if needed so transparency stays
  // lossless. We hand median-cut one fewer slot and prepend the slot after.
  let transparent = RGBA8 {
    r: 0,
    g: 0,
    b: 0,
    a: 0,
  };
  let reserve = has_transparent && max_colors >= 2;
  let cut_colors = if reserve { max_colors - 1 } else { max_colors };

  // Cluster over VISIBLE colors only. Fully-transparent pixels all collapse to
  // a single (0,0,0,0) "don't care" matte; feeding that (often huge) count into
  // median-cut / k-means wastes palette budget on an invisible color and can
  // round a shared cluster's centroid to a==0 with nonzero RGB — a spurious
  // transparent entry that would displace the exact slot reserved below.
  // Excluding a==0 here guarantees every centroid keeps a>=1.
  let entries: Vec<ColorCount> = {
    let mut v: Vec<ColorCount> = hist
      .iter()
      .filter(|(c, _)| c.a > 0)
      .map(|(&color, &count)| ColorCount { color, count })
      .collect();
    v.sort_by_key(|e| packed(e.color));
    v
  };

  // Proportionally pre-scale the total population weight so the Wu split's
  // exact-integer SSE moments (`red_num ~ 2^19 · N^4`, formed in i128) stay
  // precise on huge CONCENTRATED-weight images. This is a precision aid, not the
  // overflow guard — `split_reduction`'s checked arithmetic is what makes
  // overflow impossible regardless of entry count. A no-op for any normal image
  // (the bundled test PNG is ~697k px, far below the cap).
  //
  // Cap only the COPY fed to the Wu split; k-means must see the TRUE populations
  // so centroids and D² reseeding reflect the real image (the `.max(1)` cap floor
  // would otherwise over-weight rare colors). A no-op for any image <= 2^26 px
  // (e.g. the 697k photo), so q75 is unchanged.
  let mut split_entries = entries.clone();
  let cap: u128 = 1 << 26;
  let true_total: u128 = entries.iter().map(|e| e.count as u128).sum();
  cap_entry_weights(&mut split_entries, cap);

  // When the cap bit, `cap_entry_weights` rewrote `.count` 1:1 by color, so each
  // capped entry maps to a unique TRUE count. Build a packed-color -> true-count
  // lookup so `median_cut`'s final centroids use the real populations (the Wu
  // SPLIT still scores the capped copy). When the cap was a no-op (`split_entries
  // == entries`, e.g. any image <= 2^26 px incl. the 697k photo) we pass `None`:
  // capped == true for every color, so the centroid is identical either way and
  // q75 stays 262053.
  let true_counts: Option<HashMap<u32, u64>> = if true_total > cap {
    Some(entries.iter().map(|e| (packed(e.color), e.count)).collect())
  } else {
    None
  };

  let mut palette = median_cut(&split_entries, cut_colors.max(1), true_counts.as_ref());
  kmeans_refine(&mut palette, &entries, kmeans_iters);

  if reserve {
    // Clustering above only saw a>0 colors, so no centroid is transparent;
    // insert the single exact (0,0,0,0) slot so transparency stays lossless.
    if !palette.iter().any(|c| c.a == 0) {
      if palette.len() >= MAX_PALETTE {
        palette.pop();
      }
      palette.insert(0, transparent);
    }
  }

  // Deduplicate palette entries that collapsed onto each other.
  palette.sort_by_key(|c| packed(*c));
  palette.dedup();
  if palette.is_empty() {
    palette.push(RGBA8::default());
  }

  let mut indices = if dither {
    remap_dither(px, width, height, &palette, bits)
  } else {
    remap_nearest(px, &palette, bits)
  };

  // Force fully-transparent source pixels onto the exact transparent slot so
  // dithering never bleeds color into transparent regions.
  if reserve && let Some(tidx) = palette.iter().position(|c| c.a == 0) {
    for (i, &p) in px.iter().enumerate() {
      if p.a == 0 {
        indices[i] = tidx as u8;
      }
    }
  }

  let palette = canonicalize(palette, &mut indices);
  let quality = quality_score(px, bits, &palette, &indices);

  QuantizeOutput {
    palette,
    indices,
    quality,
  }
}

/// Quantizes an RGBA image to an 8-bit indexed palette.
///
/// This function is infallible: it always returns a palette and a full index
/// buffer; the caller decides whether the achieved `quality` is acceptable.
///
/// Fast path: if the image already has `<= max_colors` distinct colors the exact
/// colors become the palette, the remap is a direct lookup, and `quality = 100`.
///
/// Retry policy: if the first pass scores below `min_quality` and `max_colors`
/// was below 256, it retries once at 256 colors (with one extra k-means pass)
/// and keeps whichever result scored higher.
pub fn quantize_rgba(
  px: &[RGBA8],
  width: usize,
  height: usize,
  cfg: &QuantizeConfig,
) -> QuantizeOutput {
  let bits = cfg.posterization;
  let hist = build_histogram(px, bits);
  let max_colors = (cfg.max_colors as usize).clamp(1, MAX_PALETTE);

  // ---- Fast path: already fits in the palette (lossless). ----
  if hist.len() <= max_colors {
    let mut palette: Vec<RGBA8> = hist.keys().copied().collect();
    palette.sort_by_key(|c| alpha_first_key(*c));
    // Direct exact lookup for every (posterized) pixel.
    let mut lut: HashMap<RGBA8, u8> = HashMap::with_capacity(palette.len());
    for (i, &c) in palette.iter().enumerate() {
      lut.insert(c, i as u8);
    }
    let mut indices = Vec::with_capacity(px.len());
    for &p in px {
      let key = canonical_key(p, bits);
      // Posterization is applied identically when building the histogram, so
      // every key is guaranteed present.
      indices.push(*lut.get(&key).unwrap_or(&0));
    }
    return QuantizeOutput {
      palette,
      indices,
      quality: 100,
    };
  }

  let has_transparent = hist.keys().any(|c| c.a == 0);
  let input = PassInput {
    px,
    width,
    height,
    hist: &hist,
    has_transparent,
    bits,
  };

  // ---- First pass at the requested palette size. ----
  let first = quantize_pass(&input, max_colors, cfg.kmeans_iters, cfg.dither);

  // ---- Retry once at full 256 colors if we fell short. ----
  if first.quality < cfg.min_quality && max_colors < MAX_PALETTE {
    let retry = quantize_pass(
      &input,
      MAX_PALETTE,
      cfg.kmeans_iters.saturating_add(1),
      cfg.dither,
    );
    if retry.quality >= first.quality {
      return retry;
    }
  }

  first
}

#[cfg(test)]
mod tests {
  use super::*;

  fn cfg(max_colors: u16, dither: bool, kmeans: u8) -> QuantizeConfig {
    QuantizeConfig {
      max_colors,
      min_quality: 0,
      kmeans_iters: kmeans,
      dither,
      posterization: 0,
    }
  }

  fn rgba(r: u8, g: u8, b: u8, a: u8) -> RGBA8 {
    RGBA8 { r, g, b, a }
  }

  // A tiny deterministic PRNG so the fuzz-lite test needs no extra deps.
  struct Lcg(u64);
  impl Lcg {
    fn next_u32(&mut self) -> u32 {
      // Numerical Recipes LCG constants.
      self.0 = self
        .0
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
      (self.0 >> 32) as u32
    }
    fn byte(&mut self) -> u8 {
      (self.next_u32() & 0xff) as u8
    }
    /// Full 64-bit draw, composed from two 32-bit draws.
    fn next_u64(&mut self) -> u64 {
      ((self.next_u32() as u64) << 32) | (self.next_u32() as u64)
    }
  }

  #[test]
  fn fast_path_lossless_exact_palette() {
    // 4 distinct colors, max_colors well above that -> lossless.
    let px = vec![
      rgba(10, 20, 30, 255),
      rgba(200, 100, 50, 255),
      rgba(0, 0, 0, 0),
      rgba(255, 255, 255, 255),
      rgba(10, 20, 30, 255),
    ];
    let c = cfg(64, true, 3);
    let out = quantize_rgba(&px, 5, 1, &c);
    assert_eq!(out.quality, 100, "fast path must be lossless");
    assert_eq!(out.palette.len(), 4, "exactly the distinct colors");
    assert_eq!(out.indices.len(), px.len());
    // Every pixel maps to its exact color.
    for (i, &p) in px.iter().enumerate() {
      assert_eq!(out.palette[out.indices[i] as usize], p);
    }
  }

  #[test]
  fn single_color_image() {
    let px = vec![rgba(123, 45, 67, 255); 16];
    let c = cfg(16, true, 3);
    let out = quantize_rgba(&px, 4, 4, &c);
    assert_eq!(out.palette.len(), 1);
    assert_eq!(out.quality, 100);
    assert!(out.indices.iter().all(|&i| i == 0));
  }

  #[test]
  fn determinism_same_input_same_output() {
    // Build a >max_colors image so we exercise the full pipeline, not fast path.
    let mut px = Vec::new();
    let mut g = Lcg(0xDEADBEEF);
    for _ in 0..1024 {
      px.push(rgba(g.byte(), g.byte(), g.byte(), 255));
    }
    let c = cfg(16, true, 4);
    let a = quantize_rgba(&px, 32, 32, &c);
    let b = quantize_rgba(&px, 32, 32, &c);
    assert_eq!(a.palette, b.palette, "palette must be deterministic");
    assert_eq!(a.indices, b.indices, "indices must be deterministic");
    assert_eq!(a.quality, b.quality);
  }

  #[test]
  fn fuzz_lite_no_panic_invariants() {
    let mut g = Lcg(0x1234_5678_9ABC_DEF0);
    for trial in 0..40 {
      let w = (1 + (g.next_u32() % 17)) as usize;
      let h = (1 + (g.next_u32() % 17)) as usize;
      let mut px = Vec::with_capacity(w * h);
      for _ in 0..(w * h) {
        px.push(rgba(g.byte(), g.byte(), g.byte(), g.byte()));
      }
      let max_colors = (2 + (g.next_u32() % 255)) as u16;
      // min_quality is fixed at 0 here so the at-256 retry never fires; that
      // keeps the `palette.len() <= max_colors` invariant meaningful. (The
      // retry path can deliberately exceed the requested size, up to 256, to
      // meet a high min_quality — that is exercised by the throwing JS test.)
      let c = QuantizeConfig {
        max_colors,
        min_quality: 0,
        kmeans_iters: (g.next_u32() % 6) as u8,
        dither: g.byte() & 1 == 0,
        posterization: (g.next_u32() % 8) as u8,
      };
      let out = quantize_rgba(&px, w, h, &c);
      assert_eq!(out.indices.len(), w * h, "trial {trial}: index count");
      assert!(
        out.palette.len() <= max_colors as usize,
        "trial {trial}: palette {} > max {}",
        out.palette.len(),
        max_colors
      );
      assert!(
        out.palette.len() <= MAX_PALETTE,
        "trial {trial}: palette exceeds 256"
      );
      assert!(!out.palette.is_empty(), "trial {trial}: empty palette");
      for &idx in &out.indices {
        assert!(
          (idx as usize) < out.palette.len(),
          "trial {trial}: index out of range"
        );
      }
    }
  }

  #[test]
  fn retry_can_exceed_requested_max_when_quality_demanded() {
    // A noisy photo-like buffer with far more than `max_colors` colors and a
    // strict min_quality forces the retry-at-256 path; the resulting palette
    // may exceed the requested size but never the hard 256 cap.
    let mut px = Vec::new();
    let mut g = Lcg(0xFEED_FACE_CAFE_BEEF);
    for _ in 0..4096 {
      px.push(rgba(g.byte(), g.byte(), g.byte(), 255));
    }
    let c = QuantizeConfig {
      max_colors: 16,
      min_quality: 100,
      kmeans_iters: 2,
      dither: false,
      posterization: 0,
    };
    let out = quantize_rgba(&px, 64, 64, &c);
    assert!(out.palette.len() <= MAX_PALETTE, "never exceed 256");
    assert!(
      out.palette.len() > 16,
      "retry should have expanded past the requested 16, got {}",
      out.palette.len()
    );
    for &idx in &out.indices {
      assert!((idx as usize) < out.palette.len());
    }
  }

  #[test]
  fn posterization_reduces_distinct_colors() {
    // A smooth gradient with many near-neighbor colors.
    let mut px = Vec::new();
    for i in 0..256u32 {
      px.push(rgba(i as u8, (255 - i) as u8, (i / 2) as u8, 255));
    }
    let distinct = |bits: u8| {
      let h = build_histogram(&px, bits);
      h.len()
    };
    let none = distinct(0);
    let heavy = distinct(4);
    assert!(
      heavy < none,
      "posterization must reduce distinct colors: {heavy} !< {none}"
    );
  }

  #[test]
  fn transparent_slot_is_exact() {
    // Many opaque colors + some fully transparent pixels; small palette.
    let mut px = Vec::new();
    let mut g = Lcg(0xABCDEF01);
    for _ in 0..500 {
      px.push(rgba(g.byte(), g.byte(), g.byte(), 255));
    }
    for _ in 0..100 {
      px.push(rgba(g.byte(), g.byte(), g.byte(), 0));
    }
    let c = cfg(16, false, 2);
    let out = quantize_rgba(&px, 30, 20, &c);
    // There must be an exact a==0 palette entry and every transparent source
    // pixel must map to it.
    assert!(
      out.palette.iter().any(|p| p.a == 0),
      "must reserve a transparent slot"
    );
    for (i, &p) in px.iter().enumerate() {
      if p.a == 0 {
        assert_eq!(out.palette[out.indices[i] as usize].a, 0);
      }
    }
  }

  #[test]
  fn from_options_mapping() {
    use crate::png::PngQuantOptions;
    let o = PngQuantOptions {
      min_quality: None,
      max_quality: None,
      speed: None,
      posterization: None,
    };
    let c = QuantizeConfig::from_options(&o);
    assert_eq!(c.min_quality, 70);
    // max_quality default 99 -> ~251 colors.
    assert!(
      c.max_colors >= 248 && c.max_colors <= 254,
      "got {}",
      c.max_colors
    );
    // speed default 5 -> 5 kmeans iters, dither on.
    assert_eq!(c.kmeans_iters, 5);
    assert!(c.dither);

    let o2 = PngQuantOptions {
      min_quality: Some(80),
      max_quality: Some(50),
      speed: Some(10),
      posterization: Some(9),
    };
    let c2 = QuantizeConfig::from_options(&o2);
    // min and max are independent gates; min must NOT be clamped to max.
    assert_eq!(c2.min_quality, 80);
    assert_eq!(c2.kmeans_iters, 0); // speed 10
    assert!(!c2.dither); // speed 10 skips dither
    assert_eq!(c2.posterization, 7); // clamped 0..=7
  }

  #[test]
  fn posterization_preserves_full_transparency() {
    // Regression (Codex F1): posterizing the alpha channel turned a==0 into a
    // nonzero midpoint, making fully-transparent pixels faintly visible. A
    // transparent source pixel must stay fully transparent with posterization on.
    let mut px = vec![rgba(255, 0, 0, 0)]; // transparent red matte
    for i in 0..300u32 {
      px.push(rgba(i as u8, (i / 2) as u8, (i / 3) as u8, 255));
    }
    let c = QuantizeConfig {
      max_colors: 64,
      min_quality: 0,
      kmeans_iters: 2,
      dither: true,
      posterization: 4,
    };
    let out = quantize_rgba(&px, 7, 43, &c);
    assert_eq!(
      out.palette[out.indices[0] as usize].a, 0,
      "transparent pixel must remain fully transparent under posterization"
    );
  }

  #[test]
  fn transparent_matte_does_not_bleed() {
    // Regression (Codex F2): RGB hidden behind fully-transparent pixels must not
    // affect the visible (opaque) output — neither via histogram pollution nor
    // via dithering error diffusion. Two images that differ only in the matte
    // color behind transparency must map every opaque pixel identically.
    let mut base = Vec::new();
    let mut g = Lcg(0x5151_5151_2323_2323);
    for _ in 0..400 {
      base.push(rgba(g.byte(), g.byte(), g.byte(), 255));
    }
    let mut a = base.clone();
    let mut b = base.clone();
    for i in (0..a.len()).step_by(5) {
      a[i] = rgba(255, 0, 0, 0); // red matte, transparent
      b[i] = rgba(0, 0, 255, 0); // blue matte, transparent
    }
    let c = QuantizeConfig {
      max_colors: 32,
      min_quality: 0,
      kmeans_iters: 2,
      dither: true,
      posterization: 0,
    };
    let oa = quantize_rgba(&a, 20, 20, &c);
    let ob = quantize_rgba(&b, 20, 20, &c);
    for (i, px) in a.iter().enumerate() {
      if px.a == 255 {
        assert_eq!(
          oa.palette[oa.indices[i] as usize], ob.palette[ob.indices[i] as usize],
          "matte behind transparency must not change visible pixel {i}"
        );
      }
    }
  }

  #[test]
  fn transparency_excluded_from_palette_construction() {
    // Regression (Codex F1, re-verified): fully-transparent pixels must NOT be
    // counted in median-cut / k-means. A transparency-heavy image whose visible
    // colors include a low-alpha one used to let the huge transparent population
    // drag a shared cluster's centroid to a==0 with nonzero RGB (e.g. (2,0,0,0)).
    // That spurious entry satisfied the "any a==0?" reserve check and displaced
    // the exact transparent slot. We assert the only a==0 entry is exactly
    // (0,0,0,0) and that every transparent source pixel maps to it. (We do NOT
    // assert where the low-alpha visible pixel lands: under a tiny palette a
    // near-transparent color may legitimately be nearest to (0,0,0,0).)
    let mut px = Vec::new();
    for _ in 0..5000 {
      px.push(rgba(255, 0, 0, 0)); // transparent, non-black matte (RGB don't-care)
    }
    for _ in 0..30 {
      px.push(rgba(255, 0, 0, 1)); // faintly-visible red, metric-near transparent
    }
    for k in 0..300u32 {
      let v = (10 + (k % 6)) as u8;
      px.push(rgba(v, v, 240, 255)); // opaque blue cluster
    }
    for k in 0..300u32 {
      let v = (10 + (k % 6)) as u8;
      px.push(rgba(240, v, v, 255)); // opaque red cluster
    }
    let c = QuantizeConfig {
      max_colors: 4,
      min_quality: 0,
      kmeans_iters: 5,
      dither: false,
      posterization: 0,
    };
    let width = px.len();
    let out = quantize_rgba(&px, width, 1, &c);
    // The exact transparent slot is present...
    assert!(
      out.palette.contains(&rgba(0, 0, 0, 0)),
      "exact (0,0,0,0) slot must be reserved, got {:?}",
      out.palette
    );
    // ...and it is the ONLY a==0 entry (no spurious nonzero-RGB transparent).
    assert!(
      out
        .palette
        .iter()
        .all(|c| c.a != 0 || (c.r == 0 && c.g == 0 && c.b == 0)),
      "no a==0 entry may carry nonzero RGB, got {:?}",
      out.palette
    );
    // ...and every fully-transparent source pixel maps to it exactly.
    for (i, &p) in px.iter().enumerate() {
      if p.a == 0 {
        assert_eq!(out.palette[out.indices[i] as usize], rgba(0, 0, 0, 0));
      }
    }
  }

  #[test]
  fn transparent_padding_does_not_inflate_quality() {
    // Regression (Codex F2): quality is scored over visible pixels only. Two
    // images with identical visible content must report the same quality score
    // regardless of surrounding fully-transparent padding; otherwise a sprite
    // with a large transparent canvas could pass min_quality while its visible
    // pixels are badly quantized.
    let mut visible = Vec::new();
    let mut g = Lcg(0x00C0_FFEE_F00D_1234);
    for _ in 0..400 {
      visible.push(rgba(g.byte(), g.byte(), g.byte(), 255)); // fully opaque
    }
    // Image A: visible content only, K color slots.
    let ca = cfg(16, false, 3);
    let out_a = quantize_rgba(&visible, visible.len(), 1, &ca);

    // Image B: same visible content + heavy transparent padding, K+1 slots so the
    // reserved transparent entry leaves the same K slots for color — both cluster
    // the identical visible colors into the identical K palette entries.
    let mut padded = visible.clone();
    for _ in 0..2000 {
      padded.push(rgba(255, 0, 0, 0)); // transparent, arbitrary matte
    }
    let cb = cfg(17, false, 3);
    let out_b = quantize_rgba(&padded, padded.len(), 1, &cb);

    assert_eq!(
      out_a.quality, out_b.quality,
      "transparent padding must not change the visible-region quality score"
    );
  }

  #[test]
  fn posterize_channel_drops_exact_bits() {
    // Regression (Codex F3): posterization drops the low `bits` and leaves exactly
    // 256>>bits evenly-spaced levels (the documented LSB-drop), not an off-by-one
    // bucket count from centered rounding (which yielded e.g. 17 levels at bits=4).
    use std::collections::HashSet;
    for bits in 1..=7u8 {
      let levels: HashSet<u8> = (0..=255u8).map(|v| posterize_channel(v, bits)).collect();
      assert_eq!(
        levels.len(),
        256usize >> bits,
        "bits={bits}: expected {} levels, got {}",
        256usize >> bits,
        levels.len()
      );
    }
    // bits == 0 is the identity (all 256 levels preserved).
    let identity: HashSet<u8> = (0..=255u8).map(|v| posterize_channel(v, 0)).collect();
    assert_eq!(identity.len(), 256);
  }

  // ---- D²-weighted reseed tests ----

  /// Helper: build a sorted `Vec<ColorCount>` from a slice of (color, count) pairs.
  fn make_entries(items: &[(RGBA8, u64)]) -> Vec<ColorCount> {
    let mut v: Vec<ColorCount> = items
      .iter()
      .map(|&(color, count)| ColorCount { color, count })
      .collect();
    v.sort_by_key(|e| packed(e.color));
    v
  }

  #[test]
  fn reseed_weights_by_population() {
    // Fix A canary: the empty-cluster reseed weight is `count · D²`, not `D²`
    // alone. A HIGH-population color and a LOW-population color at (essentially)
    // the same residual must NOT have equal reseed probability — the
    // high-population color should dominate. With one live center and one dead
    // slot there is exactly one weighted LCG draw, so the pick is a single
    // deterministic decision that this test pins.
    //
    // `anchor` (1,000,000 px) pins palette slot 0's recomputed centroid to
    // (60,130,128). `hi` and `lo` are two satellites of `anchor`, in different
    // channels, with near-equal residuals against that centroid — but `hi`
    // carries 100,000 px and `lo` just 1. Under the OLD D²-only weighting their
    // masses were close and the fixed-LCG draw landed on `lo` (the 1-pixel
    // outlier); the `count · D²` weighting gives `hi` ~100,000:1 odds, so the
    // dead slot is reseeded to `hi` instead. (Config found by exhaustively
    // simulating the reseed draw; it is a genuine divergence, not a coincidence.)
    let anchor = rgba(60, 128, 128, 255); // pins slot-0 centroid to (60,130,128)
    let hi = rgba(60, 148, 128, 255); // high population
    let lo = rgba(60, 128, 148, 255); // 1-pixel outlier, near-equal residual
    let entries = make_entries(&[(anchor, 1_000_000), (hi, 100_000), (lo, 1)]);

    // 2-slot palette: slot 0 == `anchor` (a live cluster every entry is nearest
    // to), slot 1 == a far corner that ends up empty after assignment, so
    // exactly one dead slot (index 1) is reseeded.
    let far = rgba(0, 0, 0, 255);
    let mut palette = vec![anchor, far];
    kmeans_refine(&mut palette, &entries, 1);

    // The reseeded dead slot must be the HIGH-population color, never the
    // 1-pixel outlier. The old D²-only weighting put `lo` here; population
    // weighting deterministically picks `hi`.
    assert_eq!(
      palette[1], hi,
      "population-weighted reseed must put the high-count color {hi:?} in the dead slot, \
       not the 1-pixel outlier {lo:?}; palette={palette:?}"
    );

    // Deterministic across independent calls.
    let mut palette2 = vec![anchor, far];
    kmeans_refine(&mut palette2, &entries, 1);
    assert_eq!(
      palette, palette2,
      "population-weighted reseed must be deterministic"
    );
  }

  #[test]
  fn reseed_is_deterministic_with_empty_clusters() {
    // 4 distinct visible colors as entries.
    let red = rgba(255, 0, 0, 255);
    let green = rgba(0, 255, 0, 255);
    let blue = rgba(0, 0, 255, 255);
    let yellow = rgba(255, 255, 0, 255);

    let entries = make_entries(&[(red, 10), (green, 10), (blue, 10), (yellow, 10)]);

    // 8-slot palette: 4 real colors + 4 garbage (zeros). The 4 garbage slots
    // will be empty clusters and must be reseeded deterministically.
    let garbage = rgba(0, 0, 0, 0);
    let init_palette = vec![red, green, blue, yellow, garbage, garbage, garbage, garbage];

    let mut palette_a = init_palette.clone();
    let mut palette_b = init_palette.clone();

    // Use 3 iters so multiple reseed rounds run.
    kmeans_refine(&mut palette_a, &entries, 3);
    kmeans_refine(&mut palette_b, &entries, 3);

    assert_eq!(
      palette_a, palette_b,
      "reseed must produce identical palettes across two independent calls"
    );

    // No slot should still be the initial garbage color after reseeding
    // (the 4 real entries will fill the 4 dead slots).
    for &slot in &palette_a {
      assert!(
        slot != garbage || entries.iter().any(|e| e.color == garbage),
        "dead slot was not reseeded: {:?}",
        slot
      );
    }
  }

  #[test]
  fn reseed_handles_zero_residual() {
    // All entries have the SAME color; total D² == 0 → fallback path must not panic.
    let red = rgba(255, 0, 0, 255);
    let entries = make_entries(&[(red, 100)]);

    // 4-slot palette, all initialized to red — 3 slots will be empty after first assign.
    let mut palette_a = vec![red, red, red, red];
    let mut palette_b = vec![red, red, red, red];

    // Must not panic.
    kmeans_refine(&mut palette_a, &entries, 3);
    kmeans_refine(&mut palette_b, &entries, 3);

    assert_eq!(
      palette_a, palette_b,
      "zero-residual reseed must still be deterministic"
    );
  }

  #[test]
  fn reseed_picks_high_residual_outlier() {
    // Canary: pins the D²-weighted-LCG behavior.
    // OLD argmax reseed produced a different pick; this test fails before the change.
    //
    // Use 6 entries and a 2-slot palette: all entries are forced into just 2
    // clusters, so 4 of the (hypothetical extra) entries have large residuals.
    // We directly call kmeans_refine with a palette that has 6 slots, only 2 of
    // which will be non-empty after the first assign step, exercising reseed.
    //
    // Entries spread across the RGB gamut (sorted by packed for determinism):
    let c0 = rgba(0, 0, 50, 255); // deep blue
    let c1 = rgba(0, 0, 200, 255); // bright blue
    let c2 = rgba(0, 200, 0, 255); // bright green
    let c3 = rgba(200, 0, 0, 255); // bright red
    let c4 = rgba(200, 200, 0, 255); // yellow
    let c5 = rgba(255, 255, 255, 255); // white

    let entries = make_entries(&[(c0, 10), (c1, 10), (c2, 10), (c3, 10), (c4, 10), (c5, 10)]);

    // 6-slot palette seeded with only 2 real centroids (blue cluster, red
    // cluster) and 4 copies of the first centroid as dead placeholders.
    let blue_center = rgba(100, 0, 100, 255);
    let red_center = rgba(200, 100, 0, 255);
    let mut palette = vec![
      blue_center,
      red_center,
      blue_center,
      blue_center,
      blue_center,
      blue_center,
    ];

    // 1 iter so we see exactly one reseed round for the 4 dead slots.
    kmeans_refine(&mut palette, &entries, 1);

    // Slots 0 and 1 will become centroids (not original entry colors) after k-means
    // updates their means; slots 2-5 are the dead slots reseeded from entry colors.
    let known: std::collections::HashSet<u32> = [c0, c1, c2, c3, c4, c5]
      .iter()
      .map(|&c| packed(c))
      .collect();
    // Only the reseeded slots (2-5) must contain entry colors.
    for p in &palette[2..] {
      assert!(
        known.contains(&packed(*p)),
        "reseeded slot {:?} is not one of the 6 entry colors",
        p
      );
    }

    // The result must be deterministic (two calls on identical inputs agree).
    let mut palette2 = vec![
      blue_center,
      red_center,
      blue_center,
      blue_center,
      blue_center,
      blue_center,
    ];
    kmeans_refine(&mut palette2, &entries, 1);
    assert_eq!(
      palette, palette2,
      "D²-weighted reseed must be deterministic"
    );

    // Pin the specific reseeded slots the population-weighted (count · D²) LCG
    // implementation produces. All six entries here carry count == 10, so the
    // count factor is uniform — but it still rescales `total`, hence the modular
    // draw `r = lcg % total` and the spread of picks. P3 Phase 2 moved the reseed D²
    // from RGB `dist2` to perceptual `pdist` (CIE76 ΔE), which reshapes the residual
    // weights and so the spread of picks (was [c5, c3, c2, c0] under `dist2`); the
    // reseeded slots are still all valid entry colors and fully deterministic.
    assert_eq!(
      &palette[2..],
      &[c4, c3, c5, c2],
      "population-weighted (count · pdist D²) LCG must pick these specific reseeded \
       slots (canary; RGB `dist2` D² gave [c5, c3, c2, c0])"
    );
  }

  #[test]
  fn reseed_uses_nearest_live_center_d2() {
    // Fix A canary: the empty-cluster D² baseline must be each entry's squared
    // distance to its NEAREST CURRENT LIVE center, NOT the (now-moved) center of
    // the cluster it was assigned to at the START of the iteration.
    //
    // Construction (entries are sorted by `packed`, so the reseed walk order is
    // fixed). Two live seeds `p0`/`p1` plus one `far` slot that empties after the
    // assignment pass, giving exactly one dead slot (index 2). After the in-place
    // centroid recompute, at least one entry's nearest LIVE center is a different
    // cluster than the one it was assigned to (its old center moved away), so the
    // stale baseline assigns that entry a different residual than the nearest-live
    // baseline. That flips which entry the fixed-LCG `count · D²` walk lands on:
    //   - STALE (buggy) baseline reseeds slot 2 to the 90,000-px satellite.
    //   - NEAREST-LIVE (fixed) baseline reseeds slot 2 to the 1,000,000-px anchor.
    // P3 Phase 2 moved the reseed D² to perceptual `pdist` (CIE76 ΔE), so this
    // construction was re-derived by exhaustively simulating BOTH baselines over the
    // exact `pdist` reseed math; it is a genuine divergence, not a coincidence. The
    // load-bearing entry is the satellite: it is ASSIGNED to cluster 1 but its
    // NEAREST LIVE center after the centroid move is cluster 0 — the exact stale-vs-
    // nearest-live distinction. The larger stale residual inflates the satellite's
    // `count·D²` weight, shifting the modular LCG draw into the satellite's bucket.
    let anchor = rgba(40, 140, 100, 255); // 1,000,000 px — the nearest-live pick
    let satellite = rgba(248, 168, 8, 255); // 90,000 px — the stale (buggy) pick
    let entries = make_entries(&[
      (anchor, 1_000_000),
      (rgba(123, 4, 83, 255), 600_000),
      (rgba(153, 238, 58, 255), 800_000),
      (satellite, 90_000),
      (rgba(244, 240, 166, 255), 40_000),
    ]);

    let p0 = rgba(40, 140, 100, 255);
    let p1 = rgba(180, 40, 50, 255);
    let far = rgba(0, 0, 0, 255); // empties after assignment → dead slot 2
    let mut palette = vec![p0, p1, far];
    kmeans_refine(&mut palette, &entries, 1);

    // The dead slot must be reseeded from the NEAREST-LIVE-center D² baseline,
    // i.e. the high-population anchor — NOT the lower-population satellite the
    // stale (moved old-assigned center) baseline would pick.
    assert_eq!(
      palette[2], anchor,
      "nearest-live-center D² reseed must pick the high-population anchor {anchor:?}, \
       not the satellite {satellite:?} the stale moved-assigned-center baseline picks; \
       palette={palette:?}"
    );
    assert_ne!(
      palette[2], satellite,
      "stale moved-assigned-center baseline pick {satellite:?} must NOT win"
    );

    // The full reseeded palette is pinned (slots 0/1 are recomputed centroids,
    // identical under both baselines since the fix only changes the reseed D²).
    assert_eq!(
      palette,
      vec![rgba(94, 185, 83, 255), rgba(139, 25, 73, 255), anchor],
      "nearest-live-center reseed palette pin"
    );

    // Deterministic across independent calls.
    let mut palette2 = vec![p0, p1, far];
    kmeans_refine(&mut palette2, &entries, 1);
    assert_eq!(
      palette, palette2,
      "nearest-live reseed must be deterministic"
    );
  }

  // ---- Wu variance-minimizing median-cut split tests ----

  #[test]
  fn wu_split_is_deterministic() {
    // A crafted multi-cluster image, run the FULL quantize_rgba twice; palette and
    // indices must be byte-identical. Exercises the integer-only Wu split decision
    // (no f32/f64 in the split) across many splits.
    let mut px = Vec::new();
    // Four tight, well-separated clusters at the corners of the RGB cube, plus a
    // few mid-gamut stragglers so several boxes compete for the next split.
    let centers = [
      (20u8, 20u8, 20u8),
      (230, 30, 30),
      (30, 230, 40),
      (40, 50, 230),
    ];
    for &(cr, cg, cb) in &centers {
      for dx in 0..6u8 {
        for dy in 0..6u8 {
          px.push(rgba(
            cr.saturating_add(dx),
            cg.saturating_add(dy),
            cb.saturating_add(dx ^ dy),
            255,
          ));
        }
      }
    }
    px.push(rgba(128, 64, 200, 255));
    px.push(rgba(200, 128, 64, 255));
    px.push(rgba(64, 200, 128, 255));

    let c = cfg(8, true, 4);
    let a = quantize_rgba(&px, px.len(), 1, &c);
    let b = quantize_rgba(&px, px.len(), 1, &c);
    assert_eq!(
      a.palette, b.palette,
      "Wu split palette must be deterministic"
    );
    assert_eq!(
      a.indices, b.indices,
      "Wu split indices must be deterministic"
    );
    assert_eq!(a.quality, b.quality);
  }

  /// Total within-box weighted SSE of an assignment, summed over R,G,B,A using the
  /// same integer moment definition the Wu split minimizes. Used only by tests to
  /// compare partitions; lower is better.
  fn total_sse(groups: &[Vec<ColorCount>]) -> f64 {
    let mut sse = 0.0f64;
    for g in groups {
      let mut n = 0.0f64;
      let mut s = [0.0f64; 4];
      let mut q = [0.0f64; 4];
      for e in g {
        let w = e.count as f64;
        let ch = [e.color.r, e.color.g, e.color.b, e.color.a];
        n += w;
        for k in 0..4 {
          let x = ch[k] as f64;
          s[k] += w * x;
          q[k] += w * x * x;
        }
      }
      if n > 0.0 {
        for k in 0..4 {
          sse += q[k] - s[k] * s[k] / n;
        }
      }
    }
    sse
  }

  #[test]
  fn wu_split_separates_distinct_clusters() {
    // K well-separated tight clusters with max_colors == K: variance-minimization
    // must place one palette entry at (essentially) each cluster centroid, giving a
    // far lower total SSE than a deliberately-bad partition that splits one cluster
    // and merges two others.
    let clusters = [
      rgba(40, 40, 40, 255),
      rgba(220, 40, 40, 255),
      rgba(40, 220, 40, 255),
    ];
    let mut entries: Vec<ColorCount> = Vec::new();
    for &c in &clusters {
      // three tight members per cluster (±2 on R) so each is a real >=2 box.
      for &dr in &[-2i16, 0, 2] {
        entries.push(ColorCount {
          color: rgba((c.r as i16 + dr) as u8, c.g, c.b, c.a),
          count: 4,
        });
      }
    }
    entries.sort_by_key(|e| packed(e.color));

    let palette = median_cut(&entries, clusters.len(), None);
    assert_eq!(palette.len(), clusters.len(), "one box per cluster");

    // Every cluster centroid has a palette entry within a couple LSBs.
    for &c in &clusters {
      let near = palette.iter().any(|p| {
        (p.r as i32 - c.r as i32).abs() <= 3
          && (p.g as i32 - c.g as i32).abs() <= 3
          && (p.b as i32 - c.b as i32).abs() <= 3
      });
      assert!(
        near,
        "no palette entry near cluster {c:?}; palette={palette:?}"
      );
    }

    // The Wu partition (group entries by nearest palette color) must beat a
    // deliberately-bad partition that lumps the two red-ish clusters together and
    // splits the dark cluster.
    let mut wu_groups: Vec<Vec<ColorCount>> = vec![Vec::new(); palette.len()];
    for &e in &entries {
      wu_groups[nearest(&palette, e.color)].push(e);
    }
    // Bad partition: {first dark member} | {rest of dark + all of cluster1} | {cluster2}
    let bad = vec![
      vec![entries[0]],
      entries[1..6].to_vec(),
      entries[6..].to_vec(),
    ];
    assert!(
      total_sse(&wu_groups) < total_sse(&bad),
      "Wu SSE {} must beat bad-split SSE {}",
      total_sse(&wu_groups),
      total_sse(&bad)
    );
  }

  #[test]
  fn wu_split_picks_max_variance_axis() {
    // Prove-fails canary. A tiny box where the variance-minimizing split chooses a
    // DIFFERENT axis/boundary than the OLD widest-channel/population-median split.
    //
    // Input (5 entries): two G-separated pairs at low/high R, plus one mid straggler.
    //   OLD split: widest channel is G (span 250) so it sorts by G and splits at the
    //              population median, producing palette [(15,0,0,255),(23,238,0,255)].
    //   NEW (Wu): the variance-min split still uses axis G but at a DIFFERENT
    //              boundary (the straggler joins the low-G side), giving palette
    //              [(15,250,0,255),(23,11,0,255)].
    // So this exact-pin FAILS against the old population-median split.
    let entries = make_entries(&[
      (rgba(0, 0, 0, 255), 5),
      (rgba(30, 0, 0, 255), 5),
      (rgba(0, 250, 0, 255), 5),
      (rgba(30, 250, 0, 255), 5),
      (rgba(100, 120, 0, 255), 1),
    ]);
    let mut palette = median_cut(&entries, 2, None);
    palette.sort_by_key(|c| packed(*c));
    assert_eq!(
      palette,
      vec![rgba(15, 250, 0, 255), rgba(23, 11, 0, 255)],
      "Wu split must pin this variance-min palette (old population-median gave \
       [(15,0,0,255),(23,238,0,255)]; canary fails pre-change)"
    );
  }

  #[test]
  fn wu_split_separates_alpha_not_color_on_equal_counts() {
    // The Wu split objective must weight alpha like `dist2` (ALPHA_WEIGHT=3), or a
    // small partial-alpha palette gets an alpha-blind split. Four equal-count
    // entries form a 2x2 grid: red varies on one axis (0 vs 255), alpha on the
    // other (1 vs 255). The RGB span (255) equals the alpha span (254), but alpha
    // error counts 3x in the real metric, so the variance-min split must cut on
    // ALPHA, collapsing the two reds together while preserving the two alphas.
    let a = rgba(0, 0, 0, 1);
    let b = rgba(0, 0, 0, 255);
    let c = rgba(255, 0, 0, 1);
    let d = rgba(255, 0, 0, 255);
    let entries = make_entries(&[(a, 1), (b, 1), (c, 1), (d, 1)]);

    // (1) PRIMARY: the best split is on the ALPHA axis (3), not a color axis.
    // Pre-fix (unweighted) this is axis 0 (red).
    let split = MCBox::compute_best_split(&entries).unwrap();
    assert_eq!(
      split.axis, 3,
      "Wu split must cut on alpha (axis 3), not color; got axis {} \
       (pre-fix the unweighted objective picks axis 0 = red)",
      split.axis
    );

    // (2) median_cut to 2 colors must PRESERVE both alphas, not collapse them to
    // two ~128 averages. Exact and robust to RGB 127/128 rounding.
    let palette = median_cut(&entries, 2, None);
    assert_eq!(palette.len(), 2, "expected a 2-color palette");
    let mut alphas: Vec<u8> = palette.iter().map(|p| p.a).collect();
    alphas.sort_unstable();
    assert_eq!(
      alphas,
      vec![1, 255],
      "alpha must be preserved (alpha-only split); pre-fix the alpha-blind \
       split averages alpha to [128, 128]; palette={palette:?}"
    );
    // The split is alpha-only: both entries share the same (averaged) RGB.
    assert_eq!(
      (palette[0].r, palette[0].g, palette[0].b),
      (palette[1].r, palette[1].g, palette[1].b),
      "an alpha-only split leaves both palette entries with equal RGB; palette={palette:?}"
    );

    // (3) The alpha-preserving palette must score the optimal total `dist2`.
    // Each of A..D maps to its nearest palette entry; the alpha-preserving
    // optimum is <= 32640, while the pre-fix alpha-blind palette scores 193548.
    let total: i64 = [a, b, c, d]
      .iter()
      .map(|&color| dist2(palette[nearest(&palette, color)], color))
      .sum();
    assert!(
      total <= 32640,
      "alpha-preserving palette must minimize total dist2 (got {total}); \
       pre-fix the alpha-blind palette scores 193548"
    );
  }

  #[test]
  fn min_quality_100_rejects_lossy_near_opaque() {
    // Regression (Codex round-7 [high]): the `min_quality == 100` lossless gate
    // must reject ANY lossy visible remap. `dist2` truncates the alpha-weighted
    // RGB term with integer division (`* wa / 510`), so for a < 255 a 1-LSB RGB
    // difference floors to 0 (e.g. dist2((100,0,0,254),(101,0,0,254)) == 0). On a
    // partial-alpha image with > 256 distinct near-opaque colors, the chosen
    // palette is NOT byte-identical to the source, yet the accumulated truncating
    // `dist2` is 0 -> pre-fix `quality_score` returned 100 and the gate ACCEPTED
    // visibly-lossy output. The exact `lossless` flag closes that gap.

    // dist2 truncation is real at a=254 for a 1-LSB neighbor (the root cause).
    // A single-channel +-1 RGB difference (dr^2+dg^2+db^2 == 1) floors to 0 once
    // multiplied by `wa/510` (508/510 < 1), so the metric cannot see it.
    assert_eq!(dist2(rgba(100, 0, 0, 254), rgba(101, 0, 0, 254)), 0);
    assert_eq!(dist2(rgba(128, 128, 5, 254), rgba(128, 128, 6, 254)), 0);

    // A tight near-opaque cluster of 257 distinct colors at a=254: a 256-long
    // blue ramp (128,128,b) plus one extra (128,129,0). max_colors=256 forces the
    // quantizer to drop one slot; every dropped pixel remaps to a neighbor that
    // differs by exactly 1 LSB in one channel, so EVERY visible pixel's `dist2`
    // truncates to 0 -> the accumulated error is exactly 0 even though hundreds of
    // pixels are NOT byte-identical. This is the precise pre-fix 100-verdict trap.
    let mut px = Vec::new();
    for b in 0..=255u8 {
      px.push(rgba(128, 128, b, 254));
    }
    px.push(rgba(128, 129, 0, 254));
    let distinct: std::collections::HashSet<u32> = px.iter().map(|c| packed(*c)).collect();
    assert_eq!(
      distinct.len(),
      257,
      "construction must yield 257 distinct colors"
    );
    assert!(
      distinct.len() > 256,
      "must exceed max_colors to force quantization"
    );

    // Public slow path: max_colors=256, min_quality=100, dither OFF, kmeans off.
    let lossy_cfg = QuantizeConfig {
      max_colors: 256,
      min_quality: 100,
      kmeans_iters: 0,
      dither: false,
      posterization: 0,
    };
    let lossy = quantize_rgba(&px, px.len(), 1, &lossy_cfg);

    // The remap IS lossy: hundreds of visible pixels map to a non-identical
    // palette color (257 distinct colors cannot fit in <=256 slots).
    let remapped = px
      .iter()
      .enumerate()
      .filter(|(i, p)| lossy.palette[lossy.indices[*i] as usize] != **p)
      .count();
    assert!(
      remapped > 0,
      "257 distinct colors in <=256 slots must remap at least one pixel"
    );

    // CRITICAL: the accumulated truncating `dist2` over all visible pixels is
    // EXACTLY 0 (every remap is a 1-LSB single-channel neighbor). This is what
    // drove `quality_score`'s `mse <= 0.0` branch to return 100 pre-fix; the
    // exact `lossless` flag is the only thing that can distinguish this lossy
    // case from a truly byte-identical one.
    let sum_dist2: i64 = px
      .iter()
      .enumerate()
      .map(|(i, &p)| dist2(p, lossy.palette[lossy.indices[i] as usize]))
      .sum();
    assert_eq!(
      sum_dist2, 0,
      "the truncating dist2 must accumulate 0 here (the bug trigger); \
       without that, this test would not exercise the 100-verdict trap"
    );

    // The gate verdict: a lossy visible remap must score BELOW 100 so the
    // `min_quality == 100` gate (png.rs: `if out.quality < min_quality {{ throw }}`)
    // rejects it. Pre-fix (no `lossless` flag) `mse == 0` returns exactly 100 (the
    // prove-fail value); the exact byte-identity flag forces it to 99.
    assert!(
      lossy.quality < 100,
      "lossy near-opaque remap must score < 100 (got {}); pre-fix the truncating \
       dist2 accumulates 0 and quality_score returns 100, so the lossless gate \
       wrongly ACCEPTS {remapped} remapped pixels",
      lossy.quality
    );

    // Conversely, a genuinely lossless case (<=256 exact distinct colors) must
    // STILL score 100, so the fix never lowers a truly-lossless verdict.
    let mut exact = Vec::new();
    for r in 0..=200u8 {
      exact.push(rgba(r, 0, 0, 254));
    }
    let exact_distinct: std::collections::HashSet<u32> = exact.iter().map(|c| packed(*c)).collect();
    assert_eq!(exact_distinct.len(), 201);
    assert!(exact_distinct.len() <= 256);
    let lossless = quantize_rgba(&exact, exact.len(), 1, &lossy_cfg);
    assert_eq!(
      lossless.quality, 100,
      "<=256 exact near-opaque colors are losslessly representable and must score 100"
    );
    // And every pixel is byte-identical, confirming true losslessness.
    for (i, &p) in exact.iter().enumerate() {
      assert_eq!(
        lossless.palette[lossless.indices[i] as usize], p,
        "lossless case must reproduce every visible pixel byte-for-byte"
      );
    }
  }

  #[test]
  fn cap_entry_weights_bounds_total() {
    // Fix B unit test: the population-weight cap keeps the Wu split's i128 SSE
    // moments from overflowing on very large images. It must bound the total,
    // keep every entry present (>= 1), and preserve proportions/order so the
    // variance-min decision is unchanged up to rounding.
    let cap: u128 = 1 << 26;

    // Four entries whose counts sum to far above the cap (4 · 2^25 == 2^27).
    let big = 1u64 << 25;
    let mut v = vec![
      ColorCount {
        color: rgba(10, 20, 30, 255),
        count: big,
      },
      ColorCount {
        color: rgba(40, 50, 60, 255),
        count: big / 2,
      },
      ColorCount {
        color: rgba(70, 80, 90, 255),
        count: big * 2, // the originally-largest
      },
      ColorCount {
        color: rgba(100, 110, 120, 255),
        count: 1, // a 1-pixel outlier that must survive (>= 1)
      },
    ];
    let largest_color = v[2].color;
    cap_entry_weights(&mut v, cap);

    let total_after: u128 = v.iter().map(|e| e.count as u128).sum();
    assert!(
      total_after <= cap + v.len() as u128,
      "capped total {total_after} must be <= cap + len ({})",
      cap + v.len() as u128
    );
    for e in &v {
      assert!(
        e.count >= 1,
        "every entry must keep weight >= 1 after capping"
      );
    }
    // The originally-largest entry is still the largest (proportions preserved).
    let max_entry = v.iter().max_by_key(|e| e.count).unwrap();
    assert_eq!(
      max_entry.color, largest_color,
      "capping must preserve which entry is largest"
    );

    // No-op when the total already fits under the cap.
    let mut small = vec![
      ColorCount {
        color: rgba(1, 2, 3, 255),
        count: 1000,
      },
      ColorCount {
        color: rgba(4, 5, 6, 255),
        count: 2000,
      },
    ];
    let before = small.clone();
    cap_entry_weights(&mut small, cap);
    assert_eq!(
      small.iter().map(|e| e.count).collect::<Vec<_>>(),
      before.iter().map(|e| e.count).collect::<Vec<_>>(),
      "cap must be a no-op when total <= cap"
    );
  }

  #[test]
  fn population_count_beyond_u32_is_honored() {
    // Regression (Codex round-9): the per-color population count is `u64`, so a
    // single color whose population exceeds `u32::MAX` (a >4.3-gigapixel
    // single-color image) keeps its full weight in the population-weighted math.
    //
    // A `u32` counter would WRAP: `(u32::MAX as u64) + 1_000_000` truncates to
    // `999_999`, collapsing the dominant color's weight far below the rare
    // color's 2,000,000 pixels — so the single-box centroid would be dragged
    // toward the rare color instead of staying at the dominant one. With `u64`
    // the dominant weight (~4.295e9) dwarfs the rare 2e6 and the centroid stays
    // essentially the dominant color. The dominant count is built directly as a
    // `u64` here; if it ever truncated through `u32` this test would fail.
    let dominant = rgba(200, 50, 100, 255);
    let rare = rgba(10, 220, 30, 255);

    // >u32::MAX pixels of the dominant color. A `u32` count cannot hold this.
    let dominant_count: u64 = (u32::MAX as u64) + 1_000_000; // 4_295_967_295
    assert!(
      dominant_count > u32::MAX as u64,
      "dominant population must exceed u32::MAX to exercise the widening"
    );
    // The rare color carries 2,000,000 px: tiny (0.05%) next to the >4.3e9
    // dominant population, but FAR above the 999,999 a wrapped `u32` would show.
    let rare_count: u64 = 2_000_000;

    let entries = make_entries(&[(dominant, dominant_count), (rare, rare_count)]);

    // Single box -> the population-weighted centroid of all entries.
    let palette = median_cut(&entries, 1, None);
    assert_eq!(palette.len(), 1, "max_colors == 1 yields a single centroid");
    let centroid = palette[0];

    // The centroid must sit on the DOMINANT color (within rounding), not be
    // pulled toward the rare color. Each channel must be within a couple LSBs of
    // the dominant color; a wrapped-u32 count would shift G from ~50 to ~163.
    for (got, want, name) in [
      (centroid.r, dominant.r, "r"),
      (centroid.g, dominant.g, "g"),
      (centroid.b, dominant.b, "b"),
      (centroid.a, dominant.a, "a"),
    ] {
      assert!(
        (got as i32 - want as i32).abs() <= 3,
        "centroid {name} channel = {got}, expected ~{want} (dominant color); a \
         u32-wrapped population would pull it toward the rare color {rare:?}; \
         centroid={centroid:?}"
      );
    }
    // And it must NOT have collapsed onto the rare color's distinctive channels.
    assert!(
      (centroid.g as i32 - rare.g as i32).abs() > 100,
      "centroid must not be dragged to the rare color's green ({}); centroid={centroid:?}",
      rare.g
    );
  }

  #[test]
  fn split_reduction_rejects_overflow() {
    // Fix B unit test: the SSE-reduction `red_num / red_den` is formed in i128.
    // For every physically realizable image the products fit i128 with vast
    // margin, but a pathological input with more distinct unit-count colors than
    // the population cap leaves `N = entries.len()` unbounded, so `red_num ~
    // 2^19·N^4` can exceed i128::MAX. `split_reduction` must then report "no
    // usable split" (None) instead of wrapping (release) or panicking (debug).

    // Normal operands reproduce the old unchecked formula exactly:
    //   red_num = merit_num*parent_n - parent_num*merit_den = 100*5 - 30*7 = 290
    //   red_den = merit_den*parent_n                        = 7*5         = 35
    assert_eq!(
      split_reduction(100, 7, 30, 5),
      Some((290, 35)),
      "normal operands must match merit_num*parent_n - parent_num*merit_den \
       over merit_den*parent_n"
    );

    // A non-positive reduction (no split reduces SSE) returns None:
    //   red_num = 10*2 - 20*5 = -80  <= 0
    assert_eq!(
      split_reduction(10, 5, 20, 2),
      None,
      "red_num <= 0 must return None (no SSE-reducing split)"
    );

    // Overflow in the FIRST product (merit_num * parent_n) returns None.
    assert_eq!(
      split_reduction(i128::MAX, 1, 0, 2),
      None,
      "merit_num*parent_n overflow must return None"
    );

    // Overflow in the inner product (parent_num * merit_den) inside checked_sub
    // returns None.
    assert_eq!(
      split_reduction(0, i128::MAX, 2, 1),
      None,
      "parent_num*merit_den overflow must return None"
    );

    // Overflow in the DENOMINATOR product (merit_den * parent_n) returns None,
    // even when red_num itself fits (here red_num = 1*MAX - 0 = MAX > 0, but
    // red_den = 2*MAX overflows).
    assert_eq!(
      split_reduction(1, 2, 0, i128::MAX),
      None,
      "merit_den*parent_n overflow must return None even when red_num fits"
    );
  }

  // ---- i256 primitive guards: widen_mul_u128 + cmp_ratio ----
  //
  // These are the load-bearing helpers behind the Wu split's exact,
  // overflow-free rational comparison. The Wu tests above only feed small
  // inputs whose cross-products fit `i128`; the regime these primitives exist
  // for (products up to ~187 bits, plus signed numerators) is guarded here.

  #[test]
  fn widen_mul_u128_matches_u64_product_common_path() {
    // For any a,b drawn from the u64 range, (a as u128)*(b as u128) < 2^128, so
    // the 256-bit product must have hi == 0 and lo == the exact 128-bit product.
    // This pins the common path (the only path the Wu split takes for typical
    // image sizes) exactly, over many deterministic draws.
    let mut g = Lcg(0x5DEE_CE66_D000_0001);
    for _ in 0..2000 {
      let a = g.next_u64() as u128;
      let b = g.next_u64() as u128;
      let (hi, lo) = widen_mul_u128(a, b);
      assert_eq!(hi, 0, "u64*u64 product cannot reach the high limb");
      assert_eq!(lo, a * b, "low limb must equal the exact 128-bit product");
    }
    // Include the u64 extremes explicitly so the corners are not left to chance.
    for &(a, b) in &[
      (0u128, u64::MAX as u128),
      (u64::MAX as u128, 0u128),
      (u64::MAX as u128, u64::MAX as u128),
      (1u128, u64::MAX as u128),
    ] {
      let (hi, lo) = widen_mul_u128(a, b);
      assert_eq!(hi, 0);
      assert_eq!(lo, a * b);
    }
  }

  #[test]
  fn widen_mul_u128_hand_verified_256bit_identities() {
    // Hand-computed full-width products (no oracle): each (hi, lo) is derived by
    // reasoning about the 256-bit result directly.

    // 2^64 * 2^64 = 2^128  ->  hi = 1, lo = 0.
    assert_eq!(widen_mul_u128(1u128 << 64, 1u128 << 64), (1, 0));

    // 2^127 * 2 = 2^128  ->  hi = 1, lo = 0.
    assert_eq!(widen_mul_u128(1u128 << 127, 2), (1, 0));

    // u128::MAX * u128::MAX = (2^128 - 1)^2 = 2^256 - 2^129 + 1
    //   = (2^128 - 2)*2^128 + 1  ->  hi = u128::MAX - 1, lo = 1.
    assert_eq!(
      widen_mul_u128(u128::MAX, u128::MAX),
      (u128::MAX - 1, 1),
      "carry from the high cross-products must propagate exactly"
    );

    // u128::MAX * 1  ->  hi = 0, lo = u128::MAX.
    assert_eq!(widen_mul_u128(u128::MAX, 1), (0, u128::MAX));

    // 0 * anything  ->  (0, 0), and multiplication is commutative.
    assert_eq!(widen_mul_u128(0, u128::MAX), (0, 0));
    assert_eq!(widen_mul_u128(u128::MAX, 0), (0, 0));
  }

  #[test]
  fn cmp_ratio_matches_i128_oracle_when_cross_products_fit() {
    use std::cmp::Ordering;
    // Draw a1,a2 in [-2^40, 2^40) and b1,b2 in (0, 2^40). Then a1*b2 and a2*b1
    // each fit in i128 (< ~2^81), so the direct cross-multiply is an exact
    // oracle. cmp_ratio must agree, across every numerator sign combination.
    let mut g = Lcg(0x1357_9BDF_2468_ACE0);
    let mut seen_neg_neg = false;
    let mut seen_neg_pos = false;
    let mut seen_zero = false;
    let mut seen_pos_pos = false;
    for i in 0..5000u32 {
      // |a| < 2^40 (signed), 0 < b < 2^40.
      let mut a1 = (g.next_u64() % (1 << 41)) as i128 - (1 << 40);
      let mut a2 = (g.next_u64() % (1 << 41)) as i128 - (1 << 40);
      let b1 = 1 + (g.next_u64() % ((1 << 40) - 1)) as i128;
      let b2 = 1 + (g.next_u64() % ((1 << 40) - 1)) as i128;
      // Deterministically inject zero numerators on a small subset so the
      // zero-sign branch is genuinely exercised (uniform draws never hit it).
      match i % 64 {
        0 => a1 = 0,
        1 => a2 = 0,
        2 => {
          a1 = 0;
          a2 = 0;
        }
        _ => {}
      }

      let oracle = (a1 * b2).cmp(&(a2 * b1));
      assert_eq!(
        cmp_ratio(a1, b1, a2, b2),
        oracle,
        "cmp_ratio disagreed with i128 oracle: a1={a1} b1={b1} a2={a2} b2={b2}"
      );

      match (a1.signum(), a2.signum()) {
        (-1, -1) => seen_neg_neg = true,
        (s1, s2) if s1 <= 0 && s2 >= 0 && (s1 < 0 || s2 > 0) => seen_neg_pos = true,
        _ => {}
      }
      if a1 == 0 || a2 == 0 {
        seen_zero = true;
      }
      if a1 > 0 && a2 > 0 {
        seen_pos_pos = true;
      }
    }
    // The numerator sign space is actually exercised, not just asserted in the
    // abstract.
    assert!(
      seen_neg_neg,
      "expected some negative/negative numerator pairs"
    );
    assert!(seen_neg_pos, "expected some mixed-sign numerator pairs");
    assert!(
      seen_pos_pos,
      "expected some positive/positive numerator pairs"
    );
    assert!(seen_zero, "expected some zero numerator(s)");

    // Pin the pure-sign shortcuts explicitly (these never touch widen_mul_u128).
    assert_eq!(cmp_ratio(0, 7, 0, 3), Ordering::Equal, "0/b == 0/b");
    assert_eq!(cmp_ratio(0, 7, 5, 3), Ordering::Less, "0 < positive");
    assert_eq!(cmp_ratio(5, 7, 0, 3), Ordering::Greater, "positive > 0");
    assert_eq!(cmp_ratio(-1, 7, 0, 3), Ordering::Less, "negative < 0");
    assert_eq!(cmp_ratio(0, 7, -1, 3), Ordering::Greater, "0 > negative");
    assert_eq!(
      cmp_ratio(-1, 7, 5, 3),
      Ordering::Less,
      "negative < positive"
    );
  }

  #[test]
  fn cmp_ratio_hand_verified_beyond_i128_cross_products() {
    use std::cmp::Ordering;
    // These compare rationals whose cross-products a1*b2 / a2*b1 reach ~158 bits
    // (well past i128's 127-bit signed range) yet whose operands each stay below
    // 2^127. A naive i128 cross-multiply would overflow; the i256 path must give
    // the answer derived here by hand. Reference denominators b2 = 3*2^36,
    // b1 = 3*2^60.
    let b1 = 3 * (1i128 << 60);
    let b2 = 3 * (1i128 << 36);

    // Equal: 2^120/(3*2^60) = 2^60/3 = 2^96/(3*2^36). Cross-products are both
    // 2^120*3*2^36 = 3*2^156 (~158 bits) and identical -> Equal.
    assert_eq!(
      cmp_ratio(1i128 << 120, b1, 1i128 << 96, b2),
      Ordering::Equal,
      "equal ratios whose cross-products overflow i128 must compare Equal"
    );

    // Strictly greater: bump the first numerator to 2^121 -> 2^61/3 > 2^60/3.
    assert_eq!(
      cmp_ratio(1i128 << 121, b1, 1i128 << 96, b2),
      Ordering::Greater,
      "larger ratio must compare Greater despite i128-overflowing cross-products"
    );
    // ...and the reverse orientation is symmetric.
    assert_eq!(cmp_ratio(1i128 << 96, b2, 1i128 << 121, b1), Ordering::Less,);

    // Large negative numerator vs positive -> Less (sign shortcut also covers
    // this, but pin it with operands in the overflow regime regardless).
    assert_eq!(
      cmp_ratio(-(1i128 << 121), b1, 1i128 << 96, b2),
      Ordering::Less,
      "negative numerator must always be less than a positive one"
    );
    // Both negative: -2^121/(3*2^60) < -2^120/(3*2^60) since magnitudes flip.
    assert_eq!(
      cmp_ratio(-(1i128 << 121), b1, -(1i128 << 120), b1),
      Ordering::Less,
      "more-negative ratio is Less; magnitude comparison must be reversed"
    );
  }

  // ---- Selective Floyd-Steinberg dither regression tests (P2.1) ----
  //
  // These lock the shipped behavior of the residual-aware, edge-gated selective
  // dither: its pure-function strength/dead-zone/clamp helpers, that a cleanly
  // covered flat region diffuses nothing, that a smooth high-residual field
  // (which a flat remap would BAND) re-engages dither, and that the whole path
  // stays deterministic. They assert the shipped consts directly (no revert).

  /// Count of distinct index values in a remap output buffer.
  fn distinct_indices(idx: &[u8]) -> usize {
    let mut set: std::collections::HashSet<u8> = std::collections::HashSet::new();
    for &i in idx {
      set.insert(i);
    }
    set.len()
  }

  #[test]
  fn dither_strength_clean_pixel_is_zero() {
    // A cleanly-mapped pixel (residual at/below LO) diffuses nothing, regardless
    // of activity. The ramp is 0 there, so strength is exactly 0.0.
    assert_eq!(
      dither_strength(0.0, 0.0),
      0.0,
      "zero residual, zero activity must produce exactly zero strength"
    );
    assert_eq!(
      dither_strength(DITHER_RESID_LO, 0.0),
      0.0,
      "residual == LO must produce exactly zero strength"
    );
    // Any residual at/below LO is clamped to the bottom of the ramp -> 0.0, even
    // on a perfectly flat (zero-activity) region where the edge term is 1.0.
    assert_eq!(
      dither_strength(DITHER_RESID_LO - 10.0, 0.0),
      0.0,
      "residual below LO must still produce exactly zero strength"
    );
  }

  #[test]
  fn dither_strength_high_residual_smooth_engages() {
    // On a smooth (zero-activity) region a maximally-bad residual runs at the
    // full configured strength: ramp == 1, edge == 1, so strength == MAX_STRENGTH.
    let s = dither_strength(DITHER_RESID_HI, 0.0);
    assert!(
      (s - DITHER_MAX_STRENGTH).abs() <= 1e-6,
      "resid==HI, activity==0 must equal DITHER_MAX_STRENGTH ({DITHER_MAX_STRENGTH}), got {s}"
    );
    // Well past HI clamps to the same full strength (the ramp saturates at 1).
    let s2 = dither_strength(DITHER_RESID_HI * 4.0, 0.0);
    assert!(
      (s2 - DITHER_MAX_STRENGTH).abs() <= 1e-6,
      "resid well above HI must still equal DITHER_MAX_STRENGTH, got {s2}"
    );
  }

  #[test]
  fn dither_strength_edge_is_suppressed() {
    // A hard edge / busy texture (activity at/above EDGE_ACT) suppresses dither
    // to the edge floor (MAX_STRENGTH * EDGE_FLOOR) even at a maximally-bad
    // residual: ramp == 1, edge == EDGE_FLOOR, so strength == MAX * FLOOR. A
    // NONZERO floor deliberately keeps a trace of dither across edges so a
    // smooth high-residual pixel hugging an edge still propagates error (no
    // 1-px solid halo column) — see selective_dither_no_nearest_halo_beside_edge.
    let floor_strength = DITHER_MAX_STRENGTH * DITHER_EDGE_FLOOR;
    let edge = dither_strength(DITHER_RESID_HI, DITHER_EDGE_ACT);
    assert!(
      (edge - floor_strength).abs() <= 1e-6,
      "resid==HI but activity>=EDGE_ACT must suppress to MAX*EDGE_FLOOR \
       ({floor_strength}), got {edge}"
    );
    let edge_hi = dither_strength(DITHER_RESID_HI, DITHER_EDGE_ACT * 2.0);
    assert!(
      (edge_hi - floor_strength).abs() <= 1e-6,
      "activity well above EDGE_ACT must stay suppressed to MAX*EDGE_FLOOR \
       ({floor_strength}), got {edge_hi}"
    );
    // Monotonic suppression: for the same high residual, a flat region must
    // dither strictly more than a hard edge.
    let flat = dither_strength(DITHER_RESID_HI, 0.0);
    assert!(
      flat > edge,
      "flat-region strength {flat} must be strictly greater than edge strength {edge}"
    );
  }

  #[test]
  fn dither_deadzone_drops_subthreshold() {
    // HARD dead-zone: magnitudes strictly below the threshold drop to exactly 0,
    // magnitudes at/above pass through UNCHANGED (no soft shrink -> no energy
    // loss / color drift above threshold).
    assert_eq!(dither_deadzone(1.5), 0.0, "|1.5| < DEADZONE must drop to 0");
    assert_eq!(
      dither_deadzone(-1.0),
      0.0,
      "|-1.0| < DEADZONE must drop to 0"
    );
    assert_eq!(
      dither_deadzone(DITHER_ERR_DEADZONE - 0.001),
      0.0,
      "just below DEADZONE must drop to 0"
    );
    // At/above threshold: passed through with no change (proves HARD, not soft).
    assert_eq!(
      dither_deadzone(3.0),
      3.0,
      "|3.0| >= DEADZONE must pass unchanged"
    );
    assert_eq!(
      dither_deadzone(-10.0),
      -10.0,
      "|-10.0| >= DEADZONE must pass unchanged"
    );
    assert_eq!(
      dither_deadzone(DITHER_ERR_DEADZONE),
      DITHER_ERR_DEADZONE,
      "exactly DEADZONE must pass unchanged (boundary is inclusive above)"
    );
  }

  #[test]
  fn dither_clamp_err_bounds_accumulated_error() {
    // Symmetric clamp to [-DITHER_ERR_CLAMP, DITHER_ERR_CLAMP]; values inside the
    // band pass through untouched.
    assert_eq!(
      dither_clamp_err(200.0),
      DITHER_ERR_CLAMP,
      "200 must clamp down to +DITHER_ERR_CLAMP"
    );
    assert_eq!(
      dither_clamp_err(-200.0),
      -DITHER_ERR_CLAMP,
      "-200 must clamp up to -DITHER_ERR_CLAMP"
    );
    assert_eq!(
      dither_clamp_err(50.0),
      50.0,
      "50 is within the band and must pass through unchanged"
    );
    assert_eq!(
      dither_clamp_err(DITHER_ERR_CLAMP),
      DITHER_ERR_CLAMP,
      "exactly +CLAMP stays put"
    );
    assert_eq!(
      dither_clamp_err(-DITHER_ERR_CLAMP),
      -DITHER_ERR_CLAMP,
      "exactly -CLAMP stays put"
    );
  }

  #[test]
  fn selective_dither_keeps_clean_flat_region_flat() {
    // A flat region the palette covers EXACTLY must stay a single index: zero
    // residual -> zero strength -> no diffusion churn (the DEFLATE win). Build a
    // palette containing the exact field color C and a couple of decoys, then
    // remap an all-C buffer.
    let c = rgba(100, 150, 200, 255);
    let palette = vec![
      rgba(0, 0, 0, 255),
      c,
      rgba(255, 255, 255, 255),
      rgba(30, 60, 90, 255),
    ];
    let c_idx = nearest(&palette, c) as u8;
    let (w, h) = (8usize, 5usize);
    let px = vec![c; w * h];

    let dith = remap_dither(&px, w, h, &palette, 0);
    assert_eq!(dith.len(), w * h);
    assert!(
      dith.iter().all(|&i| i == c_idx),
      "clean flat region must stay the single exact-color index {c_idx}; got {dith:?}"
    );
    assert_eq!(
      distinct_indices(&dith),
      1,
      "a cleanly-covered flat must yield exactly ONE distinct index"
    );

    // Nearest gives the same uniform result here (no contrast on a clean flat).
    let near = remap_nearest(&px, &palette, 0);
    assert!(
      near.iter().all(|&i| i == c_idx),
      "nearest remap of a clean flat must also be the single exact index"
    );
  }

  #[test]
  fn selective_dither_smooths_what_nearest_would_band() {
    // Banding-protection regression. Palette = two far-apart colors; pixel buffer
    // = a uniform MID-GRAY field (smooth, zero source activity) whose residual to
    // either palette entry is large. A flat nearest remap maps every pixel to the
    // single closest entry (posterizes -> bands); selective dither re-engages on
    // this smooth high-residual region and mixes BOTH indices.
    let palette = vec![rgba(0, 0, 0, 255), rgba(255, 255, 255, 255)];
    let (w, h) = (8usize, 6usize);
    let px = vec![rgba(128, 128, 128, 255); w * h];

    let near = remap_nearest(&px, &palette, 0);
    let dith = remap_dither(&px, w, h, &palette, 0);

    let near_distinct = distinct_indices(&near);
    let dith_distinct = distinct_indices(&dith);

    assert_eq!(
      near_distinct, 1,
      "nearest must BAND the mid-gray field to a single index, got {near_distinct} distinct"
    );
    assert_eq!(
      dith_distinct, 2,
      "selective dither must mix BOTH palette indices on the smooth high-residual \
       field, got {dith_distinct} distinct"
    );
  }

  #[test]
  fn selective_dither_endpoint_tone_still_dithers() {
    // Endpoint dead-band regression (Codex finding). With palette {black, white}
    // and a flat near-endpoint tone, the inbound DITHER_ERR_CLAMP must be large
    // enough to push `want` across the 128 decision threshold so the field
    // DITHERS rather than collapsing to a flat endpoint. At the shipped 192 the
    // dead-band width `max(0, ceil(255/2) - CLAMP)` is 0, so both the bright
    // (T=224) and dark (T=31) near-endpoint fields mix in the opposite color.
    // The midpoint-only `selective_dither_smooths_what_nearest_would_band`
    // (T=128) never exercised the endpoints, where the old clamp=96 was inert.
    let palette = vec![rgba(0, 0, 0, 255), rgba(255, 255, 255, 255)];
    let (w, h) = (16usize, 16usize);

    // Bright near-white endpoint: must mix in BLACK, not collapse to flat white.
    let top = vec![rgba(224, 224, 224, 255); w * h];
    let top_dith = remap_dither(&top, w, h, &palette, 0);
    assert!(
      distinct_indices(&top_dith) >= 2,
      "flat T=224 field must DITHER (mix in black), not collapse to flat white; \
       got {} distinct",
      distinct_indices(&top_dith)
    );

    // Dark near-black endpoint (symmetric): must mix in WHITE, not flat black.
    let bot = vec![rgba(31, 31, 31, 255); w * h];
    let bot_dith = remap_dither(&bot, w, h, &palette, 0);
    assert!(
      distinct_indices(&bot_dith) >= 2,
      "flat T=31 field must DITHER (mix in white), not collapse to flat black; \
       got {} distinct",
      distinct_indices(&bot_dith)
    );
  }

  #[test]
  fn selective_dither_no_nearest_halo_beside_edge() {
    // Edge-halo regression (Codex finding). A SMOOTH, HIGH-residual field placed
    // immediately beside a HARD edge column must still dither in the column that
    // hugs the edge — it must NOT collapse to a single nearest-mapped (solid)
    // index. The bug: source activity is the MAX 4-neighbor delta, so the
    // edge-adjacent column sees the full edge step and the edge multiplier ramps
    // it to DITHER_EDGE_FLOOR. With a zero floor (the old value) strength there
    // is exactly 0, so that column propagates NO outbound error even though it
    // still consumes inbound error at index selection — every pixel maps to the
    // single nearest entry => a 1-px solid column hugging the edge inside an
    // otherwise-dithered gradient (the halo). A small nonzero floor restores
    // >= MAX*FLOOR dither on that column.
    //
    // Construction: column 0 is a hard BLACK edge; columns 1.. are a uniform
    // mid-gray field. Palette = {black, white} contains NO gray, so the gray
    // residual is large and the INTERIOR (zero-activity) gray columns all
    // dither. The smooth column at x==1 is adjacent to the black edge, so its
    // activity is the full black-vs-gray step => it is the pixel the edge gate
    // can zero. Verified empirically: at FLOOR=0.0 column 1 is solid
    // ([1,1,1,...], distinct==1, the halo); at the shipped FLOOR=0.2 it dithers
    // ([1,0,1,0,...], distinct==2). This test FAILS at EDGE_FLOOR=0.0.
    let palette = vec![rgba(0, 0, 0, 255), rgba(255, 255, 255, 255)];
    let (w, h) = (10usize, 16usize);
    let mut px = vec![rgba(128, 128, 128, 255); w * h];
    for y in 0..h {
      px[y * w] = rgba(0, 0, 0, 255); // column 0 = hard black edge
    }

    let dith = remap_dither(&px, w, h, &palette, 0);

    // The smooth column IMMEDIATELY beside the edge (x == 1) must dither, not be
    // a single solid nearest-mapped index. (At EDGE_FLOOR=0.0 this is the halo:
    // it would be a single distinct index.)
    let col1: Vec<u8> = (0..h).map(|y| dith[y * w + 1]).collect();
    assert!(
      distinct_indices(&col1) >= 2,
      "the smooth high-residual column hugging the hard edge must still dither \
       (>= 2 distinct indices), not collapse to a solid nearest-mapped column; \
       got {col1:?} -- this is the edge halo and means EDGE_FLOOR is zero"
    );

    // And it must dither comparably to a smooth INTERIOR column far from the edge
    // (x == w/2), whose activity is zero: the edge-adjacent column must not be
    // suppressed below the interior's behavior.
    let interior: Vec<u8> = (0..h).map(|y| dith[y * w + w / 2]).collect();
    assert!(
      distinct_indices(&col1) >= distinct_indices(&interior),
      "edge-adjacent column {col1:?} must dither at least as richly as the \
       interior column {interior:?} (no halo suppression beside the edge)"
    );
  }

  #[test]
  fn dither_source_activity_treats_alpha_edge_as_edge() {
    // Alpha-edge regression (Codex finding). A VISIBLE pixel whose only
    // differing 4-neighbor is FULLY TRANSPARENT sits on a sprite/cutout
    // silhouette — a real, visible hard edge. The activity gate must classify
    // it as an edge (>= DITHER_EDGE_ACT) so dither is suppressed there, exactly
    // like a spatial color edge.
    //
    // PROVE-FAILS: under the OLD `if n.a == 0 { return; }` skip the transparent
    // neighbor was ignored and the only remaining neighbor is the SAME visible
    // color, so a color-only activity is 0 (< DITHER_EDGE_ACT) and this assert
    // would fail. The fix treats the transparent neighbor as canonical
    // (0,0,0,0): dist2(opaque, (0,0,0,0)) includes da*da*ALPHA_WEIGHT
    // (255*255*3 = 195075) plus the alpha-scaled color term, far above
    // DITHER_EDGE_ACT (2048).
    //
    // Grid (3x1): [visible | visible | transparent]. The center visible pixel
    // (x==1) has a same-color visible neighbor on the left and a fully
    // transparent neighbor on the right.
    let here = rgba(200, 120, 60, 255);
    let px = vec![here, here, rgba(0, 0, 0, 0)];
    let act = dither_source_activity(&px, 3, 1, 1, 0, here, 0);
    assert!(
      act >= DITHER_EDGE_ACT,
      "a visible pixel beside a fully-transparent neighbor must register as an \
       edge (activity {act} >= DITHER_EDGE_ACT {DITHER_EDGE_ACT}); the alpha \
       discontinuity is a real silhouette edge -- under the old a==0 skip this \
       would be 0 (identical visible neighbor only)"
    );

    // The same visible color with NO transparent (or differing) neighbor stays
    // a smooth interior: activity 0. This anchors that the edge above comes
    // from the alpha boundary, not the pixel's brightness.
    let interior = vec![here, here, here];
    let act_interior = dither_source_activity(&interior, 3, 1, 1, 0, here, 0);
    assert_eq!(
      act_interior, 0.0,
      "a visible pixel surrounded by the same visible color is a smooth \
       interior (activity 0); got {act_interior}"
    );
  }

  #[test]
  fn selective_dither_is_deterministic() {
    // The f32 strength path is pure over integer-dist2 inputs (no RNG, no map
    // iteration in control flow), so the same input yields byte-identical indices
    // on every run. Use a multi-color, multi-row input that genuinely diffuses.
    let palette = vec![
      rgba(0, 0, 0, 255),
      rgba(255, 255, 255, 255),
      rgba(200, 30, 30, 255),
      rgba(30, 30, 200, 255),
    ];
    let (w, h) = (12usize, 9usize);
    let mut px = Vec::with_capacity(w * h);
    let mut g = Lcg(0x0DDF_00D5_1234_5678);
    for _ in 0..(w * h) {
      px.push(rgba(g.byte(), g.byte(), g.byte(), 255));
    }

    let a = remap_dither(&px, w, h, &palette, 0);
    let b = remap_dither(&px, w, h, &palette, 0);
    assert_eq!(
      a, b,
      "remap_dither must produce byte-identical index buffers on identical input"
    );
  }

  #[test]
  fn dither_matte_rgb_does_not_leak_into_opaque_neighbor() {
    // Partial-alpha matte-leak regression (Codex finding, premul-visibility fix).
    // A near-transparent FRINGE pixel (a == 1) carries an INVISIBLE matte RGB.
    // Because dist2's alpha term dominates SELECTION, that fringe maps to a
    // low-alpha palette entry whose color residual is large — so without
    // visibility weighting its FULL straight-alpha matte RGB (up to ~255) is
    // diffused into NEIGHBORS, including fully-opaque ones, flipping their index
    // purely on color the fringe can never show. The fix scales the diffused
    // COLOR error by `vis = src.a / 255`, so the opaque neighbor's output must be
    // INVARIANT to the fringe's matte RGB.
    //
    // FIXED palette deliberately excludes the fringe color (no white/black low-
    // alpha slot), forcing a nonzero color residual at the fringe:
    let palette = vec![
      rgba(0, 0, 0, 0),         // 0: fully transparent
      rgba(0, 0, 0, 255),       // 1: opaque black
      rgba(255, 255, 255, 255), // 2: opaque white
    ];
    // 5x1 serpentine row (l2r): three a==1 fringe pixels, then an OPAQUE mid-gray
    // (124) DOWNSTREAM of the fringe (receives the fringe's forward-diffused
    // error), then a fully-transparent tail. The two variants differ ONLY in the
    // fringe's invisible matte RGB: BRIGHT = white matte, DARK = black matte.
    let bright = vec![
      rgba(255, 255, 255, 1),
      rgba(255, 255, 255, 1),
      rgba(255, 255, 255, 1),
      rgba(124, 124, 124, 255), // opaque neighbor under test (index 3)
      rgba(0, 0, 0, 0),
    ];
    let dark = vec![
      rgba(0, 0, 0, 1),
      rgba(0, 0, 0, 1),
      rgba(0, 0, 0, 1),
      rgba(124, 124, 124, 255), // opaque neighbor under test (index 3)
      rgba(0, 0, 0, 0),
    ];
    let rb = remap_dither(&bright, 5, 1, &palette, 0);
    let rd = remap_dither(&dark, 5, 1, &palette, 0);

    assert_eq!(
      rb[3], rd[3],
      "the OPAQUE mid-gray neighbor's index must NOT depend on the invisible \
       matte RGB of the near-transparent (a==1) fringe; bright-matte buffer \
       {rb:?} vs dark-matte buffer {rd:?}. PROVE-FAILS without `* vis`: the \
       opaque index flips (2 for the white matte vs 1 for the black matte) as \
       the fringe diffuses its full invisible RGB into the opaque pixel."
    );
  }

  #[test]
  fn dither_visibility_scale_is_opaque_neutral() {
    // Opaque-neutrality invariant. The premul `vis = src.a / 255` scale must be a
    // strict no-op on an ALL-OPAQUE buffer: every src.a == 255 -> vis == 1.0
    // exactly in f32 (255.0/255.0 == 1.0), so the diffused color error — and thus
    // every output index — is byte-identical to the unscaled path. We pin this by
    // forcing a NONZERO residual (palette = {black, white} has no gray) over a
    // multi-tone, multi-row opaque buffer that genuinely diffuses, then asserting
    // the result equals a PINNED INDEPENDENT ORACLE captured from a real run under
    // the correct /255 scale. A wrong `src.a / 256.0` scale would change BOTH the
    // got and any `again` recompute identically (so the old got==again check was
    // vacuous — it only proved determinism), but it CANNOT match this pin: under
    // /256 several diffused indices shift off the pinned values.
    // All-opaque midtone buffer (tones 96..=159, clustered right around the 128
    // black/white decision threshold) over a multi-row 10x6 field. Sitting on the
    // rounding edge is what makes the pin SENSITIVE: a 0.39% (255 vs 256) opaque
    // vis scale nudges the accumulated diffusion error across the threshold on
    // EIGHT pixels, shifting those indices off the pin. A smooth gradient or
    // wide-range noise washes that signal out; midtone clustering preserves it.
    // The deterministic LCG seed was selected (offline) to maximize that shift.
    let palette = vec![rgba(0, 0, 0, 255), rgba(255, 255, 255, 255)];
    let (w, h) = (10usize, 6usize);
    let mut px = Vec::with_capacity(w * h);
    let mut g = Lcg(0x1000_0000_0000_0001u64.wrapping_mul(169));
    for _ in 0..(w * h) {
      let t = 96 + (g.byte() % 64); // 96..=159, all opaque
      px.push(rgba(t, t, t, 255));
    }

    let got = remap_dither(&px, w, h, &palette, 0);

    // The opaque path must genuinely dither (both indices present), otherwise this
    // guard would be vacuous (a flat result is trivially scale-invariant).
    assert!(
      distinct_indices(&got) >= 2,
      "test setup must produce a genuinely dithered (multi-index) opaque buffer; \
       got {} distinct",
      distinct_indices(&got)
    );

    // Pinned independent oracle: the EXACT index buffer produced by the final
    // build (correct vis = src.a / 255, DITHER_ERR_CLAMP = 192) on the all-opaque
    // input above. Because vis == 1.0 on every opaque pixel, this is also the
    // unscaled-outbound-error result; an off-by-one 255/256 scale shifts eight
    // indices and breaks the pin.
    // Regenerate this pin ONLY on an intentional dither-calibration change (edits
    // to remap_dither / dither_strength / DITHER_* consts).
    // Oracle regenerated for P3 Phase 2: index selection now uses the perceptual
    // `pdist` (CIE76 ΔE) instead of RGB `dist2`, so the black/white decision
    // threshold over this midtone field shifts and several indices move; the
    // opaque-neutrality property this test guards (vis == 1.0 on every opaque pixel,
    // so a /256 mis-scale still breaks the pin) is unchanged and metric-independent.
    let expected: Vec<u8> = vec![
      1, 0, 1, 1, 1, 1, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 1, 1, 1, 0, 1, 1, 0, 1, 1, 0,
      1, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 1, 1, 0, 1, 0, 1, 1, 0, 0, 0, 1, 0, 0, 1, 0, 0, 1, 1,
    ];
    assert_eq!(
      got, expected,
      "opaque dithered output must match the pinned /255 oracle; a 255/256 (or any \
       != 1.0) opaque vis scale shifts diffused indices off this pin"
    );
  }

  #[test]
  fn wu_split_premultiplied_respects_alpha_visibility() {
    // FIX A canary: the Wu split's RGB first moments are PREMULTIPLIED by alpha
    // (`Moments::add`) so near-INVISIBLE RGB cannot dominate the split. These four
    // colors (count 1 each) are a Codex K=2 counterexample: the two near-opaque
    // colors carry the visible signal, the two near-transparent ones (a==64, a==8)
    // carry large but barely-visible RGB. With raw-RGB moments the Wu split groups
    // by raw RGB and total nearest-`dist2` is 4459; premultiplying the RGB moments
    // makes the split respect `dist2`'s `wa/510` visibility weighting, dropping the
    // total to the visibility optimum (654). PROVE-FAILS on raw-RGB moments
    // (revert FIX A -> total 4459 >> 654).
    let colors = [
      rgba(192, 32, 224, 1),
      rgba(32, 64, 96, 1),
      rgba(16, 0, 0, 64),
      rgba(0, 160, 255, 8),
    ];
    let entries = make_entries(&colors.map(|c| (c, 1u64)));
    let pal = median_cut(&entries, 2, None);
    let total: i64 = colors
      .iter()
      .map(|&c| dist2(pal[nearest(&pal, c)], c))
      .sum();
    assert!(
      total <= 654,
      "premultiplied Wu split must respect dist2 visibility weighting: total \
       nearest-dist2 over the 4 colors must be <= 654 (the visibility optimum), \
       got {total} (raw-RGB moments score 4459); palette={pal:?}"
    );
  }

  #[test]
  fn kmeans_uses_true_population_not_capped_weights() {
    // FIX B canary: `cap_entry_weights` caps only the COPY fed to the Wu split;
    // k-means must see the TRUE populations. The cap's `.max(1)` floor is the
    // distortion vector: when one color's count vastly exceeds the cap it is scaled
    // DOWN toward the floor, while a swarm of rare colors each stay floored at 1 —
    // so the capped histogram massively over-represents the rare swarm.
    //
    // Construction: a dominant color (R=40) with count cap*100, plus 327,680
    // DISTINCT rare colors clustered at R=240 (R in 238..=242 over every G,B),
    // count 1 each. True population: the dominant is ~20,000:1, so the lone
    // centroid's R rounds to 40. Capped population: dominant -> ~cap, each rare ->
    // 1, total ~2*cap, so the rare swarm pulls the centroid's R up to 41.
    let cap: u64 = 1 << 26;
    let dominant = rgba(40, 80, 120, 255);
    let mut items: Vec<(RGBA8, u64)> = Vec::with_capacity(327_681);
    items.push((dominant, cap * 100));
    for r in 238u16..=242 {
      for g in 0u16..256 {
        for b in 0u16..256 {
          items.push((rgba(r as u8, g as u8, b as u8, 255), 1));
        }
      }
    }
    let entries = make_entries(&items);

    // Sanity: the population genuinely trips the cap (the split copy IS scaled),
    // and the cap's `.max(1)` floor keeps every rare color at weight 1.
    let mut split_copy = entries.clone();
    cap_entry_weights(&mut split_copy, cap as u128);
    let true_total: u128 = entries.iter().map(|e| e.count as u128).sum();
    let cap_total: u128 = split_copy.iter().map(|e| e.count as u128).sum();
    assert!(
      true_total > cap as u128 && cap_total <= cap as u128 + entries.len() as u128,
      "cap must bite: true_total={true_total} cap_total={cap_total}"
    );

    // True-population path: one slot, one k-means pass over the TRUE entries. Every
    // entry assigns to the lone slot, so the centroid is the true weighted mean;
    // R rounds to the dominant's 40.
    let mut palette = vec![rgba(128, 128, 128, 255)];
    kmeans_refine(&mut palette, &entries, 1);
    assert_eq!(
      palette[0].r, 40,
      "k-means must use TRUE populations: the dominant R=40 color (count cap*100) \
       must pull the lone centroid's R to 40, got {:?}",
      palette[0]
    );

    // PROVE-FAIL guard: feeding the CAPPED copy to the SAME path distorts the
    // centroid — the over-weighted rare swarm drags R off 40 (to 41). This pins
    // that the cap genuinely changes k-means output, so passing the true entries
    // is load-bearing, not vacuous.
    let mut capped_palette = vec![rgba(128, 128, 128, 255)];
    kmeans_refine(&mut capped_palette, &split_copy, 1);
    assert_ne!(
      capped_palette[0].r, palette[0].r,
      "the capped copy MUST distort the centroid (proving FIX B matters): capped \
       R={} should differ from true R={}",
      capped_palette[0].r, palette[0].r
    );
  }

  #[test]
  fn kmeans_guard_never_worsens_objective() {
    // FIX #2 canary: k-means here is NOT monotone. The assignment minimizes the
    // alpha-weighted `dist2`, but the centroid update is the plain count-weighted
    // RGBA mean, which is not that metric's minimizer when alpha varies (the
    // per-pair RGB weight `(a_i+a_c)/510` couples to the center's own alpha). So a
    // pass can RAISE the objective — and the unguarded code kept it unconditionally,
    // which could trip `min_quality` and force the 256-color retry. The keep-best
    // guard makes `kmeans_refine` return a palette whose objective is `<=` the
    // seed's.
    //
    // These 7 partial-alpha entries are a verified counterexample under the perceptual
    // `pdist` objective: the K=2 median-cut seed scores 10_622_975_578, and ONE raw
    // (unguarded) refine pass RAISES it to 10_686_135_592. The guard must REJECT that
    // pass and keep the seed. This exercises the guard's REJECTION path (not just
    // adoption): we compute the unguarded pass HERE (not pin it from a comment), prove
    // it worsens, and prove `kmeans_refine` returns the seed objective instead.
    // (Found by `unguarded_kmeans_pass` + an exhaustive deterministic search over
    // random low-alpha inputs; no cluster empties, so the pass needs no reseed.)
    let entries = make_entries(&[
      (rgba(10, 245, 33, 32), 524),
      (rgba(68, 231, 156, 22), 1518),
      (rgba(116, 106, 11, 27), 851),
      (rgba(225, 232, 89, 20), 534),
      (rgba(231, 158, 17, 14), 1635),
      (rgba(231, 250, 145, 23), 1948),
      (rgba(236, 35, 221, 15), 575),
    ]);

    // Seed: the K=2 median-cut palette over the TRUE entries.
    let seed = median_cut(&entries, 2, None);
    let obj_seed = kmeans_objective(&seed, &entries);
    assert_eq!(
      obj_seed, 10_622_975_578,
      "perceptual seed objective pin (median_cut K=2)"
    );

    // What ONE UNGUARDED pass would produce — computed here, not pinned from a comment,
    // so the rejection coverage is real. It WORSENS the seed (the non-monotone case the
    // guard exists to catch: assignment minimizes `pdist`, but the count-weighted RGBA
    // mean centroid is not that metric's minimizer when alpha varies).
    let unguarded = unguarded_kmeans_pass(&seed, &entries).expect("no cluster empties here");
    let obj_unguarded = kmeans_objective(&unguarded, &entries);
    assert_eq!(
      obj_unguarded, 10_686_135_592,
      "unguarded one-pass objective pin"
    );
    assert!(
      obj_unguarded > obj_seed,
      "setup: one raw pass must WORSEN the seed so this tests the guard's REJECTION \
       path (obj_unguarded={obj_unguarded} <= obj_seed={obj_seed})"
    );

    // The guard must REJECT the worsening pass and return the seed objective.
    let mut guarded = seed.clone();
    kmeans_refine(&mut guarded, &entries, 1);
    let obj_guarded = kmeans_objective(&guarded, &entries);
    assert!(
      obj_guarded <= obj_seed,
      "guard must never worsen the objective: obj_guarded={obj_guarded} > obj_seed={obj_seed}"
    );
    assert_eq!(
      obj_guarded, obj_seed,
      "guard must REJECT the worsening pass and keep the seed objective (got \
       {obj_guarded}, seed {obj_seed}, unguarded {obj_unguarded}); guarded={guarded:?}"
    );
    assert!(
      obj_guarded < obj_unguarded,
      "the guarded result must be strictly better than the unguarded pass it rejected"
    );
  }

  #[test]
  fn median_cut_centroid_uses_true_population_at_cap() {
    // FIX #3 canary (median_cut analogue of `kmeans_uses_true_population_not_capped
    // _weights`): the Wu split DECISIONS run on the capped `split_entries`, but each
    // box's FINAL centroid VALUE must use the TRUE per-color populations — under the
    // SAME box membership, with NO nearest re-assignment. `cap_entry_weights`'s
    // `.max(1)` floor over-represents a swarm of rare colors, which can round the
    // centroid to a different `u8`.
    //
    // Construction: a dominant color (R=40, count cap*100) plus 327,680 DISTINCT rare
    // colors at R in 238..=242 (count 1 each). One box (`max_colors==1`) holds them
    // all. TRUE population: dominant is ~20,000:1, so the centroid R rounds to 40.
    // CAPPED population: dominant -> ~cap, each rare -> 1 (floored), so the rare swarm
    // pulls the centroid R up to 41.
    let cap: u64 = 1 << 26;
    let dominant = rgba(40, 80, 120, 255);
    let mut items: Vec<(RGBA8, u64)> = Vec::with_capacity(327_681);
    items.push((dominant, cap * 100));
    for r in 238u16..=242 {
      for g in 0u16..256 {
        for b in 0u16..256 {
          items.push((rgba(r as u8, g as u8, b as u8, 255), 1));
        }
      }
    }
    let entries = make_entries(&items);

    // The split copy fed to median_cut is capped; the true-count map is built from
    // the ORIGINAL entries (packed color -> true count), as quantize_pass does.
    let mut split = entries.clone();
    cap_entry_weights(&mut split, cap as u128);
    let true_map: HashMap<u32, u64> = entries.iter().map(|e| (packed(e.color), e.count)).collect();

    // Sanity: the cap genuinely bit (the split copy IS scaled).
    let true_total: u128 = entries.iter().map(|e| e.count as u128).sum();
    assert!(
      true_total > cap as u128,
      "cap must bite: true_total={true_total}"
    );

    // True-population centroid: the dominant R=40 pulls the lone box's centroid to 40.
    let true_centroid = median_cut(&split, 1, Some(&true_map))[0];
    assert_eq!(
      true_centroid.r, 40,
      "median_cut must value the centroid with TRUE populations: R must be 40, got {true_centroid:?}"
    );

    // PROVE-FAIL guard: taking the centroid from the CAPPED copy (pass `None`) shifts
    // R to 41 — proving the true-count path is load-bearing, not vacuous.
    let capped_centroid = median_cut(&split, 1, None)[0];
    assert_ne!(
      capped_centroid.r, true_centroid.r,
      "the capped copy MUST shift the centroid (proving FIX #3 matters): capped R={} \
       should differ from true R={}",
      capped_centroid.r, true_centroid.r
    );
  }

  // ---- P3 Phase 2: perceptual `pdist` ASSIGNMENT metric ----
  //
  // These pin that `pdist` is genuinely PERCEPTUAL (CIE76 ΔE), not a renamed RGB
  // metric, that its alpha term is balanced against the Lab color term, and that
  // it is deterministic. `pdist` (and thus `nearest`/`nearest_lab`,
  // `kmeans_objective`, the D² reseed, and remap index selection) is the only path
  // switched to perceptual; `dist2` (quality gate, dither residual/activity, Wu
  // split) is unchanged, so the `dist2`-value tests above still pass untouched.

  #[test]
  fn pdist_is_perceptual_not_rgb_euclidean() {
    // The metric must order by CIE76 ΔE, NOT by raw RGB-Euclidean — and on a pair
    // where the two DISAGREE. Reference = opaque black. Candidate A = opaque WHITE
    // (255,255,255); candidate B = opaque RED (255,0,0). All opaque, so `pdist`'s
    // color term is exactly the CIE76 ΔE² (wa==510 -> wa/510==1) and the alpha term
    // is 0.
    //
    //   Plain RGB-Euclidean from black: white = 3·255² = 195075 (FARTHER),
    //                                    red   =   255² =  65025 (CLOSER).
    //   Perceptual ΔE² from black:       white ≈ (100·100)² = 1e8-scale (CLOSER),
    //                                    red   ≈ (~117·100)²        (FARTHER).
    // So RGB says "red is nearer black", but perceptually white is nearer (red is a
    // high-chroma saturated color, white is pure luminance). `pdist` must agree with
    // PERCEPTION (white nearer), i.e. REVERSE the RGB-Euclidean verdict.
    let black = rgba(0, 0, 0, 255);
    let white = rgba(255, 255, 255, 255);
    let red = rgba(255, 0, 0, 255);

    // Raw RGB-Euclidean (no alpha weighting) — the metric `pdist` must NOT mimic.
    let rgb_euclid = |p: RGBA8, q: RGBA8| -> i64 {
      let dr = p.r as i64 - q.r as i64;
      let dg = p.g as i64 - q.g as i64;
      let db = p.b as i64 - q.b as i64;
      dr * dr + dg * dg + db * db
    };
    assert!(
      rgb_euclid(black, white) > rgb_euclid(black, red),
      "setup: RGB-Euclidean must rank WHITE farther from black than RED \
       ({} vs {})",
      rgb_euclid(black, white),
      rgb_euclid(black, red)
    );

    // `pdist` must REVERSE that: white is perceptually CLOSER to black than red.
    let pd_white = pdist(black, white);
    let pd_red = pdist(black, red);
    assert!(
      pd_white < pd_red,
      "pdist must be perceptual: white must rank CLOSER to black than red \
       (pdist white={pd_white}, red={pd_red}); a renamed RGB metric would rank \
       red closer and FAIL here"
    );

    // And it really is the ΔE color term (all opaque -> pdist == delta_e76_sq).
    assert_eq!(
      pd_white,
      delta_e76_sq(rgb_to_lab(0, 0, 0), rgb_to_lab(255, 255, 255)),
      "opaque pdist must equal the bare CIE76 ΔE² (wa/510 == 1, alpha term 0)"
    );
  }

  #[test]
  fn pdist_alpha_balance() {
    // The alpha term (ALPHA_WEIGHT_LAB) must be balanced: a FULL alpha flip
    // outranks a small color change, but a TINY alpha change must not dominate a
    // LARGE color change.

    // (1) Full alpha flip (same RGB) outranks a small color change (same alpha).
    //   - alpha flip: opaque vs fully-transparent gray, da==255.
    //   - small color change: two near-identical opaque grays (ΔE tiny).
    let gray_opaque = rgba(128, 128, 128, 255);
    let gray_transp = rgba(128, 128, 128, 0);
    let gray_opaque2 = rgba(130, 131, 129, 255); // ~2-LSB color nudge, fully opaque
    let alpha_flip = pdist(gray_opaque, gray_transp);
    let small_color = pdist(gray_opaque, gray_opaque2);
    assert!(
      alpha_flip > small_color,
      "a full alpha flip must outrank a small color change \
       (alpha_flip={alpha_flip}, small_color={small_color})"
    );

    // (2) A TINY alpha change (da==1) must NOT dominate a LARGE color change.
    //   - large color change: black vs white, both opaque (max ΔE).
    //   - tiny alpha change: same opaque gray with alpha 255 vs 254.
    let black = rgba(0, 0, 0, 255);
    let white = rgba(255, 255, 255, 255);
    let large_color = pdist(black, white);
    let a255 = rgba(128, 128, 128, 255);
    let a254 = rgba(128, 128, 128, 254);
    let tiny_alpha = pdist(a255, a254);
    assert!(
      large_color > tiny_alpha,
      "a tiny alpha change (da=1) must not dominate a large color change \
       (large_color={large_color}, tiny_alpha={tiny_alpha})"
    );
    // Concretely: tiny_alpha is just 1·1·ALPHA_WEIGHT_LAB == ALPHA_WEIGHT_LAB
    // (color term 0 for identical RGB), far below a maximal ΔE² color gap.
    assert_eq!(
      tiny_alpha, ALPHA_WEIGHT_LAB,
      "da==1 with identical RGB must score exactly ALPHA_WEIGHT_LAB"
    );
  }

  #[test]
  fn pdist_is_deterministic() {
    // Integer/fixed-point only: identical inputs -> identical i64, every call.
    let samples = [
      (rgba(0, 0, 0, 255), rgba(255, 255, 255, 255)),
      (rgba(12, 200, 33, 128), rgba(240, 5, 99, 17)),
      (rgba(128, 128, 128, 0), rgba(128, 128, 128, 255)),
      (rgba(64, 128, 192, 255), rgba(200, 30, 90, 200)),
    ];
    for (p, q) in samples {
      let d1 = pdist(p, q);
      let d2 = pdist(p, q);
      assert_eq!(d1, d2, "pdist must be deterministic for {p:?} vs {q:?}");
      // Symmetric, like dist2.
      assert_eq!(pdist(p, q), pdist(q, p), "pdist must be symmetric");
      // The cached-Lab form must agree byte-for-byte with the direct form.
      let cached = pdist_lab(
        rgb_to_lab(p.r, p.g, p.b),
        p.a,
        rgb_to_lab(q.r, q.g, q.b),
        q.a,
      );
      assert_eq!(cached, d1, "pdist_lab must equal pdist for {p:?} vs {q:?}");
    }
  }

  #[test]
  fn visible_pixel_never_maps_to_reserved_transparent_slot() {
    // Regression: under `pdist` a VISIBLE saturated color can rank the reserved
    // fully-transparent slot CLOSER than a far visible entry, so without the `a == 0`
    // exclusion in `nearest_lab` a visible pixel would be mapped to transparent and
    // VANISH. Palette = [transparent slot, blue]; query opaque green. The metric ranks
    // transparent closer (green->transparent < green->blue), but assignment MUST pick
    // the visible entry (blue, index 1).
    let transparent = rgba(0, 0, 0, 0);
    let blue = rgba(0, 0, 255, 255);
    let green = rgba(0, 255, 0, 255);
    let palette = vec![transparent, blue]; // index 0 = transparent slot, 1 = blue

    // Load-bearing setup invariant (so this is NOT a vacuous test): the perceptual
    // metric really does rank the transparent slot closer than the visible entry here,
    // so it is the EXCLUSION — not the metric — that keeps the pixel visible. Without
    // the exclusion `nearest` would return 0 (transparent) and the assert below fails.
    assert!(
      pdist(green, transparent) < pdist(green, blue),
      "setup: pdist must rank transparent ({}) closer than blue ({}) for this regression \
       to be meaningful",
      pdist(green, transparent),
      pdist(green, blue)
    );

    assert_eq!(
      nearest(&palette, green),
      1,
      "a visible pixel must map to the visible entry, never the reserved transparent \
       slot (pdist green->transparent={} < green->blue={})",
      pdist(green, transparent),
      pdist(green, blue)
    );

    // A fully-transparent query, by contrast, IS allowed to pick the transparent slot.
    assert_eq!(
      nearest(&palette, transparent),
      0,
      "a transparent query must still resolve to the exact transparent slot"
    );
  }

  #[test]
  fn dithered_visible_pixel_never_vanishes_into_transparent_slot() {
    // Regression (Codex round-2 HIGH): in the DITHER path the transparent-slot
    // exclusion must key on the RAW SOURCE pixel's visibility, NOT the dither-adjusted
    // `want.a`. A row of near-transparent (a=1) but VISIBLE green pixels maps to a
    // palette whose only visible entry has higher alpha, so each pixel diffuses
    // negative alpha error forward and within a few pixels `want.a` clamps to 0. If the
    // exclusion keyed on `want.a` the reserved slot would be re-admitted and the
    // perceptual metric (which can rank transparent-black CLOSER than the lone visible
    // entry) would map these VISIBLE pixels onto the transparent slot — they would
    // VANISH. This mirrors the maxQuality:1 / speed:5 end-to-end repro at the
    // `quantize_rgba` boundary (the exact fn the JS path calls). Fails pre-fix.
    let green = rgba(0, 255, 0, 1);
    let red = rgba(255, 0, 0, 100);
    let transparent = rgba(0, 0, 0, 0);
    let px = vec![green, green, green, green, green, green, red, transparent];

    // maxQuality:1 -> max_colors 2; speed:5 -> kmeans_iters 5, dither on; posterize 0.
    let out = quantize_rgba(&px, 8, 1, &cfg(2, true, 5));

    // Load-bearing setup invariant: the reserved transparent slot must actually exist
    // (input has a fully-transparent pixel AND max_colors >= 2), else the regression is
    // vacuous — there would be no transparent entry to vanish into.
    assert!(
      out.palette.iter().any(|c| c.a == 0),
      "setup: a reserved transparent slot must exist for this regression to apply; \
       palette = {:?}",
      out.palette
    );

    // Every VISIBLE source pixel must map to a VISIBLE palette entry; only the
    // fully-transparent source pixel may resolve to the transparent slot.
    for (i, &p) in px.iter().enumerate() {
      let entry = out.palette[out.indices[i] as usize];
      if p.a > 0 {
        assert!(
          entry.a > 0,
          "visible source px{i} (a={}) mapped to a transparent palette entry (a=0) — \
           it would VANISH; entry = {entry:?}",
          p.a
        );
      } else {
        assert_eq!(
          entry.a, 0,
          "fully-transparent source px{i} must map to the transparent slot; entry = \
           {entry:?}"
        );
      }
    }
  }

  #[test]
  fn opaque_pixel_never_vanishes_onto_low_alpha_entry() {
    // Regression: an OPAQUE (a=255) source pixel can be assigned to a NON-zero but
    // LOW-alpha palette entry of the SAME hue and visually VANISH. In `pdist_lab` the
    // color term `delta_e76_sq * wa/510` (wa = query_a + entry_a) discounts color
    // distance up to ~2x for a low-alpha entry, and `da² * ALPHA_WEIGHT_LAB` is far
    // smaller than Lab color distances, so an opaque green is otherwise pulled onto a
    // same-hue near-invisible green entry instead of the correct opaque red. The
    // final-remap visibility guard (the smooth `dim_penalty` + `vanish_penalty` score
    // terms, keyed on the source alpha) makes the dimmer entry lose. This asserts the fix
    // at the `quantize_rgba` boundary; it FAILS pre-fix (the opaque greens map to a green
    // entry with a≈11).
    let mut px: Vec<RGBA8> = Vec::with_capacity(308);
    px.extend(std::iter::repeat_n(rgba(0, 200, 0, 1), 200)); // faint visible green
    px.extend(std::iter::repeat_n(rgba(0, 200, 0, 255), 8)); // opaque green
    px.extend(std::iter::repeat_n(rgba(220, 0, 0, 255), 100)); // opaque red

    // max_colors 2, no dither, 4 k-means iters; min_quality 0.
    let out = quantize_rgba(&px, 308, 1, &cfg(2, false, 4));

    // Setup invariant: there is NO transparent source pixel here, so there is no
    // reserved transparent slot — assert the palette has a visible entry (this is NOT
    // the transparent-slot regression; it is the low-alpha-but-visible vanishing case).
    assert!(
      out.palette.iter().any(|c| c.a > 0),
      "setup: palette must have a visible entry; palette = {:?}",
      out.palette
    );

    // Every OPAQUE (a=255) source pixel must map to a clearly-visible entry — here the
    // opaque red cluster (a=255). Pre-fix the opaque greens map to a low-alpha green
    // entry (a≈11) and vanish; the guard redirects them to the visible entry.
    for (i, &p) in px.iter().enumerate() {
      if p.a == 255 {
        let entry = out.palette[out.indices[i] as usize];
        assert!(
          entry.a >= 128,
          "opaque source px{i} (a=255) mapped to a low-alpha entry (a={}) — it would \
           VANISH; the dimming guard must keep it on a visible entry; entry = {entry:?}, \
           palette = {:?}",
          entry.a,
          out.palette
        );
      }
    }

    // PARTIAL-ALPHA PRESERVED: the faint (a=1) greens must still map to the LOW-alpha
    // GREEN cluster — they are NOT forced onto opaque red. The dimming guard is keyed on
    // the source alpha, so for a faint source it barely fires (small `src_a − entry_a`).
    for (i, &p) in px.iter().enumerate() {
      if p.a == 1 {
        let entry = out.palette[out.indices[i] as usize];
        assert!(
          entry.r == 0 && entry.g > 0,
          "faint (a=1) green source px{i} must map to the green cluster (r==0, g>0), \
           not red; entry = {entry:?}, palette = {:?}",
          out.palette
        );
      }
    }
  }

  #[test]
  fn opaque_pixel_never_vanishes_onto_low_alpha_entry_with_dither() {
    // Regression (DITHER ON): the same opaque-vanish as the sibling test above, but through
    // `remap_dither`. Here `want.a` stays high (~255) for the opaque pixels, so the gentle
    // quadratic `dim_penalty` is NOT enough on its own: Floyd–Steinberg COLOUR diffusion pushes
    // `want` toward a saturated green `(0,255,0)`, whose colour gap to the only opaque alternative
    // (red) grows past the quadratic penalty, so pre-fix the opaque greens are pulled onto the dim
    // green centroid (a≈29) and VANISH. The steeper cubic `vanish_penalty` dominates at this large
    // drop (255→29) and forces them onto the visible red instead. The sibling test
    // uses `dither = false`, so it exercises `remap_nearest`, not `remap_dither`, and never
    // caught this path. FAILS pre-fix (the FS-saturated opaque greens map to a≈29).
    let mut px: Vec<RGBA8> = Vec::with_capacity(172);
    px.extend(std::iter::repeat_n(rgba(0, 200, 0, 1), 64)); // faint green: drags green centroid to a≈29
    px.extend(std::iter::repeat_n(rgba(0, 200, 0, 255), 8)); // opaque green
    px.extend(std::iter::repeat_n(rgba(220, 0, 0, 255), 100)); // opaque red

    // max_colors 2, DITHER ON, 5 k-means iters; min_quality 0. width 172, height 1 so the
    // serpentine error diffusion runs along the row.
    let out = quantize_rgba(&px, 172, 1, &cfg(2, true, 5));

    // Setup invariants (keep the test non-vacuous):
    //  * no transparent source -> no reserved transparent slot (this is the low-alpha-but-
    //    visible case, not the transparent-slot regression);
    //  * the DIM green entry the penalty must out-rank genuinely exists (r==0, g>0, a<128) —
    //    without it the `>= 128` assertion below could pass vacuously.
    assert!(
      !out.palette.iter().any(|c| c.a == 0),
      "setup: no transparent source, so no transparent slot should be reserved; palette = {:?}",
      out.palette
    );
    assert!(
      out.palette.iter().any(|c| c.r == 0 && c.g > 0 && c.a < 128),
      "setup: the dim green entry (r==0, g>0, a<128) must exist for this to be the vanish \
       case; palette = {:?}",
      out.palette
    );

    // Every OPAQUE (a=255) source pixel must map to a clearly-visible entry (a>=128). Pre-fix
    // (no vanish_penalty) the FS-saturated opaque greens map to the dim green entry (a≈29) and
    // vanish; the cubic penalty redirects them to the visible red.
    for (i, &p) in px.iter().enumerate() {
      if p.a == 255 {
        let entry = out.palette[out.indices[i] as usize];
        assert!(
          entry.a >= 128,
          "opaque source px{i} (a=255) mapped to a low-alpha entry (a={}) under dither — it \
           would VANISH; the vanish_penalty must keep it on a visible entry; entry = {entry:?}, \
           palette = {:?}",
          entry.a,
          out.palette
        );
      }
    }

    // PARTIAL-ALPHA PRESERVED: the faint (a=1) greens must still map to the LOW-alpha GREEN
    // cluster — the cubic `vanish_penalty` is keyed on the source alpha, so for an `a == 1` source
    // it barely fires (the drop to a same-hue dim entry is tiny); faint (and any translucent)
    // sources keep their hue and are NOT forced onto opaque red.
    for (i, &p) in px.iter().enumerate() {
      if p.a == 1 {
        let entry = out.palette[out.indices[i] as usize];
        assert!(
          entry.r == 0 && entry.g > 0,
          "faint (a=1) green source px{i} must map to the green cluster (r==0, g>0), not red \
           (the penalty must not flip partial-alpha sources); entry = {entry:?}, palette = {:?}",
          out.palette
        );
      }
    }
  }

  #[test]
  fn near_opaque_pixel_never_vanishes_onto_low_alpha_entry_with_dither() {
    // Regression (DITHER ON, NEAR-OPAQUE source): the `vanish_penalty` is a CONTINUOUS function of
    // the source alpha, so there is no threshold (no `== 255` gate, no `>= 224` band) below which a
    // near-opaque pixel abruptly loses protection and vanishes. A pixel at `a == 254` is visually
    // opaque, and under dither it vanishes onto the dim green centroid (a≈29) exactly like a `255`
    // pixel does; the cubic penalty at the large drop (254→29) is essentially as strong as at 255→29
    // (no one-alpha cliff), so it keeps the 254 source visible too. This is the `near_opaque`
    // analogue of the sibling `opaque_pixel_..._with_dither` test, with the 8 opaque greens dropped
    // to `a == 254`. FAILS against an `a == 255`-gated hard floor (which never fires for the 254
    // sources, so the FS-saturated near-opaque greens are pulled onto the dim green entry at a≈29).
    let mut px: Vec<RGBA8> = Vec::with_capacity(172);
    px.extend(std::iter::repeat_n(rgba(0, 200, 0, 1), 64)); // faint green: drags green centroid to a≈29
    px.extend(std::iter::repeat_n(rgba(0, 200, 0, 254), 8)); // NEAR-opaque green (one below 255)
    px.extend(std::iter::repeat_n(rgba(220, 0, 0, 255), 100)); // opaque red

    // max_colors 2, DITHER ON, 5 k-means iters; min_quality 0. width 172, height 1.
    let out = quantize_rgba(&px, 172, 1, &cfg(2, true, 5));

    // Setup invariants (keep the test non-vacuous): no transparent slot, and the dim green entry
    // the penalty must out-rank genuinely exists (r==0, g>0, a<128).
    assert!(
      !out.palette.iter().any(|c| c.a == 0),
      "setup: no transparent source, so no transparent slot should be reserved; palette = {:?}",
      out.palette
    );
    assert!(
      out.palette.iter().any(|c| c.r == 0 && c.g > 0 && c.a < 128),
      "setup: the dim green entry (r==0, g>0, a<128) must exist for this to be the vanish case; \
       palette = {:?}",
      out.palette
    );

    // Every NEAR-OPAQUE (a=254) source must map to a HALF-OPAQUE-or-better entry (`entry_a >= 128`)
    // — i.e. it must not vanish onto the dim green (a≈29). The cubic `vanish_penalty` out-ranks the
    // dim green for a 254 source essentially as strongly as for a 255 one (no one-alpha cliff).
    for (i, &p) in px.iter().enumerate() {
      if p.a == 254 {
        let entry = out.palette[out.indices[i] as usize];
        assert!(
          entry.a >= 128,
          "near-opaque source px{i} (a=254) mapped to entry a={} (< half opaque) under dither — it \
           would VANISH; the vanish_penalty must keep it on a visible entry; entry = {entry:?}, \
           palette = {:?}",
          entry.a,
          out.palette
        );
      }
    }
  }

  #[test]
  fn opaque_pixel_never_vanishes_onto_low_alpha_entry_against_far_hue() {
    // Regression (FAR-HUE fallback): the anti-vanish penalty must dominate the WORST-case hue gap,
    // not just a near one. With `VANISH_WEIGHT` calibrated only against red (ΔE² ≈ 2.9e8) an opaque
    // green still vanished when its only VISIBLE alternative was a far hue like blue (ΔE² up to the
    // gamut diameter 6.69e8): the dim same-hue green out-scored blue and won. Here the visible
    // alternative is blue@255 (the farthest hue from green), and the green centroid is dragged to
    // a≈11 by faint greens. The opaque greens must still map to blue (a>=128), i.e. stay VISIBLE,
    // never onto the near-invisible green. PROVE-FAILS at a too-small weight (the opaque greens map
    // to green@11 and disappear); passes once the weight dominates `MAX_DELTA_E76_SQ`.
    let mut px: Vec<RGBA8> = Vec::with_capacity(308);
    px.extend(std::iter::repeat_n(rgba(0, 200, 0, 1), 200)); // faint green: green centroid -> a≈11
    px.extend(std::iter::repeat_n(rgba(0, 200, 0, 255), 8)); // opaque green (same hue as the dim entry)
    px.extend(std::iter::repeat_n(rgba(0, 0, 255, 255), 100)); // opaque BLUE: the far-hue visible entry

    // max_colors 2, no dither, 4 k-means iters; min_quality 0.
    let out = quantize_rgba(&px, 308, 1, &cfg(2, false, 4));

    // Setup invariants (keep the test non-vacuous): no transparent slot, a dim GREEN entry exists
    // (r==0, g>0, a<128), and the only visible entry is a FAR hue (blue: b-channel dominant).
    assert!(
      !out.palette.iter().any(|c| c.a == 0),
      "setup: no transparent source, so no transparent slot should be reserved; palette = {:?}",
      out.palette
    );
    assert!(
      out.palette.iter().any(|c| c.r == 0 && c.g > 0 && c.a < 128),
      "setup: the dim green entry (r==0, g>0, a<128) must exist for this to be the vanish case; \
       palette = {:?}",
      out.palette
    );
    assert!(
      out
        .palette
        .iter()
        .any(|c| c.b > c.r && c.b > c.g && c.a >= 128),
      "setup: the only visible entry must be the far-hue blue; palette = {:?}",
      out.palette
    );

    // Every OPAQUE green source must map to the VISIBLE blue (a>=128), accepting the hue change as
    // the price of staying visible — NOT vanish onto the near-invisible same-hue green.
    for (i, &p) in px.iter().enumerate() {
      if p.a == 255 && p.g > 0 {
        let entry = out.palette[out.indices[i] as usize];
        assert!(
          entry.a >= 128,
          "opaque green source px{i} (a=255) mapped to a low-alpha entry (a={}) when only a far-hue \
           visible entry existed — it would VANISH; the vanish_penalty must dominate the worst-case \
           hue gap; entry = {entry:?}, palette = {:?}",
          entry.a,
          out.palette
        );
      }
    }
  }

  #[test]
  fn near_opaque_pixel_never_vanishes_onto_low_alpha_entry_against_far_hue_with_dither() {
    // Regression (round-10): the far-hue guarantee must cover the whole NEAR-OPAQUE band, not just
    // `a == 255`. The cubic penalty is keyed on the source-alpha DROP, so a source at `a == 224`
    // onto a ~20%-opacity same-hue entry has a smaller drop than a 255 source does, and under dither
    // `want.a` can rise toward 255, which only WEAKENS the dim entry's disadvantage. At a too-small
    // weight a 224 source therefore still vanished onto a dim same-hue green when its only visible
    // alternative was a far hue (blue). `VANISH_WEIGHT` is sized (and compile-time-proven, see its
    // const block) so the guarantee holds across `src_a >= VANISH_GUARD_MIN_SRC` and entry alpha
    // `<= VANISH_GUARD_MAX_ENTRY` for ANY query alpha including the dither maximum. Here the green
    // centroid is dragged to a≈38 (well inside the <=50 zone) and the only visible entry is blue;
    // the near-opaque greens must map to blue (stay VISIBLE). PROVE-FAILS at the prior weight (the
    // 224 greens vanish onto the dim green under dither).
    let mut px: Vec<RGBA8> = Vec::with_capacity(308);
    px.extend(std::iter::repeat_n(rgba(0, 200, 0, 30), 200)); // faint green: centroid -> a≈38
    px.extend(std::iter::repeat_n(rgba(0, 200, 0, 224), 8)); // NEAR-opaque green (essentially solid)
    px.extend(std::iter::repeat_n(rgba(0, 0, 255, 255), 100)); // opaque BLUE: the far-hue visible entry

    // max_colors 2, DITHER ON, 5 k-means iters; min_quality 0. width 308, height 1.
    let out = quantize_rgba(&px, 308, 1, &cfg(2, true, 5));

    // Setup invariants: no transparent slot, a dim GREEN entry exists (r==0, g>0, a<=50), and the
    // only visible entry is the far hue (blue: b dominant, a>=128).
    assert!(
      !out.palette.iter().any(|c| c.a == 0),
      "setup: no transparent source, so no transparent slot should be reserved; palette = {:?}",
      out.palette
    );
    assert!(
      out.palette.iter().any(|c| c.r == 0 && c.g > 0 && c.a <= 50),
      "setup: a dim green entry (r==0, g>0, a<=50) must exist inside the guarantee zone; palette = {:?}",
      out.palette
    );
    assert!(
      out
        .palette
        .iter()
        .any(|c| c.b > c.r && c.b > c.g && c.a >= 128),
      "setup: the only visible entry must be the far-hue blue; palette = {:?}",
      out.palette
    );

    // Every NEAR-OPAQUE green source must map to the VISIBLE blue (a>=128) -- not vanish onto the dim
    // same-hue green -- even though dither can lift its want.a toward full opacity.
    for (i, &p) in px.iter().enumerate() {
      if p.a == 224 && p.g > 0 {
        let entry = out.palette[out.indices[i] as usize];
        assert!(
          entry.a >= 128,
          "near-opaque green source px{i} (a=224) mapped to a low-alpha entry (a={}) under dither when \
           only a far-hue visible entry existed — it would VANISH; the vanish_penalty must cover the \
           near-opaque band; entry = {entry:?}, palette = {:?}",
          entry.a,
          out.palette
        );
      }
    }
  }

  #[test]
  fn translucent_pixel_keeps_hue_not_flipped_to_brighter_wrong_hue() {
    // Regression (round-7): the anti-vanish term must NOT recolour genuinely TRANSLUCENT sources.
    // A translucent pixel whose only same-hue palette entry is dim must keep its HUE at a lower
    // alpha, NOT be forced onto a brighter WRONG-hue entry. (An earlier fully-proportional hard
    // floor `2·entry_a < src_a` excluded the same-hue dim green for an a=100 source and flipped it
    // onto opaque red — a visible recolour.) The cubic `vanish_penalty` stays tiny at the SMALL drop
    // of a translucent source to a same-hue dim entry (here 100→38: `100·62³ ≈ 2.4e7`, far below the
    // green→red hue cost), so `pdist_lab` still decides and the a=100 green keeps the dim same-hue
    // green. FAILS against a fully-proportional floor (the a=100 greens decode as red). Mirrors the
    // public-path fixture a reviewer used (green@30 x64, green@100 x8, red@255 x100, blue@255 x100,
    // maxQuality:6).
    let mut px: Vec<RGBA8> = Vec::with_capacity(272);
    px.extend(std::iter::repeat_n(rgba(0, 200, 0, 30), 64)); // faint green -> green centroid a≈38
    px.extend(std::iter::repeat_n(rgba(0, 200, 0, 100), 8)); // TRANSLUCENT green (the round-7 source)
    px.extend(std::iter::repeat_n(rgba(255, 0, 0, 255), 100)); // opaque red
    px.extend(std::iter::repeat_n(rgba(0, 0, 255, 255), 100)); // opaque blue

    // max_colors 3 (maxQuality:6), NO dither (speed:10 -> 0 k-means iters); min_quality 0.
    let out = quantize_rgba(&px, 272, 1, &cfg(3, false, 0));

    // Setup invariants (keep the test non-vacuous): a dim GREEN entry exists (r==0, g>0, a<128) AND
    // a brighter half-opaque-or-better entry exists (the only green is sub-128, so any a>=128 entry
    // is a wrong hue) — so the flip onto a brighter wrong-hue entry was physically possible.
    assert!(
      out.palette.iter().any(|c| c.r == 0 && c.g > 0 && c.a < 128),
      "setup: dim green entry (r==0,g>0,a<128) must exist; palette = {:?}",
      out.palette
    );
    assert!(
      out.palette.iter().any(|c| c.a >= 128),
      "setup: a brighter (a>=128) wrong-hue entry must exist for the flip to be possible; \
       palette = {:?}",
      out.palette
    );

    // Every TRANSLUCENT (a=100) green source must keep its HUE: map to the green entry (r==0, g>0),
    // NOT be flipped onto the brighter opaque red/blue.
    for (i, &p) in px.iter().enumerate() {
      if p.a == 100 {
        let entry = out.palette[out.indices[i] as usize];
        assert!(
          entry.r == 0 && entry.g > 0,
          "translucent green source px{i} (a=100) was flipped to a wrong-hue entry {entry:?} — the \
           cubic vanish_penalty must stay small at this drop and not recolour translucent sources; \
           palette = {:?}",
          out.palette
        );
      }
    }
  }

  #[test]
  fn opaque_source_never_remaps_to_transparent_slot_when_only_dim_entries_exist() {
    // BOUNDARY of the anti-vanish penalty (the case it cannot fully fix — and must still stay safe
    // in). When transparency reserves a slot AND a forced 2-colour palette leaves a single
    // faint-dominated visible centroid below a=128, the palette holds NO half-opaque (a>=128) entry
    // at all — no single visible colour can represent both the faint (a=1) and the opaque (a=255)
    // same-hue sources. The cubic `vanish_penalty` cannot conjure a brighter entry that does not
    // exist, so an opaque source maps to the only visible entry (the dim green) — but it must NEVER
    // be re-admitted to the reserved transparent slot: that exclusion is `skip_transparent`
    // (independent of the penalty), and `nearest_lab`'s Tier-2 fallback runs only when EVERY entry
    // is transparent, which is not the case here. This pins the degenerate boundary: best-effort
    // visible mapping, never the transparent slot. Such a palette fails the default min_quality
    // gate and self-heals via retry; it is reachable only at minQuality:0 (the caller opted out of
    // quality).
    let mut px: Vec<RGBA8> = Vec::with_capacity(102);
    px.extend(std::iter::repeat_n(rgba(0, 200, 0, 1), 64)); // faint green: green centroid -> a≈29 (<128)
    px.extend(std::iter::repeat_n(rgba(0, 200, 0, 255), 8)); // opaque green
    px.extend(std::iter::repeat_n(rgba(0, 0, 0, 0), 30)); // transparent: reserves the transparent slot

    // max_colors 2, no dither, 4 k-means iters; min_quality 0. width 102, height 1.
    let out = quantize_rgba(&px, 102, 1, &cfg(2, false, 4));

    // Setup invariants — the degenerate boundary is genuinely present:
    //  * a transparent slot was reserved (an a==0 entry exists);
    //  * there is NO half-opaque entry — EVERY entry is below a=128 — so the penalty physically
    //    cannot deliver an a>=128 result and this is the best-effort case, not the guarantee.
    assert!(
      out.palette.iter().any(|c| c.a == 0),
      "setup: a transparent slot must be reserved; palette = {:?}",
      out.palette
    );
    assert!(
      out.palette.iter().all(|c| c.a < 128),
      "setup: there must be NO half-opaque (a>=128) entry, so this exercises the best-effort \
       case (map to the only visible entry) rather than the guarantee; palette = {:?}",
      out.palette
    );

    // Best-effort guarantee: every OPAQUE source maps to a VISIBLE entry (a>0) — and is NEVER
    // re-admitted to the reserved transparent slot (a==0).
    for (i, &p) in px.iter().enumerate() {
      if p.a == 255 {
        let entry = out.palette[out.indices[i] as usize];
        assert!(
          entry.a > 0,
          "opaque source px{i} (a=255) was re-admitted to the transparent slot (a=0) — it must \
           map to a VISIBLE entry instead; entry = {entry:?}, palette = {:?}",
          out.palette
        );
      }
    }
  }

  // One UNGUARDED k-means pass: assign every entry to its perceptual-nearest palette
  // entry, then move each centroid to the count-weighted RGBA mean — identical math to
  // `kmeans_refine`'s pass body, minus the keep-best guard and the empty-cluster
  // reseed. Returns `None` if any cluster empties (so callers can skip the reseed case).
  #[cfg(test)]
  fn unguarded_kmeans_pass(palette: &[RGBA8], entries: &[ColorCount]) -> Option<Vec<RGBA8>> {
    let k = palette.len();
    let pal_labs = palette_labs(palette);
    let pal_alphas: Vec<u8> = palette.iter().map(|p| p.a).collect();
    let mut sr = vec![0u64; k];
    let mut sg = vec![0u64; k];
    let mut sb = vec![0u64; k];
    let mut sa = vec![0u64; k];
    let mut wn = vec![0u64; k];
    for e in entries {
      let idx = nearest_lab(
        &pal_labs,
        &pal_alphas,
        rgb_to_lab(e.color.r, e.color.g, e.color.b),
        e.color.a,
        e.color.a > 0,
        // Mirrors `kmeans_refine`'s clustering assignment: guard disabled.
        0,
      );
      let c = e.count;
      sr[idx] += e.color.r as u64 * c;
      sg[idx] += e.color.g as u64 * c;
      sb[idx] += e.color.b as u64 * c;
      sa[idx] += e.color.a as u64 * c;
      wn[idx] += c;
    }
    let mut out = palette.to_vec();
    for i in 0..k {
      if wn[i] == 0 {
        return None;
      }
      let n = wn[i];
      out[i] = RGBA8 {
        r: ((sr[i] + n / 2) / n) as u8,
        g: ((sg[i] + n / 2) / n) as u8,
        b: ((sb[i] + n / 2) / n) as u8,
        a: ((sa[i] + n / 2) / n) as u8,
      };
    }
    Some(out)
  }
}
