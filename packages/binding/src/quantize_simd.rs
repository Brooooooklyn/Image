//! Runtime-dispatched SIMD kernels for the quantizer's hottest inner loop: the
//! per-pixel **nearest-palette argmin** in `remap_dither` (and the remap / k-means
//! assignment scans).
//!
//! # What this accelerates
//!
//! For a fully-opaque palette and an opaque query the perceptual distance
//! `nearest_lab` minimizes collapses (proven in `quantize.rs`) to exactly the
//! squared CIE76 ΔE:
//!
//! ```text
//! d == de == dl² + da² + db²        (dl,da,db = query.Lab − entry.Lab, in ×100 units)
//! ```
//!
//! For in-gamut Lab this is `de ≤ MAX_DELTA_E76_SQ = 669_160_034`; even an out-of-gamut
//! k-means centroid keeps each `|Δ| ≤ ~26_000`, so `dl²+da²+db² ≤ 3·26000² ≈ 2.03e9 <
//! i32::MAX`. The whole scan therefore fits in **i32 lanes** — no divide, no 64-bit, no
//! penalties, no branches. This module provides an `i32` argmin over Structure-of-Arrays
//! palette Lab components (`l[]`, `a[]`, `b[]`), dispatched at runtime to the widest SIMD the
//! host supports (AVX2 8-wide or SSE4.1 4-wide on x86; NEON 4-wide on aarch64; simd128 4-wide
//! on wasm), with a scalar reference that defines the result every width reproduces.
//!
//! # Determinism is SACRED
//!
//! The encoded PNG must stay **byte-identical** across x86_64 / aarch64 / wasm32 and
//! run-to-run. Every kernel here is **integer-only** (i32 add/mul/min); integer
//! arithmetic is associative and exact, so lane order cannot change the result. Each
//! SIMD kernel reproduces the scalar reference *bit-for-bit*, including the
//! **lowest-index-wins** tie-break (`scalar uses strict `d < best_d``, scanning indices
//! ascending). The `kernel_matches_scalar*` tests are the gate.
//!
//! # The general path (`general_argmin`)
//!
//! When the palette is not fully opaque (or the query is translucent), `nearest_lab`'s
//! full perceptual score applies per entry:
//!
//! ```text
//! skip entry if (skip_transparent && pa == 0)
//! de    = dl² + da² + db²                       (i64, ≤ ~2.1e9)
//! wa    = query_a + pa                          (0..=510)
//! score = (de·wa)/510 + da²·1500                (integer floor-div; de·wa ≤ ~1.1e12)
//!       + (src>0 && pa<src ? 3000·(src−pa)² : 0)   // dim_penalty,   src = guard_src_alpha
//!       + (src>pa         ?  100·(src−pa)³ : 0)    // vanish_penalty
//! argmin by strict `<`, lowest index wins
//! ```
//!
//! plus `nearest_lab`'s two tiers: tier 1 scans with the transparent-slot exclusion; if
//! EVERY entry was excluded (all-transparent palette) tier 2 rescans without it.
//!
//! The general kernels run this scan in **f64 lanes**, which is still bit-exact:
//!
//! * Every operand is an integer < 2⁵³ → exactly representable in f64.
//! * `de·wa ≤ ~1.1e12 < 2⁵³` → the product is exact.
//! * `(de·wa)/510` in f64 is the correctly-rounded real quotient, and
//!   `floor(quotient_f64) == floor(exact rational)`: the exact quotient's fractional
//!   part is a multiple of 1/510 (~0.002), far larger than the f64 representation error
//!   (≤ ~2⁻⁵² relative ≈ 1e-7 absolute at a ~2e9 quotient) — the correctly-rounded value
//!   can only cross an integer boundary if the true value is within ~1e-7 of one, which
//!   is impossible when the nearest fractional values are 0 and ≥1/510 away. So `floor`
//!   reproduces the integer floor-division exactly.
//! * `da²·1500`, `3000·d²`, `100·d³` are exact integers < 2⁵³.
//! * The sum is < ~4e9 < 2⁵³ → exact; comparisons are exact. The lowest-index tie-break
//!   via strict `<` is preserved.
//! * f64 add/mul/div/floor are IEEE-754 correctly rounded → identical on
//!   x86/aarch64/wasm (wasm f64x2 ops are IEEE too). The determinism contract holds.
//!
//! # Never panic
//!
//! Each `*_simd` kernel is an `unsafe fn` marked `#[target_feature(enable = …)]`; it is
//! ONLY ever reached through [`detect`], which gates on `is_*_feature_detected!` (x86) or
//! a guaranteed-baseline arch (`neon` on aarch64, `simd128` compiled-in on wasm). A host
//! lacking the feature takes the scalar path, so no illegal instruction can execute — but on
//! x86 this holds ONLY because the distributed builds are not compiled with a static
//! `+avx2`/`+sse4.1` floor: `is_x86_feature_detected!` short-circuits on
//! `cfg!(target_feature)` (std_detect), so a static floor would fold the probe to a
//! compile-time `true` and run the kernel unconditionally — SIGILL on a host that lacks it.
//! Keep those floors out of `.cargo/config.toml` for distributed x86_64 targets (see the note
//! there). The `IMAGE_QUANTIZE_SCALAR=1` escape hatch (and the test-only thread-local
//! override) force the scalar path on a SIMD-capable host to prove the fallback.

use std::sync::OnceLock;

/// Which opaque-scan kernel to run. `Copy` so callers detect once and pass it by value
/// down the hot loop with no per-pixel cost.
///
/// Variants are added per SIMD phase; Phase 0 ships only [`OpaqueKernel::Scalar`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum OpaqueKernel {
  /// Portable integer reference. Defines the byte-exact result every other kernel
  /// must reproduce. Always available on every target.
  Scalar,
  /// NEON (aarch64) 4-wide i32 argmin. NEON is part of the AArch64 baseline, so it is
  /// always present on aarch64 — no runtime probe, no host can lack it.
  #[cfg(target_arch = "aarch64")]
  Neon,
  /// AVX2 (x86_64) 8-wide i32 argmin — the widest x86 path, preferred when the host reports
  /// AVX2 (the common case on modern x86 and the CodSpeed runner). Wider lanes than the 4-wide
  /// kernels, but byte-identical: integer add/mul/min are exact, so the 8-lane reduction
  /// reaches the same lowest-index argmin as scalar (the `kernel_matches_scalar_*` tests gate
  /// every width against the scalar reference).
  #[cfg(target_arch = "x86_64")]
  Avx2,
  /// SSE4.1 (x86_64) 4-wide i32 argmin — the pre-AVX2 x86 fallback (older Intel Macs, some
  /// musl/Windows hosts). Selected when the host reports SSE4.1 but not AVX2; older x86 still
  /// falls back to scalar. Produces the identical argmin as AVX2/scalar — different lane width,
  /// same exact-integer result, so the byte-identical cross-arch contract holds across widths.
  #[cfg(target_arch = "x86_64")]
  Sse41,
  /// wasm32 `simd128` 4-wide i32 argmin — the 128-bit 4-lane path, uniform with NEON/SSE4.1.
  /// simd128 is a COMPILE-TIME feature on wasm (no runtime detection exists); the build
  /// enables it via `.cargo/config.toml` (`+simd128`). A wasm build without it uses scalar.
  #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
  Simd128,
}

/// Selects the opaque-scan kernel for this host, honouring the `IMAGE_QUANTIZE_SCALAR`
/// escape hatch and (in tests) a thread-local override. Cheap to call repeatedly — the
/// underlying feature probe is cached by the std macro and the env read by a `OnceLock` —
/// but callers should still detect ONCE above the per-pixel loop, not per pixel.
#[inline]
pub(crate) fn detect() -> OpaqueKernel {
  #[cfg(test)]
  {
    if let Some(forced) = test_override::get() {
      return forced;
    }
  }
  if force_scalar() {
    return OpaqueKernel::Scalar;
  }
  detect_native()
}

/// Runtime feature probe, per architecture. Kept separate from [`detect`] so the override
/// / escape-hatch logic is shared.
#[inline]
fn detect_native() -> OpaqueKernel {
  #[cfg(target_arch = "aarch64")]
  {
    // NEON is mandatory in the AArch64 baseline, so it is always present — no probe
    // needed and no host can lack it.
    OpaqueKernel::Neon
  }
  #[cfg(target_arch = "x86_64")]
  {
    // Probe at runtime, preferring the widest path the host supports so no host ever
    // executes an unsupported instruction: AVX2 (8-wide) on modern x86, SSE4.1 (4-wide)
    // on pre-AVX2 x86, scalar on anything older. All three reach the byte-identical argmin.
    if std::is_x86_feature_detected!("avx2") {
      OpaqueKernel::Avx2
    } else if std::is_x86_feature_detected!("sse4.1") {
      OpaqueKernel::Sse41
    } else {
      OpaqueKernel::Scalar
    }
  }
  #[cfg(target_arch = "wasm32")]
  {
    // simd128 is compile-time on wasm — no runtime probe. Present (build sets +simd128) ->
    // Simd128; absent -> scalar. Either way, never an illegal instruction.
    #[cfg(target_feature = "simd128")]
    {
      OpaqueKernel::Simd128
    }
    #[cfg(not(target_feature = "simd128"))]
    {
      OpaqueKernel::Scalar
    }
  }
  #[cfg(not(any(
    target_arch = "aarch64",
    target_arch = "x86_64",
    target_arch = "wasm32"
  )))]
  {
    OpaqueKernel::Scalar
  }
}

/// `true` when `IMAGE_QUANTIZE_SCALAR` is set to a truthy value (`1`/`true`/`yes`). Read
/// once per process. A production escape hatch: forces the scalar path on any host, so a
/// suspected SIMD mismatch can be ruled out without a rebuild.
fn force_scalar() -> bool {
  static FORCE: OnceLock<bool> = OnceLock::new();
  *FORCE.get_or_init(|| {
    std::env::var("IMAGE_QUANTIZE_SCALAR")
      .map(|v| matches!(v.trim(), "1" | "true" | "yes"))
      .unwrap_or(false)
  })
}

