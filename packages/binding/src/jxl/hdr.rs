//! PQ and HLG to SDR sRGB, the way libjxl renders them (`djxl --display_nits`).
//!
//! Used for files whose output colour space is an enum (`JxlColorProfile::Simple`) with the PQ or
//! HLG transfer function, RGB or gray. An HDR file that carries its own ICC profile still goes
//! through that profile and `moxcms` (see `super::decode_first_frame`).
//!
//! Why not the ICC profile jxl-rs writes for these enums: its A2B0 tag is a 9x9x9 8-bit Lab LUT, and
//! moxcms 0.8.1 always interpolates a Lab/XYZ-PCS table trilinearly. Next to the neutral axis that
//! pulls in coloured nodes, so a grey PQ ramp came out tinted (up to 2950/65535) and not monotonic
//! (21 drops). For gray PQ/HLG (and custom primaries) jxl-rs cannot build a usable ICC at all.
//!
//! Per pixel, on 16-bit code values, in place:
//! 1. PQ or HLG code -> linear light (65537-entry table).
//! 2. PQ: Rec. 2408 tone mapping from the intensity target down to 250 nits (only when the target
//!    is above 250 nits; otherwise 1.0 = intensity target). HLG: the OOTF for the intensity target,
//!    then the OOTF from there down to 80 nits. Both scale the pixel by a gain that depends only on
//!    its luminance, read from a 4096-entry table indexed by the f32 bits of the luminance.
//! 3. Matrix to linear sRGB (with Bradford adaptation for a non-D65 white point).
//! 4. Out-of-gamut pixels are desaturated towards grey in linear sRGB with sRGB luminances
//!    (`gamut_map`), not clipped per channel. That is what djxl does with sRGB output for an XYB
//!    file, whose XYB stage already writes linear light in the output primaries (`dec_xyb.cc`
//!    `SetColorEncoding`). Only for a non-XYB file does libjxl map in the source gamut, then hand
//!    the result to its CMS.
//! 5. Clip to [0, 1] and apply the sRGB curve (65536-entry table on sqrt-spaced input).
//!
//! Ported from libjxl (<https://github.com/libjxl/libjxl>, main at e47d2a2b, 0.13 development),
//! Copyright (c) the JPEG XL Project Authors, BSD-3-Clause (license text in `credits.md`):
//! - `lib/jxl/cms/tone_mapping.h`: `Rec2408ToneMapperBase` (:23-98), `HlgOOTF_Base` (:100-130),
//!   `GamutMapScalar` (:132-178)
//! - `lib/jxl/cms/tone_mapping-inl.h`: `HlgOOTF::FromSceneLight` (:106-111)
//! - `lib/jxl/cms/transfer_functions.h`: `TF_PQ_Base` (:90-127), `TF_HLG_Base::InvOETF` (:54-64)
//! - `lib/jxl/render_pipeline/stage_tone_mapping.cc` (:36-108): when each step applies

use std::sync::OnceLock;

use jxl::api::{JxlColorEncoding, JxlColorProfile, JxlTransferFunction, JxlWhitePoint};

type Matrix = [[f64; 3]; 3];

// PQ (SMPTE ST 2084), `TF_PQ_Base`.
const M1: f64 = 2610.0 / 16384.0;
const M2: f64 = (2523.0 / 4096.0) * 128.0;
const C1: f64 = 3424.0 / 4096.0;
const C2: f64 = (2413.0 / 4096.0) * 32.0;
const C3: f64 = (2392.0 / 4096.0) * 32.0;

// HLG (ARIB STD-B67), `TF_HLG_Base`.
const HLG_A: f64 = 0.17883277;
const HLG_B: f64 = 1.0 - 4.0 * HLG_A;
const HLG_C: f64 = 0.5599107295;

