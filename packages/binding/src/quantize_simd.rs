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
//! with `de ≤ MAX_DELTA_E76_SQ = 669_160_034 < 2³¹`, so the whole scan fits in **i32
//! lanes** — no divide, no 64-bit, no penalties, no branches. This module provides an
//! `i32` argmin over Structure-of-Arrays palette Lab components (`l[]`, `a[]`, `b[]`),
//! dispatched at runtime to the widest SIMD the host actually supports, with a scalar
//! reference that defines the result.
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

/// Runtime feature probe, per architecture. Phase 0 has no SIMD variants yet, so every
/// arch resolves to [`OpaqueKernel::Scalar`]; later phases fill in the `is_*_detected`
/// branches. Kept separate from [`detect`] so the override / escape-hatch logic is shared.
#[inline]
fn detect_native() -> OpaqueKernel {
  OpaqueKernel::Scalar
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
}