/// Index of the palette entry nearest `q = [L, a, b]` (×100 units) by squared CIE76 ΔE,
/// over the Structure-of-Arrays palette components. **Opaque fast path only** — the
/// caller guarantees the palette is fully opaque and the query is opaque, so this exact
/// `dl²+da²+db²` argmin equals the general `nearest_lab` result.
///
/// `l`, `a`, `b` are parallel, equal-length, and non-empty (the caller's palette always
/// has ≥1 entry). Ties resolve to the **lowest index**.
#[inline]
pub(crate) fn opaque_argmin(
  kernel: OpaqueKernel,
  l: &[i32],
  a: &[i32],
  b: &[i32],
  q: [i32; 3],
) -> usize {
  debug_assert_eq!(l.len(), a.len());
  debug_assert_eq!(l.len(), b.len());
  debug_assert!(!l.is_empty());
  match kernel {
    OpaqueKernel::Scalar => opaque_scan_scalar(l, a, b, q),
    #[cfg(target_arch = "aarch64")]
    OpaqueKernel::Neon => unsafe { opaque_scan_neon(l, a, b, q) },
    #[cfg(target_arch = "x86_64")]
    OpaqueKernel::Avx2 => unsafe { opaque_scan_avx2(l, a, b, q) },
    #[cfg(target_arch = "x86_64")]
    OpaqueKernel::Sse41 => unsafe { opaque_scan_sse41(l, a, b, q) },
    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    OpaqueKernel::Simd128 => unsafe { opaque_scan_simd128(l, a, b, q) },
  }
}

/// Portable integer reference for [`opaque_argmin`]. Computed in `i64` to mirror
/// `nearest_lab`'s arithmetic exactly (the values fit `i32`, so every SIMD kernel reaches
/// the same argmin in `i32`). Scans ascending with strict `<`, so the lowest index wins
/// on a tie.
fn opaque_scan_scalar(l: &[i32], a: &[i32], b: &[i32], q: [i32; 3]) -> usize {
  let [ql, qa, qb] = q;
  let mut best = 0usize;
  let mut best_d = i64::MAX;
  for (i, ((&li, &ai), &bi)) in l.iter().zip(a).zip(b).enumerate() {
    let dl = (ql - li) as i64;
    let da = (qa - ai) as i64;
    let db = (qb - bi) as i64;
    let d = dl * dl + da * da + db * db;
    if d < best_d {
      best_d = d;
      best = i;
    }
  }
  best
}

/// NEON (aarch64) implementation of [`opaque_argmin`]: 4-wide i32 argmin of
/// `dl²+da²+db²`, bit-identical to [`opaque_scan_scalar`]. Each lane keeps the lowest
/// index achieving its running min (strict `vcltq`); then a scalar reduction over the 4
/// lanes followed by the `n % 4` tail reproduces the ascending lowest-index-wins
/// tie-break. Every intermediate stays < i32::MAX (in-gamut `de ≤ MAX_DELTA_E76_SQ =
/// 669_160_034`; even an out-of-gamut `|Δ| ≤ ~26_000` gives `dl²+da²+db² ≈ 2.03e9 < 2³¹`),
/// so the i32 lanes never overflow. SAFETY: only reachable via `detect`/`opaque_argmin` on
/// aarch64, where NEON is guaranteed.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn opaque_scan_neon(l: &[i32], a: &[i32], b: &[i32], q: [i32; 3]) -> usize {
  use core::arch::aarch64::*;
  unsafe {
    let n = l.len();
    let ql = vdupq_n_s32(q[0]);
    let qa = vdupq_n_s32(q[1]);
    let qb = vdupq_n_s32(q[2]);
    let idx_arr = [0i32, 1, 2, 3];
    let lane_idx = vld1q_s32(idx_arr.as_ptr());
    let mut min_d = vdupq_n_s32(i32::MAX);
    let mut min_i = vdupq_n_s32(i32::MAX);

    let mut i = 0usize;
    while i + 4 <= n {
      let dl = vsubq_s32(ql, vld1q_s32(l.as_ptr().add(i)));
      let da = vsubq_s32(qa, vld1q_s32(a.as_ptr().add(i)));
      let db = vsubq_s32(qb, vld1q_s32(b.as_ptr().add(i)));
      let d = vaddq_s32(
        vaddq_s32(vmulq_s32(dl, dl), vmulq_s32(da, da)),
        vmulq_s32(db, db),
      );
      let cur_i = vaddq_s32(vdupq_n_s32(i as i32), lane_idx);
      // lanes where d < min_d (STRICT: a later equal value does NOT replace -> lowest
      // index kept within the lane, matching the scalar `<`).
      let mask = vcltq_s32(d, min_d);
      min_d = vbslq_s32(mask, d, min_d);
      min_i = vbslq_s32(mask, cur_i, min_i);
      i += 4;
    }

    // Reduce the 4 lanes: lowest d, tie -> lowest index.
    let mut ld = [0i32; 4];
    let mut li = [0i32; 4];
    vst1q_s32(ld.as_mut_ptr(), min_d);
    vst1q_s32(li.as_mut_ptr(), min_i);

    let mut best_d = i64::MAX;
    let mut best = 0usize;
    for (&d_lane, &i_lane) in ld.iter().zip(li.iter()) {
      if i_lane == i32::MAX {
        continue; // lane saw no full block (n < 4)
      }
      let d = d_lane as i64;
      let idx = i_lane as usize;
      if d < best_d || (d == best_d && idx < best) {
        best_d = d;
        best = idx;
      }
    }
    // Tail (n % 4): ascending, strict `<`, so a tail entry only wins on a STRICT
    // improvement — preserving lowest-index-on-tie against the lower-indexed lane winners.
    while i < n {
      let dl = (q[0] - l[i]) as i64;
      let da = (q[1] - a[i]) as i64;
      let db = (q[2] - b[i]) as i64;
      let d = dl * dl + da * da + db * db;
      if d < best_d {
        best_d = d;
        best = i;
      }
      i += 1;
    }
    best
  }
}

/// AVX2 (x86_64) implementation of [`opaque_argmin`]: 8-wide i32 argmin of `dl²+da²+db²`, the
/// widest x86 path. Bit-identical to [`opaque_scan_scalar`] and to the 4-wide kernels: the
/// per-lane compare is STRICT (`_mm256_cmpgt_epi32(min_d, d)` is `d < min_d`), so a later equal
/// value never displaces the lower index within a lane; an 8-lane lowest-index cross-lane
/// reduction + the `n % 8` scalar tail reproduce the scalar reference's ascending
/// lowest-index-wins tie-break. Every intermediate stays < i32::MAX (in-gamut
/// `de ≤ MAX_DELTA_E76_SQ = 669_160_034`; even an out-of-gamut `|Δ| ≤ ~26_000` gives
/// `dl²+da²+db² ≈ 2.03e9 < 2³¹`), so the i32 lanes never overflow. SAFETY: only reachable via
/// `detect`/`opaque_argmin` after `is_x86_feature_detected!("avx2")`.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn opaque_scan_avx2(l: &[i32], a: &[i32], b: &[i32], q: [i32; 3]) -> usize {
  use core::arch::x86_64::*;
  unsafe {
    let n = l.len();
    let ql = _mm256_set1_epi32(q[0]);
    let qa = _mm256_set1_epi32(q[1]);
    let qb = _mm256_set1_epi32(q[2]);
    let lane_idx = _mm256_setr_epi32(0, 1, 2, 3, 4, 5, 6, 7);
    let mut min_d = _mm256_set1_epi32(i32::MAX);
    let mut min_i = _mm256_set1_epi32(i32::MAX);

    let mut i = 0usize;
    while i + 8 <= n {
      let lv = _mm256_loadu_si256(l.as_ptr().add(i) as *const __m256i);
      let av = _mm256_loadu_si256(a.as_ptr().add(i) as *const __m256i);
      let bv = _mm256_loadu_si256(b.as_ptr().add(i) as *const __m256i);
      let dl = _mm256_sub_epi32(ql, lv);
      let da = _mm256_sub_epi32(qa, av);
      let db = _mm256_sub_epi32(qb, bv);
      let d = _mm256_add_epi32(
        _mm256_add_epi32(_mm256_mullo_epi32(dl, dl), _mm256_mullo_epi32(da, da)),
        _mm256_mullo_epi32(db, db),
      );
      let cur_i = _mm256_add_epi32(_mm256_set1_epi32(i as i32), lane_idx);
      // lanes where d < min_d: cmpgt(min_d, d) == (min_d > d) == (d < min_d), STRICT.
      let mask = _mm256_cmpgt_epi32(min_d, d);
      min_d = _mm256_blendv_epi8(min_d, d, mask);
      min_i = _mm256_blendv_epi8(min_i, cur_i, mask);
      i += 8;
    }

    let mut ld = [0i32; 8];
    let mut li = [0i32; 8];
    _mm256_storeu_si256(ld.as_mut_ptr() as *mut __m256i, min_d);
    _mm256_storeu_si256(li.as_mut_ptr() as *mut __m256i, min_i);

    let mut best_d = i64::MAX;
    let mut best = 0usize;
    for (&d_lane, &i_lane) in ld.iter().zip(li.iter()) {
      if i_lane == i32::MAX {
        continue;
      }
      let d = d_lane as i64;
      let idx = i_lane as usize;
      if d < best_d || (d == best_d && idx < best) {
        best_d = d;
        best = idx;
      }
    }
    while i < n {
      let dl = (q[0] - l[i]) as i64;
      let da = (q[1] - a[i]) as i64;
      let db = (q[2] - b[i]) as i64;
      let d = dl * dl + da * da + db * db;
      if d < best_d {
        best_d = d;
        best = i;
      }
      i += 1;
    }
    best
  }
}