/// Peak of the SDR display PQ is tone mapped to. The same 250 nits libjxl uses for the tone curve
/// in its ICC profiles (`ToneMapPixel`), so 203 nits comes out at about 74% of sRGB white.
const TARGET_PQ_NITS: f32 = 250.0;
/// Peak of the SDR display for the HLG OOTF (libjxl's `ToneMapPixel` uses 80 too).
const TARGET_HLG_NITS: f32 = 80.0;
/// Intensity target used when the header holds no usable one. Only a guard: jxl-rs reads it as a
/// finite f16 (NaN and Inf fail the decode) and rejects values `<= 0`.
const DEFAULT_PQ_NITS: f32 = 10000.0;
const DEFAULT_HLG_NITS: f32 = 1000.0;
/// `GamutMapScalar`'s default: mostly keep luminance, a little saturation.
const PRESERVE_SATURATION: f32 = 0.1;
/// Rec. 709 / sRGB luminance of each primary (the Y row of the sRGB to XYZ matrix).
const SRGB_LUMINANCE: [f32; 3] = [0.2126, 0.7152, 0.0722];
/// `Rec2408ToneMapperBase::ToneMap`: at or below this many nits a pixel becomes neutral grey.
const MIN_LUMINANCE: f32 = 1e-6;

/// The gain table holds `1 << GAIN_STEP_BITS` entries per octave of luminance, from
/// `2^GAIN_MIN_OCTAVE` to `2^GAIN_MAX_OCTAVE`. A luminance is looked up through its f32 bit pattern
/// (exponent and the top mantissa bits), so there is no `log2` per pixel.
const GAIN_STEP_BITS: u32 = 7;
const GAIN_MIN_OCTAVE: i32 = -30;
/// PQ code 1.0 is `10000 / intensity_target` relative to the source peak, and PQ is only tone
/// mapped when the target is above 250 nits, so `y < 40 < 2^6`. HLG stays at or below 1.
const GAIN_MAX_OCTAVE: i32 = 6;
const GAIN_LEN: usize = ((GAIN_MAX_OCTAVE - GAIN_MIN_OCTAVE) as usize) << GAIN_STEP_BITS;
const GAIN_MIN_BITS: u32 = ((127 + GAIN_MIN_OCTAVE) as u32) << 23;
const GAIN_FRACTION_BITS: u32 = 23 - GAIN_STEP_BITS;

/// 16-bit code -> linear light; entry 65536 repeats entry 65535.
type DecodeTable = [f32; 65537];
/// PQ code -> nits / 10000.
static PQ_DECODE: OnceLock<Box<DecodeTable>> = OnceLock::new();
/// HLG code -> scene light in [0, 1].
static HLG_DECODE: OnceLock<Box<DecodeTable>> = OnceLock::new();
/// `sqrt(linear) * 65535` (rounded) -> 16-bit sRGB code.
static SRGB_ENCODE: OnceLock<Box<[u16; 65536]>> = OnceLock::new();

/// `TF_PQ_Base::DisplayFromEncoded` with `display_intensity_target = 1`: code -> nits.
fn pq_nits_from_encoded(e: f64) -> f64 {
  if e <= 0.0 {
    return 0.0;
  }
  let xp = e.powf(1.0 / M2);
  let num = (xp - C1).max(0.0);
  let den = C2 - C3 * xp;
  (num / den).powf(1.0 / M1) * 10000.0
}

/// `TF_PQ_Base::EncodedFromDisplay` with `display_intensity_target = 1`: nits -> code.
fn pq_encoded_from_nits(nits: f32) -> f32 {
  if nits <= 0.0 {
    return 0.0;
  }
  let xp = (f64::from(nits) * (1.0 / 10000.0)).powf(M1);
  ((C1 + xp * C2) / (1.0 + xp * C3)).powf(M2) as f32
}

/// `TF_HLG_Base::InvOETF`: code -> scene light in [0, 1].
fn hlg_scene_from_encoded(e: f64) -> f64 {
  if e <= 0.0 {
    return 0.0;
  }
  if e <= 0.5 {
    return e * e * (1.0 / 3.0);
  }
  (((e - HLG_C) / HLG_A).exp() + HLG_B) * (1.0 / 12.0)
}

fn srgb_oetf(v: f64) -> f64 {
  if v <= 0.0031308 {
    12.92 * v
  } else {
    1.055 * v.powf(1.0 / 2.4) - 0.055
  }
}

#[cold]
fn build_decode_table(decode: fn(f64) -> f64) -> Box<DecodeTable> {
  let mut table = vec![0f32; 65537];
  for (code, value) in table[..65536].iter_mut().enumerate() {
    *value = decode(code as f64 / 65535.0) as f32;
  }
  table[65536] = table[65535];
  table
    .into_boxed_slice()
    .try_into()
    .expect("vec len == 65537")
}

