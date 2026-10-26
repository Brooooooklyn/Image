//! SIMD kernels for [`HdrToSrgb::apply_u16`] and [`HdrToSrgb::apply_f32`] on RGB(A) pixels.
//!
//! One generic body (`apply_u16_v` / `apply_f32_v`) over a small lane trait ([`Lanes`]), built
//! once per ISA: NEON (aarch64, 4 lanes), AVX2 (x86_64, 8 lanes), SSE4.1 (x86_64, 4 lanes) and
//! simd128 (wasm32 built with `+simd128`, 4 lanes). Eight pixels are moved into one array per
//! channel; the maths between the table lookups (gain interpolation, matrix, out-of-gamut test,
//! `gamut_map`, sqrt, encode index) runs on lanes; the three tables are still read one lane at a
//! time. Gray goes to the scalar code (or to the gray table in `apply_u16`).
//!
//! # Same bits as the scalar reference
//!
//! Every kernel must give exactly what `apply_u16_scalar` / `apply_f32_scalar` give, for every
//! input: an out-of-gamut pixel at the source peak can move by more than 1000 codes on a change
//! of one ulp (see `gamut_map`). So each lane op is the scalar op, in the scalar order:
//! - No FMA anywhere. Rust never fuses `a * b + c` on its own; neither may a kernel.
//! - `max_num` / `min_num` are Rust's `f32::max` / `f32::min`, which ignore a NaN argument.
//!   NEON `vmaxnmq`/`vminnmq` are exactly that. x86 `maxps`/`minps` return their second operand
//!   when either is NaN, so `blendv(max_ps(a, b), a, unord(b, b))`. wasm `f32x4_max` spreads NaN,
//!   so `bitselect(b, pmax(a, b), a != a)`.
//! - `clamp01` is compare plus select, like `f32::clamp`: it keeps NaN and the sign of -0.
//! - `trunc_u` is `as u32` (NaN to 0): x86 `cvttps` gives `0x80000000` for NaN, so those lanes
//!   are masked with `cmpord`. `round_u` is `round_ties_even() as u32`; on x86 `cvtps` rounds by
//!   MXCSR, which is round-to-nearest-even unless something changes it (Rust and Node never do).
//! - No closures or `array::map` in a kernel body: a closure does not inherit
//!   `#[target_feature]`, so on x86 its intrinsics become out-of-line calls.
//! - The rare per-lane cases (gain past the table, the near-black cap, f32 codes outside [0, 1])
//!   go to `#[cold] #[inline(never)]` functions that run the scalar code for those lanes.
//! - NEON only on aarch64 (armv7 NEON flushes denormals to zero) and x86 SIMD only on x86_64,
//!   picked at runtime ([`detect`]). Never a static `+target-feature` for x86_64: it would fold
//!   `is_x86_feature_detected!` to `true` (see `.cargo/config.toml`).
//!
//! The `kernel_matches_scalar_*` tests run every kernel against the scalar code on every 16-bit
//! code in every channel, edge cases, random pixels and f32 special values; the
//! `lane_ops_match_rust_semantics_*` tests check each lane op on special values, which the sweep
//! alone does not reach (NaN never gets to `max_num` there).

// Targets with no kernel (x86, armv7, wasm32 without simd128) build only the scalar dispatch.
#![cfg_attr(
  not(any(
    target_arch = "aarch64",
    target_arch = "x86_64",
    all(target_arch = "wasm32", target_feature = "simd128")
  )),
  allow(dead_code)
)]

use std::sync::OnceLock;

use super::{
  Curve, GAIN_FRACTION_BITS, GAIN_LEN, GAIN_MIN_BITS, HdrToSrgb, MAX_LINEAR, MIN_LUMINANCE,
  PRESERVE_SATURATION, Rec2408, SRGB_LUMINANCE,
};

/// Which kernel runs `apply_u16` / `apply_f32` on this host.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum HdrKernel {
  /// `apply_u16_scalar` / `apply_f32_scalar`: the reference.
  Scalar,
  /// NEON is part of the aarch64 baseline: no runtime probe.
  #[cfg(target_arch = "aarch64")]
  Neon,
  /// 8 lanes; picked when the host reports AVX2.
  #[cfg(target_arch = "x86_64")]
  Avx2,
  /// 4 lanes; picked when the host reports SSE4.1 but not AVX2.
  #[cfg(target_arch = "x86_64")]
  Sse41,
  /// simd128 is a compile-time feature on wasm (`.cargo/config.toml` sets it for the wasi build).
  #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
  Simd128,
}

/// The kernel for this host, after the test override and the `IMAGE_JXL_HDR_SCALAR` escape hatch.
/// Cheap (the probe and the env read are cached); call it once per image, not per pixel.
#[inline]
pub(crate) fn detect() -> HdrKernel {
  #[cfg(test)]
  {
    if let Some(forced) = test_override::get() {
      return forced;
    }
  }
  if force_scalar() {
    return HdrKernel::Scalar;
  }
  detect_native()
}

#[inline]
fn detect_native() -> HdrKernel {
  #[cfg(target_arch = "aarch64")]
  {
    HdrKernel::Neon
  }
  #[cfg(target_arch = "x86_64")]
  {
    // No AVX-512 kernel: the loop is bound by the table reads, not the maths, so an AVX-512 host
    // takes the AVX2 one.
    if std::is_x86_feature_detected!("avx2") {
      HdrKernel::Avx2
    } else if std::is_x86_feature_detected!("sse4.1") {
      HdrKernel::Sse41
    } else {
      HdrKernel::Scalar
    }
  }
  #[cfg(target_arch = "wasm32")]
  {
    #[cfg(target_feature = "simd128")]
    {
      HdrKernel::Simd128
    }
    #[cfg(not(target_feature = "simd128"))]
    {
      HdrKernel::Scalar
    }
  }
  #[cfg(not(any(
    target_arch = "aarch64",
    target_arch = "x86_64",
    target_arch = "wasm32"
  )))]
  {
    HdrKernel::Scalar
  }
}

/// `true` when `IMAGE_JXL_HDR_SCALAR` is `1`/`true`/`yes`. Read once per process: forces the
/// scalar path on any host, to rule out a SIMD mismatch without a rebuild.
fn force_scalar() -> bool {
  static FORCE: OnceLock<bool> = OnceLock::new();
  *FORCE.get_or_init(|| {
    std::env::var("IMAGE_JXL_HDR_SCALAR")
      .map(|v| matches!(v.trim(), "1" | "true" | "yes"))
      .unwrap_or(false)
  })
}

/// [`HdrToSrgb::apply_u16`] on `kernel`.
pub(super) fn apply_u16(hdr: &HdrToSrgb, kernel: HdrKernel, pixels: &mut [u16], channels: usize) {
  match kernel {
    HdrKernel::Scalar => hdr.apply_u16_scalar(pixels, channels),
    // SAFETY: NEON is always present on aarch64.
    #[cfg(target_arch = "aarch64")]
    HdrKernel::Neon => unsafe { hdr.apply_u16_neon(pixels, channels) },
    // SAFETY: `detect` returns `Avx2` / `Sse41` only when the host reports the feature.
    #[cfg(target_arch = "x86_64")]
    HdrKernel::Avx2 => unsafe { hdr.apply_u16_avx2(pixels, channels) },
    #[cfg(target_arch = "x86_64")]
    HdrKernel::Sse41 => unsafe { hdr.apply_u16_sse41(pixels, channels) },
    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    HdrKernel::Simd128 => hdr.apply_u16_simd128(pixels, channels),
  }
}

/// [`HdrToSrgb::apply_f32`] on `kernel`.
pub(super) fn apply_f32(
  hdr: &HdrToSrgb,
  kernel: HdrKernel,
  pixels: &[f32],
  output: &mut [u16],
  channels: usize,
) {
  match kernel {
    HdrKernel::Scalar => hdr.apply_f32_scalar(pixels, output, channels),
    // SAFETY: NEON is always present on aarch64.
    #[cfg(target_arch = "aarch64")]
    HdrKernel::Neon => unsafe { hdr.apply_f32_neon(pixels, output, channels) },
    // SAFETY: `detect` returns `Avx2` / `Sse41` only when the host reports the feature.
    #[cfg(target_arch = "x86_64")]
    HdrKernel::Avx2 => unsafe { hdr.apply_f32_avx2(pixels, output, channels) },
    #[cfg(target_arch = "x86_64")]
    HdrKernel::Sse41 => unsafe { hdr.apply_f32_sse41(pixels, output, channels) },
    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    HdrKernel::Simd128 => hdr.apply_f32_simd128(pixels, output, channels),
  }
}

/// Test-only thread-local kernel override (thread-local so parallel tests do not race), like
/// `quantize_simd::test_override`.
#[cfg(test)]
pub(crate) mod test_override {
  use std::cell::Cell;

  use super::HdrKernel;

  thread_local! {
    static OVERRIDE: Cell<Option<HdrKernel>> = const { Cell::new(None) };
  }

  pub(crate) fn get() -> Option<HdrKernel> {
    OVERRIDE.with(|c| c.get())
  }

  /// Runs `f` with [`super::detect`] forced to `k`, restoring the prior override after.
  pub(crate) fn with<R>(k: HdrKernel, f: impl FnOnce() -> R) -> R {
    let prev = get();
    OVERRIDE.with(|c| c.set(Some(k)));
    let r = f();
    OVERRIDE.with(|c| c.set(prev));
    r
  }
}

/// Widest lane count (AVX2), the size of the scratch arrays.
const LANES_MAX: usize = 8;
/// Pixels per block: two calls of a 4-lane ISA, one of AVX2.
const BLOCK: usize = 8;

// `gain_v` shifts by a constant 16 (`ushr16`).
const _: () = assert!(GAIN_FRACTION_BITS == 16);