/// SSE4.1 (x86_64) implementation of [`opaque_argmin`]: 4-wide i32 argmin of `dl²+da²+db²`,
/// the pre-AVX2 x86 fallback (older Intel Macs, some musl/Windows hosts). Bit-identical to
/// [`opaque_scan_scalar`] and to AVX2 — same exact-integer argmin, narrower lanes:
/// strict per-lane compare (`_mm_cmplt_epi32`) keeps the lowest index within a lane, then a
/// lowest-index cross-lane reduction + the `n % 4` scalar tail reproduce the ascending
/// tie-break. Every intermediate stays < i32::MAX (in-gamut `de ≤ MAX_DELTA_E76_SQ =
/// 669_160_034`; even an out-of-gamut `|Δ| ≤ ~26_000` gives `dl²+da²+db² ≈ 2.03e9 < 2³¹`), so
/// the i32 lanes never overflow. SAFETY: only reachable via `detect`/`opaque_argmin` after
/// `is_x86_feature_detected!("sse4.1")`.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse4.1")]
unsafe fn opaque_scan_sse41(l: &[i32], a: &[i32], b: &[i32], q: [i32; 3]) -> usize {
  use core::arch::x86_64::*;
  unsafe {
    let n = l.len();
    let ql = _mm_set1_epi32(q[0]);
    let qa = _mm_set1_epi32(q[1]);
    let qb = _mm_set1_epi32(q[2]);
    let lane_idx = _mm_setr_epi32(0, 1, 2, 3);
    let mut min_d = _mm_set1_epi32(i32::MAX);
    let mut min_i = _mm_set1_epi32(i32::MAX);

    let mut i = 0usize;
    while i + 4 <= n {
      let lv = _mm_loadu_si128(l.as_ptr().add(i) as *const __m128i);
      let av = _mm_loadu_si128(a.as_ptr().add(i) as *const __m128i);
      let bv = _mm_loadu_si128(b.as_ptr().add(i) as *const __m128i);
      let dl = _mm_sub_epi32(ql, lv);
      let da = _mm_sub_epi32(qa, av);
      let db = _mm_sub_epi32(qb, bv);
      let d = _mm_add_epi32(
        _mm_add_epi32(_mm_mullo_epi32(dl, dl), _mm_mullo_epi32(da, da)),
        _mm_mullo_epi32(db, db),
      );
      let cur_i = _mm_add_epi32(_mm_set1_epi32(i as i32), lane_idx);
      let mask = _mm_cmplt_epi32(d, min_d); // d < min_d, STRICT (SSE2)
      min_d = _mm_blendv_epi8(min_d, d, mask);
      min_i = _mm_blendv_epi8(min_i, cur_i, mask);
      i += 4;
    }

    let mut ld = [0i32; 4];
    let mut li = [0i32; 4];
    _mm_storeu_si128(ld.as_mut_ptr() as *mut __m128i, min_d);
    _mm_storeu_si128(li.as_mut_ptr() as *mut __m128i, min_i);

    let mut best_d = i64::MAX;
    let mut best = 0usize;
    for (&d_lane, &i_lane) in ld.iter().zip(li.iter()) {
      if i_lane == i32::MAX {
        continue;
      }
      let d = d_lane as i64;
      let idx = i_lane as usize;
      if d < best_d || (d == best_d && idx < best) {
        best_d = d;
        best = idx;
      }
    }
    while i < n {
      let dl = (q[0] - l[i]) as i64;
      let da = (q[1] - a[i]) as i64;
      let db = (q[2] - b[i]) as i64;
      let d = dl * dl + da * da + db * db;
      if d < best_d {
        best_d = d;
        best = i;
      }
      i += 1;
    }
    best
  }
}

/// wasm32 `simd128` implementation of [`opaque_argmin`]: 4-wide i32 argmin of `dl²+da²+db²`,
/// the 128-bit path uniform with NEON/SSE4.1. Bit-identical to [`opaque_scan_scalar`]: strict
/// per-lane compare (`i32x4_lt`) keeps the lowest index within a lane, then a lowest-index
/// cross-lane reduction + the `n % 4` scalar tail reproduce the ascending tie-break. Every
/// intermediate stays < i32::MAX (in-gamut `de ≤ MAX_DELTA_E76_SQ = 669_160_034`; even an
/// out-of-gamut `|Δ| ≤ ~26_000` gives `dl²+da²+db² ≈ 2.03e9 < 2³¹`), so the i32 lanes never
/// overflow. SAFETY: only compiled/reached when `simd128` is statically enabled (gated by
/// `cfg(target_feature = "simd128")`), so the ops are always legal.
#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
#[target_feature(enable = "simd128")]
unsafe fn opaque_scan_simd128(l: &[i32], a: &[i32], b: &[i32], q: [i32; 3]) -> usize {
  use core::arch::wasm32::*;
  unsafe {
    let n = l.len();
    let ql = i32x4_splat(q[0]);
    let qa = i32x4_splat(q[1]);
    let qb = i32x4_splat(q[2]);
    let lane_idx = i32x4(0, 1, 2, 3);
    let mut min_d = i32x4_splat(i32::MAX);
    let mut min_i = i32x4_splat(i32::MAX);

    let mut i = 0usize;
    while i + 4 <= n {
      let lv = v128_load(l.as_ptr().add(i) as *const v128);
      let av = v128_load(a.as_ptr().add(i) as *const v128);
      let bv = v128_load(b.as_ptr().add(i) as *const v128);
      let dl = i32x4_sub(ql, lv);
      let da = i32x4_sub(qa, av);
      let db = i32x4_sub(qb, bv);
      let d = i32x4_add(
        i32x4_add(i32x4_mul(dl, dl), i32x4_mul(da, da)),
        i32x4_mul(db, db),
      );
      let cur_i = i32x4_add(i32x4_splat(i as i32), lane_idx);
      // lanes where d < min_d, STRICT signed compare. bitselect(a,b,mask): mask lane all-ones
      // -> pick a. So pick d/cur_i exactly where d < min_d -> lowest index kept within lane.
      let mask = i32x4_lt(d, min_d);
      min_d = v128_bitselect(d, min_d, mask);
      min_i = v128_bitselect(cur_i, min_i, mask);
      i += 4;
    }

    let mut ld = [0i32; 4];
    let mut li = [0i32; 4];
    v128_store(ld.as_mut_ptr() as *mut v128, min_d);
    v128_store(li.as_mut_ptr() as *mut v128, min_i);

    let mut best_d = i64::MAX;
    let mut best = 0usize;
    for (&d_lane, &i_lane) in ld.iter().zip(li.iter()) {
      if i_lane == i32::MAX {
        continue;
      }
      let d = d_lane as i64;
      let idx = i_lane as usize;
      if d < best_d || (d == best_d && idx < best) {
        best_d = d;
        best = idx;
      }
    }
    while i < n {
      let dl = (q[0] - l[i]) as i64;
      let da = (q[1] - a[i]) as i64;
      let db = (q[2] - b[i]) as i64;
      let d = dl * dl + da * da + db * db;
      if d < best_d {
        best_d = d;
        best = i;
      }
      i += 1;
    }
    best
  }
}

// ---------------------------------------------------------------------------
// General path: the full `nearest_lab` perceptual score (pdist_lab + dim/vanish
// penalties + the transparent-slot exclusion), in exact f64 lanes.
// ---------------------------------------------------------------------------

/// Integer-exact reference score of one palette entry — identical to the score
/// `nearest_lab` builds in `quantize.rs` (`pdist_lab` plus `dim_penalty` plus
/// `vanish_penalty`), given `de = delta_e76_sq(query, entry)` already computed.
/// Kept private to this module (the `nearest_lab` original stays in `quantize.rs`);
/// the `general_matches_scalar_*` tests gate every SIMD kernel against it.
///
/// OVERFLOW: `de ≤ ~2.1e9` (in-gamut `MAX_DELTA_E76_SQ ≈ 6.7e8`; out-of-gamut k-means
/// centroids keep `|Δ| ≤ ~26_000` so `de ≤ ~2.03e9`), `wa ≤ 510` so `de·wa ≤ ~1.1e12`;
/// `da²·1500 ≤ 9.75e7`, `3000·d² ≤ 1.95e8`, `100·d³ ≤ 1.66e9` — all far inside `i64`.
#[inline]
#[cfg_attr(not(test), allow(dead_code))] // wired into quantize.rs next
fn general_score_i64(de: i64, entry_a: i64, query_a: i64, guard_src_alpha: i64) -> i64 {
  let wa = query_a + entry_a; // 0..=510
  let da = query_a - entry_a;
  let mut d = de * wa / 510 + da * da * 1500; // pdist_lab: ALPHA_WEIGHT_LAB = 1500
  if guard_src_alpha > 0 && entry_a < guard_src_alpha {
    let drop = guard_src_alpha - entry_a; // 1..=255
    d += 3000 * drop * drop; // dim_penalty: DIM_WEIGHT = 2 * 1500
  }
  if guard_src_alpha > entry_a {
    let drop = guard_src_alpha - entry_a; // 1..=255
    d += 100 * drop * drop * drop; // vanish_penalty: VANISH_WEIGHT = 100
  }
  d
}

/// The same score in `f64` — bit-exact versus [`general_score_i64`]: every operand
/// and intermediate is an integer < 2⁵³, so add/mul/div are exact except the
/// `(de·wa)/510` quotient, which is correctly rounded; `floor` then reproduces the
/// integer floor-division exactly (the exact rational's fractional part is a multiple
/// of 1/510 ≈ 2e-3, while the rounding error is ≤ ~1e-7 at a ~2e9 quotient — an
/// integer boundary can never be crossed). Used by the SIMD lane math and by the
/// per-kernel scalar tails.
#[cfg(any(
  target_arch = "aarch64",
  target_arch = "x86_64",
  all(target_arch = "wasm32", target_feature = "simd128")
))]
#[inline]
#[cfg_attr(not(test), allow(dead_code))] // wired into quantize.rs next
fn general_score_f64(de: f64, entry_a: f64, query_a: f64, guard_src_alpha: f64) -> f64 {
  let wa = query_a + entry_a;
  let da = query_a - entry_a;
  let mut d = (de * wa / 510.0).floor() + da * da * 1500.0;
  if guard_src_alpha > 0.0 && entry_a < guard_src_alpha {
    let drop = guard_src_alpha - entry_a;
    d += 3000.0 * drop * drop;
  }
  if guard_src_alpha > entry_a {
    let drop = guard_src_alpha - entry_a;
    d += 100.0 * drop * drop * drop;
  }
  d
}