#[cold]
fn build_srgb_encode_table() -> Box<[u16; 65536]> {
  let mut table = vec![0u16; 65536];
  for (i, value) in table.iter_mut().enumerate() {
    let s = i as f64 / 65535.0;
    *value = (srgb_oetf(s * s) * 65535.0).round() as u16;
  }
  table
    .into_boxed_slice()
    .try_into()
    .expect("vec len == 65536")
}

fn pq_decode_table() -> &'static DecodeTable {
  PQ_DECODE.get_or_init(|| build_decode_table(|e| pq_nits_from_encoded(e) / 10000.0))
}

fn hlg_decode_table() -> &'static DecodeTable {
  HLG_DECODE.get_or_init(|| build_decode_table(hlg_scene_from_encoded))
}

fn srgb_encode_table() -> &'static [u16; 65536] {
  SRGB_ENCODE.get_or_init(build_srgb_encode_table)
}

/// `Rec2408ToneMapperBase` with a source range `[0, source_peak]` and a target range
/// `[0, target_peak]` (nits). Pixels are relative to `source_peak` going in and relative to
/// `target_peak` coming out.
struct Rec2408 {
  source_peak: f32,
  target_peak: f32,
  pq_min: f32,
  pq_range: f32,
  inv_pq_range: f32,
  min_lum: f32,
  max_lum: f32,
  ks: f32,
  inv_one_minus_ks: f32,
  normalizer: f32,
  inv_target_peak: f32,
}

impl Rec2408 {
  fn new(source_peak: f32, target_peak: f32) -> Self {
    let pq_min = pq_encoded_from_nits(0.0);
    let pq_max = pq_encoded_from_nits(source_peak);
    let pq_range = pq_max - pq_min;
    let inv_pq_range = 1.0 / pq_range;
    let min_lum = (pq_encoded_from_nits(0.0) - pq_min) * inv_pq_range;
    let max_lum = (pq_encoded_from_nits(target_peak) - pq_min) * inv_pq_range;
    let ks = 1.5 * max_lum - 0.5;
    Self {
      source_peak,
      target_peak,
      pq_min,
      pq_range,
      inv_pq_range,
      min_lum,
      max_lum,
      ks,
      inv_one_minus_ks: 1.0 / (1.0 - ks).max(1e-6),
      normalizer: source_peak / target_peak,
      inv_target_peak: 1.0 / target_peak,
    }
  }

  /// The hermite spline above the knee (`P`).
  fn spline(&self, b: f32) -> f32 {
    let t = (b - self.ks) * self.inv_one_minus_ks;
    let t2 = t * t;
    let t3 = t2 * t;
    (2.0 * t3 - 3.0 * t2 + 1.0) * self.ks
      + (t3 - 2.0 * t2 + t) * (1.0 - self.ks)
      + (-2.0 * t3 + 3.0 * t2) * self.max_lum
  }

  /// Tone-mapped luminance (nits) for a source luminance (nits).
  fn new_luminance(&self, luminance: f32) -> f32 {
    let normalized_pq =
      ((pq_encoded_from_nits(luminance) - self.pq_min) * self.inv_pq_range).min(1.0);
    let e2 = if normalized_pq < self.ks {
      normalized_pq
    } else {
      self.spline(normalized_pq)
    };
    let one_minus_e2 = 1.0 - e2;
    let one_minus_e2_2 = one_minus_e2 * one_minus_e2;
    let e3 = self.min_lum * one_minus_e2_2 * one_minus_e2_2 + e2;
    let e4 = e3 * self.pq_range + self.pq_min;
    let d4 = pq_nits_from_encoded(f64::from(e4)) as f32;
    d4.clamp(0.0, self.target_peak)
  }

  /// The factor `ToneMap` multiplies a pixel with relative luminance `y` by (when it is above
  /// [`MIN_LUMINANCE`] nits).
  fn multiplier(&self, y: f32) -> f32 {
    let luminance = self.source_peak * y;
    self.new_luminance(luminance) / luminance.max(MIN_LUMINANCE) * self.normalizer
  }

  /// `ToneMap` for a pixel at or below [`MIN_LUMINANCE`] nits: every channel becomes this value.
  fn cap(&self, y: f32) -> f32 {
    self.new_luminance(self.source_peak * y) * self.inv_target_peak
  }
}

