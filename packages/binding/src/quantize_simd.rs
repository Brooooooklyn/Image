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
//! palette Lab components (`l[]`, `a[]`, `b[]`), dispatched at runtime to a 128-bit 4-lane
//! SIMD kernel on every supported arch, with a scalar reference that defines the result.
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
//! # Never panic
//!
//! Each `*_simd` kernel is an `unsafe fn` marked `#[target_feature(enable = …)]`; it is
//! ONLY ever reached through [`detect`], which gates on `is_*_feature_detected!` (x86) or
//! a guaranteed-baseline arch (`neon` on aarch64, `simd128` compiled-in on wasm). A host
//! lacking the feature takes the scalar path, so no illegal instruction can execute. The
//! `IMAGE_QUANTIZE_SCALAR=1` escape hatch (and the test-only thread-local override) force
//! the scalar path on a SIMD-capable host to prove the fallback.

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
  /// SSE4.1 (x86_64) 4-wide i32 argmin — the 128-bit 4-lane path, uniform with
  /// NEON/simd128. Selected when the host reports SSE4.1; older x86 falls back to scalar.
  /// AVX2's extra width is intentionally unused so every platform runs the identical
  /// 4-lane reduction (one structure to audit for the byte-identical cross-platform contract).
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
    // Probe at runtime so a pre-SSE4.1 x86 host never executes an unsupported
    // instruction — it falls back to scalar. SSE4.1 is the 4-lane 128-bit path; AVX2's
    // extra width is intentionally unused so every platform runs the identical reduction.
    if std::is_x86_feature_detected!("sse4.1") {
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

/// SSE4.1 (x86_64) implementation of [`opaque_argmin`]: 4-wide i32 argmin of `dl²+da²+db²`,
/// the 128-bit path uniform with NEON/simd128 (AVX2's extra width is intentionally unused so
/// every arch runs the identical 4-lane reduction). Bit-identical to [`opaque_scan_scalar`]:
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
}