/// Index of the palette entry nearest the query under `nearest_lab`'s full perceptual
/// metric — the GENERAL path for translucent queries / palettes carrying a transparent
/// slot. `l`, `a`, `b` are the SoA Lab components (×100 units) and `alpha` the
/// parallel per-entry alphas — parallel, equal-length, non-empty (the caller's palette
/// always has ≥1 entry). `q*` are the query's precomputed Lab components; `query_a`
/// the query alpha; `skip_transparent` / `guard_src_alpha` exactly as in
/// `nearest_lab`.
///
/// Two tiers, identical to `nearest_lab`: tier 1 scans skipping `alpha == 0` entries
/// when `skip_transparent`; if every entry was skipped, tier 2 rescans without the
/// exclusion so the result is always defined. Argmin by strict `<`, lowest index wins.
///
/// The SIMD kernels evaluate the score in f64 lanes (see the module docs for the
/// exactness proof) and return `None` when no lane produced a finite score — i.e.
/// tier 1 excluded everything — so the shared scalar tier-2 fallback runs, exactly
/// like `nearest_lab`'s. `IMAGE_QUANTIZE_SCALAR` / the test override reach this via
/// [`OpaqueKernel::Scalar`], like [`opaque_argmin`].
#[inline]
#[allow(clippy::too_many_arguments)] // flat signature mirrors the per-pixel call site
pub(crate) fn general_argmin(
  kernel: OpaqueKernel,
  l: &[i32],
  a: &[i32],
  b: &[i32],
  alpha: &[u8],
  ql: i32,
  qa_l: i32,
  qb: i32,
  query_a: u8,
  skip_transparent: bool,
  guard_src_alpha: u8,
) -> usize {
  debug_assert_eq!(l.len(), a.len());
  debug_assert_eq!(l.len(), b.len());
  debug_assert_eq!(l.len(), alpha.len());
  debug_assert!(!l.is_empty());
  let q = [ql, qa_l, qb];
  // TIER 1 — `nearest_lab`'s production scan. `None` = every entry was skipped
  // (all-transparent palette), the `best == usize::MAX` case in `nearest_lab`.
  let tier1 = match kernel {
    OpaqueKernel::Scalar => {
      general_scan_tier1_scalar(l, a, b, alpha, q, query_a, skip_transparent, guard_src_alpha)
    }
    #[cfg(target_arch = "aarch64")]
    OpaqueKernel::Neon => unsafe {
      general_scan_neon(l, a, b, alpha, q, query_a, skip_transparent, guard_src_alpha)
    },
    #[cfg(target_arch = "x86_64")]
    OpaqueKernel::Avx2 => unsafe {
      general_scan_avx2(l, a, b, alpha, q, query_a, skip_transparent, guard_src_alpha)
    },
    #[cfg(target_arch = "x86_64")]
    OpaqueKernel::Sse41 => unsafe {
      general_scan_sse41(l, a, b, alpha, q, query_a, skip_transparent, guard_src_alpha)
    },
    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    OpaqueKernel::Simd128 => unsafe {
      general_scan_simd128(l, a, b, alpha, q, query_a, skip_transparent, guard_src_alpha)
    },
  };
  match tier1 {
    Some(best) => best,
    // TIER 2 — all entries excluded by `skip_transparent`: rescan without the
    // exclusion, identical to `nearest_lab`'s fallback (unreachable in production;
    // the palette always has ≥1 visible entry).
    None => general_scan_tier2(l, a, b, alpha, q, query_a, guard_src_alpha),
  }
}

/// Portable integer reference for [`general_argmin`]'s tier-1 scan — `nearest_lab`'s
/// first loop verbatim in i64 (`skip_transparent` exclusion included), returning
/// `None` where `nearest_lab` leaves `best == usize::MAX`. Ascending strict `<` keeps
/// the lowest index on ties.
#[cfg_attr(not(test), allow(dead_code))] // wired into quantize.rs next
#[allow(clippy::too_many_arguments)] // flat per-entry scan args mirror nearest_lab
fn general_scan_tier1_scalar(
  l: &[i32],
  a: &[i32],
  b: &[i32],
  alpha: &[u8],
  q: [i32; 3],
  query_a: u8,
  skip_transparent: bool,
  guard_src_alpha: u8,
) -> Option<usize> {
  let mut best = usize::MAX;
  let mut best_d = i64::MAX;
  for (i, &pa) in alpha.iter().enumerate() {
    if skip_transparent && pa == 0 {
      continue;
    }
    let dl = (q[0] - l[i]) as i64;
    let da = (q[1] - a[i]) as i64;
    let db = (q[2] - b[i]) as i64;
    let de = dl * dl + da * da + db * db;
    let d = general_score_i64(de, pa as i64, query_a as i64, guard_src_alpha as i64);
    if d < best_d {
      best_d = d;
      best = i;
    }
  }
  if best == usize::MAX { None } else { Some(best) }
}

/// Shared tier-2 fallback — `nearest_lab`'s second loop verbatim: same scan without
/// the transparent-slot exclusion, `best` seeded at 0. Reached only when tier 1
/// skipped every entry (all-transparent palette).
#[cfg_attr(not(test), allow(dead_code))] // wired into quantize.rs next
fn general_scan_tier2(
  l: &[i32],
  a: &[i32],
  b: &[i32],
  alpha: &[u8],
  q: [i32; 3],
  query_a: u8,
  guard_src_alpha: u8,
) -> usize {
  let mut best = 0usize;
  let mut best_d = i64::MAX;
  for (i, &pa) in alpha.iter().enumerate() {
    let dl = (q[0] - l[i]) as i64;
    let da = (q[1] - a[i]) as i64;
    let db = (q[2] - b[i]) as i64;
    let de = dl * dl + da * da + db * db;
    let d = general_score_i64(de, pa as i64, query_a as i64, guard_src_alpha as i64);
    if d < best_d {
      best_d = d;
      best = i;
    }
  }
  best
}

/// Cross-lane reduction for the f64 SIMD kernels: fold `(d_lane, i_lane)` pairs
/// into the ascending argmin — lowest score wins, tie goes to the lowest index.
/// `d_lane` non-finite (+INF) means the lane saw only skipped entries (or no block
/// at all when `n < WIDTH`) — it can never win and is skipped, exactly like the
/// `i_lane == i32::MAX` sentinel check in the opaque kernels.
#[cfg(any(
  target_arch = "aarch64",
  target_arch = "x86_64",
  all(target_arch = "wasm32", target_feature = "simd128")
))]
#[inline]
#[cfg_attr(not(test), allow(dead_code))] // wired into quantize.rs next
fn reduce_general_lanes(ld: &[f64], li: &[f64], best_d: &mut f64, best: &mut usize) {
  for (&d_lane, &i_lane) in ld.iter().zip(li.iter()) {
    if !d_lane.is_finite() {
      continue;
    }
    let idx = i_lane as usize;
    if d_lane < *best_d || (d_lane == *best_d && idx < *best) {
      *best_d = d_lane;
      *best = idx;
    }
  }
}

/// Scalar tail for the f64 kernels: entries `i0..n` with the tier-1 exclusion,
/// evaluated by [`general_score_f64`] (bit-exact vs the i64 reference). Strict `<`
/// against the lane winners preserves lowest-index-on-tie.
#[cfg(any(
  target_arch = "aarch64",
  target_arch = "x86_64",
  all(target_arch = "wasm32", target_feature = "simd128")
))]
#[inline]
#[cfg_attr(not(test), allow(dead_code))] // wired into quantize.rs next
#[allow(clippy::too_many_arguments)] // flat tail args mirror the kernel loop state
fn general_tail_f64(
  l: &[i32],
  a: &[i32],
  b: &[i32],
  alpha: &[u8],
  q: [i32; 3],
  query_a: u8,
  skip_transparent: bool,
  guard_src_alpha: u8,
  i0: usize,
  best_d: &mut f64,
  best: &mut usize,
) {
  let qa = query_a as f64;
  let src = guard_src_alpha as f64;
  let mut i = i0;
  while i < l.len() {
    let pa = alpha[i];
    if !(skip_transparent && pa == 0) {
      let dl = (q[0] - l[i]) as f64;
      let da = (q[1] - a[i]) as f64;
      let db = (q[2] - b[i]) as f64;
      let de = dl * dl + da * da + db * db;
      let d = general_score_f64(de, pa as f64, qa, src);
      if d < *best_d {
        *best_d = d;
        *best = i;
      }
    }
    i += 1;
  }
}