/// `GamutMapScalar`: mix in just enough grey of the same luminance to bring every channel to
/// `>= 0`, then scale so that no channel is above 1.
fn gamut_map(rgb: &mut [f32; 3], lum: [f32; 3], preserve_saturation: f32) {
  let luminance = lum[0] * rgb[0] + lum[1] * rgb[1] + lum[2] * rgb[2];
  let mut gray_mix_saturation = 0.0f32;
  let mut gray_mix_luminance = 0.0f32;
  for &val in rgb.iter() {
    let val_minus_gray = val - luminance;
    let inv_val_minus_gray = 1.0
      / if val_minus_gray == 0.0 {
        1.0
      } else {
        val_minus_gray
      };
    let val_over_val_minus_gray = val * inv_val_minus_gray;
    if val_minus_gray < 0.0 {
      gray_mix_saturation = gray_mix_saturation.max(val_over_val_minus_gray);
    }
    gray_mix_luminance = gray_mix_luminance.max(if val_minus_gray <= 0.0 {
      gray_mix_saturation
    } else {
      val_over_val_minus_gray - inv_val_minus_gray
    });
  }
  let gray_mix = (preserve_saturation * (gray_mix_saturation - gray_mix_luminance)
    + gray_mix_luminance)
    .clamp(0.0, 1.0);
  for val in rgb.iter_mut() {
    *val += gray_mix * (luminance - *val);
  }
  let normalizer = 1.0 / rgb[0].max(rgb[1]).max(rgb[2]).max(1.0);
  for val in rgb.iter_mut() {
    *val *= normalizer;
  }
}

fn mul3(a: &Matrix, b: &Matrix) -> Matrix {
  std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}

fn mul3_vec(m: &Matrix, v: [f64; 3]) -> [f64; 3] {
  std::array::from_fn(|i| (0..3).map(|k| m[i][k] * v[k]).sum())
}

fn inv3(m: &Matrix) -> Matrix {
  let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
    - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
    + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
  std::array::from_fn(|i| {
    std::array::from_fn(|j| {
      let (a, b) = ((j + 1) % 3, (j + 2) % 3);
      let (c, d) = ((i + 1) % 3, (i + 2) % 3);
      (m[a][c] * m[b][d] - m[a][d] * m[b][c]) / det
    })
  })
}

fn xy_to_xyz((x, y): (f64, f64)) -> [f64; 3] {
  [x / y, 1.0, (1.0 - x - y) / y]
}

/// Linear RGB -> XYZ (white = Y 1) for the given primaries and white point.
fn rgb_to_xyz(primaries: [(f64, f64); 3], white: (f64, f64)) -> Matrix {
  let columns = primaries.map(xy_to_xyz);
  let m: Matrix = std::array::from_fn(|r| std::array::from_fn(|c| columns[c][r]));
  let scale = mul3_vec(&inv3(&m), xy_to_xyz(white));
  std::array::from_fn(|r| std::array::from_fn(|c| m[r][c] * scale[c]))
}

/// Bradford chromatic adaptation from one white point to another (XYZ -> XYZ).
fn bradford(from: (f64, f64), to: (f64, f64)) -> Matrix {
  const B: Matrix = [
    [0.8951, 0.2664, -0.1614],
    [-0.7502, 1.7135, 0.0367],
    [0.0389, -0.0685, 1.0296],
  ];
  let from = mul3_vec(&B, xy_to_xyz(from));
  let to = mul3_vec(&B, xy_to_xyz(to));
  let diag: Matrix = [
    [to[0] / from[0], 0.0, 0.0],
    [0.0, to[1] / from[1], 0.0],
    [0.0, 0.0, to[2] / from[2]],
  ];
  mul3(&inv3(&B), &mul3(&diag, &B))
}

/// How the luminance of a pixel sets its gain.
enum Curve {
  /// PQ with an intensity target above 250 nits: Rec. 2408, gain table plus the exact near-black
  /// path of `ToneMap`.
  ToneMap(Rec2408),
  /// HLG: the product of the two OOTFs, from the gain table.
  Gain,
  /// PQ with an intensity target at or below 250 nits: nothing to tone map.
  None,
}

