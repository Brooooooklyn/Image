//! Deterministic sRGB -> Oklab conversion and squared Oklab distance.
//!
//! # Why this exists (determinism gate for the clean-room PNG quantizer)
//!
//! The quantizer's palette-selection pipeline is integer-only by design so its output is
//! byte-identical across x86 / arm / wasm. Perceptual palette distance must preserve that
//! property. The forward map is pure `f32` linear algebra plus `f32::cbrt` — IEEE-754
//! `add`/`mul`/`cbrt` are correctly rounded on every target's libm (single-precision cube
//! root is small enough that mainstream libms return the correctly-rounded result), and the
//! Q14 quantization absorbs any sub-ulp residue — so the stored components are
//! bit-identical across platforms in practice, and every comparison derived from them is
//! exact integer math. The published Oklab matrices (B. Ottosson, "A perceptual color space
//! for image processing", updated 2021-01-25) are applied verbatim in `f32`.
//!
//! # Fixed-point scheme
//! - sRGB -> linear: the shared `SRGB_TO_LINEAR: [u32; 256]` Q16 table (the same table the
//!   dither's linear-light path uses — one notion of "linear" everywhere).
//! - linear sRGB -> Oklab: the published M1 matrix, `f32::cbrt` on each LMS response, then
//!   the published M2 matrix — all `f32`.
//! - output `OkLab` quantizes each component to a **Q14** triple (stored as `i32`):
//!   `Lq = round(L * 16383)`, `aq = round((a + 0.5) * 16383)`, `bq = round((b + 0.5) * 16383)`.
//!   The `+0.5` bias maps the in-gamut `a`/`b` range (~±0.34) comfortably inside `0..=16383`;
//!   the bias cancels in every difference.
//! - `oklab_dist_sq` returns `dL² + da² + db²` in those quantized squared units as `i64`.
//!
//! ## Why Q14 and not u16
//!
//! The u16 quantization (scale 65535) pushed the worst-case squared distance to
//! `3·65535² ≈ 1.29e10 > i32::MAX`, which forced the hot nearest-palette SIMD scan
//! into 2-/4-wide `f64` lanes. At Q14 every stored component is in `0..=16383`, so for
//! ANY two stored triples `|Δ| ≤ 16383` and `dL² + da² + db² ≤ 3·16383² = 805_208_067 <
//! i32::MAX` — unconditionally, no gamut-bound argument needed (compile-proven below by
//! `const _` assert on [`OKLAB_QMAX`]). That restores 4-wide `i32` NEON / 8-wide `i32` AVX2 /
//! 4-wide SSE4.1 / 4-wide simd128 lanes, ~2× the f64 lane throughput, at a quantization
//! step still ~1.6× finer on the lightness axis than the CIELAB ×100 grid (0..=10000) this
//! pipeline started on.

use std::sync::OnceLock;

/// An Oklab color quantized to a fixed **Q14** triple (stored as `i32`).
///
/// `l` holds `round(L * 16383)` (`L in 0..=1`); `a`/`b` hold
/// `round((component + 0.5) * 16383)`, so the zero-chroma point sits at
/// `round(0.5 * 16383) == 8192` and the in-gamut chroma range (~±0.34) stays
/// well inside `0..=16383`. Two `OkLab` values produced on different platforms
/// from the same `(r, g, b)` are bit-identical in practice (see the module
/// docs), and every comparison derived from them is exact integer math.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OkLab {
  /// Lightness L, `round(L * 16383)` (range 0..=16383).
  pub l: i32,
  /// Green–red a, `round((a + 0.5) * 16383)` (range 0..=16383).
  pub a: i32,
  /// Blue–yellow b, `round((b + 0.5) * 16383)` (range 0..=16383).
  pub b: i32,
}

/// Quantization scale of the stored Q14 triple (`round(component * 16383)`).
pub(crate) const OKLAB_SCALE: f32 = 16383.0;

/// Integer form of [`OKLAB_SCALE`] for const bounds (the Q14 component max).
const OKLAB_QMAX: i64 = 16383;

/// Compile-time proof that a squared Q14 Oklab distance ALWAYS fits an `i32` lane:
/// stored components are clamped to `0..=16383`, so for ANY two stored triples
/// `|Δ| ≤ 16383` and `de ≤ 3·16383² = 805_208_067 < i32::MAX` — unconditionally, no
/// gamut-bound argument needed. This is what licenses the `i32` SIMD lanes in
/// `quantize_simd` (4-wide NEON / 8-wide AVX2 / 4-wide SSE4.1 / 4-wide simd128).
/// (`oklab_dist_sq` still returns `i64`: the scalar reference keeps the wider type so
/// callers accumulating `de·wa` products stay far from any edge.)
const _: () = assert!(
  3 * OKLAB_QMAX * OKLAB_QMAX <= i32::MAX as i64,
  "Q14 squared Oklab distance must fit i32 SIMD lanes"
);