/// NEON (aarch64) implementation of [`general_argmin`]'s tier-1 scan: 2-wide f64
/// argmin of the full perceptual score, bit-identical to [`general_scan_tier1_scalar`]
/// (see the module docs for why f64 lanes are exact). Per-lane strict compare
/// (`vcltq_f64`: `d < min_d`) keeps the lowest index within a lane; the 2-lane
/// lowest-index reduction + the `n % 2` scalar tail reproduce the ascending
/// tie-break. `skip_transparent` lanes are forced to +INF so they can never win;
/// when every entry is skipped the winning score stays +INF and `None` is returned,
/// sending the caller to the tier-2 rescan — `nearest_lab`'s `best == usize::MAX`.
/// SAFETY: only reachable via `detect`/`general_argmin` on aarch64, where NEON is
/// guaranteed.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
#[cfg_attr(not(test), allow(dead_code))] // wired into quantize.rs next
#[allow(clippy::too_many_arguments)] // flat SoA args mirror opaque_scan_neon
unsafe fn general_scan_neon(
  l: &[i32],
  a: &[i32],
  b: &[i32],
  alpha: &[u8],
  q: [i32; 3],
  query_a: u8,
  skip_transparent: bool,
  guard_src_alpha: u8,
) -> Option<usize> {
  use core::arch::aarch64::*;
  unsafe {
    let n = l.len();
    let vql = vdupq_n_f64(q[0] as f64);
    let vqa = vdupq_n_f64(q[1] as f64);
    let vqb = vdupq_n_f64(q[2] as f64);
    let vqalpha = vdupq_n_f64(query_a as f64);
    let vsrc = vdupq_n_f64(guard_src_alpha as f64);
    let vzero = vdupq_n_f64(0.0);
    let v510 = vdupq_n_f64(510.0);
    let v1500 = vdupq_n_f64(1500.0);
    let v3000 = vdupq_n_f64(3000.0);
    let v100 = vdupq_n_f64(100.0);
    let vinf = vdupq_n_f64(f64::INFINITY);
    // Winning index per lane, stored as f64 (indices < 2⁵³ are exact); -1.0 sentinel.
    let lane_idx = vld1q_f64([0.0, 1.0].as_ptr());
    let mut min_d = vinf;
    let mut min_i = vdupq_n_f64(-1.0);

    let mut i = 0usize;
    while i + 2 <= n {
      // i32x2 -> i64x2 -> f64x2: every i32 is exactly representable in f64.
      let lf = vcvtq_f64_s64(vmovl_s32(vld1_s32(l.as_ptr().add(i))));
      let af = vcvtq_f64_s64(vmovl_s32(vld1_s32(a.as_ptr().add(i))));
      let bf = vcvtq_f64_s64(vmovl_s32(vld1_s32(b.as_ptr().add(i))));
      let paf = vld1q_f64([alpha[i] as f64, alpha[i + 1] as f64].as_ptr());

      let dl = vsubq_f64(vql, lf);
      let da = vsubq_f64(vqa, af);
      let db = vsubq_f64(vqb, bf);
      let de = vaddq_f64(
        vaddq_f64(vmulq_f64(dl, dl), vmulq_f64(da, da)),
        vmulq_f64(db, db),
      );
      let wa = vaddq_f64(vqalpha, paf);
      // floor((de·wa)/510) — the exact integer floor-division (module docs).
      let term = vrndmq_f64(vdivq_f64(vmulq_f64(de, wa), v510));
      let dalpha = vsubq_f64(vqalpha, paf);
      let mut d = vaddq_f64(term, vmulq_f64(vmulq_f64(dalpha, dalpha), v1500));
      // dim_penalty: (src > 0 && pa < src) -> 3000·(src−pa)², else 0.
      let drop = vsubq_f64(vsrc, paf);
      let dim_m = vandq_u64(vcgtq_f64(vsrc, vzero), vcltq_f64(paf, vsrc));
      let dim = vmulq_f64(v3000, vmulq_f64(drop, drop));
      d = vaddq_f64(d, vbslq_f64(dim_m, dim, vzero));
      // vanish_penalty: (src > pa) -> 100·(src−pa)³, else 0.
      let van_m = vcgtq_f64(vsrc, paf);
      let van = vmulq_f64(v100, vmulq_f64(drop, vmulq_f64(drop, drop)));
      d = vaddq_f64(d, vbslq_f64(van_m, van, vzero));
      // skip_transparent: excluded lanes get +INF — they can never win a strict `<`.
      let skip_m = if skip_transparent {
        vceqq_f64(paf, vzero)
      } else {
        vdupq_n_u64(0)
      };
      d = vbslq_f64(skip_m, vinf, d);

      let cur_i = vaddq_f64(vdupq_n_f64(i as f64), lane_idx);
      // STRICT d < min_d: a later equal score does NOT replace -> lowest index kept.
      let m = vcltq_f64(d, min_d);
      min_d = vbslq_f64(m, d, min_d);
      min_i = vbslq_f64(m, cur_i, min_i);
      i += 2;
    }

    let mut ld = [0.0f64; 2];
    let mut li = [0.0f64; 2];
    vst1q_f64(ld.as_mut_ptr(), min_d);
    vst1q_f64(li.as_mut_ptr(), min_i);
    let mut best_d = f64::INFINITY;
    let mut best = 0usize;
    reduce_general_lanes(&ld, &li, &mut best_d, &mut best);
    general_tail_f64(
      l,
      a,
      b,
      alpha,
      q,
      query_a,
      skip_transparent,
      guard_src_alpha,
      i,
      &mut best_d,
      &mut best,
    );
    // +INF best = every entry skipped -> tier 2 in the caller.
    if best_d.is_finite() { Some(best) } else { None }
  }
}

/// AVX2 (x86_64) implementation of [`general_argmin`]'s tier-1 scan: 4-wide f64
/// argmin of the full perceptual score, the widest x86 path. Bit-identical to
/// [`general_scan_tier1_scalar`] and to the 2-wide kernels — f64 lanes are exact
/// (module docs), the per-lane compare is STRICT (`_CMP_LT_OQ` is ordered `d < min_d`,
/// false on NaN — which cannot occur), and the 4-lane lowest-index reduction + the
/// `n % 4` scalar tail reproduce the ascending tie-break. Skipped lanes score +INF;
/// an all-skipped palette returns `None` for the tier-2 rescan. SAFETY: only
/// reachable via `detect`/`general_argmin` after `is_x86_feature_detected!("avx2")`.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[cfg_attr(not(test), allow(dead_code))] // wired into quantize.rs next
#[allow(clippy::too_many_arguments)] // flat SoA args mirror opaque_scan_avx2
unsafe fn general_scan_avx2(
  l: &[i32],
  a: &[i32],
  b: &[i32],
  alpha: &[u8],
  q: [i32; 3],
  query_a: u8,
  skip_transparent: bool,
  guard_src_alpha: u8,
) -> Option<usize> {
  use core::arch::x86_64::*;
  unsafe {
    let n = l.len();
    let vql = _mm256_set1_pd(q[0] as f64);
    let vqa = _mm256_set1_pd(q[1] as f64);
    let vqb = _mm256_set1_pd(q[2] as f64);
    let vqalpha = _mm256_set1_pd(query_a as f64);
    let vsrc = _mm256_set1_pd(guard_src_alpha as f64);
    let vzero = _mm256_setzero_pd();
    let v510 = _mm256_set1_pd(510.0);
    let v1500 = _mm256_set1_pd(1500.0);
    let v3000 = _mm256_set1_pd(3000.0);
    let v100 = _mm256_set1_pd(100.0);
    let vinf = _mm256_set1_pd(f64::INFINITY);
    // Winning index per lane, stored as f64 (indices < 2⁵³ are exact); -1.0 sentinel.
    let lane_idx = _mm256_set_pd(3.0, 2.0, 1.0, 0.0); // lane0=0 .. lane3=3
    let mut min_d = vinf;
    let mut min_i = _mm256_set1_pd(-1.0);

    let mut i = 0usize;
    while i + 4 <= n {
      // 4×i32 -> 4×f64: vcvtdq2pd ymm reads the four i32 lanes of an xmm.
      let lf = _mm256_cvtepi32_pd(_mm_loadu_si128(l.as_ptr().add(i) as *const __m128i));
      let af = _mm256_cvtepi32_pd(_mm_loadu_si128(a.as_ptr().add(i) as *const __m128i));
      let bf = _mm256_cvtepi32_pd(_mm_loadu_si128(b.as_ptr().add(i) as *const __m128i));
      let paf = _mm256_set_pd(
        alpha[i + 3] as f64,
        alpha[i + 2] as f64,
        alpha[i + 1] as f64,
        alpha[i] as f64,
      );

      let dl = _mm256_sub_pd(vql, lf);
      let da = _mm256_sub_pd(vqa, af);
      let db = _mm256_sub_pd(vqb, bf);
      let de = _mm256_add_pd(
        _mm256_add_pd(_mm256_mul_pd(dl, dl), _mm256_mul_pd(da, da)),
        _mm256_mul_pd(db, db),
      );
      let wa = _mm256_add_pd(vqalpha, paf);
      // floor((de·wa)/510) — the exact integer floor-division (module docs).
      let term = _mm256_floor_pd(_mm256_div_pd(_mm256_mul_pd(de, wa), v510));
      let dalpha = _mm256_sub_pd(vqalpha, paf);
      let mut d = _mm256_add_pd(term, _mm256_mul_pd(_mm256_mul_pd(dalpha, dalpha), v1500));
      // dim_penalty: (src > 0 && pa < src) -> 3000·(src−pa)², else 0.
      let drop = _mm256_sub_pd(vsrc, paf);
      let dim_m = _mm256_and_pd(
        _mm256_cmp_pd::<_CMP_GT_OQ>(vsrc, vzero),
        _mm256_cmp_pd::<_CMP_LT_OQ>(paf, vsrc),
      );
      let dim = _mm256_mul_pd(v3000, _mm256_mul_pd(drop, drop));
      d = _mm256_add_pd(d, _mm256_blendv_pd(vzero, dim, dim_m));
      // vanish_penalty: (src > pa) -> 100·(src−pa)³, else 0.
      let van_m = _mm256_cmp_pd::<_CMP_GT_OQ>(vsrc, paf);
      let van = _mm256_mul_pd(v100, _mm256_mul_pd(drop, _mm256_mul_pd(drop, drop)));
      d = _mm256_add_pd(d, _mm256_blendv_pd(vzero, van, van_m));
      // skip_transparent: excluded lanes get +INF — they can never win a strict `<`.
      let skip_m = if skip_transparent {
        _mm256_cmp_pd::<_CMP_EQ_OQ>(paf, vzero)
      } else {
        _mm256_setzero_pd()
      };
      d = _mm256_blendv_pd(d, vinf, skip_m);

      let cur_i = _mm256_add_pd(_mm256_set1_pd(i as f64), lane_idx);
      // STRICT d < min_d (ordered): a later equal score does NOT replace.
      let m = _mm256_cmp_pd::<_CMP_LT_OQ>(d, min_d);
      min_d = _mm256_blendv_pd(min_d, d, m);
      min_i = _mm256_blendv_pd(min_i, cur_i, m);
      i += 4;
    }

    let mut ld = [0.0f64; 4];
    let mut li = [0.0f64; 4];
    _mm256_storeu_pd(ld.as_mut_ptr(), min_d);
    _mm256_storeu_pd(li.as_mut_ptr(), min_i);
    let mut best_d = f64::INFINITY;
    let mut best = 0usize;
    reduce_general_lanes(&ld, &li, &mut best_d, &mut best);
    general_tail_f64(
      l,
      a,
      b,
      alpha,
      q,
      query_a,
      skip_transparent,
      guard_src_alpha,
      i,
      &mut best_d,
      &mut best,
    );
    if best_d.is_finite() { Some(best) } else { None }
  }
}

