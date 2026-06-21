//! Deterministic, floating-point-free sRGB -> CIELAB conversion and squared CIE76 ΔE.
//!
//! # Why this exists (determinism gate for the clean-room PNG quantizer)
//!
//! The quantizer's palette-selection pipeline is integer-only by design so its output is
//! byte-identical across x86 / arm / wasm. Perceptual palette distance (CIELAB ΔE) must preserve
//! that property. A naive sRGB->Lab conversion uses `powf`/`cbrt`, whose results are NOT guaranteed
//! bit-identical across platforms/CPUs/LLVM backends. This module therefore performs the entire
//! conversion + ΔE in **integer / fixed-point arithmetic only** at runtime. The only floating point
//! in this file lives in `#[cfg(test)]` (accuracy comparison against a float reference) and in
//! non-runtime const-generation comments that document how the embedded constants were derived.
//!
//! # Fixed-point scheme
//! - sRGB -> linear: hardcoded `SRGB_TO_LINEAR: [u32; 256]` in **Q16** (`linear(255) == 65535`).
//! - linear RGB -> normalized XYZ ratios: D65 matrix coefficients in **Q16**. The matrix rows are
//!   pre-divided by the white point (Xn, Yn, Zn) so that pure white maps to ratios `(1, 1, 1)` and
//!   thus `L == 100, a == 0, b == 0` exactly.
//! - the XYZ->Lab cube-root nonlinearity `f(t) = t^(1/3)` is computed with a **deterministic
//!   bit-by-bit integer cube root** (no `cbrt`/`powf`), operating on a `u128` so that
//!   `f(t)` is produced in **Q16**.
//! - output `Lab` stores L, a, b as `i32` scaled by **100** (i.e. value ×100). So `L == 5358`
//!   means `53.58`. This scale gives ~0.01-unit resolution, far finer than the ≤0.5 accuracy bar.
//! - `delta_e76_sq` returns dL² + da² + db² in those (×100) squared units as `i64`.

/// A CIELAB color stored in fixed-point integers (each component scaled by [`LAB_SCALE`]).
///
/// No floating-point fields, so two `Lab` values produced on different platforms from the same
/// `(r, g, b)` are bit-identical, and so is every comparison derived from them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lab {
  /// Lightness L*, ×100 (range ≈ 0..=10000).
  pub l: i32,
  /// Green–red a*, ×100.
  pub a: i32,
  /// Blue–yellow b*, ×100.
  pub b: i32,
}

/// Fixed-point scale applied to every `Lab` component (value ×100).
pub(crate) const LAB_SCALE: i32 = 100;

/// Number of fractional bits in the Q16 fixed-point used internally for linear light,
/// the sRGB->XYZ matrix, and the `f(t)` cube-root output.
const Q: u32 = 16;
/// `1 << Q`, i.e. the Q16 representation of `1.0` (== 65536).
const ONE: i64 = 1 << Q;
/// Rounding bias for a round-to-nearest `>> Q` shift on a non-negative value.
const HALF: i64 = ONE / 2;

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

/// White-point-normalized sRGB(linear) -> XYZ-ratio matrix, **Q16**.
///
/// Const-gen (NOT runtime; documents provenance):
/// ```text
/// M = [[0.412453, 0.357580, 0.180423],
///      [0.212671, 0.715160, 0.072169],
///      [0.019334, 0.119193, 0.950227]]   # standard sRGB D65
/// each row r is divided by sum(r) (== Xn/Yn/Zn) so the row sums to 1.0 and white -> (1,1,1).
/// MAT = round(M_normalized * 65536)
/// ```
/// Each entry multiplies a Q16 linear value; the Q32 product is shifted back to Q16.
const MAT: [[i64; 3]; 3] = [
  [28440, 24656, 12441],
  [13938, 46869, 4730],
  [1164, 7175, 57198],
];

/// CIELAB cube-root threshold ε = (6/29)³, in Q16 (`round(0.008856451679 * 65536) == 580`).
/// For ratios at or below this, the linear branch of `f(t)` is used.
const EPS_Q16: i64 = 580;
/// Linear-branch slope `1 / (3·(6/29)²)` in Q16 (`round(7.787037037 * 65536) == 510331`).
const F_LINEAR_SLOPE_Q16: i64 = 510_331;
/// Linear-branch intercept `4/29` in Q16 (`round(0.137931034 * 65536) == 9039`).
const F_LINEAR_INTERCEPT_Q16: i64 = 9039;