/// Bias folded into the stored `a`/`b` components (`(component + 0.5) * 16383`).
const CHROMA_BIAS: f32 = 0.5;

/// Exact global maximum of [`oklab_dist_sq`] over the whole sRGB cube — the squared-distance
/// *diameter* of the gamut. It is reached between pure black `(0,0,0)` and pure white
/// `(255,255,255)` — the full `L` axis with (in quantized terms) zero chroma gap — verified
/// against the gamut corners and a farthest-point iteration over the dense cube. So for ANY
/// two colors at full alpha, the perceptual color term in `pdist_oklab` (`de · wa/510`,
/// `wa <= 510`) is at most this value. The quantizer uses it to size the anti-vanish
/// penalty (`quantize::VANISH_WEIGHT`) so a fully-opaque pixel cannot be pulled onto a
/// clearly-invisible same-hue entry by even the WORST-case hue gap. Pinned by
/// `max_oklab_dist_sq_is_gamut_diameter`; if the Oklab conversion ever drifts, that test fails.
///
/// At Q14 this is `16383²` — comfortably INSIDE `i32::MAX`, which is why the
/// opaque-scan SIMD kernels run in `i32` lanes (see the `const _` assert on
/// [`OKLAB_QMAX`]).
pub(crate) const MAX_OKLAB_DIST_SQ: i64 = 268_402_689;

/// sRGB 8-bit -> linear light, **Q16** (so `SRGB_TO_LINEAR[255] == 65535 == 1.0`).
///
/// Const-gen (NOT runtime; pure documentation of provenance):
/// ```text
/// c     = i / 255
/// lin   = c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
/// table = round(lin * 65535)
/// ```
pub(crate) const SRGB_TO_LINEAR: [u32; 256] = [
  0, 20, 40, 60, 80, 99, 119, 139, 159, 179, 199, 219, 241, 264, 288, 313, 340, 367, 396, 427, 458,
  491, 526, 562, 599, 637, 677, 718, 761, 805, 851, 898, 947, 997, 1048, 1101, 1156, 1212, 1270,
  1330, 1391, 1453, 1517, 1583, 1651, 1720, 1790, 1863, 1937, 2013, 2090, 2170, 2250, 2333, 2418,
  2504, 2592, 2681, 2773, 2866, 2961, 3058, 3157, 3258, 3360, 3464, 3570, 3678, 3788, 3900, 4014,
  4129, 4247, 4366, 4488, 4611, 4736, 4864, 4993, 5124, 5257, 5392, 5530, 5669, 5810, 5953, 6099,
  6246, 6395, 6547, 6700, 6856, 7014, 7174, 7335, 7500, 7666, 7834, 8004, 8177, 8352, 8528, 8708,
  8889, 9072, 9258, 9445, 9635, 9828, 10022, 10219, 10417, 10619, 10822, 11028, 11235, 11446,
  11658, 11873, 12090, 12309, 12530, 12754, 12980, 13209, 13440, 13673, 13909, 14146, 14387, 14629,
  14874, 15122, 15371, 15623, 15878, 16135, 16394, 16656, 16920, 17187, 17456, 17727, 18001, 18277,
  18556, 18837, 19121, 19407, 19696, 19987, 20281, 20577, 20876, 21177, 21481, 21787, 22096, 22407,
  22721, 23038, 23357, 23678, 24002, 24329, 24658, 24990, 25325, 25662, 26001, 26344, 26688, 27036,
  27386, 27739, 28094, 28452, 28813, 29176, 29542, 29911, 30282, 30656, 31033, 31412, 31794, 32179,
  32567, 32957, 33350, 33745, 34143, 34544, 34948, 35355, 35764, 36176, 36591, 37008, 37429, 37852,
  38278, 38706, 39138, 39572, 40009, 40449, 40891, 41337, 41785, 42236, 42690, 43147, 43606, 44069,
  44534, 45002, 45473, 45947, 46423, 46903, 47385, 47871, 48359, 48850, 49344, 49841, 50341, 50844,
  51349, 51858, 52369, 52884, 53401, 53921, 54445, 54971, 55500, 56032, 56567, 57105, 57646, 58190,
  58737, 59287, 59840, 60396, 60955, 61517, 62082, 62650, 63221, 63795, 64372, 64952, 65535,
];

/// sRGB 8-bit -> linear light as `f32` in `[0.0, 1.0]`, read from the SAME
/// [`SRGB_TO_LINEAR`] Q16 table the dither path uses, so the Oklab conversion's
/// notion of "linear light" is bit-consistent with error diffusion. Deterministic:
/// a table lookup and one IEEE-754 `f32` divide — no `powf`.
#[inline]
pub(crate) fn srgb_to_linear_f(c: u8) -> f32 {
  SRGB_TO_LINEAR[c as usize] as f32 / 65535.0
}