/// PQ or HLG code values -> 16-bit sRGB, in place. Build it once per image with [`HdrToSrgb::new`].
pub(super) struct HdrToSrgb {
  decode: &'static DecodeTable,
  encode: &'static [u16; 65536],
  /// Multiplies a decoded value to make it relative to the source peak (PQ: 10000 / target).
  scale: f32,
  /// Luminance of each source primary. Gray: `[1, 0, 0]`, applied to the single channel.
  lum: [f32; 3],
  /// Linear source RGB -> linear sRGB. `None` for gray.
  to_srgb: Option<[[f32; 3]; 3]>,
  curve: Curve,
  /// Gain over luminance (see [`GAIN_STEP_BITS`]), unused for [`Curve::None`].
  gain: Box<[f32; GAIN_LEN]>,
}

impl HdrToSrgb {
  /// `Some` only for PQ or HLG in a colour enum (RGB or gray). An ICC profile, even an HDR one,
  /// is left to the ICC path.
  pub(super) fn new(profile: &JxlColorProfile, intensity_target: f32) -> Option<Self> {
    let JxlColorProfile::Simple(encoding) = profile else {
      return None;
    };
    let (transfer_function, rgb) = match encoding {
      JxlColorEncoding::RgbColorSpace {
        white_point,
        primaries,
        transfer_function,
        ..
      } => (transfer_function, Some((white_point, primaries))),
      JxlColorEncoding::GrayscaleColorSpace {
        transfer_function, ..
      } => (transfer_function, None),
      JxlColorEncoding::XYB { .. } => return None,
    };
    let pq = match transfer_function {
      JxlTransferFunction::PQ => true,
      JxlTransferFunction::HLG => false,
      _ => return None,
    };
    let intensity_target = if intensity_target.is_finite() && intensity_target > 0.0 {
      intensity_target
    } else if pq {
      DEFAULT_PQ_NITS
    } else {
      DEFAULT_HLG_NITS
    };

    let (lum, to_srgb) = match rgb {
      Some((white_point, primaries)) => {
        let white = white_point.to_xy_coords();
        let white = (f64::from(white.0), f64::from(white.1));
        let primaries = primaries
          .to_xy_coords()
          .map(|(x, y)| (f64::from(x), f64::from(y)));
        let to_xyz = rgb_to_xyz(primaries, white);
        let d65 = JxlWhitePoint::D65.to_xy_coords();
        let d65 = (f64::from(d65.0), f64::from(d65.1));
        let srgb = rgb_to_xyz([(0.64, 0.33), (0.30, 0.60), (0.15, 0.06)], d65);
        let to_xyz = if (white.0 - d65.0).abs() > 1e-6 || (white.1 - d65.1).abs() > 1e-6 {
          mul3(&bradford(white, d65), &to_xyz)
        } else {
          to_xyz
        };
        let m = mul3(&inv3(&srgb), &to_xyz);
        (
          to_xyz[1].map(|v| v as f32),
          Some(m.map(|row| row.map(|v| v as f32))),
        )
      }
      None => ([1.0, 0.0, 0.0], None),
    };

    let mut gain = Box::new([1.0f32; GAIN_LEN]);
    let gain_at = |i: usize| f32::from_bits(GAIN_MIN_BITS + ((i as u32) << GAIN_FRACTION_BITS));
    let (decode, scale, curve) = if pq {
      // `stage_tone_mapping.cc`: PQ is only tone mapped down, never up.
      let curve = if intensity_target > TARGET_PQ_NITS {
        let tone_map = Rec2408::new(intensity_target, TARGET_PQ_NITS);
        for (i, value) in gain.iter_mut().enumerate() {
          *value = tone_map.multiplier(gain_at(i));
        }
        Curve::ToneMap(tone_map)
      } else {
        Curve::None
      };
      (pq_decode_table(), 10000.0 / intensity_target, curve)
    } else {
      // `HlgOOTF::FromSceneLight(intensity_target)` (scene -> display light, in the to-linear
      // stage), then `HlgOOTF(intensity_target, 80)` (the tone-mapping stage). An exponent within
      // 0.01 of 0 is skipped, like `apply_ootf_`.
      let exponents = [
        1.2 * 1.111f32.powf((intensity_target / 1000.0).log2()) - 1.0,
        1.111f32.powf((TARGET_HLG_NITS / intensity_target).log2()) - 1.0,
      ];
      for (i, value) in gain.iter_mut().enumerate() {
        let (mut g, mut y) = (1.0f32, gain_at(i));
        for exponent in exponents {
          if exponent < -0.01 || 0.01 < exponent {
            let ratio = y.powf(exponent).min(1e9);
            g *= ratio;
            y *= ratio;
          }
        }
        *value = g;
      }
      (hlg_decode_table(), 1.0, Curve::Gain)
    };

    Some(Self {
      decode,
      encode: srgb_encode_table(),
      scale,
      lum,
      to_srgb,
      curve,
      gain,
    })
  }