/// The lane operations the kernels use, each one equal to the scalar Rust operation it stands for
/// (see the module docs). `F`: f32 lanes, `M`: all-ones / all-zeros masks, `U`: u32 lanes.
///
/// # Safety
/// Every method may use the ISA's intrinsics: call them only where the ISA is available (inside
/// the `#[target_feature]` entry points, or after the runtime check).
trait Lanes: Copy {
  const N: usize;
  type F: Copy;
  type M: Copy;
  type U: Copy;
  unsafe fn splat(x: f32) -> Self::F;
  unsafe fn splat_u(x: u32) -> Self::U;
  unsafe fn load(p: *const f32) -> Self::F;
  unsafe fn store(v: Self::F, p: *mut f32);
  unsafe fn store_u(v: Self::U, p: *mut u32);
  unsafe fn store_m(v: Self::M, p: *mut u32);
  unsafe fn add(a: Self::F, b: Self::F) -> Self::F;
  unsafe fn sub(a: Self::F, b: Self::F) -> Self::F;
  unsafe fn mul(a: Self::F, b: Self::F) -> Self::F;
  unsafe fn div(a: Self::F, b: Self::F) -> Self::F;
  unsafe fn sqrt(a: Self::F) -> Self::F;
  /// Ordered comparisons (false when either side is NaN), like Rust's `<`, `<=`, ...
  unsafe fn lt(a: Self::F, b: Self::F) -> Self::M;
  unsafe fn le(a: Self::F, b: Self::F) -> Self::M;
  unsafe fn gt(a: Self::F, b: Self::F) -> Self::M;
  unsafe fn ge(a: Self::F, b: Self::F) -> Self::M;
  unsafe fn eq(a: Self::F, b: Self::F) -> Self::M;
  unsafe fn isnan(a: Self::F) -> Self::M;
  unsafe fn or(a: Self::M, b: Self::M) -> Self::M;
  unsafe fn and(a: Self::M, b: Self::M) -> Self::M;
  /// `a & !b`
  unsafe fn andnot(a: Self::M, b: Self::M) -> Self::M;
  unsafe fn any(m: Self::M) -> bool;
  /// `m ? a : b`
  unsafe fn sel(m: Self::M, a: Self::F, b: Self::F) -> Self::F;
  /// Rust `f32::max` / `f32::min`: a NaN argument is ignored.
  unsafe fn max_num(a: Self::F, b: Self::F) -> Self::F;
  unsafe fn min_num(a: Self::F, b: Self::F) -> Self::F;
  unsafe fn bits(a: Self::F) -> Self::U;
  unsafe fn usub(a: Self::U, b: Self::U) -> Self::U;
  unsafe fn ushr16(a: Self::U) -> Self::U;
  unsafe fn uand(a: Self::U, b: Self::U) -> Self::U;
  /// `a > b` for lanes below 2^31.
  unsafe fn ugt(a: Self::U, b: Self::U) -> Self::M;
  /// `m ? 0 : a`
  unsafe fn uzero(m: Self::M, a: Self::U) -> Self::U;
  /// `m ? a : 0`
  unsafe fn umask(m: Self::M, a: Self::U) -> Self::U;
  /// `as f32`, exact for values below 2^24.
  unsafe fn u2f(a: Self::U) -> Self::F;
  /// `as u32` for lanes in [0, 2^31) or NaN (to 0): truncation.
  unsafe fn trunc_u(a: Self::F) -> Self::U;
  /// `round_ties_even() as u32` for lanes in [0, 2^31); other lanes are unspecified.
  unsafe fn round_u(a: Self::F) -> Self::U;

  /// `f32::clamp(x, 0.0, 1.0)`: `if x < 0 { 0 } else if x > 1 { 1 } else { x }` (keeps NaN, -0).
  #[inline(always)]
  unsafe fn clamp01(x: Self::F) -> Self::F {
    unsafe {
      let x = Self::sel(Self::lt(x, Self::splat(0.0)), Self::splat(0.0), x);
      Self::sel(Self::gt(x, Self::splat(1.0)), Self::splat(1.0), x)
    }
  }

  /// `(table[idx][0], table[idx][1])` per lane. Every `idx` must be in bounds.
  #[inline(always)]
  unsafe fn gather_pair(table: *const [f32; 2], idx: Self::U) -> (Self::F, Self::F) {
    unsafe {
      let mut i = [0u32; LANES_MAX];
      Self::store_u(idx, i.as_mut_ptr());
      let mut a = [0f32; LANES_MAX];
      let mut b = [0f32; LANES_MAX];
      for k in 0..Self::N {
        let pair = *table.add(i[k] as usize);
        a[k] = pair[0];
        b[k] = pair[1];
      }
      (Self::load(a.as_ptr()), Self::load(b.as_ptr()))
    }
  }
}

/// `k[0] * a + k[1] * b + k[2] * c`, left to right, unfused.
#[inline(always)]
unsafe fn dot<S: Lanes>(k: [f32; 3], a: S::F, b: S::F, c: S::F) -> S::F {
  unsafe {
    S::add(
      S::add(S::mul(S::splat(k[0]), a), S::mul(S::splat(k[1]), b)),
      S::mul(S::splat(k[2]), c),
    )
  }
}

/// `gamut_map(rgb, SRGB_LUMINANCE, PRESERVE_SATURATION)`, op for op.
#[inline(always)]
unsafe fn gamut_map_v<S: Lanes>(s: [S::F; 3]) -> [S::F; 3] {
  unsafe {
    let zero = S::splat(0.0);
    let one = S::splat(1.0);
    let luminance = dot::<S>(SRGB_LUMINANCE, s[0], s[1], s[2]);
    let mut gray_mix_saturation = zero;
    let mut gray_mix_luminance = zero;
    for val in s {
      let val_minus_gray = S::sub(val, luminance);
      let inv_val_minus_gray = S::div(
        one,
        S::sel(S::eq(val_minus_gray, zero), one, val_minus_gray),
      );
      let val_over_val_minus_gray = S::mul(val, inv_val_minus_gray);
      gray_mix_saturation = S::sel(
        S::lt(val_minus_gray, zero),
        S::max_num(gray_mix_saturation, val_over_val_minus_gray),
        gray_mix_saturation,
      );
      gray_mix_luminance = S::max_num(
        gray_mix_luminance,
        S::sel(
          S::le(val_minus_gray, zero),
          gray_mix_saturation,
          S::sub(val_over_val_minus_gray, inv_val_minus_gray),
        ),
      );
    }
    let gray_mix = S::clamp01(S::add(
      S::mul(
        S::splat(PRESERVE_SATURATION),
        S::sub(gray_mix_saturation, gray_mix_luminance),
      ),
      gray_mix_luminance,
    ));
    let mut out = s;
    for c in 0..3 {
      out[c] = S::add(s[c], S::mul(gray_mix, S::sub(luminance, s[c])));
    }
    let normalizer = S::div(
      one,
      S::max_num(S::max_num(S::max_num(out[0], out[1]), out[2]), one),
    );
    for value in &mut out {
      *value = S::mul(*value, normalizer);
    }
    out
  }
}

// Index loops over several parallel lane arrays read better than zipped iterators here.
#[allow(clippy::needless_range_loop)]
impl HdrToSrgb {
  /// `gain_of` for `S::N` lanes.
  #[inline(always)]
  unsafe fn gain_v<S: Lanes>(&self, y: S::F) -> S::F {
    unsafe {
      let low = S::or(
        S::isnan(y),
        S::le(y, S::splat(f32::from_bits(GAIN_MIN_BITS))),
      );
      let offset = S::usub(S::bits(y), S::splat_u(GAIN_MIN_BITS));
      let i = S::ushr16(offset);
      // `i + 1 >= GAIN_LEN`: past the table. Low lanes hold garbage in `i`, so leave them out.
      let high = S::andnot(S::ugt(i, S::splat_u(GAIN_LEN as u32 - 2)), low);
      // Both kinds read entry 0, which is in bounds, and are replaced below.
      let i = S::uzero(S::or(low, high), i);
      let fraction = S::mul(
        S::u2f(S::uand(offset, S::splat_u((1 << GAIN_FRACTION_BITS) - 1))),
        S::splat(1.0 / (1u32 << GAIN_FRACTION_BITS) as f32),
      );
      let (g0, d) = S::gather_pair(self.gain2.as_ptr(), i);
      let mut gain = S::add(g0, S::mul(d, fraction));
      gain = S::sel(low, S::splat(self.gain[0]), gain);
      if S::any(high) {
        let mut out = [0f32; LANES_MAX];
        let mut ys = [0f32; LANES_MAX];
        let mut mask = [0u32; LANES_MAX];
        S::store(gain, out.as_mut_ptr());
        S::store(y, ys.as_mut_ptr());
        S::store_m(high, mask.as_mut_ptr());
        self.gain_exact_lanes(&ys, &mask, &mut out, S::N);
        gain = S::load(out.as_ptr());
      }
      gain
    }
  }

  /// `rgb_to_srgb` for `S::N` lanes, up to the index into the sRGB encode table.
  #[inline(always)]
  unsafe fn core_v<S: Lanes>(&self, m: &[[f32; 3]; 3], r: S::F, g: S::F, b: S::F) -> [S::U; 3] {
    unsafe {
      let y = dot::<S>(self.lum, r, g, b);
      let (mut r, mut g, mut b) = (r, g, b);
      match &self.curve {
        Curve::None => {}
        Curve::Gain(_) | Curve::ToneMap(_) => {
          let gain = self.gain_v::<S>(y);
          r = S::mul(r, gain);
          g = S::mul(g, gain);
          b = S::mul(b, gain);
          if let Curve::ToneMap(tone_map) = &self.curve {
            // `y * source_peak <= MIN_LUMINANCE`: every channel becomes `cap(y)`, which is
            // `cap0` for every `y <= 0`. Only `0 < y * source_peak <= 1e-6` needs the exact cap.
            let cap = S::le(
              S::mul(y, S::splat(tone_map.source_peak)),
              S::splat(MIN_LUMINANCE),
            );
            let black = S::le(y, S::splat(0.0));
            let cap0 = S::splat(self.cap0);
            r = S::sel(black, cap0, r);
            g = S::sel(black, cap0, g);
            b = S::sel(black, cap0, b);
            let cap = S::andnot(cap, black);
            if S::any(cap) {
              let mut ys = [0f32; LANES_MAX];
              let mut mask = [0u32; LANES_MAX];
              let mut rgb = [[0f32; LANES_MAX]; 3];
              S::store(y, ys.as_mut_ptr());
              S::store_m(cap, mask.as_mut_ptr());
              S::store(r, rgb[0].as_mut_ptr());
              S::store(g, rgb[1].as_mut_ptr());
              S::store(b, rgb[2].as_mut_ptr());
              cap_lanes(tone_map, &ys, &mask, &mut rgb, S::N);
              r = S::load(rgb[0].as_ptr());
              g = S::load(rgb[1].as_ptr());
              b = S::load(rgb[2].as_ptr());
            }
          }
        }
      }
      let mut srgb = [
        dot::<S>(m[0], r, g, b),
        dot::<S>(m[1], r, g, b),
        dot::<S>(m[2], r, g, b),
      ];
      let max = S::max_num(S::max_num(srgb[0], srgb[1]), srgb[2]);
      let min = S::min_num(S::min_num(srgb[0], srgb[1]), srgb[2]);
      let out_of_gamut = S::or(S::gt(max, S::splat(1.0)), S::lt(min, S::splat(0.0)));
      if S::any(out_of_gamut) {
        let mapped = gamut_map_v::<S>(srgb);
        for c in 0..3 {
          srgb[c] = S::sel(out_of_gamut, mapped[c], srgb[c]);
        }
      }
      // `encode`: `(v.clamp(0, 1).sqrt() * 65535 + 0.5) as u16`, at most 65535.
      let mut index = [S::splat_u(0); 3];
      for c in 0..3 {
        index[c] = S::trunc_u(S::add(
          S::mul(S::sqrt(S::clamp01(srgb[c])), S::splat(65535.0)),
          S::splat(0.5),
        ));
      }
      index
    }
  }