/// Nearest-code inverse of [`srgb_to_linear_f`]: the 8-bit sRGB code whose linear
/// value is closest to `lin` (clamped to `[0,1]`). Deterministic — `lin` is
/// quantized to the Q16 scale of [`SRGB_TO_LINEAR`] and a binary search over that
/// strictly-increasing table selects the closest code with integer comparisons
/// (the only float ops are a multiply + round-to-nearest, which are IEEE-754
/// deterministic; no `powf`, no platform-dependent branch on a float result).
///
/// Exact LEFT INVERSE at the table points: `linear_to_srgb8(srgb_to_linear_f(c))
/// == c` for every `c` (pinned by `linear_srgb_roundtrip_is_exact`), so a flat /
/// zero-error pixel re-encodes to its original code and dither-free regions stay
/// byte-identical to a plain sRGB remap.
///
/// # Lookup table
///
/// `q` is closed and tiny (`[0, 65535]`), so the nearest-code search is tabulated in
/// [`LINEAR_TO_SRGB8_LUT`]: `q` is computed exactly as the pre-LUT body did (same
/// clamp/multiply/round and the same saturating `as` cast — NaN still maps to `q == 0`,
/// `±inf` clamps to the endpoints), then one table load returns what the search returned.
/// `LUT[q] == linear_to_srgb8_lookup(q)` BY CONSTRUCTION (the fill calls the preserved
/// body, tie-break included), so outputs are byte-identical for every `lin`; the
/// `OnceLock` init race cannot change the contents. 64 KiB of heap, built once.
#[inline]
pub(crate) fn linear_to_srgb8(lin: f32) -> u8 {
  // Quantize the linear value to the table's Q16 units (0..=65535) — identical to the
  // pre-LUT body.
  let q = (lin.clamp(0.0, 1.0) * 65535.0).round() as i64;
  LINEAR_TO_SRGB8_LUT.get_or_init(build_linear_to_srgb8_lut)[q as usize]
}

/// Lazily-built exact inverse table for [`linear_to_srgb8`] indexed by the quantized linear
/// value `q ∈ [0, 65535]` — 64 KiB of heap, built once per process. Filled by the exact
/// search below, so identical by construction; the `OnceLock` init race cannot change the
/// contents.
static LINEAR_TO_SRGB8_LUT: OnceLock<Box<[u8; 65536]>> = OnceLock::new();

/// Fills [`LINEAR_TO_SRGB8_LUT`] by calling the preserved binary-search body for every
/// reachable `q`. Heap-allocated via `Vec` so the table is never a stack temporary.
#[cold]
fn build_linear_to_srgb8_lut() -> Box<[u8; 65536]> {
  let mut t = vec![0u8; 65536];
  for (q, e) in t.iter_mut().enumerate() {
    *e = linear_to_srgb8_lookup(q as i64);
  }
  t.into_boxed_slice().try_into().expect("vec len == 65536")
}