  /// Linear interpolation in the gain table, by the f32 bits of `y`.
  #[inline(always)]
  fn gain_of(&self, y: f32) -> f32 {
    // Also catches negative values, whose bit pattern would be above the table.
    if y.is_nan() || y <= f32::from_bits(GAIN_MIN_BITS) {
      return self.gain[0];
    }
    let offset = y.to_bits() - GAIN_MIN_BITS;
    let i = (offset >> GAIN_FRACTION_BITS) as usize;
    if i + 1 >= GAIN_LEN {
      return self.gain[GAIN_LEN - 1];
    }
    let fraction = (offset & ((1 << GAIN_FRACTION_BITS) - 1)) as f32
      * (1.0 / (1u32 << GAIN_FRACTION_BITS) as f32);
    let g0 = self.gain[i];
    g0 + (self.gain[i + 1] - g0) * fraction
  }

  /// Tone map a pixel (relative to the source peak), with luminance `y`.
  #[inline(always)]
  fn tone_map(&self, rgb: [f32; 3], y: f32) -> [f32; 3] {
    match &self.curve {
      Curve::ToneMap(tone_map) if y * tone_map.source_peak <= MIN_LUMINANCE => [tone_map.cap(y); 3],
      Curve::ToneMap(_) | Curve::Gain => {
        let g = self.gain_of(y);
        rgb.map(|v| v * g)
      }
      Curve::None => rgb,
    }
  }

  /// Linear sRGB (clipped to [0, 1] here) -> 16-bit sRGB code. `as u16` saturates and maps NaN to 0.
  #[inline(always)]
  fn encode(&self, v: f32) -> u16 {
    self.encode[(v.clamp(0.0, 1.0).sqrt() * 65535.0 + 0.5) as u16 as usize]
  }