/// Deterministic, floating-point-free integer cube root: returns `floor(n^(1/3))`.
///
/// Integer Newton's method from an upper-bound seed, followed by an exact floor correction.
/// Pure integer arithmetic on `u128`, so the result is identical on every platform — and
/// byte-identical to the previous restoring bit-by-bit implementation (the
/// `icbrt_is_floor_cube_root` test pins both against a reference `floor(n^(1/3))` over a strided
/// sweep). Newton converges in ~5-6 iterations versus the bit-by-bit method's 42, which matters
/// because `f_q16` calls this once per cube-root, three per `rgb_to_lab`, once per dither pixel.
///
/// Overflow/range: production input is `f_q16`'s `(t << 32)` with `t <= ~76_000`, so `n <= ~2^48`
/// and the root `x <= ~2^16`. The seed `1 << ceil(bitlen/3)` is a strict upper bound on the root;
/// for any `n < 2^126` the root `x <= 2^42`, so `x*x <= 2^84` and `x*x*x <= 2^126` stay inside
/// `u128` (the correction never overflows). The iteration keeps `x >= 1` (the `n < 8` cases return
/// directly), so the `n / (x*x)` divisor is never zero.
#[inline]
fn icbrt_u128(n: u128) -> u128 {
  if n < 8 {
    // floor(0^(1/3)) == 0; floor(k^(1/3)) == 1 for k in 1..=7.
    return (n > 0) as u128;
  }
  let bits = 128 - n.leading_zeros();
  // `1 << ceil(bits/3)` is a strict upper bound on `floor(n^(1/3))`, so Newton descends to the floor.
  let mut x = 1u128 << bits.div_ceil(3);
  loop {
    let y = (2 * x + n / (x * x)) / 3;
    if y >= x {
      break;
    }
    x = y;
  }
  // Exact floor correction (0-1 steps in practice): guarantees `x^3 <= n < (x+1)^3` regardless of
  // any Newton off-by-one, so the result is byte-identical to the reference `floor(n^(1/3))`.
  while x * x * x > n {
    x -= 1;
  }
  while (x + 1) * (x + 1) * (x + 1) <= n {
    x += 1;
  }
  x
}

/// CIELAB nonlinearity `f(t)`, input and output both **Q16**.
///
/// `t` is a white-normalized XYZ ratio in Q16. For `t > ε` returns `t^(1/3)` via the integer cube
/// root; otherwise returns the linear branch `t / (3·(6/29)²) + 4/29`. Integer-only.
#[inline]
fn f_q16(t: i64) -> i64 {
  if t > EPS_Q16 {
    // t is Q16. Shift left by 32 so the value represents `t_real * 2^48`; its integer cube
    // root is `t_real^(1/3) * 2^16`, i.e. the result in Q16.
    icbrt_u128((t as u128) << 32) as i64
  } else {
    ((t * F_LINEAR_SLOPE_Q16 + HALF) >> Q) + F_LINEAR_INTERCEPT_Q16
  }
}

/// Round-to-nearest signed division by `2^16` (Q16 -> integer), without floating point.
#[inline]
fn rdiv_q16(v: i64) -> i64 {
  if v >= 0 {
    (v + HALF) / ONE
  } else {
    (v - HALF) / ONE
  }
}

/// Round-to-nearest arithmetic shift of a non-negative Q16 value down by `Q` bits.
#[inline]
fn rshift_q16(v: i64) -> i64 {
  (v + HALF) >> Q
}

/// Convert an 8-bit sRGB color to fixed-point CIELAB (D65, standard sRGB transfer).
///
/// Fully deterministic: integer/fixed-point arithmetic only. The returned components are scaled by
/// [`LAB_SCALE`] (×100).
pub(crate) fn rgb_to_lab(r: u8, g: u8, b: u8) -> Lab {
  // sRGB -> linear, Q16. Non-negative, max 65535.
  let lr = SRGB_TO_LINEAR[r as usize] as i64;
  let lg = SRGB_TO_LINEAR[g as usize] as i64;
  let lb = SRGB_TO_LINEAR[b as usize] as i64;

  // linear RGB -> normalized XYZ ratios (Q16). Each product is Q32 (Q16 coeff × Q16 light);
  // rounded shift back to Q16. Max intermediate ≈ 65535 × 65537 × 3 ≈ 1.3e10, fits i64 easily.
  let xr = rshift_q16(MAT[0][0] * lr + MAT[0][1] * lg + MAT[0][2] * lb);
  let yr = rshift_q16(MAT[1][0] * lr + MAT[1][1] * lg + MAT[1][2] * lb);
  let zr = rshift_q16(MAT[2][0] * lr + MAT[2][1] * lg + MAT[2][2] * lb);

  // CIELAB nonlinearity, Q16.
  let fx = f_q16(xr);
  let fy = f_q16(yr);
  let fz = f_q16(zr);

  // L* = 116·fy − 16 ; a* = 500·(fx − fy) ; b* = 200·(fy − fz)
  // fx/fy/fz are Q16. Multiply by the CIE coefficient and by LAB_SCALE, then round-divide the
  // Q16 back out. The `− 16` offset becomes `− 16·LAB_SCALE` in scaled units.
  let scale = LAB_SCALE as i64;
  let l = (rdiv_q16(116 * fy * scale) - 16 * scale) as i32;
  let a = rdiv_q16(500 * (fx - fy) * scale) as i32;
  let bb = rdiv_q16(200 * (fy - fz) * scale) as i32;

  Lab { l, a, b: bb }
}