  /// [`HdrToSrgb::apply_u16_scalar`] on `S`, eight pixels at a time.
  #[inline(always)]
  unsafe fn apply_u16_v<S: Lanes>(&self, pixels: &mut [u16], channels: usize) {
    let Some(m) = &self.to_srgb else {
      return self.apply_u16_scalar(pixels, channels);
    };
    if channels < 3 {
      return self.apply_u16_scalar(pixels, channels);
    }
    let decode = self.decode;
    let encode = self.encode;
    let mut blocks = pixels.chunks_exact_mut(channels * BLOCK);
    for block in &mut blocks {
      unsafe {
        let mut linear = [[0f32; BLOCK]; 3];
        for p in 0..BLOCK {
          for c in 0..3 {
            // SAFETY: `p * channels + c < BLOCK * channels` (`c < 3 <= channels`).
            linear[c][p] = decode[usize::from(*block.get_unchecked(p * channels + c))];
          }
        }
        let scale = S::splat(self.scale);
        let mut index = [[0u32; BLOCK]; 3];
        // Two explicit calls for 4 lanes, not a loop: the halves overlap (a loop cost about 50%).
        let out = self.core_v::<S>(
          m,
          S::mul(S::load(linear[0].as_ptr()), scale),
          S::mul(S::load(linear[1].as_ptr()), scale),
          S::mul(S::load(linear[2].as_ptr()), scale),
        );
        for c in 0..3 {
          S::store_u(out[c], index[c].as_mut_ptr());
        }
        if S::N == 4 {
          let out = self.core_v::<S>(
            m,
            S::mul(S::load(linear[0].as_ptr().add(4)), scale),
            S::mul(S::load(linear[1].as_ptr().add(4)), scale),
            S::mul(S::load(linear[2].as_ptr().add(4)), scale),
          );
          for c in 0..3 {
            S::store_u(out[c], index[c].as_mut_ptr().add(4));
          }
        }
        for p in 0..BLOCK {
          for c in 0..3 {
            // SAFETY: as above. The index is at most 65535 (see `core_v`).
            *block.get_unchecked_mut(p * channels + c) = encode[index[c][p] as u16 as usize];
          }
        }
      }
    }
    self.apply_u16_scalar(blocks.into_remainder(), channels);
  }

  /// [`HdrToSrgb::apply_f32_scalar`] on `S`, eight pixels at a time.
  #[inline(always)]
  unsafe fn apply_f32_v<S: Lanes>(&self, pixels: &[f32], output: &mut [u16], channels: usize) {
    let Some(m) = &self.to_srgb else {
      return self.apply_f32_scalar(pixels, output, channels);
    };
    if channels < 3 {
      return self.apply_f32_scalar(pixels, output, channels);
    }
    let decode = self.decode;
    let encode = self.encode;
    let blocks = pixels.len().min(output.len()) / (channels * BLOCK);
    let (head_in, tail_in) = pixels.split_at(blocks * channels * BLOCK);
    let (head_out, tail_out) = output.split_at_mut(blocks * channels * BLOCK);
    for (block, out) in head_in
      .chunks_exact(channels * BLOCK)
      .zip(head_out.chunks_exact_mut(channels * BLOCK))
    {
      unsafe {
        let mut codes = [[0f32; BLOCK]; 3];
        for p in 0..BLOCK {
          for c in 0..3 {
            // SAFETY: `p * channels + c < BLOCK * channels` (`c < 3 <= channels`).
            codes[c][p] = *block.get_unchecked(p * channels + c);
          }
        }
        // A code in [0, 1] becomes `round_ties_even(v * 65535)`; any other (or NaN) reads entry
        // 0 and is replaced by `decode_exact_lanes`.
        let mut code = [[0u32; BLOCK]; 3];
        let mut in_range = [[0u32; BLOCK]; 3];
        for c in 0..3 {
          for h in (0..BLOCK).step_by(S::N) {
            let x = S::load(codes[c].as_ptr().add(h));
            let ok = S::and(S::ge(x, S::splat(0.0)), S::le(x, S::splat(1.0)));
            let q = S::round_u(S::mul(x, S::splat(65535.0)));
            S::store_u(S::umask(ok, q), code[c].as_mut_ptr().add(h));
            S::store_m(ok, in_range[c].as_mut_ptr().add(h));
          }
        }
        let mut raw = [[0f32; BLOCK]; 3];
        let mut slow = false;
        for c in 0..3 {
          for p in 0..BLOCK {
            raw[c][p] = decode[code[c][p] as u16 as usize];
            slow |= in_range[c][p] == 0;
          }
        }
        // Scaled into a fresh array, not in place: rescaling the table reads where they were
        // stored made this front end 12-19% slower on NEON.
        let scale = S::splat(self.scale);
        let mut linear = [[0f32; BLOCK]; 3];
        for c in 0..3 {
          for h in (0..BLOCK).step_by(S::N) {
            let v = S::mul(S::load(raw[c].as_ptr().add(h)), scale);
            S::store(v, linear[c].as_mut_ptr().add(h));
          }
        }
        if slow {
          self.decode_exact_lanes(&codes, &in_range, &mut linear);
        }
        let mut index = [[0u32; BLOCK]; 3];
        let o = self.core_v::<S>(
          m,
          S::load(linear[0].as_ptr()),
          S::load(linear[1].as_ptr()),
          S::load(linear[2].as_ptr()),
        );
        for c in 0..3 {
          S::store_u(o[c], index[c].as_mut_ptr());
        }
        if S::N == 4 {
          let o = self.core_v::<S>(
            m,
            S::load(linear[0].as_ptr().add(4)),
            S::load(linear[1].as_ptr().add(4)),
            S::load(linear[2].as_ptr().add(4)),
          );
          for c in 0..3 {
            S::store_u(o[c], index[c].as_mut_ptr().add(4));
          }
        }
        for p in 0..BLOCK {
          for c in 0..3 {
            // SAFETY: as above; `out` has the length of `block`.
            *out.get_unchecked_mut(p * channels + c) = encode[index[c][p] as u16 as usize];
          }
          if channels == 4 {
            // SAFETY: `p * 4 + 3 < BLOCK * 4`.
            let alpha = *block.get_unchecked(p * 4 + 3);
            *out.get_unchecked_mut(p * 4 + 3) =
              (alpha.clamp(0.0, 1.0) * 65535.0).round_ties_even() as u16;
          }
        }
      }
    }
    self.apply_f32_scalar(tail_in, tail_out, channels);
  }

  // Cold per-lane fallbacks, out of line so they add no register pressure to the hot loop.

  /// `gain_exact` for the lanes in `mask`.
  #[cold]
  #[inline(never)]
  fn gain_exact_lanes(
    &self,
    y: &[f32; LANES_MAX],
    mask: &[u32; LANES_MAX],
    out: &mut [f32; LANES_MAX],
    n: usize,
  ) {
    for k in 0..n {
      if mask[k] != 0 {
        out[k] = self.gain_exact(y[k]);
      }
    }
  }

  /// The exact decode of `apply_f32_scalar` for codes outside [0, 1] (and NaN).
  #[cold]
  #[inline(never)]
  fn decode_exact_lanes(
    &self,
    codes: &[[f32; BLOCK]; 3],
    in_range: &[[u32; BLOCK]; 3],
    linear: &mut [[f32; BLOCK]; 3],
  ) {
    for c in 0..3 {
      for p in 0..BLOCK {
        if in_range[c][p] == 0 {
          let v = codes[c][p];
          linear[c][p] = if v.is_nan() {
            0.0
          } else {
            let exact = (self.decode_exact)(f64::from(v.abs())).min(MAX_LINEAR);
            (exact as f32).copysign(v) * self.scale
          };
        }
      }
    }
  }
}

/// `cap(y)` on every channel of the lanes in `mask` (`0 < y * source_peak <= MIN_LUMINANCE`).
#[cold]
#[inline(never)]
#[allow(clippy::needless_range_loop)]
fn cap_lanes(
  tone_map: &Rec2408,
  y: &[f32; LANES_MAX],
  mask: &[u32; LANES_MAX],
  rgb: &mut [[f32; LANES_MAX]; 3],
  n: usize,
) {
  for k in 0..n {
    if mask[k] != 0 {
      let v = tone_map.cap(y[k]);
      for channel in rgb.iter_mut() {
        channel[k] = v;
      }
    }
  }
}