  /// Convert interleaved 16-bit PQ/HLG samples to sRGB in place. `channels` is 1 or 2 for gray
  /// (with alpha), 3 or 4 for RGB (with alpha); alpha is left as it is.
  pub(super) fn apply_u16(&self, pixels: &mut [u16], channels: usize) {
    let decode = |code: u16| self.decode[usize::from(code)] * self.scale;
    match &self.to_srgb {
      Some(m) => {
        for pixel in pixels.chunks_exact_mut(channels) {
          let rgb = [decode(pixel[0]), decode(pixel[1]), decode(pixel[2])];
          let lum = self.lum;
          let y = lum[0] * rgb[0] + lum[1] * rgb[1] + lum[2] * rgb[2];
          let rgb = self.tone_map(rgb, y);
          let mut srgb: [f32; 3] =
            std::array::from_fn(|c| m[c][0] * rgb[0] + m[c][1] * rgb[1] + m[c][2] * rgb[2]);
          let max = srgb[0].max(srgb[1]).max(srgb[2]);
          let min = srgb[0].min(srgb[1]).min(srgb[2]);
          if max > 1.0 || min < 0.0 {
            gamut_map(&mut srgb, SRGB_LUMINANCE, PRESERVE_SATURATION);
          }
          for (out, value) in pixel.iter_mut().zip(srgb) {
            *out = self.encode(value);
          }
        }
      }
      None => {
        for pixel in pixels.chunks_exact_mut(channels) {
          let v = decode(pixel[0]);
          // `lum` is [1, 0, 0]: the luminance is the sample itself.
          pixel[0] = self.encode(self.tone_map([v; 3], v * self.lum[0])[0]);
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use jxl::api::{
    JxlColorEncoding, JxlColorProfile, JxlPrimaries, JxlTransferFunction, JxlWhitePoint,
    RenderingIntent,
  };

  use super::{HdrToSrgb, gamut_map, pq_encoded_from_nits, pq_nits_from_encoded};

  fn rgb(primaries: JxlPrimaries, transfer_function: JxlTransferFunction) -> JxlColorProfile {
    JxlColorProfile::Simple(JxlColorEncoding::RgbColorSpace {
      white_point: JxlWhitePoint::D65,
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

  #[test]
  fn only_pq_and_hlg_enums() {
    for tf in [JxlTransferFunction::PQ, JxlTransferFunction::HLG] {
      assert!(HdrToSrgb::new(&rgb(JxlPrimaries::BT2100, tf.clone()), 1000.0).is_some());
      assert!(HdrToSrgb::new(&gray(tf), 1000.0).is_some());
    }
    for tf in [
      JxlTransferFunction::SRGB,
      JxlTransferFunction::Linear,
      JxlTransferFunction::DCI,
    ] {
      assert!(HdrToSrgb::new(&rgb(JxlPrimaries::BT2100, tf.clone()), 1000.0).is_none());
      assert!(HdrToSrgb::new(&gray(tf), 1000.0).is_none());
    }
    // An ICC profile stays on the ICC path, whatever it describes.
    assert!(HdrToSrgb::new(&JxlColorProfile::Icc(vec![0; 132]), 1000.0).is_none());
  }

  #[test]
  fn pq_round_trip() {
    for nits in [0.01f32, 1.0, 100.0, 203.0, 250.0, 1000.0, 10000.0] {
      let back = pq_nits_from_encoded(f64::from(pq_encoded_from_nits(nits))) as f32;
      assert!((back - nits).abs() <= nits * 1e-4, "{nits} -> {back}");
    }
    // PQ code of 250 nits (jxl-rs had 0.5991247, the code of 242 nits).
    assert!((pq_encoded_from_nits(250.0) - 0.602_559_2).abs() < 1e-6);
  }

  #[test]
  fn gamut_map_keeps_in_gamut_pixels() {
    let mut rgb = [0.2f32, 0.5, 0.9];
    gamut_map(&mut rgb, super::SRGB_LUMINANCE, 0.1);
    assert_eq!(rgb, [0.2, 0.5, 0.9]);
    // Out of gamut: all channels end up in [0, 1].
    let mut rgb = [1.4f32, -0.2, 0.1];
    gamut_map(&mut rgb, super::SRGB_LUMINANCE, 0.1);
    assert!(rgb.iter().all(|v| (0.0..=1.0).contains(v)), "{rgb:?}");
  }

  /// Neutral PQ ramp (codes x * 257): monotonic, neutral, black stays black, 10000 nits is white.
  #[test]
  fn pq_ramp_is_monotonic_and_neutral() {
    for (profile, channels) in [
      (rgb(JxlPrimaries::BT2100, JxlTransferFunction::PQ), 3),
      (gray(JxlTransferFunction::PQ), 1),
    ] {
      let hdr = HdrToSrgb::new(&profile, 10000.0).unwrap();
      let mut pixels: Vec<u16> = (0..256u16)
        .flat_map(|x| std::iter::repeat_n(x * 257, channels))
        .collect();
      hdr.apply_u16(&mut pixels, channels);
      let first: Vec<u16> = pixels.iter().step_by(channels).copied().collect();
      assert_eq!(first[0], 0);
      assert_eq!(first[255], 65535);
      assert!(first.windows(2).all(|w| w[0] <= w[1]), "{first:?}");
      for pixel in pixels.chunks_exact(channels) {
        let (min, max) = (pixel.iter().min().unwrap(), pixel.iter().max().unwrap());
        assert!(max - min <= 1, "{pixel:?}");
      }
    }
  }

  #[test]
  fn alpha_is_untouched_and_nan_target_is_safe() {
    let hdr = HdrToSrgb::new(
      &rgb(JxlPrimaries::BT2100, JxlTransferFunction::HLG),
      f32::NAN,
    )
    .unwrap();
    let mut pixels = [65535u16, 0, 0, 1234, 30000, 30000, 30000, 0];
    hdr.apply_u16(&mut pixels, 4);
    assert_eq!(pixels[3], 1234);
    assert_eq!(pixels[7], 0);
    // Saturated Rec.2020 red is outside sRGB: desaturated, not clipped to pure red.
    assert!(pixels[1] > 0 && pixels[2] > 0, "{pixels:?}");
  }
}