/// Squared CIE76 distance `dL² + da² + db²` in (×[`LAB_SCALE`])² units.
///
/// Squared (no `sqrt`) because the quantizer only ever compares distances. Components differ by at
/// most ~20000 (×100 units), so each squared term is ≤ ~4e8 and the sum ≤ ~1.2e9, well inside
/// `i64`. Widening to `i64` before squaring prevents any `i32` overflow.
#[inline]
pub(crate) fn delta_e76_sq(p: Lab, q: Lab) -> i64 {
  let dl = (p.l - q.l) as i64;
  let da = (p.a - q.a) as i64;
  let db = (p.b - q.b) as i64;
  dl * dl + da * da + db * db
}

#[cfg(test)]
mod tests {
  use super::*;

  /// An (r, g, b) input paired with its reference Lab in real units.
  type RefRow = ((u8, u8, u8), (f64, f64, f64));
  /// An (r, g, b) input paired with its golden integer Lab output.
  type GoldenRow = ((u8, u8, u8), (i32, i32, i32));

  /// Reference table (skimage.color.rgb2lab, D65) embedded from the P3 phase-1 brief.
  /// Values are real Lab units.
  const REFERENCE: &[RefRow] = &[
    ((0, 0, 0), (0.0000, 0.0000, 0.0000)),
    ((255, 255, 255), (100.0000, -0.0025, 0.0047)),
    ((128, 128, 128), (53.5850, -0.0015, 0.0028)),
    ((255, 0, 0), (53.2406, 80.0923, 67.2028)),
    ((0, 255, 0), (87.7351, -86.1830, 83.1797)),
    ((0, 0, 255), (32.2957, 79.1856, -107.8573)),
    ((255, 255, 0), (97.1395, -21.5547, 94.4781)),
    ((0, 255, 255), (91.1133, -48.0906, -14.1263)),
    ((255, 0, 255), (60.3235, 98.2331, -60.8210)),
    ((64, 128, 192), (52.2105, 0.0953, -39.4843)),
    ((200, 30, 90), (44.1609, 65.8066, 10.6150)),
    ((18, 52, 86), (21.0416, 1.0523, -24.0992)),
    ((245, 222, 179), (89.3517, 1.5098, 24.0113)),
    ((1, 1, 1), (0.2742, -0.0000, 0.0000)),
    ((254, 254, 254), (99.6549, -0.0024, 0.0046)),
  ];

  /// GOLDEN cross-platform canary: exact integer outputs. If any platform's integer math diverged
  /// these byte-exact triples would change. Includes the all-zero and all-255 corners.
  #[test]
  fn golden_exact_integer_outputs() {
    let cases: &[GoldenRow] = &[
      ((0, 0, 0), (0, 0, 0)),
      ((255, 255, 255), (10000, 0, 0)),
      ((128, 128, 128), (5358, 0, 0)),
      ((255, 0, 0), (5324, 8009, 6720)),
      ((0, 255, 0), (8773, -8618, 8318)),
      ((0, 0, 255), (3230, 7919, -10786)),
      ((1, 1, 1), (28, 0, 0)),
      ((254, 254, 254), (9965, 0, 0)),
    ];
    for &((r, g, b), (l, a, bb)) in cases {
      let got = rgb_to_lab(r, g, b);
      assert_eq!(
        (got.l, got.a, got.b),
        (l, a, bb),
        "golden mismatch for ({r},{g},{b}) — integer math diverged?"
      );
    }
  }

  /// Accuracy vs the skimage reference table: max abs error must be ≤ 1.0 Lab unit per channel
  /// (target ≤ 0.5). Float is allowed here — this is the test path, not runtime.
  #[test]
  fn accuracy_vs_reference_table() {
    let (mut max_l, mut max_a, mut max_b) = (0.0f64, 0.0f64, 0.0f64);
    for &((r, g, b), (rl, ra, rb)) in REFERENCE {
      let lab = rgb_to_lab(r, g, b);
      let (l, a, bb) = (
        lab.l as f64 / LAB_SCALE as f64,
        lab.a as f64 / LAB_SCALE as f64,
        lab.b as f64 / LAB_SCALE as f64,
      );
      max_l = max_l.max((l - rl).abs());
      max_a = max_a.max((a - ra).abs());
      max_b = max_b.max((bb - rb).abs());
    }
    assert!(
      max_l <= 1.0 && max_a <= 1.0 && max_b <= 1.0,
      "accuracy bar exceeded: max err L={max_l:.4} a={max_a:.4} b={max_b:.4}"
    );
    // Tighter target check (informational; should hold comfortably).
    assert!(
      max_l <= 0.5 && max_a <= 0.5 && max_b <= 0.5,
      "did not meet ≤0.5 target: L={max_l:.4} a={max_a:.4} b={max_b:.4}"
    );
  }