/// The `#[target_feature]` entry points: one `apply_u16_*` / `apply_f32_*` pair per ISA, each the
/// generic body built for that ISA's lanes.
macro_rules! kernel_entry {
  ($(#[$cfg:meta])* $u16:ident, $f32:ident, $lanes:ty, $feature:literal) => {
    $(#[$cfg])*
    impl HdrToSrgb {
      /// # Safety
      /// The host must support the target feature.
      #[target_feature(enable = $feature)]
      unsafe fn $u16(&self, pixels: &mut [u16], channels: usize) {
        unsafe { self.apply_u16_v::<$lanes>(pixels, channels) }
      }

      /// # Safety
      /// The host must support the target feature.
      #[target_feature(enable = $feature)]
      unsafe fn $f32(&self, pixels: &[f32], output: &mut [u16], channels: usize) {
        unsafe { self.apply_f32_v::<$lanes>(pixels, output, channels) }
      }
    }
  };
}

kernel_entry!(
  #[cfg(target_arch = "aarch64")]
  apply_u16_neon,
  apply_f32_neon,
  neon::Neon,
  "neon"
);
kernel_entry!(
  #[cfg(target_arch = "x86_64")]
  apply_u16_avx2,
  apply_f32_avx2,
  x86::Avx2,
  "avx2"
);
kernel_entry!(
  #[cfg(target_arch = "x86_64")]
  apply_u16_sse41,
  apply_f32_sse41,
  x86::Sse41,
  "sse4.1"
);

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
impl HdrToSrgb {
  // simd128 is a compile-time feature on wasm: these are safe to call wherever they exist.
  fn apply_u16_simd128(&self, pixels: &mut [u16], channels: usize) {
    // SAFETY: built only with `+simd128`.
    unsafe { self.apply_u16_v::<wasm::Simd128>(pixels, channels) }
  }

  fn apply_f32_simd128(&self, pixels: &[f32], output: &mut [u16], channels: usize) {
    // SAFETY: built only with `+simd128`.
    unsafe { self.apply_f32_v::<wasm::Simd128>(pixels, output, channels) }
  }
}

#[cfg(target_arch = "aarch64")]
mod neon {
  use std::arch::aarch64::*;

  #[derive(Clone, Copy)]
  pub(super) struct Neon;

  impl super::Lanes for Neon {
    const N: usize = 4;
    type F = float32x4_t;
    type M = uint32x4_t;
    type U = uint32x4_t;
    #[inline(always)]
    unsafe fn splat(x: f32) -> Self::F {
      unsafe { vdupq_n_f32(x) }
    }
    #[inline(always)]
    unsafe fn splat_u(x: u32) -> Self::U {
      unsafe { vdupq_n_u32(x) }
    }
    #[inline(always)]
    unsafe fn load(p: *const f32) -> Self::F {
      unsafe { vld1q_f32(p) }
    }
    #[inline(always)]
    unsafe fn store(v: Self::F, p: *mut f32) {
      unsafe { vst1q_f32(p, v) }
    }
    #[inline(always)]
    unsafe fn store_u(v: Self::U, p: *mut u32) {
      unsafe { vst1q_u32(p, v) }
    }
    #[inline(always)]
    unsafe fn store_m(v: Self::M, p: *mut u32) {
      unsafe { vst1q_u32(p, v) }
    }
    #[inline(always)]
    unsafe fn add(a: Self::F, b: Self::F) -> Self::F {
      unsafe { vaddq_f32(a, b) }
    }
    #[inline(always)]
    unsafe fn sub(a: Self::F, b: Self::F) -> Self::F {
      unsafe { vsubq_f32(a, b) }
    }
    #[inline(always)]
    unsafe fn mul(a: Self::F, b: Self::F) -> Self::F {
      unsafe { vmulq_f32(a, b) }
    }
    #[inline(always)]
    unsafe fn div(a: Self::F, b: Self::F) -> Self::F {
      unsafe { vdivq_f32(a, b) }
    }
    #[inline(always)]
    unsafe fn sqrt(a: Self::F) -> Self::F {
      unsafe { vsqrtq_f32(a) }
    }
    #[inline(always)]
    unsafe fn lt(a: Self::F, b: Self::F) -> Self::M {
      unsafe { vcltq_f32(a, b) }
    }
    #[inline(always)]
    unsafe fn le(a: Self::F, b: Self::F) -> Self::M {
      unsafe { vcleq_f32(a, b) }
    }
    #[inline(always)]
    unsafe fn gt(a: Self::F, b: Self::F) -> Self::M {
      unsafe { vcgtq_f32(a, b) }
    }
    #[inline(always)]
    unsafe fn ge(a: Self::F, b: Self::F) -> Self::M {
      unsafe { vcgeq_f32(a, b) }
    }
    #[inline(always)]
    unsafe fn eq(a: Self::F, b: Self::F) -> Self::M {
      unsafe { vceqq_f32(a, b) }
    }
    #[inline(always)]
    unsafe fn isnan(a: Self::F) -> Self::M {
      unsafe { vmvnq_u32(vceqq_f32(a, a)) }
    }
    #[inline(always)]
    unsafe fn or(a: Self::M, b: Self::M) -> Self::M {
      unsafe { vorrq_u32(a, b) }
    }
    #[inline(always)]
    unsafe fn and(a: Self::M, b: Self::M) -> Self::M {
      unsafe { vandq_u32(a, b) }
    }
    #[inline(always)]
    unsafe fn andnot(a: Self::M, b: Self::M) -> Self::M {
      unsafe { vbicq_u32(a, b) }
    }
    #[inline(always)]
    unsafe fn any(m: Self::M) -> bool {
      unsafe { vmaxvq_u32(m) != 0 }
    }
    #[inline(always)]
    unsafe fn sel(m: Self::M, a: Self::F, b: Self::F) -> Self::F {
      unsafe { vbslq_f32(m, a, b) }
    }
    // FMAXNM/FMINNM are IEEE maxNum/minNum: what Rust's f32::max/min compile to on aarch64.
    #[inline(always)]
    unsafe fn max_num(a: Self::F, b: Self::F) -> Self::F {
      unsafe { vmaxnmq_f32(a, b) }
    }
    #[inline(always)]
    unsafe fn min_num(a: Self::F, b: Self::F) -> Self::F {
      unsafe { vminnmq_f32(a, b) }
    }
    #[inline(always)]
    unsafe fn bits(a: Self::F) -> Self::U {
      unsafe { vreinterpretq_u32_f32(a) }
    }
    #[inline(always)]
    unsafe fn usub(a: Self::U, b: Self::U) -> Self::U {
      unsafe { vsubq_u32(a, b) }
    }
    #[inline(always)]
    unsafe fn ushr16(a: Self::U) -> Self::U {
      unsafe { vshrq_n_u32::<16>(a) }
    }
    #[inline(always)]
    unsafe fn uand(a: Self::U, b: Self::U) -> Self::U {
      unsafe { vandq_u32(a, b) }
    }
    #[inline(always)]
    unsafe fn ugt(a: Self::U, b: Self::U) -> Self::M {
      unsafe { vcgtq_u32(a, b) }
    }
    #[inline(always)]
    unsafe fn uzero(m: Self::M, a: Self::U) -> Self::U {
      unsafe { vbicq_u32(a, m) }
    }
    #[inline(always)]
    unsafe fn umask(m: Self::M, a: Self::U) -> Self::U {
      unsafe { vandq_u32(a, m) }
    }
    #[inline(always)]
    unsafe fn u2f(a: Self::U) -> Self::F {
      unsafe { vcvtq_f32_u32(a) }
    }
    // FCVTZU: truncate, saturate, NaN -> 0.
    #[inline(always)]
    unsafe fn trunc_u(a: Self::F) -> Self::U {
      unsafe { vcvtq_u32_f32(a) }
    }
    // FCVTNU: round to nearest, ties to even.
    #[inline(always)]
    unsafe fn round_u(a: Self::F) -> Self::U {
      unsafe { vcvtnq_u32_f32(a) }
    }
  }
}

#[cfg(target_arch = "x86_64")]
mod x86 {
  use std::arch::x86_64::*;

  #[derive(Clone, Copy)]
  pub(super) struct Sse41;

  impl super::Lanes for Sse41 {
    const N: usize = 4;
    type F = __m128;
    type M = __m128;
    type U = __m128i;
    #[inline(always)]
    unsafe fn splat(x: f32) -> Self::F {
      unsafe { _mm_set1_ps(x) }
    }
    #[inline(always)]
    unsafe fn splat_u(x: u32) -> Self::U {
      unsafe { _mm_set1_epi32(x as i32) }
    }
    #[inline(always)]
    unsafe fn load(p: *const f32) -> Self::F {
      unsafe { _mm_loadu_ps(p) }
    }
    #[inline(always)]
    unsafe fn store(v: Self::F, p: *mut f32) {
      unsafe { _mm_storeu_ps(p, v) }
    }
    #[inline(always)]
    unsafe fn store_u(v: Self::U, p: *mut u32) {
      unsafe { _mm_storeu_si128(p.cast(), v) }
    }
    #[inline(always)]
    unsafe fn store_m(v: Self::M, p: *mut u32) {
      unsafe { _mm_storeu_ps(p.cast(), v) }
    }
    #[inline(always)]
    unsafe fn add(a: Self::F, b: Self::F) -> Self::F {
      unsafe { _mm_add_ps(a, b) }
    }
    #[inline(always)]
    unsafe fn sub(a: Self::F, b: Self::F) -> Self::F {
      unsafe { _mm_sub_ps(a, b) }
    }
    #[inline(always)]
    unsafe fn mul(a: Self::F, b: Self::F) -> Self::F {
      unsafe { _mm_mul_ps(a, b) }
    }
    #[inline(always)]
    unsafe fn div(a: Self::F, b: Self::F) -> Self::F {
      unsafe { _mm_div_ps(a, b) }
    }
    #[inline(always)]
    unsafe fn sqrt(a: Self::F) -> Self::F {
      unsafe { _mm_sqrt_ps(a) }
    }
    #[inline(always)]
    unsafe fn lt(a: Self::F, b: Self::F) -> Self::M {
      unsafe { _mm_cmplt_ps(a, b) }
    }
    #[inline(always)]
    unsafe fn le(a: Self::F, b: Self::F) -> Self::M {
      unsafe { _mm_cmple_ps(a, b) }
    }
    #[inline(always)]
    unsafe fn gt(a: Self::F, b: Self::F) -> Self::M {
      unsafe { _mm_cmpgt_ps(a, b) }
    }
    #[inline(always)]
    unsafe fn ge(a: Self::F, b: Self::F) -> Self::M {
      unsafe { _mm_cmpge_ps(a, b) }
    }
    #[inline(always)]
    unsafe fn eq(a: Self::F, b: Self::F) -> Self::M {
      unsafe { _mm_cmpeq_ps(a, b) }
    }
    #[inline(always)]
    unsafe fn isnan(a: Self::F) -> Self::M {
      unsafe { _mm_cmpunord_ps(a, a) }
    }
    #[inline(always)]
    unsafe fn or(a: Self::M, b: Self::M) -> Self::M {
      unsafe { _mm_or_ps(a, b) }
    }
    #[inline(always)]
    unsafe fn and(a: Self::M, b: Self::M) -> Self::M {
      unsafe { _mm_and_ps(a, b) }
    }
    #[inline(always)]
    unsafe fn andnot(a: Self::M, b: Self::M) -> Self::M {
      unsafe { _mm_andnot_ps(b, a) }
    }
    #[inline(always)]
    unsafe fn any(m: Self::M) -> bool {
      unsafe { _mm_movemask_ps(m) != 0 }
    }
    #[inline(always)]
    unsafe fn sel(m: Self::M, a: Self::F, b: Self::F) -> Self::F {
      unsafe { _mm_blendv_ps(b, a, m) }
    }
    // MAXPS/MINPS return the SECOND operand when either is NaN: put `a` back where `b` is NaN,
    // which gives maxNum/minNum (Rust's f32::max/min).
    #[inline(always)]
    unsafe fn max_num(a: Self::F, b: Self::F) -> Self::F {
      unsafe { _mm_blendv_ps(_mm_max_ps(a, b), a, _mm_cmpunord_ps(b, b)) }
    }
    #[inline(always)]
    unsafe fn min_num(a: Self::F, b: Self::F) -> Self::F {
      unsafe { _mm_blendv_ps(_mm_min_ps(a, b), a, _mm_cmpunord_ps(b, b)) }
    }
    #[inline(always)]
    unsafe fn bits(a: Self::F) -> Self::U {
      unsafe { _mm_castps_si128(a) }
    }
    #[inline(always)]
    unsafe fn usub(a: Self::U, b: Self::U) -> Self::U {
      unsafe { _mm_sub_epi32(a, b) }
    }
    #[inline(always)]
    unsafe fn ushr16(a: Self::U) -> Self::U {
      unsafe { _mm_srli_epi32::<16>(a) }
    }
    #[inline(always)]
    unsafe fn uand(a: Self::U, b: Self::U) -> Self::U {
      unsafe { _mm_and_si128(a, b) }
    }
    #[inline(always)]
    unsafe fn ugt(a: Self::U, b: Self::U) -> Self::M {
      unsafe { _mm_castsi128_ps(_mm_cmpgt_epi32(a, b)) }
    }
    #[inline(always)]
    unsafe fn uzero(m: Self::M, a: Self::U) -> Self::U {
      unsafe { _mm_andnot_si128(_mm_castps_si128(m), a) }
    }
    #[inline(always)]
    unsafe fn umask(m: Self::M, a: Self::U) -> Self::U {
      unsafe { _mm_and_si128(_mm_castps_si128(m), a) }
    }
    #[inline(always)]
    unsafe fn u2f(a: Self::U) -> Self::F {
      unsafe { _mm_cvtepi32_ps(a) }
    }
    // CVTTPS2DQ gives 0x80000000 for NaN: zero those lanes (`as` maps NaN to 0).
    #[inline(always)]
    unsafe fn trunc_u(a: Self::F) -> Self::U {
      unsafe { _mm_and_si128(_mm_cvttps_epi32(a), _mm_castps_si128(_mm_cmpord_ps(a, a))) }
    }
    // CVTPS2DQ rounds by MXCSR: round to nearest even unless something changed MXCSR (scalar
    // `round_ties_even` does not read it; Rust and Node never change it).
    #[inline(always)]
    unsafe fn round_u(a: Self::F) -> Self::U {
      unsafe { _mm_cvtps_epi32(a) }
    }
  }

  #[derive(Clone, Copy)]
  pub(super) struct Avx2;

  impl super::Lanes for Avx2 {
    const N: usize = 8;
    type F = __m256;
    type M = __m256;
    type U = __m256i;
    #[inline(always)]
    unsafe fn splat(x: f32) -> Self::F {
      unsafe { _mm256_set1_ps(x) }
    }
    #[inline(always)]
    unsafe fn splat_u(x: u32) -> Self::U {
      unsafe { _mm256_set1_epi32(x as i32) }
    }
    #[inline(always)]
    unsafe fn load(p: *const f32) -> Self::F {
      unsafe { _mm256_loadu_ps(p) }
    }
    #[inline(always)]
    unsafe fn store(v: Self::F, p: *mut f32) {
      unsafe { _mm256_storeu_ps(p, v) }
    }
    #[inline(always)]
    unsafe fn store_u(v: Self::U, p: *mut u32) {
      unsafe { _mm256_storeu_si256(p.cast(), v) }
    }
    #[inline(always)]
    unsafe fn store_m(v: Self::M, p: *mut u32) {
      unsafe { _mm256_storeu_ps(p.cast(), v) }
    }
    #[inline(always)]
    unsafe fn add(a: Self::F, b: Self::F) -> Self::F {
      unsafe { _mm256_add_ps(a, b) }
    }
    #[inline(always)]
    unsafe fn sub(a: Self::F, b: Self::F) -> Self::F {
      unsafe { _mm256_sub_ps(a, b) }
    }
    #[inline(always)]
    unsafe fn mul(a: Self::F, b: Self::F) -> Self::F {
      unsafe { _mm256_mul_ps(a, b) }
    }
    #[inline(always)]
    unsafe fn div(a: Self::F, b: Self::F) -> Self::F {
      unsafe { _mm256_div_ps(a, b) }
    }
    #[inline(always)]
    unsafe fn sqrt(a: Self::F) -> Self::F {
      unsafe { _mm256_sqrt_ps(a) }
    }
    #[inline(always)]
    unsafe fn lt(a: Self::F, b: Self::F) -> Self::M {
      unsafe { _mm256_cmp_ps::<_CMP_LT_OQ>(a, b) }
    }
    #[inline(always)]
    unsafe fn le(a: Self::F, b: Self::F) -> Self::M {
      unsafe { _mm256_cmp_ps::<_CMP_LE_OQ>(a, b) }
    }
    #[inline(always)]
    unsafe fn gt(a: Self::F, b: Self::F) -> Self::M {
      unsafe { _mm256_cmp_ps::<_CMP_GT_OQ>(a, b) }
    }
    #[inline(always)]
    unsafe fn ge(a: Self::F, b: Self::F) -> Self::M {
      unsafe { _mm256_cmp_ps::<_CMP_GE_OQ>(a, b) }
    }
    #[inline(always)]
    unsafe fn eq(a: Self::F, b: Self::F) -> Self::M {
      unsafe { _mm256_cmp_ps::<_CMP_EQ_OQ>(a, b) }
    }
    #[inline(always)]
    unsafe fn isnan(a: Self::F) -> Self::M {
      unsafe { _mm256_cmp_ps::<_CMP_UNORD_Q>(a, a) }
    }
    #[inline(always)]
    unsafe fn or(a: Self::M, b: Self::M) -> Self::M {
      unsafe { _mm256_or_ps(a, b) }
    }
    #[inline(always)]
    unsafe fn and(a: Self::M, b: Self::M) -> Self::M {
      unsafe { _mm256_and_ps(a, b) }
    }
    #[inline(always)]
    unsafe fn andnot(a: Self::M, b: Self::M) -> Self::M {
      unsafe { _mm256_andnot_ps(b, a) }
    }
    #[inline(always)]
    unsafe fn any(m: Self::M) -> bool {
      unsafe { _mm256_movemask_ps(m) != 0 }
    }
    #[inline(always)]
    unsafe fn sel(m: Self::M, a: Self::F, b: Self::F) -> Self::F {
      unsafe { _mm256_blendv_ps(b, a, m) }
    }
    // See `Sse41::max_num`.
    #[inline(always)]
    unsafe fn max_num(a: Self::F, b: Self::F) -> Self::F {
      unsafe { _mm256_blendv_ps(_mm256_max_ps(a, b), a, _mm256_cmp_ps::<_CMP_UNORD_Q>(b, b)) }
    }
    #[inline(always)]
    unsafe fn min_num(a: Self::F, b: Self::F) -> Self::F {
      unsafe { _mm256_blendv_ps(_mm256_min_ps(a, b), a, _mm256_cmp_ps::<_CMP_UNORD_Q>(b, b)) }
    }
    #[inline(always)]
    unsafe fn bits(a: Self::F) -> Self::U {
      unsafe { _mm256_castps_si256(a) }
    }
    #[inline(always)]
    unsafe fn usub(a: Self::U, b: Self::U) -> Self::U {
      unsafe { _mm256_sub_epi32(a, b) }
    }
    #[inline(always)]
    unsafe fn ushr16(a: Self::U) -> Self::U {
      unsafe { _mm256_srli_epi32::<16>(a) }
    }
    #[inline(always)]
    unsafe fn uand(a: Self::U, b: Self::U) -> Self::U {
      unsafe { _mm256_and_si256(a, b) }
    }
    #[inline(always)]
    unsafe fn ugt(a: Self::U, b: Self::U) -> Self::M {
      unsafe { _mm256_castsi256_ps(_mm256_cmpgt_epi32(a, b)) }
    }
    #[inline(always)]
    unsafe fn uzero(m: Self::M, a: Self::U) -> Self::U {
      unsafe { _mm256_andnot_si256(_mm256_castps_si256(m), a) }
    }
    #[inline(always)]
    unsafe fn umask(m: Self::M, a: Self::U) -> Self::U {
      unsafe { _mm256_and_si256(_mm256_castps_si256(m), a) }
    }
    #[inline(always)]
    unsafe fn u2f(a: Self::U) -> Self::F {
      unsafe { _mm256_cvtepi32_ps(a) }
    }
    // See `Sse41::trunc_u` and `Sse41::round_u`.
    #[inline(always)]
    unsafe fn trunc_u(a: Self::F) -> Self::U {
      unsafe {
        _mm256_and_si256(
          _mm256_cvttps_epi32(a),
          _mm256_castps_si256(_mm256_cmp_ps::<_CMP_ORD_Q>(a, a)),
        )
      }
    }
    #[inline(always)]
    unsafe fn round_u(a: Self::F) -> Self::U {
      unsafe { _mm256_cvtps_epi32(a) }
    }
  }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
mod wasm {
  use std::arch::wasm32::*;

  #[derive(Clone, Copy)]
  pub(super) struct Simd128;

  impl super::Lanes for Simd128 {
    const N: usize = 4;
    type F = v128;
    type M = v128;
    type U = v128;
    #[inline(always)]
    unsafe fn splat(x: f32) -> Self::F {
      f32x4_splat(x)
    }
    #[inline(always)]
    unsafe fn splat_u(x: u32) -> Self::U {
      u32x4_splat(x)
    }
    #[inline(always)]
    unsafe fn load(p: *const f32) -> Self::F {
      unsafe { v128_load(p.cast()) }
    }
    #[inline(always)]
    unsafe fn store(v: Self::F, p: *mut f32) {
      unsafe { v128_store(p.cast(), v) }
    }
    #[inline(always)]
    unsafe fn store_u(v: Self::U, p: *mut u32) {
      unsafe { v128_store(p.cast(), v) }
    }
    #[inline(always)]
    unsafe fn store_m(v: Self::M, p: *mut u32) {
      unsafe { v128_store(p.cast(), v) }
    }
    #[inline(always)]
    unsafe fn add(a: Self::F, b: Self::F) -> Self::F {
      f32x4_add(a, b)
    }
    #[inline(always)]
    unsafe fn sub(a: Self::F, b: Self::F) -> Self::F {
      f32x4_sub(a, b)
    }
    #[inline(always)]
    unsafe fn mul(a: Self::F, b: Self::F) -> Self::F {
      f32x4_mul(a, b)
    }
    #[inline(always)]
    unsafe fn div(a: Self::F, b: Self::F) -> Self::F {
      f32x4_div(a, b)
    }
    #[inline(always)]
    unsafe fn sqrt(a: Self::F) -> Self::F {
      f32x4_sqrt(a)
    }
    #[inline(always)]
    unsafe fn lt(a: Self::F, b: Self::F) -> Self::M {
      f32x4_lt(a, b)
    }
    #[inline(always)]
    unsafe fn le(a: Self::F, b: Self::F) -> Self::M {
      f32x4_le(a, b)
    }
    #[inline(always)]
    unsafe fn gt(a: Self::F, b: Self::F) -> Self::M {
      f32x4_gt(a, b)
    }
    #[inline(always)]
    unsafe fn ge(a: Self::F, b: Self::F) -> Self::M {
      f32x4_ge(a, b)
    }
    #[inline(always)]
    unsafe fn eq(a: Self::F, b: Self::F) -> Self::M {
      f32x4_eq(a, b)
    }
    #[inline(always)]
    unsafe fn isnan(a: Self::F) -> Self::M {
      f32x4_ne(a, a)
    }
    #[inline(always)]
    unsafe fn or(a: Self::M, b: Self::M) -> Self::M {
      v128_or(a, b)
    }
    #[inline(always)]
    unsafe fn and(a: Self::M, b: Self::M) -> Self::M {
      v128_and(a, b)
    }
    #[inline(always)]
    unsafe fn andnot(a: Self::M, b: Self::M) -> Self::M {
      v128_andnot(a, b)
    }
    #[inline(always)]
    unsafe fn any(m: Self::M) -> bool {
      v128_any_true(m)
    }
    #[inline(always)]
    unsafe fn sel(m: Self::M, a: Self::F, b: Self::F) -> Self::F {
      v128_bitselect(a, b, m)
    }
    // f32x4_max/min spread NaN (unlike Rust's f32::max/min). pmax(a, b) is `a < b ? b : a`: right
    // when only `b` is NaN; where `a` is NaN take `b`.
    #[inline(always)]
    unsafe fn max_num(a: Self::F, b: Self::F) -> Self::F {
      v128_bitselect(b, f32x4_pmax(a, b), f32x4_ne(a, a))
    }
    #[inline(always)]
    unsafe fn min_num(a: Self::F, b: Self::F) -> Self::F {
      v128_bitselect(b, f32x4_pmin(a, b), f32x4_ne(a, a))
    }
    #[inline(always)]
    unsafe fn bits(a: Self::F) -> Self::U {
      a
    }
    #[inline(always)]
    unsafe fn usub(a: Self::U, b: Self::U) -> Self::U {
      i32x4_sub(a, b)
    }
    #[inline(always)]
    unsafe fn ushr16(a: Self::U) -> Self::U {
      u32x4_shr(a, 16)
    }
    #[inline(always)]
    unsafe fn uand(a: Self::U, b: Self::U) -> Self::U {
      v128_and(a, b)
    }
    #[inline(always)]
    unsafe fn ugt(a: Self::U, b: Self::U) -> Self::M {
      u32x4_gt(a, b)
    }
    #[inline(always)]
    unsafe fn uzero(m: Self::M, a: Self::U) -> Self::U {
      v128_andnot(a, m)
    }
    #[inline(always)]
    unsafe fn umask(m: Self::M, a: Self::U) -> Self::U {
      v128_and(a, m)
    }
    #[inline(always)]
    unsafe fn u2f(a: Self::U) -> Self::F {
      f32x4_convert_u32x4(a)
    }
    // trunc_sat: truncate, saturate, NaN -> 0.
    #[inline(always)]
    unsafe fn trunc_u(a: Self::F) -> Self::U {
      u32x4_trunc_sat_f32x4(a)
    }
    // `nearest` rounds ties to even.
    #[inline(always)]
    unsafe fn round_u(a: Self::F) -> Self::U {
      u32x4_trunc_sat_f32x4(f32x4_nearest(a))
    }
  }
}

#[cfg(test)]
mod tests {
  use jxl::api::{
    JxlColorEncoding, JxlColorProfile, JxlPrimaries, JxlTransferFunction, JxlWhitePoint,
    RenderingIntent,
  };

  use super::*;

  /// A small deterministic LCG, so the sweeps need no `rand`.
  struct Lcg(u64);

  impl Lcg {
    fn next_u32(&mut self) -> u32 {
      self.0 = self
        .0
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
      (self.0 >> 32) as u32
    }

    /// In [0, 1).
    fn unit(&mut self) -> f32 {
      self.next_u32() as f32 / 4294967296.0
    }
  }

  fn rgb(
    white_point: JxlWhitePoint,
    primaries: JxlPrimaries,
    transfer_function: JxlTransferFunction,
  ) -> JxlColorProfile {
    JxlColorProfile::Simple(JxlColorEncoding::RgbColorSpace {
      white_point,
      primaries,
      transfer_function,
      rendering_intent: RenderingIntent::Relative,
    })
  }

  fn gray(transfer_function: JxlTransferFunction) -> JxlColorProfile {
    JxlColorProfile::Simple(JxlColorEncoding::GrayscaleColorSpace {
      white_point: JxlWhitePoint::D65,
      transfer_function,
      rendering_intent: RenderingIntent::Relative,
    })
  }

  /// Every curve and matrix shape: tone mapped PQ (10000, 1000, 4000 XYB), PQ without tone mapping
  /// (203), a NaN target, P3 with the DCI white point (Bradford), primaries with `y = 0`, HLG with
  /// both OOTFs, with the XYB skip (250), with a gain above 1 (80) and at 4000, gray PQ and HLG.
  fn configs() -> Vec<(&'static str, HdrToSrgb)> {
    use JxlTransferFunction::{HLG, PQ};
    let bt2100 = |tf| rgb(JxlWhitePoint::D65, JxlPrimaries::BT2100, tf);
    let zero_y = JxlPrimaries::Chromaticities {
      rx: 0.7347,
      ry: 0.2653,
      gx: 0.1596,
      gy: 0.8404,
      bx: 0.0366,
      by: 0.0,
    };
    let configs = [
      ("pq10000", bt2100(PQ), 10000.0, false),
      ("pq1000", bt2100(PQ), 1000.0, false),
      ("pq4000-xyb", bt2100(PQ), 4000.0, true),
      ("pq203", bt2100(PQ), 203.0, false),
      ("pq-nan-target", bt2100(PQ), f32::NAN, false),
      (
        "pq1000-p3-dci",
        rgb(JxlWhitePoint::DCI, JxlPrimaries::P3, PQ),
        1000.0,
        false,
      ),
      (
        "pq1000-zero-y",
        rgb(JxlWhitePoint::D65, zero_y, PQ),
        1000.0,
        false,
      ),
      ("hlg1000", bt2100(HLG), 1000.0, false),
      ("hlg250-xyb", bt2100(HLG), 250.0, true),
      ("hlg80", bt2100(HLG), 80.0, false),
      ("hlg4000", bt2100(HLG), 4000.0, false),
      ("gray-pq1000", gray(PQ), 1000.0, false),
      ("gray-hlg1000", gray(HLG), 1000.0, false),
    ];
    configs
      .into_iter()
      .map(|(name, profile, target, xyb)| {
        let hdr = HdrToSrgb::new(&profile, target, xyb).unwrap_or_else(|| panic!("{name}"));
        (name, hdr)
      })
      .collect()
  }

  /// 16-bit inputs: every code in every channel position, random pixels, all combinations of edge
  /// codes, and a ragged tail (not a whole block, and not a whole pixel).
  fn u16_inputs() -> Vec<u16> {
    let mut rng = Lcg(7);
    let mut input = Vec::new();
    for c in 0..=65535u32 {
      input.extend([
        c as u16,
        (65535 - c) as u16,
        (c.wrapping_mul(40503) & 0xffff) as u16,
      ]);
    }
    let random = if cfg!(debug_assertions) {
      20_000
    } else {
      400_000
    };
    for _ in 0..random {
      input.extend([
        rng.next_u32() as u16,
        rng.next_u32() as u16,
        rng.next_u32() as u16,
      ]);
    }
    let edges = [0u16, 1, 2, 255, 256, 32767, 32768, 65534, 65535];
    for a in edges {
      for b in edges {
        for c in edges {
          input.extend([a, b, c]);
        }
      }
    }
    input.extend([1, 2, 3, 4, 5]);
    input
  }

  /// f32 inputs: all combinations of special values (NaN, infinities, signed zeros, denormals,
  /// tiny, huge, above 1, the PQ pole at about 1.99, the `MAX_LINEAR` caps at PQ 1.98 and HLG 2.65,
  /// rounding ties), codes on and between the 16-bit steps, random codes in [-0.3, 1.3], tiny
  /// codes near the gain table's floor and the tone mapper's cap, and a ragged tail.
  fn f32_inputs() -> Vec<f32> {
    let specials = [
      0.0f32,
      -0.0,
      1.0,
      -1.0,
      f32::NAN,
      -f32::NAN,
      f32::INFINITY,
      f32::NEG_INFINITY,
      f32::MAX,
      -f32::MAX,
      f32::MIN_POSITIVE,
      -f32::MIN_POSITIVE,
      1e-40,
      -1e-40,
      1e-19,
      -1e-19,
      1e-30,
      3e-20,
      0.5,
      1.000_000_1,
      0.999_999_94,
      -1e-7,
      1.98,
      2.0,
      2.65,
      16.0,
      1e5,
      0.5 / 65535.0,
      1.5 / 65535.0,
      7.3e-7,
    ];
    let mut rng = Lcg(11);
    let mut input = Vec::new();
    for a in specials {
      for b in specials {
        for c in specials {
          input.extend([a, b, c]);
        }
      }
    }
    for c in (0..=65535u32).step_by(3) {
      input.extend([
        c as f32 / 65535.0,
        (65535 - c) as f32 / 65535.0,
        (c as f32 + 0.5) / 65535.0,
      ]);
    }
    let random = if cfg!(debug_assertions) {
      20_000
    } else {
      400_000
    };
    for _ in 0..random {
      input.extend([
        rng.unit() * 1.6 - 0.3,
        rng.unit() * 1.6 - 0.3,
        rng.unit() * 1.6 - 0.3,
      ]);
    }
    for _ in 0..random / 4 {
      let t = rng.unit();
      input.extend([t * 1e-18, -t * 1e-19, t * 3e-20]);
    }
    input.extend([0.1, 0.2, 0.3, 0.4, 0.5]);
    input
  }

  type U16Kernel<'a> = &'a dyn Fn(&HdrToSrgb, &mut [u16], usize);
  type F32Kernel<'a> = &'a dyn Fn(&HdrToSrgb, &[f32], &mut [u16], usize);

  /// Shared sweep: `apply_u16` and `apply_f32` through the kernel give exactly the scalar output,
  /// for every config and channel count.
  fn assert_kernel_matches_scalar(name: &str, apply_u16: U16Kernel, apply_f32: F32Kernel) {
    let u16_in = u16_inputs();
    let f32_in = f32_inputs();
    for (config, hdr) in configs() {
      let channel_counts: &[usize] = if hdr.to_srgb.is_some() {
        &[3, 4]
      } else {
        &[1, 2]
      };
      for &channels in channel_counts {
        let mut want = u16_in.clone();
        hdr.apply_u16_scalar(&mut want, channels);
        let mut got = u16_in.clone();
        apply_u16(&hdr, &mut got, channels);
        if let Some(i) = (0..got.len()).find(|&i| got[i] != want[i]) {
          let p = i / channels * channels;
          panic!(
            "{name} u16 {config} x{channels}: sample {i}: input {:?} got {:?} want {:?}",
            &u16_in[p..p + channels],
            &got[p..p + channels],
            &want[p..p + channels]
          );
        }

        let mut want = vec![0u16; f32_in.len()];
        hdr.apply_f32_scalar(&f32_in, &mut want, channels);
        let mut got = vec![0u16; f32_in.len()];
        apply_f32(&hdr, &f32_in, &mut got, channels);
        if let Some(i) = (0..got.len()).find(|&i| got[i] != want[i]) {
          let p = i / channels * channels;
          panic!(
            "{name} f32 {config} x{channels}: sample {i}: input {:?} got {:?} want {:?}",
            &f32_in[p..p + channels],
            &got[p..p + channels],
            &want[p..p + channels]
          );
        }
      }
    }
  }

  #[cfg(target_arch = "aarch64")]
  #[test]
  fn kernel_matches_scalar_neon() {
    assert_kernel_matches_scalar(
      "neon",
      &|hdr, pixels, channels| unsafe { hdr.apply_u16_neon(pixels, channels) },
      &|hdr, pixels, output, channels| unsafe { hdr.apply_f32_neon(pixels, output, channels) },
    );
  }

  #[cfg(target_arch = "x86_64")]
  #[test]
  fn kernel_matches_scalar_avx2() {
    if !std::is_x86_feature_detected!("avx2") {
      return; // host (or Rosetta) without AVX2: skip; real-AVX2 x86 (CI / qemu) covers it
    }
    assert_kernel_matches_scalar(
      "avx2",
      &|hdr, pixels, channels| unsafe { hdr.apply_u16_avx2(pixels, channels) },
      &|hdr, pixels, output, channels| unsafe { hdr.apply_f32_avx2(pixels, output, channels) },
    );
  }

  #[cfg(target_arch = "x86_64")]
  #[test]
  fn kernel_matches_scalar_sse41() {
    if !std::is_x86_feature_detected!("sse4.1") {
      return; // host without SSE4.1: skip; x86 CI covers it
    }
    assert_kernel_matches_scalar(
      "sse41",
      &|hdr, pixels, channels| unsafe { hdr.apply_u16_sse41(pixels, channels) },
      &|hdr, pixels, output, channels| unsafe { hdr.apply_f32_sse41(pixels, output, channels) },
    );
  }

  #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
  #[test]
  fn kernel_matches_scalar_simd128() {
    assert_kernel_matches_scalar(
      "simd128",
      &|hdr, pixels, channels| hdr.apply_u16_simd128(pixels, channels),
      &|hdr, pixels, output, channels| hdr.apply_f32_simd128(pixels, output, channels),
    );
  }

  /// The public entry points, on whatever [`detect`] picks here (plus the gray table).
  #[test]
  fn kernel_matches_scalar_dispatch() {
    assert_kernel_matches_scalar(
      &format!("dispatch ({:?})", detect()),
      &|hdr, pixels, channels| hdr.apply_u16(pixels, channels),
      &|hdr, pixels, output, channels| hdr.apply_f32(pixels, output, channels),
    );
    // A forced scalar kernel is the scalar code.
    test_override::with(HdrKernel::Scalar, || {
      assert_eq!(detect(), HdrKernel::Scalar);
    });
  }

  /// Rust's result for `want`, compared with a lane's `got`: the same bits, except that any NaN
  /// matches any NaN and, for `max`/`min` only, `+0` matches `-0` (Rust leaves the sign of
  /// `max(-0.0, 0.0)` open, and a zero's sign does not reach the output: `sqrt(±0) * 65535 + 0.5`
  /// truncates to 0).
  fn same(got: f32, want: f32, zero_sign_free: bool) -> bool {
    got.to_bits() == want.to_bits()
      || (got.is_nan() && want.is_nan())
      || (zero_sign_free && got == 0.0 && want == 0.0)
  }

  const LANE_SPECIALS: [f32; 22] = [
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.5,
    1.5,
    2.5,
    65534.5,
    65535.5,
    0.499_999_97,
    1.000_000_1,
    1e-40,
    -1e-40,
    f32::MIN_POSITIVE,
    1e9,
    -1e9,
    f32::MAX,
    f32::MIN,
    f32::INFINITY,
    f32::NEG_INFINITY,
    f32::NAN,
    -f32::NAN,
  ];

  // Plain functions, not closures: a closure would not inherit the caller's `#[target_feature]`.
  #[inline(always)]
  unsafe fn lanes<S: Lanes>(v: S::F) -> [f32; LANES_MAX] {
    let mut out = [0f32; LANES_MAX];
    unsafe { S::store(v, out.as_mut_ptr()) };
    out
  }

  #[inline(always)]
  unsafe fn mask<S: Lanes>(m: S::M) -> [u32; LANES_MAX] {
    let mut out = [0u32; LANES_MAX];
    unsafe { S::store_m(m, out.as_mut_ptr()) };
    out
  }

  #[inline(always)]
  unsafe fn ints<S: Lanes>(u: S::U) -> [u32; LANES_MAX] {
    let mut out = [0u32; LANES_MAX];
    unsafe { S::store_u(u, out.as_mut_ptr()) };
    out
  }

  /// Each lane op against the Rust operation it stands for, on special values.
  #[inline(always)]
  unsafe fn check_lane_ops<S: Lanes>(isa: &str) {
    unsafe {
      for a in LANE_SPECIALS {
        let va = S::splat(a);
        for b in LANE_SPECIALS {
          let vb = S::splat(b);
          let ops: [(&str, f32, f32, bool); 3] = [
            ("max_num", lanes::<S>(S::max_num(va, vb))[0], a.max(b), true),
            ("min_num", lanes::<S>(S::min_num(va, vb))[0], a.min(b), true),
            (
              "sel(lt)",
              lanes::<S>(S::sel(S::lt(va, vb), va, vb))[0],
              if a < b { a } else { b },
              false,
            ),
          ];
          for (op, got, want, zero_sign_free) in ops {
            assert!(
              same(got, want, zero_sign_free),
              "{isa} {op}({a:?}, {b:?}): {got:?} != {want:?}"
            );
          }
          let compares: [(&str, S::M, bool); 6] = [
            ("lt", S::lt(va, vb), a < b),
            ("le", S::le(va, vb), a <= b),
            ("gt", S::gt(va, vb), a > b),
            ("ge", S::ge(va, vb), a >= b),
            ("eq", S::eq(va, vb), a == b),
            ("isnan", S::isnan(va), a.is_nan()),
          ];
          for (op, m, want) in compares {
            let got = mask::<S>(m);
            assert!(
              got[..S::N]
                .iter()
                .all(|&g| g == if want { u32::MAX } else { 0 }),
              "{isa} {op}({a:?}, {b:?}): {got:?}"
            );
            assert_eq!(S::any(m), want, "{isa} any({op}({a:?}, {b:?}))");
          }
        }
        let got = lanes::<S>(S::clamp01(va))[0];
        assert!(
          same(got, a.clamp(0.0, 1.0), false),
          "{isa} clamp01({a:?}): {got:?}"
        );
        // `trunc_u` / `round_u` are only used on [0, 2^31) (and NaN for `trunc_u`).
        if a.is_nan() || (0.0..2147483648.0).contains(&a) {
          let got = ints::<S>(S::trunc_u(va))[0];
          assert_eq!(got, a as u32, "{isa} trunc_u({a:?})");
        }
        if (0.0..2147483648.0).contains(&a) {
          let got = ints::<S>(S::round_u(va))[0];
          assert_eq!(got, a.round_ties_even() as u32, "{isa} round_u({a:?})");
        }
      }
      // Lanes stay apart: a different value in each lane.
      let mut values = [0f32; LANES_MAX];
      for (k, value) in values.iter_mut().enumerate() {
        *value = k as f32 + 0.5;
      }
      let v = S::load(values.as_ptr());
      let got = ints::<S>(S::round_u(v));
      for (k, &got) in got.iter().enumerate().take(S::N) {
        assert_eq!(
          got,
          (k as f32 + 0.5).round_ties_even() as u32,
          "{isa} lane {k}"
        );
      }
    }
  }

  #[cfg(target_arch = "aarch64")]
  #[test]
  fn lane_ops_match_rust_semantics_neon() {
    unsafe { check_lane_ops::<neon::Neon>("neon") }
  }

  #[cfg(target_arch = "x86_64")]
  #[target_feature(enable = "avx2")]
  unsafe fn check_lane_ops_avx2() {
    unsafe { check_lane_ops::<x86::Avx2>("avx2") }
  }

  #[cfg(target_arch = "x86_64")]
  #[target_feature(enable = "sse4.1")]
  unsafe fn check_lane_ops_sse41() {
    unsafe { check_lane_ops::<x86::Sse41>("sse41") }
  }

  #[cfg(target_arch = "x86_64")]
  #[test]
  fn lane_ops_match_rust_semantics_avx2() {
    if !std::is_x86_feature_detected!("avx2") {
      return; // see `kernel_matches_scalar_avx2`
    }
    unsafe { check_lane_ops_avx2() }
  }

  #[cfg(target_arch = "x86_64")]
  #[test]
  fn lane_ops_match_rust_semantics_sse41() {
    if !std::is_x86_feature_detected!("sse4.1") {
      return;
    }
    unsafe { check_lane_ops_sse41() }
  }

  #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
  #[test]
  fn lane_ops_match_rust_semantics_simd128() {
    unsafe { check_lane_ops::<wasm::Simd128>("simd128") }
  }

  /// The kernels give `cap0` to every pixel with `y <= 0` instead of calling `cap(y)`.
  #[test]
  fn cap_is_constant_for_nonpositive_y() {
    for target in [251.0f32, 1000.0, 4000.0, 10000.0] {
      let profile = rgb(
        JxlWhitePoint::D65,
        JxlPrimaries::BT2100,
        JxlTransferFunction::PQ,
      );
      let hdr = HdrToSrgb::new(&profile, target, false).unwrap();
      let Curve::ToneMap(tone_map) = &hdr.curve else {
        panic!("{target}: not tone mapped");
      };
      for y in [
        0.0f32,
        -0.0,
        -1e-45,
        -f32::MIN_POSITIVE / 2.0,
        -1e-30,
        -1.0,
        f32::MIN,
        f32::NEG_INFINITY,
      ] {
        assert_eq!(
          tone_map.cap(y).to_bits(),
          hdr.cap0.to_bits(),
          "{target}: cap({y:?})"
        );
      }
    }
  }

  /// The gray table (gray u16 images of at least 65536 pixels) is the scalar result for all codes.
  #[test]
  fn gray_table_matches_scalar() {
    for tf in [JxlTransferFunction::PQ, JxlTransferFunction::HLG] {
      for target in [80.0f32, 203.0, 1000.0, 10000.0, f32::NAN] {
        let hdr = HdrToSrgb::new(&gray(tf.clone()), target, false).unwrap();
        for channels in [1usize, 2] {
          let input: Vec<u16> = (0..=65535u16)
            .flat_map(|code| [code, 65535 - code].into_iter().take(channels))
            .collect();
          let mut want = input.clone();
          hdr.apply_u16_scalar(&mut want, channels);
          let mut got = input;
          assert!(got.len() / channels >= super::super::GRAY_TABLE_MIN_PIXELS);
          hdr.apply_u16(&mut got, channels);
          assert_eq!(got, want, "{tf:?} {target} x{channels}");
        }
      }
    }
  }

  /// `RUSTFLAGS='-C link-arg=-undefined -C link-arg=dynamic_lookup' cargo test --release
  /// -p napi_rs_image --lib hdr_kernel_bench -- --ignored --nocapture`: ns/px of the scalar code
  /// and of each kernel this host has, on a synthetic 3840x2160 image (a smooth field plus noise;
  /// for f32, about 25% small negative codes, like an HLG XYB file).
  #[test]
  #[ignore]
  fn hdr_kernel_bench() {
    const W: usize = 3840;
    const H: usize = 2160;
    let mut rng = Lcg(3);
    let mut codes = Vec::with_capacity(W * H * 3);
    for y in 0..H {
      for x in 0..W {
        let (fx, fy) = (x as f32 / W as f32, y as f32 / H as f32);
        for c in 0..3 {
          let smooth =
            0.45 + 0.3 * (fx * 6.0 + c as f32).sin() * (fy * 4.0 - c as f32 * 0.5).cos() + 0.1 * fx;
          codes.push((smooth + (rng.unit() - 0.5) * 0.04).clamp(0.0, 1.0));
        }
      }
    }
    let u16_rgb: Vec<u16> = codes
      .iter()
      .map(|&v| (v * 65535.0).round() as u16)
      .collect();
    let u16_rgba: Vec<u16> = u16_rgb
      .as_chunks::<3>()
      .0
      .iter()
      .flat_map(|&[r, g, b]| [r, g, b, 40000])
      .collect();
    let f32_rgb: Vec<f32> = codes
      .iter()
      .map(|&v| {
        if rng.unit() < 0.25 {
          -rng.unit() * 0.002
        } else {
          v
        }
      })
      .collect();
    let gray_codes: Vec<u16> = u16_rgb.iter().step_by(3).copied().collect();

    #[allow(unused_mut)] // Only the scalar kernel on targets without SIMD.
    let mut kernels: Vec<HdrKernel> = vec![HdrKernel::Scalar];
    #[cfg(target_arch = "aarch64")]
    kernels.push(HdrKernel::Neon);
    #[cfg(target_arch = "x86_64")]
    {
      if std::is_x86_feature_detected!("avx2") {
        kernels.push(HdrKernel::Avx2);
      }
      if std::is_x86_feature_detected!("sse4.1") {
        kernels.push(HdrKernel::Sse41);
      }
    }
    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    kernels.push(HdrKernel::Simd128);

    let pixels = (W * H) as f64;
    let best_of = |run: &mut dyn FnMut()| {
      let mut best = f64::MAX;
      for _ in 0..7 {
        let start = std::time::Instant::now();
        run();
        best = best.min(start.elapsed().as_secs_f64());
      }
      best / pixels * 1e9
    };
    let bt2100 = |tf| rgb(JxlWhitePoint::D65, JxlPrimaries::BT2100, tf);
    for (name, tf, target, xyb) in [
      ("PQ10000", JxlTransferFunction::PQ, 10000.0f32, false),
      ("PQ1000", JxlTransferFunction::PQ, 1000.0, false),
      ("HLG1000", JxlTransferFunction::HLG, 1000.0, false),
      ("PQ1000-xyb", JxlTransferFunction::PQ, 1000.0, true),
      ("HLG1000-xyb", JxlTransferFunction::HLG, 1000.0, true),
    ] {
      let hdr = HdrToSrgb::new(&bt2100(tf.clone()), target, xyb).unwrap();
      let mut line = Vec::new();
      for (label, input, channels) in [("u16 rgb", &u16_rgb, 3), ("u16 rgba", &u16_rgba, 4)] {
        if xyb {
          continue;
        }
        let mut want = input.clone();
        hdr.apply_u16_scalar(&mut want, channels);
        for &kernel in &kernels {
          let mut buf = input.clone();
          let ns = best_of(&mut || {
            buf.copy_from_slice(input);
            apply_u16(&hdr, kernel, &mut buf, channels);
          });
          assert_eq!(buf, want, "{name} {label} {kernel:?}");
          line.push(format!("{label} {kernel:?} {ns:.2}"));
        }
      }
      if xyb {
        let mut want = vec![0u16; f32_rgb.len()];
        hdr.apply_f32_scalar(&f32_rgb, &mut want, 3);
        for &kernel in &kernels {
          let mut out = vec![0u16; f32_rgb.len()];
          let ns = best_of(&mut || apply_f32(&hdr, kernel, &f32_rgb, &mut out, 3));
          assert_eq!(out, want, "{name} f32 {kernel:?}");
          line.push(format!("f32 rgb {kernel:?} {ns:.2}"));
        }
      }
      println!("{name:12} ns/px: {}", line.join(", "));
    }
    for tf in [JxlTransferFunction::PQ, JxlTransferFunction::HLG] {
      let hdr = HdrToSrgb::new(&gray(tf.clone()), 1000.0, false).unwrap();
      let mut want = gray_codes.clone();
      hdr.apply_u16_scalar(&mut want, 1);
      let mut buf = gray_codes.clone();
      let scalar = best_of(&mut || {
        buf.copy_from_slice(&gray_codes);
        hdr.apply_u16_scalar(&mut buf, 1);
      });
      let table = best_of(&mut || {
        buf.copy_from_slice(&gray_codes);
        hdr.apply_u16(&mut buf, 1);
      });
      assert_eq!(buf, want, "gray {tf:?}");
      println!("gray {tf:?} 1000 ns/px: scalar {scalar:.2}, table {table:.2}");
    }
  }
}