/// SSE4.1 (x86_64) implementation of [`general_argmin`]'s tier-1 scan: 2-wide f64
/// argmin, the pre-AVX2 x86 fallback. Bit-identical to [`general_scan_tier1_scalar`]
/// and to AVX2 — same exact-f64 argmin, narrower lanes: strict per-lane compare
/// (`_mm_cmplt_pd`), lowest-index cross-lane reduction, `n % 2` scalar tail. Skipped
/// lanes score +INF; an all-skipped palette returns `None` for the tier-2 rescan.
/// SAFETY: only reachable via `detect`/`general_argmin` after
/// `is_x86_feature_detected!("sse4.1")`.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse4.1")]
#[cfg_attr(not(test), allow(dead_code))] // wired into quantize.rs next
#[allow(clippy::too_many_arguments)] // flat SoA args mirror opaque_scan_sse41
unsafe fn general_scan_sse41(
  l: &[i32],
  a: &[i32],
  b: &[i32],
  alpha: &[u8],
  q: [i32; 3],
  query_a: u8,
  skip_transparent: bool,
  guard_src_alpha: u8,
) -> Option<usize> {
  use core::arch::x86_64::*;
  unsafe {
    let n = l.len();
    let vql = _mm_set1_pd(q[0] as f64);
    let vqa = _mm_set1_pd(q[1] as f64);
    let vqb = _mm_set1_pd(q[2] as f64);
    let vqalpha = _mm_set1_pd(query_a as f64);
    let vsrc = _mm_set1_pd(guard_src_alpha as f64);
    let vzero = _mm_setzero_pd();
    let v510 = _mm_set1_pd(510.0);
    let v1500 = _mm_set1_pd(1500.0);
    let v3000 = _mm_set1_pd(3000.0);
    let v100 = _mm_set1_pd(100.0);
    let vinf = _mm_set1_pd(f64::INFINITY);
    // Winning index per lane, stored as f64 (indices < 2⁵³ are exact); -1.0 sentinel.
    let lane_idx = _mm_set_pd(1.0, 0.0); // lane0=0, lane1=1
    let mut min_d = vinf;
    let mut min_i = _mm_set1_pd(-1.0);

    let mut i = 0usize;
    while i + 2 <= n {
      // 2×i32 -> 2×f64: movq loads the two i32s, cvtdq2pd converts them.
      let lf = _mm_cvtepi32_pd(_mm_loadl_epi64(l.as_ptr().add(i) as *const __m128i));
      let af = _mm_cvtepi32_pd(_mm_loadl_epi64(a.as_ptr().add(i) as *const __m128i));
      let bf = _mm_cvtepi32_pd(_mm_loadl_epi64(b.as_ptr().add(i) as *const __m128i));
      let paf = _mm_set_pd(alpha[i + 1] as f64, alpha[i] as f64);

      let dl = _mm_sub_pd(vql, lf);
      let da = _mm_sub_pd(vqa, af);
      let db = _mm_sub_pd(vqb, bf);
      let de = _mm_add_pd(
        _mm_add_pd(_mm_mul_pd(dl, dl), _mm_mul_pd(da, da)),
        _mm_mul_pd(db, db),
      );
      let wa = _mm_add_pd(vqalpha, paf);
      // floor((de·wa)/510) — the exact integer floor-division (module docs).
      let term = _mm_floor_pd(_mm_div_pd(_mm_mul_pd(de, wa), v510));
      let dalpha = _mm_sub_pd(vqalpha, paf);
      let mut d = _mm_add_pd(term, _mm_mul_pd(_mm_mul_pd(dalpha, dalpha), v1500));
      // dim_penalty: (src > 0 && pa < src) -> 3000·(src−pa)², else 0.
      let drop = _mm_sub_pd(vsrc, paf);
      let dim_m = _mm_and_pd(_mm_cmpgt_pd(vsrc, vzero), _mm_cmplt_pd(paf, vsrc));
      let dim = _mm_mul_pd(v3000, _mm_mul_pd(drop, drop));
      d = _mm_add_pd(d, _mm_blendv_pd(vzero, dim, dim_m));
      // vanish_penalty: (src > pa) -> 100·(src−pa)³, else 0.
      let van_m = _mm_cmpgt_pd(vsrc, paf);
      let van = _mm_mul_pd(v100, _mm_mul_pd(drop, _mm_mul_pd(drop, drop)));
      d = _mm_add_pd(d, _mm_blendv_pd(vzero, van, van_m));
      // skip_transparent: excluded lanes get +INF — they can never win a strict `<`.
      let skip_m = if skip_transparent {
        _mm_cmpeq_pd(paf, vzero)
      } else {
        _mm_setzero_pd()
      };
      d = _mm_blendv_pd(d, vinf, skip_m);

      let cur_i = _mm_add_pd(_mm_set1_pd(i as f64), lane_idx);
      // STRICT d < min_d: a later equal score does NOT replace -> lowest index kept.
      let m = _mm_cmplt_pd(d, min_d);
      min_d = _mm_blendv_pd(min_d, d, m);
      min_i = _mm_blendv_pd(min_i, cur_i, m);
      i += 2;
    }

    let mut ld = [0.0f64; 2];
    let mut li = [0.0f64; 2];
    _mm_storeu_pd(ld.as_mut_ptr(), min_d);
    _mm_storeu_pd(li.as_mut_ptr(), min_i);
    let mut best_d = f64::INFINITY;
    let mut best = 0usize;
    reduce_general_lanes(&ld, &li, &mut best_d, &mut best);
    general_tail_f64(
      l,
      a,
      b,
      alpha,
      q,
      query_a,
      skip_transparent,
      guard_src_alpha,
      i,
      &mut best_d,
      &mut best,
    );
    if best_d.is_finite() { Some(best) } else { None }
  }
}

/// wasm32 `simd128` implementation of [`general_argmin`]'s tier-1 scan: 2-wide f64
/// argmin, the 128-bit path uniform with NEON/SSE4.1. Bit-identical to
/// [`general_scan_tier1_scalar`]: wasm f64x2 ops are IEEE-754 correctly rounded, so
/// the lane math is exact (module docs); strict per-lane compare (`f64x2_lt`) keeps
/// the lowest index within a lane, then a lowest-index cross-lane reduction + the
/// `n % 2` scalar tail reproduce the ascending tie-break. Skipped lanes score +INF;
/// an all-skipped palette returns `None` for the tier-2 rescan. SAFETY: only
/// compiled/reached when `simd128` is statically enabled (gated by
/// `cfg(target_feature = "simd128")`), so the ops are always legal.
#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
#[target_feature(enable = "simd128")]
#[cfg_attr(not(test), allow(dead_code))] // wired into quantize.rs next
#[allow(clippy::too_many_arguments)] // flat SoA args mirror opaque_scan_simd128
unsafe fn general_scan_simd128(
  l: &[i32],
  a: &[i32],
  b: &[i32],
  alpha: &[u8],
  q: [i32; 3],
  query_a: u8,
  skip_transparent: bool,
  guard_src_alpha: u8,
) -> Option<usize> {
  use core::arch::wasm32::*;
  unsafe {
    let n = l.len();
    let vql = f64x2_splat(q[0] as f64);
    let vqa = f64x2_splat(q[1] as f64);
    let vqb = f64x2_splat(q[2] as f64);
    let vqalpha = f64x2_splat(query_a as f64);
    let vsrc = f64x2_splat(guard_src_alpha as f64);
    let vzero = f64x2_splat(0.0);
    let v510 = f64x2_splat(510.0);
    let v1500 = f64x2_splat(1500.0);
    let v3000 = f64x2_splat(3000.0);
    let v100 = f64x2_splat(100.0);
    let vinf = f64x2_splat(f64::INFINITY);
    // Winning index per lane, stored as f64 (indices < 2⁵³ are exact); -1.0 sentinel.
    let lane_idx = f64x2(0.0, 1.0);
    let mut min_d = vinf;
    let mut min_i = f64x2_splat(-1.0);

    let mut i = 0usize;
    while i + 2 <= n {
      // Load 8 bytes (2×i32) into the low half; convert low i32x2 -> f64x2.
      let lf = f64x2_convert_low_i32x4(v128_load64_zero(l.as_ptr().add(i) as *const u64));
      let af = f64x2_convert_low_i32x4(v128_load64_zero(a.as_ptr().add(i) as *const u64));
      let bf = f64x2_convert_low_i32x4(v128_load64_zero(b.as_ptr().add(i) as *const u64));
      let paf = f64x2(alpha[i] as f64, alpha[i + 1] as f64);

      let dl = f64x2_sub(vql, lf);
      let da = f64x2_sub(vqa, af);
      let db = f64x2_sub(vqb, bf);
      let de = f64x2_add(
        f64x2_add(f64x2_mul(dl, dl), f64x2_mul(da, da)),
        f64x2_mul(db, db),
      );
      let wa = f64x2_add(vqalpha, paf);
      // floor((de·wa)/510) — the exact integer floor-division (module docs).
      let term = f64x2_floor(f64x2_div(f64x2_mul(de, wa), v510));
      let dalpha = f64x2_sub(vqalpha, paf);
      let mut d = f64x2_add(term, f64x2_mul(f64x2_mul(dalpha, dalpha), v1500));
      // dim_penalty: (src > 0 && pa < src) -> 3000·(src−pa)², else 0.
      let drop = f64x2_sub(vsrc, paf);
      let dim_m = v128_and(f64x2_gt(vsrc, vzero), f64x2_lt(paf, vsrc));
      let dim = f64x2_mul(v3000, f64x2_mul(drop, drop));
      d = f64x2_add(d, v128_bitselect(dim, vzero, dim_m));
      // vanish_penalty: (src > pa) -> 100·(src−pa)³, else 0.
      let van_m = f64x2_gt(vsrc, paf);
      let van = f64x2_mul(v100, f64x2_mul(drop, f64x2_mul(drop, drop)));
      d = f64x2_add(d, v128_bitselect(van, vzero, van_m));
      // skip_transparent: excluded lanes get +INF — they can never win a strict `<`.
      let skip_m = if skip_transparent {
        f64x2_eq(paf, vzero)
      } else {
        v128_and(vzero, vzero) // all-zero mask
      };
      d = v128_bitselect(vinf, d, skip_m);

      let cur_i = f64x2_add(f64x2_splat(i as f64), lane_idx);
      // STRICT d < min_d: a later equal score does NOT replace -> lowest index kept.
      // bitselect(a,b,mask): mask all-ones lane -> pick a.
      let m = f64x2_lt(d, min_d);
      min_d = v128_bitselect(d, min_d, m);
      min_i = v128_bitselect(cur_i, min_i, m);
      i += 2;
    }

    let mut ld = [0.0f64; 2];
    let mut li = [0.0f64; 2];
    v128_store(ld.as_mut_ptr() as *mut v128, min_d);
    v128_store(li.as_mut_ptr() as *mut v128, min_i);
    let mut best_d = f64::INFINITY;
    let mut best = 0usize;
    reduce_general_lanes(&ld, &li, &mut best_d, &mut best);
    general_tail_f64(
      l,
      a,
      b,
      alpha,
      q,
      query_a,
      skip_transparent,
      guard_src_alpha,
      i,
      &mut best_d,
      &mut best,
    );
    if best_d.is_finite() { Some(best) } else { None }
  }
}