  /// Overflow / panic safety across the full domain: all 256 grays + a strided sweep of the 256³
  /// cube (step 17 ≈ 4k colors). Asserts no panic and in-range finite outputs.
  #[test]
  fn panic_overflow_sweep() {
    let check = |lab: Lab| {
      // L roughly 0..100·scale (allow small rounding margin); a/b within ±128·scale + margin.
      assert!((-50..=10050).contains(&lab.l), "L out of range: {}", lab.l);
      assert!(
        (-13000..=13000).contains(&lab.a),
        "a out of range: {}",
        lab.a
      );
      assert!(
        (-13000..=13000).contains(&lab.b),
        "b out of range: {}",
        lab.b
      );
    };
    // All grays.
    for i in 0..=255u16 {
      let i = i as u8;
      check(rgb_to_lab(i, i, i));
    }
    // Strided cube sweep.
    let mut r = 0u16;
    while r <= 255 {
      let mut g = 0u16;
      while g <= 255 {
        let mut b = 0u16;
        while b <= 255 {
          check(rgb_to_lab(r as u8, g as u8, b as u8));
          b += 17;
        }
        g += 17;
      }
      r += 17;
    }
  }

  /// Sign / monotonicity sanity matching the reference table.
  #[test]
  fn sign_and_monotonicity_sanity() {
    // black -> L ≈ 0, white -> L ≈ 100·scale.
    assert_eq!(rgb_to_lab(0, 0, 0).l, 0);
    assert_eq!(rgb_to_lab(255, 255, 255).l, 100 * LAB_SCALE);

    // Gray ramp: L monotonically non-decreasing.
    let mut prev = i32::MIN;
    for i in 0..=255u16 {
      let l = rgb_to_lab(i as u8, i as u8, i as u8).l;
      assert!(l >= prev, "L not monotonic at gray {i}: {l} < {prev}");
      prev = l;
    }

    // pure red: a>0 and b>0.
    let red = rgb_to_lab(255, 0, 0);
    assert!(red.a > 0 && red.b > 0, "red: {red:?}");
    // pure green: a<0.
    assert!(rgb_to_lab(0, 255, 0).a < 0, "green a should be < 0");
    // pure blue: b<0.
    assert!(rgb_to_lab(0, 0, 255).b < 0, "blue b should be < 0");
  }

  /// `delta_e76_sq` is a non-negative squared distance: zero iff equal, symmetric, and equals the
  /// hand-computed sum of squared component deltas.
  #[test]
  fn delta_e76_sq_basic() {
    let p = rgb_to_lab(255, 0, 0);
    let q = rgb_to_lab(0, 255, 0);
    assert_eq!(delta_e76_sq(p, p), 0);
    assert_eq!(delta_e76_sq(p, q), delta_e76_sq(q, p));
    assert!(delta_e76_sq(p, q) > 0);

    let dl = (p.l - q.l) as i64;
    let da = (p.a - q.a) as i64;
    let db = (p.b - q.b) as i64;
    assert_eq!(delta_e76_sq(p, q), dl * dl + da * da + db * db);
  }

  /// Direct unit test of the deterministic integer cube root against a checked reference.
  #[test]
  fn icbrt_is_floor_cube_root() {
    let floor_cbrt = |n: u128| -> u128 {
      if n == 0 {
        return 0;
      }
      // Float seed then integer-correct (test-only; runtime never uses float).
      let mut x = (n as f64).cbrt().round() as u128;
      while x.saturating_mul(x).saturating_mul(x) > n {
        x -= 1;
      }
      while (x + 1).saturating_mul(x + 1).saturating_mul(x + 1) <= n {
        x += 1;
      }
      x
    };
    for n in [
      0u128,
      1,
      7,
      8,
      9,
      26,
      27,
      28,
      1000,
      65535,
      65536,
      (76_000u128) << 32,
      ((76_000u128) << 32) + 12_345,
      1u128 << 48,
    ] {
      assert_eq!(icbrt_u128(n), floor_cbrt(n), "icbrt mismatch at {n}");
    }
    // Strided larger sweep.
    let mut n = 0u128;
    while n < (1u128 << 40) {
      assert_eq!(icbrt_u128(n), floor_cbrt(n), "icbrt mismatch at {n}");
      n += 999_983; // prime stride
    }
  }
}