/// The pre-LUT body of [`linear_to_srgb8`], unchanged: nearest-code binary search over
/// [`SRGB_TO_LINEAR`] for a quantized linear value `q` (table units, 0..=65535). Now used to
/// FILL [`LINEAR_TO_SRGB8_LUT`] and as the test reference.
fn linear_to_srgb8_lookup(q: i64) -> u8 {
  // First code whose linear value is >= q (table is strictly increasing).
  let mut lo = 0usize;
  let mut hi = 255usize;
  while lo < hi {
    let mid = (lo + hi) / 2;
    if (SRGB_TO_LINEAR[mid] as i64) < q {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }
  // The nearest code is `lo` (first >= q) or `lo - 1` (last < q); pick by |Δ|.
  let hi_code = lo;
  let lo_code = lo.saturating_sub(1);
  let d_hi = (SRGB_TO_LINEAR[hi_code] as i64 - q).abs();
  let d_lo = (q - SRGB_TO_LINEAR[lo_code] as i64).abs();
  // Strictly-closer wins; on a TIE the HIGHER code wins (the `else` branch). This is
  // deterministic, and makes the exact table point win on the round trip (there
  // `d_hi == 0 < d_lo`, so `hi_code` is returned and `linear_to_srgb8` is an exact
  // left-inverse of the table at every code).
  if d_lo < d_hi {
    lo_code as u8
  } else {
    hi_code as u8
  }
}

/// Convert an 8-bit sRGB color to the quantized Oklab triple.
///
/// The forward map is the published Oklab construction (Ottosson, matrices updated
/// 2021-01-25) in `f32`:
///
/// ```text
/// (lr, lg, lb) = SRGB_TO_LINEAR[..] / 65535          # table-exact linear light
/// (l, m, s)  = M1 · (lr, lg, lb)                     # approximate cone responses
/// (l', m', s') = (cbrt(l), cbrt(m), cbrt(s))         # f32::cbrt, NOT powf
/// (L, a, b)  = M2 · (l', m', s')
/// ```
///
/// then each component is quantized to the Q14 triple documented on [`OkLab`]
/// (`round(L·16383)`, `round((a+0.5)·16383)`, `round((b+0.5)·16383)`). The
/// quantization turns the f32 result into an integer triple once — everything
/// downstream (distance, argmin, centroid mean) is then exact integer math.
///
/// M1 (linear sRGB -> LMS, row-major) and M2 (LMS' -> Oklab):
/// ```text
/// M1 = [[0.4122214708, 0.5363325363, 0.0514459929],
///       [0.2119034982, 0.6806995451, 0.1073969566],
///       [0.0883024619, 0.2817188376, 0.6299787005]]
/// M2 = [[0.2104542553, 0.7936177850, -0.0040720468],
///       [1.9779984951, -2.4285922050, 0.4505937099],
///       [0.0259040371, 0.7827717662, -0.8086757660]]
/// ```
pub(crate) fn rgb_to_oklab(r: u8, g: u8, b: u8) -> OkLab {
  let lr = srgb_to_linear_f(r);
  let lg = srgb_to_linear_f(g);
  let lb = srgb_to_linear_f(b);

  // M1: linear sRGB -> LMS cone responses (published constants, f32).
  let l = 0.4122214708f32 * lr + 0.5363325363f32 * lg + 0.0514459929f32 * lb;
  let m = 0.2119034982f32 * lr + 0.6806995451f32 * lg + 0.1073969566f32 * lb;
  let s = 0.0883024619f32 * lr + 0.2817188376f32 * lg + 0.6299787005f32 * lb;

  // Perceptual nonlinearity: plain f32::cbrt (never powf).
  let l_ = l.cbrt();
  let m_ = m.cbrt();
  let s_ = s.cbrt();

  // M2: LMS' -> Oklab (published constants, f32).
  let big_l = 0.2104542553f32 * l_ + 0.7936177850f32 * m_ - 0.0040720468f32 * s_;
  let a = 1.9779984951f32 * l_ - 2.4285922050f32 * m_ + 0.4505937099f32 * s_;
  let bb = 0.0259040371f32 * l_ + 0.7827717662f32 * m_ - 0.8086757660f32 * s_;

  // Quantize to the Q14 triple. `round` on an f32 is IEEE-exact; the clamp keeps
  // last-ulp float residue at the gamut boundary (e.g. white's L landing on
  // 1.0000001) from escaping the `0..=16383` range.
  OkLab {
    l: (big_l * OKLAB_SCALE).round().clamp(0.0, OKLAB_SCALE) as i32,
    a: ((a + CHROMA_BIAS) * OKLAB_SCALE)
      .round()
      .clamp(0.0, OKLAB_SCALE) as i32,
    b: ((bb + CHROMA_BIAS) * OKLAB_SCALE)
      .round()
      .clamp(0.0, OKLAB_SCALE) as i32,
  }
}

/// Inverse of [`rgb_to_oklab`]: quantized Oklab triple -> 8-bit sRGB.
///
/// Dequantizes the stored Q14 triple (`l/16383`, `a/16383 − 0.5`,
/// `b/16383 − 0.5`), applies the published inverse map — M2⁻¹ into LMS',
/// the exact cube `x³`, then M1⁻¹ into linear sRGB — in `f32`, and re-encodes
/// each channel through [`linear_to_srgb8`] (the exact nearest-code inverse
/// shared with the dither path):
///
/// ```text
/// l' = L + 0.3963377774·a + 0.2158037573·b
/// m' = L − 0.1055613458·a − 0.0638541728·b
/// s' = L − 0.0894841775·a − 1.2914855480·b
/// (l, m, s) = (l'³, m'³, s'³)
/// r_lin = +4.0767416621·l − 3.3077115913·m + 0.2309699292·s
/// g_lin = −1.2684380046·l + 2.6097574011·m − 0.3413193965·s
/// b_lin = −0.0041960863·l − 0.7034186147·m + 1.7076147010·s
/// ```
///
/// Used by the k-means centroid update, which accumulates cluster means in the
/// quantized Oklab components (the space the assignment metric minimizes) and
/// maps them back to sRGB. A centroid mean can land OUTSIDE the sRGB gamut —
/// the inverse matrix then yields a linear channel below 0 or above 1 and
/// [`linear_to_srgb8`]'s clamp saturates to the nearest gamut boundary. Accuracy:
/// the strided-sweep round-trip `oklab_to_rgb8(rgb_to_oklab(c))` reproduces `c`
/// within a couple LSBs per channel (pinned by `oklab_rgb_roundtrip`); the
/// residual is f32 matrix/quantization rounding, not bias.
pub(crate) fn oklab_to_rgb8(lab: OkLab) -> (u8, u8, u8) {
  // Dequantize: undo the Q14 scale and the +0.5 chroma bias.
  let big_l = lab.l as f32 / OKLAB_SCALE;
  let a = lab.a as f32 / OKLAB_SCALE - CHROMA_BIAS;
  let b = lab.b as f32 / OKLAB_SCALE - CHROMA_BIAS;

  // M2 inverse: Oklab -> LMS' (published constants, f32).
  let l_ = big_l + 0.3963377774f32 * a + 0.2158037573f32 * b;
  let m_ = big_l - 0.1055613458f32 * a - 0.0638541728f32 * b;
  let s_ = big_l - 0.0894841775f32 * a - 1.2914855480f32 * b;

  // Inverse nonlinearity: exact cubes (x*x*x), no pow.
  let l = l_ * l_ * l_;
  let m = m_ * m_ * m_;
  let s = s_ * s_ * s_;

  // M1 inverse: LMS -> linear sRGB (published constants, f32). Out-of-gamut
  // centroids produce out-of-[0,1] channels; linear_to_srgb8 clamps them.
  let r = 4.0767416621f32 * l - 3.3077115913f32 * m + 0.2309699292f32 * s;
  let g = -1.2684380046f32 * l + 2.6097574011f32 * m - 0.3413193965f32 * s;
  let bb = -0.0041960863f32 * l - 0.7034186147f32 * m + 1.7076147010f32 * s;

  (linear_to_srgb8(r), linear_to_srgb8(g), linear_to_srgb8(bb))
}

/// Squared Oklab distance `dL² + da² + db²` in the quantized Q14² units.
///
/// Squared (no `sqrt`) because the quantizer only ever compares distances.
/// Stored components are `0..=16383`, so each squared term is `≤ 16383²` and the
/// sum is `≤ 3·16383² = 805_208_067` — INSIDE `i32::MAX` (the `const _` proof on
/// [`OKLAB_QMAX`]), which is what the `i32` SIMD lanes rely on. The accumulator
/// here stays `i64` anyway — scalar safety margin for callers that multiply `de`
/// further (`de·wa` in `pdist`), at zero cost off the SIMD path.
#[inline]
pub(crate) fn oklab_dist_sq(p: OkLab, q: OkLab) -> i64 {
  let dl = (p.l - q.l) as i64;
  let da = (p.a - q.a) as i64;
  let db = (p.b - q.b) as i64;
  dl * dl + da * da + db * db
}

#[cfg(test)]
mod tests {
  use super::*;

  /// An (r, g, b) input paired with its reference Oklab in real units.
  type RefRow = ((u8, u8, u8), (f64, f64, f64));
  /// An (r, g, b) input paired with its golden integer Oklab output.
  type GoldenRow = ((u8, u8, u8), (i32, i32, i32));

  /// Reference Oklab values (computed with the published matrices in f64, matching
  /// the widely-published reference outputs — e.g. colour-science's Oklab).
  /// Values are real Oklab units (L in 0..=1, a/b centered on 0).
  const REFERENCE: &[RefRow] = &[
    ((0, 0, 0), (0.000000, 0.000000, 0.000000)),
    ((255, 255, 255), (1.000000, 0.000000, 0.000000)),
    ((128, 128, 128), (0.599871, 0.000000, 0.000000)),
    ((255, 0, 0), (0.627955, 0.224863, 0.125846)),
    ((0, 255, 0), (0.866440, -0.233888, 0.179498)),
    ((0, 0, 255), (0.452014, -0.032457, -0.311528)),
    ((255, 255, 0), (0.967983, -0.071369, 0.198570)),
    ((0, 255, 255), (0.905399, -0.149444, -0.039398)),
    ((255, 0, 255), (0.701674, 0.274566, -0.169156)),
    ((64, 128, 192), (0.587209, -0.039537, -0.111861)),
    ((200, 30, 90), (0.544872, 0.200615, 0.027067)),
    ((1, 1, 1), (0.067205, 0.000000, 0.000000)),
  ];

  /// The linear-light dither helpers are an exact LEFT INVERSE at the table points,
  /// and the underlying table is strictly increasing (so the binary-search inverse is
  /// unambiguous). Consequence: a flat / zero-error pixel re-encodes to its own code,
  /// keeping dither-free regions byte-identical to a plain sRGB remap.
  #[test]
  fn linear_srgb_roundtrip_is_exact() {
    // Strictly increasing => no duplicate linear values => nearest-code inverse is well-defined.
    for c in 1u16..=255 {
      assert!(
        SRGB_TO_LINEAR[c as usize] > SRGB_TO_LINEAR[(c - 1) as usize],
        "SRGB_TO_LINEAR must be strictly increasing at code {c}"
      );
    }
    // Exact round trip for every code (the zero-error / flat-region guarantee).
    for c in 0u8..=255 {
      assert_eq!(
        linear_to_srgb8(srgb_to_linear_f(c)),
        c,
        "round trip must be exact for code {c}"
      );
    }
    // Endpoints hit the linear extremes; clamped/out-of-range inputs resolve to them.
    assert_eq!(srgb_to_linear_f(0), 0.0);
    assert_eq!(srgb_to_linear_f(255), 1.0);
    assert_eq!(linear_to_srgb8(-1.0), 0);
    assert_eq!(linear_to_srgb8(2.0), 255);
    // Nearest-code: a value between two adjacent codes resolves to one of them, never overshoots.
    for c in 0u8..=254 {
      let mid = (srgb_to_linear_f(c) + srgb_to_linear_f(c + 1)) / 2.0;
      let got = linear_to_srgb8(mid);
      assert!(
        got == c || got == c + 1,
        "midpoint near {c} resolved to {got}"
      );
    }
  }

  /// GOLDEN canary: exact integer outputs of the quantized Oklab conversion. If the
  /// f32 pipeline (matrices, cbrt, quantization) ever drifts these triples change.
  /// Includes the all-zero and all-255 corners.
  #[test]
  fn golden_exact_integer_outputs() {
    let cases: &[GoldenRow] = &[
      ((0, 0, 0), (0, 8192, 8192)),
      ((255, 255, 255), (16383, 8192, 8192)),
      ((255, 0, 0), (10288, 11875, 10253)),
      ((0, 255, 0), (14195, 4360, 11132)),
      ((0, 0, 255), (7405, 7660, 3088)),
      ((255, 255, 0), (15858, 7022, 11445)),
      ((0, 255, 255), (14833, 5743, 7546)),
      ((255, 0, 255), (11496, 12690, 5420)),
    ];
    for &((r, g, b), (l, a, bb)) in cases {
      let got = rgb_to_oklab(r, g, b);
      assert_eq!(
        (got.l, got.a, got.b),
        (l, a, bb),
        "golden mismatch for ({r},{g},{b}) — Oklab pipeline drifted?"
      );
    }
  }

  /// Accuracy vs the f64 reference table: the quantized conversion must track the
  /// published Oklab coordinates closely — the Q14 quantization alone is ≤ ~3.1e-5
  /// per component, so a 1e-3 bound leaves room only for real pipeline error.
  #[test]
  fn accuracy_vs_reference_table() {
    let (mut max_l, mut max_a, mut max_b) = (0.0f64, 0.0f64, 0.0f64);
    for &((r, g, b), (rl, ra, rb)) in REFERENCE {
      let lab = rgb_to_oklab(r, g, b);
      let (l, a, bb) = (
        lab.l as f64 / OKLAB_SCALE as f64,
        lab.a as f64 / OKLAB_SCALE as f64 - 0.5,
        lab.b as f64 / OKLAB_SCALE as f64 - 0.5,
      );
      max_l = max_l.max((l - rl).abs());
      max_a = max_a.max((a - ra).abs());
      max_b = max_b.max((bb - rb).abs());
    }
    assert!(
      max_l <= 1e-3 && max_a <= 1e-3 && max_b <= 1e-3,
      "accuracy bar exceeded: max err L={max_l:.6} a={max_a:.6} b={max_b:.6}"
    );
  }

  /// Overflow / panic safety across the full domain: all 256 grays + a strided sweep of
  /// the 256³ cube (step 17 ≈ 4k colors). Asserts no panic and in-range outputs.
  #[test]
  fn panic_overflow_sweep() {
    let check = |lab: OkLab| {
      assert!((0..=16383).contains(&lab.l), "L out of range: {}", lab.l);
      assert!((0..=16383).contains(&lab.a), "a out of range: {}", lab.a);
      assert!((0..=16383).contains(&lab.b), "b out of range: {}", lab.b);
    };
    // All grays.
    for i in 0..=255u16 {
      let i = i as u8;
      check(rgb_to_oklab(i, i, i));
    }
    // Strided cube sweep.
    let mut r = 0u16;
    while r <= 255 {
      let mut g = 0u16;
      while g <= 255 {
        let mut b = 0u16;
        while b <= 255 {
          check(rgb_to_oklab(r as u8, g as u8, b as u8));
          b += 17;
        }
        g += 17;
      }
      r += 17;
    }
  }

  /// Cross-platform determinism pin: `f32::cbrt` is the only non-exact op in the
  /// forward map (it lowers to the platform libm). Hash the quantized triples of
  /// a dense sRGB sweep — every color at multiples of 5 (52³ ≈ 140k colors), FNV-1a
  /// over the (l,a,b) i32 triples — so a 1-ulp `cbrt` divergence on any platform
  /// flips a component here and fails loudly instead of silently changing output
  /// bytes. If this fails after a *source* change, recompute the constant and
  /// audit the diff.
  #[test]
  fn dense_sweep_hash_pin() {
    let mut h = 0xcbf29ce484222325u64;
    let mut r = 0u16;
    while r <= 255 {
      let mut g = 0u16;
      while g <= 255 {
        let mut b = 0u16;
        while b <= 255 {
          let lab = rgb_to_oklab(r as u8, g as u8, b as u8);
          for v in [lab.l, lab.a, lab.b] {
            h ^= v as u64;
            h = h.wrapping_mul(0x100000001b3);
          }
          b += 5;
        }
        g += 5;
      }
      r += 5;
    }
    assert_eq!(
      h, 5501793295779600387u64,
      "Oklab forward-map drifted on this platform"
    );
  }

  /// Sign / monotonicity sanity matching the reference table.
  #[test]
  fn sign_and_monotonicity_sanity() {
    // black -> L == 0, white -> L == 16383 (L == 1).
    assert_eq!(rgb_to_oklab(0, 0, 0).l, 0);
    assert_eq!(rgb_to_oklab(255, 255, 255).l, 16383);

    // Gray ramp: L monotonically non-decreasing, chroma pinned at the bias point.
    let mut prev = i32::MIN;
    for i in 0..=255u16 {
      let lab = rgb_to_oklab(i as u8, i as u8, i as u8);
      assert!(
        lab.l >= prev,
        "L not monotonic at gray {i}: {} < {prev}",
        lab.l
      );
      prev = lab.l;
    }

    // pure red: a>0 and b>0 (above the 0.5 bias).
    let red = rgb_to_oklab(255, 0, 0);
    assert!(red.a > 8192 && red.b > 8192, "red: {red:?}");
    // pure green: a<0.
    assert!(rgb_to_oklab(0, 255, 0).a < 8192, "green a should be < 0");
    // pure blue: b<0.
    assert!(rgb_to_oklab(0, 0, 255).b < 8192, "blue b should be < 0");
  }

  /// `oklab_dist_sq` is a non-negative squared distance: zero iff equal, symmetric, and
  /// equals the hand-computed sum of squared component deltas.
  #[test]
  fn oklab_dist_sq_basic() {
    let p = rgb_to_oklab(255, 0, 0);
    let q = rgb_to_oklab(0, 255, 0);
    assert_eq!(oklab_dist_sq(p, p), 0);
    assert_eq!(oklab_dist_sq(p, q), oklab_dist_sq(q, p));
    assert!(oklab_dist_sq(p, q) > 0);

    let dl = (p.l - q.l) as i64;
    let da = (p.a - q.a) as i64;
    let db = (p.b - q.b) as i64;
    assert_eq!(oklab_dist_sq(p, q), dl * dl + da * da + db * db);
  }

  /// Pins [`MAX_OKLAB_DIST_SQ`]: it equals the black↔white squared distance and is the
  /// maximum over every pair of the gamut corners (black/white + the six R/G/B/C/M/Y
  /// primaries). The full-cube maximum was confirmed by a farthest-point iteration to be
  /// this same axis diameter; this test re-pins the value cheaply so any drift in the
  /// conversion is caught. The constant also documents the i32-lane inclusion: at Q14
  /// even the UNCONDITIONAL domain bound `3·16383²` fits `i32`.
  #[test]
  fn max_oklab_dist_sq_is_gamut_diameter() {
    let black = rgb_to_oklab(0, 0, 0);
    let white = rgb_to_oklab(255, 255, 255);
    assert_eq!(oklab_dist_sq(black, white), MAX_OKLAB_DIST_SQ);

    let corners = [
      (0u8, 0u8, 0u8),
      (255, 0, 0),
      (0, 255, 0),
      (0, 0, 255),
      (255, 255, 0),
      (0, 255, 255),
      (255, 0, 255),
      (255, 255, 255),
    ];
    let labs: Vec<OkLab> = corners
      .iter()
      .map(|&(r, g, b)| rgb_to_oklab(r, g, b))
      .collect();
    let mut max = 0i64;
    for (i, &p) in labs.iter().enumerate() {
      for &q in &labs[i + 1..] {
        max = max.max(oklab_dist_sq(p, q));
      }
    }
    assert_eq!(max, MAX_OKLAB_DIST_SQ, "corner-pair maximum drifted");
    // And the whole reason this matters: at Q14 the squared distance — over the
    // WHOLE `0..=16383` component domain, not just the gamut — fits i32, which is
    // what the `i32`-lane opaque-scan SIMD kernels rely on.
    assert!(3 * 16383i64 * 16383 <= i32::MAX as i64);
    assert!(MAX_OKLAB_DIST_SQ <= i32::MAX as i64);
  }

  /// Round-trip accuracy of [`oklab_to_rgb8`]: `oklab_to_rgb8(rgb_to_oklab(c))` must
  /// reproduce `c` within a couple LSBs per channel (the k-means centroid update
  /// relies on it). The residual is f32 matrix + Q14 quantization rounding, not
  /// bias. Crucially the function must be TOTAL: no panic on any OkLab, including
  /// out-of-gamut centroids — pinned below.
  #[test]
  fn oklab_rgb_roundtrip() {
    let mut max_err = 0i32;
    let mut check = |r: u8, g: u8, b: u8| {
      let (rr, gg, bb) = oklab_to_rgb8(rgb_to_oklab(r, g, b));
      max_err = max_err
        .max((rr as i32 - r as i32).abs())
        .max((gg as i32 - g as i32).abs())
        .max((bb as i32 - b as i32).abs());
    };
    // All grays + the gamut corners.
    for i in 0..=255u16 {
      check(i as u8, i as u8, i as u8);
    }
    for &(r, g, b) in &[
      (255u8, 0u8, 0u8),
      (0, 255, 0),
      (0, 0, 255),
      (255, 255, 0),
      (0, 255, 255),
      (255, 0, 255),
      (0, 0, 0),
      (255, 255, 255),
    ] {
      check(r, g, b);
    }
    // Strided cube sweep (step 17 -> ~4k colors covering the gamut).
    let mut r = 0u16;
    while r <= 255 {
      let mut g = 0u16;
      while g <= 255 {
        let mut b = 0u16;
        while b <= 255 {
          check(r as u8, g as u8, b as u8);
          b += 17;
        }
        g += 17;
      }
      r += 17;
    }
    assert!(
      max_err <= 2,
      "round-trip max per-channel error {max_err} > 2 LSB"
    );
    // Hand-checked endpoints: exact on black/white.
    assert_eq!(oklab_to_rgb8(rgb_to_oklab(0, 0, 0)), (0, 0, 0));
    assert_eq!(oklab_to_rgb8(rgb_to_oklab(255, 255, 255)), (255, 255, 255));
    // Totality: extreme/out-of-gamut triples clamp instead of panicking.
    let _ = oklab_to_rgb8(OkLab {
      l: 0,
      a: 0,
      b: 16383,
    });
    let _ = oklab_to_rgb8(OkLab {
      l: 16383,
      a: 16383,
      b: 0,
    });
    assert_eq!(
      oklab_to_rgb8(OkLab {
        l: 0,
        a: 8192,
        b: 8192
      })
      .0,
      0
    );
  }

  /// Byte-identity guard for the `linear_to_srgb8` LUT: the table must equal the preserved
  /// binary-search body at EVERY reachable `q` (all 65536 of them), plus a dense float sweep
  /// and edge values so the q-quantization + indexing path is covered end to end.
  #[test]
  fn linear_to_srgb8_lut_matches_lookup() {
    // Reference: the same q-quantization as `linear_to_srgb8`, routed to the preserved
    // pre-LUT search body instead of the table.
    let reference = |lin: f32| -> u8 {
      let q = (lin.clamp(0.0, 1.0) * 65535.0).round() as i64;
      linear_to_srgb8_lookup(q)
    };
    // Every reachable q: `lin = q/65535` re-quantizes to exactly `q` (f32 relative error
    // ~1e-7 « 0.5/65535), so this indexes LUT[q] and compares it to the search.
    for q in 0..=65535i64 {
      assert_eq!(
        linear_to_srgb8(q as f32 / 65535.0),
        linear_to_srgb8_lookup(q),
        "LUT mismatch at q={q}"
      );
    }
    // Dense sweep including sub-step values between grid points and out-of-range inputs
    // that exercise the clamp.
    for i in 0..=200_000i64 {
      let lin = i as f32 / 200_000.0 * 1.5 - 0.25;
      assert_eq!(
        linear_to_srgb8(lin),
        reference(lin),
        "sweep mismatch at {lin}"
      );
    }
    // Edge values: NaN and the infinities take the same saturating-cast path as before.
    for lin in [f32::NAN, f32::NEG_INFINITY, f32::INFINITY, -0.0] {
      assert_eq!(
        linear_to_srgb8(lin),
        reference(lin),
        "edge mismatch at {lin}"
      );
    }
  }
}