/// Test-only thread-local kernel override, so a single test process can run the same
/// quantize twice — once on the detected SIMD kernel, once forced to scalar — and assert
/// byte-identical output. Thread-local (not a global) so parallel tests don't race.
#[cfg(test)]
pub(crate) mod test_override {
  use super::OpaqueKernel;
  use std::cell::Cell;

  thread_local! {
    static OVERRIDE: Cell<Option<OpaqueKernel>> = const { Cell::new(None) };
  }

  pub(crate) fn get() -> Option<OpaqueKernel> {
    OVERRIDE.with(|c| c.get())
  }

  pub(crate) fn set(k: Option<OpaqueKernel>) {
    OVERRIDE.with(|c| c.set(k));
  }

  /// Runs `f` with [`super::detect`] forced to `k`, restoring the prior override after.
  pub(crate) fn with<R>(k: OpaqueKernel, f: impl FnOnce() -> R) -> R {
    let prev = get();
    set(Some(k));
    let r = f();
    set(prev);
    r
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// A tiny deterministic LCG so the equivalence sweeps need no `rand` dep.
  struct Lcg(u64);
  impl Lcg {
    fn next_u32(&mut self) -> u32 {
      self.0 = self
        .0
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
      (self.0 >> 32) as u32
    }
    /// A pseudo-random Lab component in a generous in-gamut range (×100 units).
    fn lab(&mut self) -> i32 {
      (self.next_u32() % 26_001) as i32 - 1000 // roughly [-1000, 25000]
    }
  }

  fn build_soa(n: usize, rng: &mut Lcg) -> (Vec<i32>, Vec<i32>, Vec<i32>) {
    let l = (0..n).map(|_| rng.lab()).collect();
    let a = (0..n).map(|_| rng.lab()).collect();
    let b = (0..n).map(|_| rng.lab()).collect();
    (l, a, b)
  }

  #[test]
  fn scalar_argmin_is_lowest_index_on_ties() {
    // Two identical entries at the exact query: the lower index must win.
    let l = vec![10, 10, 99];
    let a = vec![20, 20, 99];
    let b = vec![30, 30, 99];
    assert_eq!(opaque_scan_scalar(&l, &a, &b, [10, 20, 30]), 0);
  }

  #[test]
  fn scalar_argmin_picks_nearest() {
    let l = vec![0, 100, 5];
    let a = vec![0, 100, 5];
    let b = vec![0, 100, 5];
    // Query (4,4,4): entry 2 (5,5,5) is closest.
    assert_eq!(opaque_scan_scalar(&l, &a, &b, [4, 4, 4]), 2);
  }

  /// Shared equivalence harness reused by every SIMD phase: assert a candidate kernel
  /// reproduces the scalar reference across many randomized palettes + queries, including
  /// palette sizes that exercise non-multiple-of-lane-width tails (1..=300) and forced
  /// exact-tie queries.
  fn assert_kernel_matches_scalar(
    name: &str,
    run: impl Fn(&[i32], &[i32], &[i32], [i32; 3]) -> usize,
  ) {
    let mut rng = Lcg(0x9E3779B97F4A7C15);
    for n in 1..=300usize {
      let (l, a, b) = build_soa(n, &mut rng);
      for _ in 0..8 {
        let q = [rng.lab(), rng.lab(), rng.lab()];
        let want = opaque_scan_scalar(&l, &a, &b, q);
        let got = run(&l, &a, &b, q);
        assert_eq!(got, want, "{name}: n={n} q={q:?}");
      }
      // Query that lands exactly on a random entry -> exercises ties / exact zero.
      let hit = (rng.next_u32() as usize) % n;
      let q = [l[hit], a[hit], b[hit]];
      assert_eq!(
        run(&l, &a, &b, q),
        opaque_scan_scalar(&l, &a, &b, q),
        "{name}: exact-hit n={n}"
      );
    }
  }

  #[test]
  fn scalar_matches_itself() {
    // Sanity-checks the harness (and the dispatch) before any SIMD kernel exists; later
    // phases add `kernel_matches_scalar_<isa>` calling the same harness with the intrinsic.
    assert_kernel_matches_scalar("dispatch-scalar", |l, a, b, q| {
      opaque_argmin(OpaqueKernel::Scalar, l, a, b, q)
    });
  }

  #[cfg(target_arch = "aarch64")]
  #[test]
  fn kernel_matches_scalar_neon() {
    assert_kernel_matches_scalar("neon", |l, a, b, q| unsafe { opaque_scan_neon(l, a, b, q) });
  }

  #[cfg(target_arch = "x86_64")]
  #[test]
  fn kernel_matches_scalar_avx2() {
    if !std::is_x86_feature_detected!("avx2") {
      return; // host (or Rosetta) without AVX2: skip; real-AVX2 x86 (CI / qemu) covers it
    }
    assert_kernel_matches_scalar("avx2", |l, a, b, q| unsafe { opaque_scan_avx2(l, a, b, q) });
  }

  #[cfg(target_arch = "x86_64")]
  #[test]
  fn kernel_matches_scalar_sse41() {
    if !std::is_x86_feature_detected!("sse4.1") {
      return; // host (or Rosetta) without SSE4.1: skip; x86 CI covers it
    }
    assert_kernel_matches_scalar("sse41", |l, a, b, q| unsafe {
      opaque_scan_sse41(l, a, b, q)
    });
  }

  #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
  #[test]
  fn kernel_matches_scalar_simd128() {
    assert_kernel_matches_scalar("simd128", |l, a, b, q| unsafe {
      opaque_scan_simd128(l, a, b, q)
    });
  }

  // ---------------------------------------------------------------------
  // General path: full perceptual score — alpha term, dim/vanish penalties,
  // transparent-slot exclusion, and the all-transparent tier-2 fallback.
  // ---------------------------------------------------------------------

  /// The oracle for the general path: [`general_argmin`] on `OpaqueKernel::Scalar`,
  /// which runs `general_scan_tier1_scalar`/`general_scan_tier2` — `nearest_lab`'s
  /// algorithm verbatim in i64.
  fn reference_general(
    l: &[i32],
    a: &[i32],
    b: &[i32],
    alpha: &[u8],
    q: [i32; 3],
    query_a: u8,
    skip: bool,
    guard: u8,
  ) -> usize {
    general_argmin(
      OpaqueKernel::Scalar,
      l,
      a,
      b,
      alpha,
      q[0],
      q[1],
      q[2],
      query_a,
      skip,
      guard,
    )
  }

  /// A random palette whose alphas lean on every corner: `pa == 0` (the reserved
  /// transparent slot tier-1 skips), `pa == 255`, `pa == 1`, plus uniform random —
  /// so every penalty branch and the skip path are exercised constantly.
  fn build_general(n: usize, rng: &mut Lcg) -> (Vec<i32>, Vec<i32>, Vec<i32>, Vec<u8>) {
    let (l, a, b) = build_soa(n, rng);
    let alpha = (0..n)
      .map(|_| match rng.next_u32() % 4 {
        0 => 0u8,
        1 => 255,
        2 => 1,
        _ => (rng.next_u32() % 256) as u8,
      })
      .collect();
    (l, a, b, alpha)
  }

  fn corner_u8(rng: &mut Lcg) -> u8 {
    match rng.next_u32() % 4 {
      0 => 0,
      1 => 255,
      _ => (rng.next_u32() % 256) as u8,
    }
  }

  /// Shared general-path equivalence harness, mirroring
  /// [`assert_kernel_matches_scalar`]: the candidate kernel must return the scalar
  /// reference's index across randomized palettes (sizes 1..=96 cover every lane
  /// width's tail) and queries — `pa == 0` skips, `query_a`/`guard` at the 0/255
  /// corners, both `skip_transparent` settings — plus exact-tie queries, an
  /// alpha-corner sweep, and the all-transparent tier-2 fallback.
  fn assert_general_kernel_matches_scalar(
    name: &str,
    run: impl Fn(&[i32], &[i32], &[i32], &[u8], [i32; 3], u8, bool, u8) -> usize,
  ) {
    let mut rng = Lcg(0xD1B54A32D192ED03);
    for n in 1..=96usize {
      let (l, a, b, alpha) = build_general(n, &mut rng);
      for _ in 0..8 {
        let q = [rng.lab(), rng.lab(), rng.lab()];
        let query_a = corner_u8(&mut rng);
        let guard = corner_u8(&mut rng);
        let skip = rng.next_u32() % 2 == 0;
        let want = reference_general(&l, &a, &b, &alpha, q, query_a, skip, guard);
        let got = run(&l, &a, &b, &alpha, q, query_a, skip, guard);
        assert_eq!(
          got, want,
          "{name}: n={n} q={q:?} query_a={query_a} guard={guard} skip={skip}"
        );
      }
      // Query that lands exactly on a random entry -> exercises exact-zero scores
      // and ties (with `query_a` forced to that entry's alpha, the alpha term and
      // both penalties collapse toward 0 for a true duplicate).
      let hit = (rng.next_u32() as usize) % n;
      let q = [l[hit], a[hit], b[hit]];
      let want = reference_general(&l, &a, &b, &alpha, q, alpha[hit], false, 0);
      assert_eq!(
        run(&l, &a, &b, &alpha, q, alpha[hit], false, 0),
        want,
        "{name}: exact-hit n={n}"
      );
    }

    // Exhaustive-ish corner sweep: entry alpha / query alpha / guard each at
    // {0,1,127,255} × skip on/off over a fixed 9-entry palette (9 > every lane
    // width) — every combination of the skip mask, alpha term, and both penalties
    // switching on/off.
    let l = [0, 5000, 10000, -500, 2500, 7500, 0, 9000, 1234];
    let a = [0, -8000, 8000, 100, -100, 5000, -5000, 0, 4321];
    let b = [0, 9000, -9000, -100, 100, -5000, 5000, 0, -1111];
    let alphas = [0u8, 1, 127, 128, 254, 255, 0, 200, 64];
    for &query_a in &[0u8, 1, 127, 255] {
      for &guard in &[0u8, 1, 127, 255] {
        for &skip in &[false, true] {
          let q = [1111, -2222, 3333];
          let want = reference_general(&l, &a, &b, &alphas, q, query_a, skip, guard);
          let got = run(&l, &a, &b, &alphas, q, query_a, skip, guard);
          assert_eq!(got, want, "{name}: corners qa={query_a} g={guard} s={skip}");
        }
      }
    }

    // Exact-tie palette: every entry identical (Lab + alpha) -> every score equal
    // -> the argmin must be index 0 on every kernel, whatever the lane width.
    let n = 12;
    let l = vec![4321i32; n];
    let a = vec![-1234i32; n];
    let b = vec![2222i32; n];
    let alpha = vec![200u8; n];
    for &skip in &[false, true] {
      for &(query_a, guard) in &[(0u8, 0u8), (255, 255), (200, 100)] {
        let q = [4321, -1234, 2222];
        let want = reference_general(&l, &a, &b, &alpha, q, query_a, skip, guard);
        assert_eq!(want, 0, "sanity: all-identical palette ties at index 0");
        assert_eq!(
          run(&l, &a, &b, &alpha, q, query_a, skip, guard),
          want,
          "{name}: all-identical tie qa={query_a} g={guard} s={skip}"
        );
      }
    }
    // Near-tie: entries 4 and 9 sit one Lab unit off the minimum — the argmin must
    // land on the lowest-index exact minimum (index 2), not a 1-off near-tie.
    let mut l = vec![7000i32; 11];
    let mut a = vec![1000i32; 11];
    let mut b = vec![-3000i32; 11];
    l[2] = 5000;
    l[5] = 5000;
    l[4] = 5001;
    l[9] = 5001;
    a[2] = 0;
    a[5] = 0;
    b[2] = 0;
    b[5] = 0;
    let alpha = vec![255u8; 11];
    let q = [5000, 0, 0];
    let want = reference_general(&l, &a, &b, &alpha, q, 255, true, 255);
    assert_eq!(want, 2, "sanity: lowest-index exact minimum wins");
    assert_eq!(run(&l, &a, &b, &alpha, q, 255, true, 255), want, "{name}: near-tie");

    // All-transparent palette + skip_transparent: tier-1 excludes EVERY entry, so
    // the kernel must return `None` internally and the caller falls to tier-2 —
    // the `nearest_lab` fallback. `skip == false` on the same palette stays in
    // tier-1. n=33 exceeds every lane width and leaves a tail.
    let n = 33;
    let (l, a, b) = build_soa(n, &mut rng);
    let alpha = vec![0u8; n];
    for &skip in &[true, false] {
      for _ in 0..4 {
        let q = [rng.lab(), rng.lab(), rng.lab()];
        let query_a = corner_u8(&mut rng);
        let guard = corner_u8(&mut rng);
        let want = reference_general(&l, &a, &b, &alpha, q, query_a, skip, guard);
        let got = run(&l, &a, &b, &alpha, q, query_a, skip, guard);
        assert_eq!(got, want, "{name}: all-transparent skip={skip}");
      }
    }
  }

  #[test]
  fn general_scalar_is_lowest_index_on_ties() {
    // Two identical opaque entries at the exact query: the lower index must win.
    let l = vec![10, 10, 99];
    let a = vec![20, 20, 99];
    let b = vec![30, 30, 99];
    let alpha = vec![255u8, 255, 255];
    assert_eq!(
      general_argmin(OpaqueKernel::Scalar, &l, &a, &b, &alpha, 10, 20, 30, 255, true, 255),
      0
    );
  }

  #[test]
  fn general_skip_transparent_excludes_zero_alpha() {
    // Entry 0 is the exact query color but fully transparent (the reserved slot):
    // with `skip_transparent` it must NOT win; without, it must.
    let l = vec![100, 9999];
    let a = vec![100, 9999];
    let b = vec![100, 9999];
    let alpha = vec![0u8, 255];
    assert_eq!(
      general_argmin(OpaqueKernel::Scalar, &l, &a, &b, &alpha, 100, 100, 100, 255, true, 0),
      1
    );
    assert_eq!(
      general_argmin(OpaqueKernel::Scalar, &l, &a, &b, &alpha, 100, 100, 100, 255, false, 0),
      0
    );
  }

  #[test]
  fn general_all_skipped_falls_to_tier2() {
    // Every entry alpha == 0 + skip_transparent: tier-1 finds nothing, tier-2
    // rescans without the exclusion. Entry 2 is the color argmin, not index 0 —
    // proving the fallback ran rather than returning the tier-1 sentinel.
    let l = vec![0, 5000, 9000];
    let a = vec![0, -8000, 8000];
    let b = vec![0, 9000, -9000];
    let alpha = vec![0u8; 3];
    let q = [8888, 7777, -8888];
    let want = reference_general(&l, &a, &b, &alpha, q, 200, true, 0);
    assert_eq!(want, 2, "sanity: tier-2 argmin is 2, not the tier-1 empty result");
    assert_eq!(
      general_argmin(OpaqueKernel::Scalar, &l, &a, &b, &alpha, q[0], q[1], q[2], 200, true, 0),
      2
    );
  }

  #[test]
  fn general_scalar_matches_itself() {
    // Sanity-checks the general harness + dispatch before any SIMD general kernel
    // is gated against it.
    assert_general_kernel_matches_scalar("dispatch-scalar", |l, a, b, al, q, qa, s, g| {
      general_argmin(OpaqueKernel::Scalar, l, a, b, al, q[0], q[1], q[2], qa, s, g)
    });
  }

  /// The runtime-detected kernel through the real [`general_argmin`] dispatch:
  /// on a SIMD host this gates the production path, not just the raw intrinsic.
  #[test]
  fn general_detected_dispatch_matches_scalar() {
    let kernel = detect();
    assert_general_kernel_matches_scalar("detected-dispatch", move |l, a, b, al, q, qa, s, g| {
      general_argmin(kernel, l, a, b, al, q[0], q[1], q[2], qa, s, g)
    });
  }

  #[cfg(target_arch = "aarch64")]
  #[test]
  fn general_matches_scalar_neon() {
    assert_general_kernel_matches_scalar("neon", |l, a, b, al, q, qa, s, g| {
      match unsafe { general_scan_neon(l, a, b, al, q, qa, s, g) } {
        Some(i) => i,
        None => general_scan_tier2(l, a, b, al, q, qa, g),
      }
    });
  }

  #[cfg(target_arch = "x86_64")]
  #[test]
  fn general_matches_scalar_avx2() {
    if !std::is_x86_feature_detected!("avx2") {
      return; // host (or Rosetta) without AVX2: skip; real-AVX2 x86 (CI / qemu) covers it
    }
    assert_general_kernel_matches_scalar("avx2", |l, a, b, al, q, qa, s, g| {
      match unsafe { general_scan_avx2(l, a, b, al, q, qa, s, g) } {
        Some(i) => i,
        None => general_scan_tier2(l, a, b, al, q, qa, g),
      }
    });
  }

  #[cfg(target_arch = "x86_64")]
  #[test]
  fn general_matches_scalar_sse41() {
    if !std::is_x86_feature_detected!("sse4.1") {
      return; // host (or Rosetta) without SSE4.1: skip; x86 CI covers it
    }
    assert_general_kernel_matches_scalar("sse41", |l, a, b, al, q, qa, s, g| {
      match unsafe { general_scan_sse41(l, a, b, al, q, qa, s, g) } {
        Some(i) => i,
        None => general_scan_tier2(l, a, b, al, q, qa, g),
      }
    });
  }

  #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
  #[test]
  fn general_matches_scalar_simd128() {
    assert_general_kernel_matches_scalar("simd128", |l, a, b, al, q, qa, s, g| {
      match unsafe { general_scan_simd128(l, a, b, al, q, qa, s, g) } {
        Some(i) => i,
        None => general_scan_tier2(l, a, b, al, q, qa, g),
      }
    });
  }
}
