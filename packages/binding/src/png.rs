use napi::bindgen_prelude::*;
use napi_derive::napi;
use rgb::FromSlice;

use crate::quantize;

#[napi]
#[derive(Default, Copy, Clone)]
pub enum CompressionType {
  /// Default compression level
  #[default]
  Default,
  /// Fast, minimal compression
  Fast,
  /// High compression level
  Best,
}

impl From<CompressionType> for image::codecs::png::CompressionType {
  fn from(compression_type: CompressionType) -> Self {
    match compression_type {
      CompressionType::Default => image::codecs::png::CompressionType::Default,
      CompressionType::Fast => image::codecs::png::CompressionType::Fast,
      CompressionType::Best => image::codecs::png::CompressionType::Best,
    }
  }
}

#[napi]
#[derive(Default, Clone, Copy)]
pub enum FilterType {
  /// No processing done, best used for low bit depth greyscale or data with a
  /// low color count
  #[default]
  NoFilter,
  /// Filters based on previous pixel in the same scanline
  Sub,
  /// Filters based on the scanline above
  Up,
  /// Filters based on the average of left and right neighbor pixels
  Avg,
  /// Algorithm that takes into account the left, upper left, and above pixels
  Paeth,
  /// Uses a heuristic to select one of the preceding filters for each
  /// scanline rather than one filter for the entire image
  Adaptive,
}

impl From<FilterType> for image::codecs::png::FilterType {
  fn from(filter: FilterType) -> Self {
    match filter {
      FilterType::NoFilter => image::codecs::png::FilterType::NoFilter,
      FilterType::Sub => image::codecs::png::FilterType::Sub,
      FilterType::Up => image::codecs::png::FilterType::Up,
      FilterType::Avg => image::codecs::png::FilterType::Avg,
      FilterType::Paeth => image::codecs::png::FilterType::Paeth,
      FilterType::Adaptive => image::codecs::png::FilterType::Adaptive,
    }
  }
}

#[napi(object)]
#[derive(Default)]
pub struct PngEncodeOptions {
  /// Default is `CompressionType::Default`
  pub compression_type: Option<CompressionType>,
  /// Default is `FilterType::NoFilter`
  pub filter_type: Option<FilterType>,
}

#[napi]
pub enum PngRowFilter {
  None,
  Sub,
  Up,
  Average,
  Paeth,
}

impl From<&PngRowFilter> for oxipng::FilterStrategy {
  fn from(value: &PngRowFilter) -> Self {
    match value {
      PngRowFilter::None => oxipng::FilterStrategy::NONE,
      PngRowFilter::Sub => oxipng::FilterStrategy::SUB,
      PngRowFilter::Up => oxipng::FilterStrategy::UP,
      PngRowFilter::Average => oxipng::FilterStrategy::AVERAGE,
      PngRowFilter::Paeth => oxipng::FilterStrategy::PAETH,
    }
  }
}

#[napi(object, js_name = "PNGLosslessOptions")]
#[derive(Default)]
pub struct PNGLosslessOptions {
  /// Attempt to fix errors when decoding the input file rather than returning an Err.
  /// Default: `false`
  pub fix_errors: Option<bool>,
  /// Write to output even if there was no improvement in compression.
  /// Default: `false`
  pub force: Option<bool>,
  /// Which filters to try on the file (0-5)
  pub filter: Option<Vec<PngRowFilter>>,
  /// Whether to attempt bit depth reduction
  /// Default: `true`
  pub bit_depth_reduction: Option<bool>,
  /// Whether to attempt color type reduction
  /// Default: `true`
  pub color_type_reduction: Option<bool>,
  /// Whether to attempt palette reduction
  /// Default: `true`
  pub palette_reduction: Option<bool>,
  /// Whether to attempt grayscale reduction
  /// Default: `true`
  pub grayscale_reduction: Option<bool>,
  /// Whether to perform IDAT recoding
  /// If any type of reduction is performed, IDAT recoding will be performed regardless of this setting
  /// Default: `true`
  pub idat_recoding: Option<bool>,
  /// Whether to remove ***All non-critical headers*** on PNG
  pub strip: Option<bool>,
}

#[inline(always)]
fn to_oxipng_options(opt: &PNGLosslessOptions) -> oxipng::Options {
  oxipng::Options {
    fix_errors: opt.fix_errors.unwrap_or(false),
    force: opt.force.unwrap_or(false),
    filters: opt
      .filter
      .as_ref()
      .map(|v| v.iter().map(|i| i.into()).collect())
      .unwrap_or_else(|| {
        oxipng::IndexSet::from_iter([
          oxipng::FilterStrategy::NONE,
          oxipng::FilterStrategy::SUB,
          oxipng::FilterStrategy::UP,
          oxipng::FilterStrategy::AVERAGE,
          oxipng::FilterStrategy::PAETH,
        ])
      }),
    bit_depth_reduction: opt.bit_depth_reduction.unwrap_or(true),
    color_type_reduction: opt.color_type_reduction.unwrap_or(true),
    palette_reduction: opt.palette_reduction.unwrap_or(true),
    grayscale_reduction: opt.grayscale_reduction.unwrap_or(true),
    idat_recoding: opt.idat_recoding.unwrap_or(true),
    strip: opt
      .strip
      .map(|s| {
        if s {
          oxipng::StripChunks::All
        } else {
          oxipng::StripChunks::None
        }
      })
      .unwrap_or(oxipng::StripChunks::Safe),
    #[cfg(target_arch = "arm")]
    deflater: oxipng::Deflater::Libdeflater { compression: 12 },
    ..Default::default()
  }
}

#[napi]
pub fn lossless_compress_png_sync(
  input: Buffer,
  options: Option<PNGLosslessOptions>,
) -> Result<Buffer> {
  let output = oxipng::optimize_from_memory(
    input.as_ref(),
    &to_oxipng_options(&options.unwrap_or_default()),
  )
  .map_err(|err| Error::new(Status::InvalidArg, format!("Optimize failed {err}")))?;
  Ok(output.into())
}

pub struct LosslessPngTask {
  input: Uint8Array,
  options: PNGLosslessOptions,
}

#[napi]
impl Task for LosslessPngTask {
  type Output = Vec<u8>;
  type JsValue = Buffer;

  fn compute(&mut self) -> Result<Self::Output> {
    oxipng::optimize_from_memory(self.input.as_ref(), &to_oxipng_options(&self.options))
      .map_err(|err| Error::new(Status::InvalidArg, format!("Optimize failed {err}")))
  }

  fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
    Ok(output.into())
  }
}

#[napi]
pub fn lossless_compress_png(
  input: Uint8Array,
  options: Option<PNGLosslessOptions>,
  signal: Option<AbortSignal>,
) -> Result<AsyncTask<LosslessPngTask>> {
  Ok(AsyncTask::with_optional_signal(
    LosslessPngTask {
      input,
      options: options.unwrap_or_default(),
    },
    signal,
  ))
}

#[napi(object)]
#[derive(Default)]
pub struct PngQuantOptions {
  /// default is 70
  pub min_quality: Option<u32>,
  /// default is 99
  pub max_quality: Option<u32>,
  /// 1- 10
  /// Faster speeds generate images of lower quality, but may be useful for real-time generation of images.
  /// default: 5
  pub speed: Option<u32>,
  /// Number of least significant bits to ignore.
  /// Useful for generating palettes for VGA, 15-bit textures, or other retro platforms.
  pub posterization: Option<u32>,
}

#[napi]
pub fn png_quantize_sync(input: &[u8], options: Option<PngQuantOptions>) -> Result<Buffer> {
  let output = png_quantize_inner(input, &options.unwrap_or_default())?;
  Ok(output.into())
}

/// Decodes a PNG buffer to 8-bit RGBA pixels.
///
/// This function is extracted to support future refactoring and to guard against
/// accidental format-sniffing. It pins decoding to PNG format exclusively:
/// non-PNG inputs (e.g., JPEG, BMP, WebP) return an error rather than being
/// silently re-encoded as PNG, which would break the format contract (callers
/// rely on input format being PNG and output remaining PNG).
///
/// Valid PNGs of any color type or bit depth are normalized to 8-bit RGBA.
/// Returns the raw RGBA byte buffer (reinterpret with `as_rgba()`) plus width/height.
fn decode_rgba8(input: &[u8]) -> std::result::Result<(Vec<u8>, u32, u32), String> {
  let rgba = image::load_from_memory_with_format(input, image::ImageFormat::Png)
    .map_err(|err| err.to_string())?
    .to_rgba8();
  let (width, height) = (rgba.width(), rgba.height());
  Ok((rgba.into_raw(), width, height))
}

/// Validates the public quantization options against their documented ranges,
/// matching the previous imagequant-backed behavior, which rejected
/// out-of-range speed/quality values instead of silently clamping them.
fn validate_png_quant_options(o: &PngQuantOptions) -> Result<()> {
  if let Some(speed) = o.speed
    && !(1..=10).contains(&speed)
  {
    return Err(Error::new(
      Status::InvalidArg,
      format!("speed must be between 1 and 10, got {speed}"),
    ));
  }
  let min = o.min_quality.unwrap_or(70);
  let max = o.max_quality.unwrap_or(99);
  if min > 100 {
    return Err(Error::new(
      Status::InvalidArg,
      format!("minQuality must be between 0 and 100, got {min}"),
    ));
  }
  if max > 100 {
    return Err(Error::new(
      Status::InvalidArg,
      format!("maxQuality must be between 0 and 100, got {max}"),
    ));
  }
  if min > max {
    return Err(Error::new(
      Status::InvalidArg,
      format!("minQuality ({min}) must not exceed maxQuality ({max})"),
    ));
  }
  Ok(())
}

#[inline(never)]
fn png_quantize_inner(input: &[u8], options: &PngQuantOptions) -> Result<Vec<u8>> {
  validate_png_quant_options(options)?;
  let (rgba_bytes, width, height) = decode_rgba8(input)
    .map_err(|err| Error::new(Status::InvalidArg, format!("Decode png failed {err}")))?;

  let cfg = quantize::QuantizeConfig::from_options(options);
  let out = quantize::quantize_rgba(rgba_bytes.as_rgba(), width as usize, height as usize, &cfg);
  if out.quality < cfg.min_quality {
    return Err(Error::new(
      Status::GenericFailure,
      format!(
        "Quantization quality {} below requested minimum {}",
        out.quality, cfg.min_quality
      ),
    ));
  }
  let palette = out.palette;
  let pixels = out.indices;
  let mut encoder = lodepng::Encoder::new();
  encoder.set_palette(palette.as_slice()).map_err(|err| {
    Error::new(
      Status::GenericFailure,
      format!("Set palette on png encoder {err}"),
    )
  })?;
  let output = encoder
    .encode(pixels.as_slice(), width as usize, height as usize)
    .map_err(|err| {
      Error::new(
        Status::GenericFailure,
        format!("Encode quantized png failed {err}"),
      )
    })?;

  // P2: losslessly recompress the quantized PNG with the in-repo oxipng pass.
  // The quantizer (palette/indices/quality gate) is untouched; this only re-encodes
  // the final PNG container/IDAT smaller. oxipng is lossless (decoded pixels, incl.
  // alpha, are unchanged) and `StripChunks::Safe` preserves tRNS, so partial alpha
  // survives. Keep the oxipng bytes ONLY if strictly smaller; on any error fall back
  // to the lodepng bytes — recompression must never fail or enlarge.
  //
  // DEDICATED, self-contained options (NOT the shared `to_oxipng_options`, which also
  // drives the public `losslessCompressPng` and only pins the level-12 deflater under
  // `#[cfg(target_arch = "arm")]`). Here the deflater is pinned EXPLICITLY and
  // UNCONDITIONALLY, so the recompress path takes no arch `cfg` branch: for a given
  // pinned toolchain + dep set (oxipng/libdeflater in `Cargo.lock`) the output is
  // deterministic and reproducible across targets — libdeflate's compressed-byte choice
  // is arch-independent, and oxipng's parallel filter search picks its winner by an
  // order-independent key, so threading does not perturb the bytes. We do NOT claim a
  // frozen golden-byte contract across libdeflater version bumps; the guarantees we rely
  // on are losslessness (decoded pixels incl. alpha unchanged, tRNS preserved by
  // `StripChunks::Safe`) and never-enlarge (keep the oxipng bytes only if strictly
  // smaller, else fall back to the lodepng bytes). Lossless reductions stay ON: they only
  // shrink the file (e.g. bit-depth/palette) and preserve the decoded pixels.
  let opts = oxipng::Options {
    filters: oxipng::IndexSet::from_iter([
      oxipng::FilterStrategy::NONE,
      oxipng::FilterStrategy::SUB,
      oxipng::FilterStrategy::UP,
      oxipng::FilterStrategy::AVERAGE,
      oxipng::FilterStrategy::PAETH,
    ]),
    bit_depth_reduction: true,
    color_type_reduction: true,
    palette_reduction: true,
    grayscale_reduction: true,
    idat_recoding: true,
    strip: oxipng::StripChunks::Safe,
    deflater: oxipng::Deflater::Libdeflater { compression: 12 },
    ..Default::default()
  };
  let final_png = match oxipng::optimize_from_memory(&output, &opts) {
    Ok(optimized) if optimized.len() < output.len() => optimized,
    _ => output,
  };
  Ok(final_png)
}

pub struct PngQuantTask {
  input: Uint8Array,
  options: PngQuantOptions,
}

#[napi]
impl Task for PngQuantTask {
  type Output = Vec<u8>;
  type JsValue = Buffer;

  fn compute(&mut self) -> Result<Self::Output> {
    png_quantize_inner(self.input.as_ref(), &self.options)
  }

  fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
    Ok(output.into())
  }
}

#[napi]
pub fn png_quantize(
  input: Uint8Array,
  options: Option<PngQuantOptions>,
  signal: Option<AbortSignal>,
) -> AsyncTask<PngQuantTask> {
  AsyncTask::with_optional_signal(
    PngQuantTask {
      input,
      options: options.unwrap_or_default(),
    },
    signal,
  )
}

#[cfg(test)]
mod tests {
  use std::io::Cursor;

  use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb, Rgba};
  use rgb::FromSlice;

  use super::{PngQuantOptions, decode_rgba8, png_quantize_inner};
  use crate::quantize;

  fn encode_png(img: &DynamicImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    img
      .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
      .expect("encode test png");
    bytes
  }

  /// Replicates the bare-lodepng encode block from `png_quantize_inner` WITHOUT the
  /// P2 oxipng recompression, so a test can prove the inner output is strictly smaller
  /// than this and (prove-fail) that the difference is the oxipng pass, not the encoder.
  fn quantize_bare_lodepng(input: &[u8], options: &PngQuantOptions) -> Vec<u8> {
    let (rgba_bytes, width, height) = decode_rgba8(input).expect("decode");
    let cfg = quantize::QuantizeConfig::from_options(options);
    let out = quantize::quantize_rgba(rgba_bytes.as_rgba(), width as usize, height as usize, &cfg);
    let mut encoder = lodepng::Encoder::new();
    encoder
      .set_palette(out.palette.as_slice())
      .expect("palette");
    encoder
      .encode(out.indices.as_slice(), width as usize, height as usize)
      .expect("encode")
  }

  /// Build a deterministic, many-color opaque test PNG that compresses non-trivially
  /// (so oxipng has real headroom over the bare lodepng encode).
  fn multicolor_rgba_png(w: u32, h: u32) -> Vec<u8> {
    let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(w, h);
    for y in 0..h {
      for x in 0..w {
        // A smooth-ish gradient with enough distinct colors to need a palette.
        let r = ((x * 255) / w.max(1)) as u8;
        let g = ((y * 255) / h.max(1)) as u8;
        let b = (((x + y) * 255) / (w + h).max(1)) as u8;
        img.put_pixel(x, y, Rgba([r, g, b, 255]));
      }
    }
    encode_png(&DynamicImage::ImageRgba8(img))
  }

  /// Decode a PNG to FULL 8-bit RGBA (4 channels incl. alpha). Panics if the bytes
  /// are not a valid, decodable PNG — so callers also use this as a "valid PNG" check.
  fn decode_rgba_pixels(png: &[u8]) -> (Vec<u8>, u32, u32) {
    let img = image::load_from_memory_with_format(png, ImageFormat::Png)
      .expect("decode png")
      .to_rgba8();
    let (w, h) = (img.width(), img.height());
    (img.into_raw(), w, h)
  }

  /// Asserts `png` is a valid PNG: correct 8-byte signature and a leading IHDR chunk.
  /// (`decode_rgba_pixels` already proves it decodes; this proves the container too.)
  fn assert_valid_png(png: &[u8]) {
    const SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
    assert!(png.len() > 8 + 8 + 4, "PNG too short to hold IHDR");
    assert_eq!(&png[..8], &SIGNATURE, "PNG signature");
    // First chunk after the signature must be IHDR (4-byte length, then type).
    assert_eq!(&png[12..16], b"IHDR", "first chunk must be IHDR");
  }

  /// Asserts the bare-lodepng encode and the `png_quantize_inner` output of the SAME
  /// quantizer input decode to BYTE-IDENTICAL full RGBA (all 4 channels, incl. alpha),
  /// and that the inner output is a valid PNG. This is the lossless contract: the
  /// oxipng recompression may pick any smaller representation, but the decoded pixels
  /// (incl. alpha) must be exactly preserved.
  fn assert_lossless_full_rgba(input: &[u8], opts: &PngQuantOptions) {
    let bare = quantize_bare_lodepng(input, opts);
    let inner = png_quantize_inner(input, opts).expect("quantize");
    assert_valid_png(&inner);
    let (bare_px, bw, bh) = decode_rgba_pixels(&bare);
    let (inner_px, iw, ih) = decode_rgba_pixels(&inner);
    assert_eq!((bw, bh), (iw, ih), "dimensions preserved");
    assert_eq!(
      bare_px.len() % 4,
      0,
      "decoded buffer must be full RGBA (4 channels)"
    );
    assert_eq!(
      bare_px, inner_px,
      "recompressed full RGBA (incl. alpha) must equal the bare lodepng pixels (lossless)"
    );
  }

  #[test]
  fn recompress_shrinks_and_is_lossless() {
    // P2: png_quantize_inner now losslessly recompresses the quantized PNG with
    // oxipng. Prove (1) the inner output is STRICTLY smaller than the bare lodepng
    // encode of the SAME quantizer output, and (2) it decodes to IDENTICAL pixels
    // (oxipng is lossless). PROVE-FAIL: drop the oxipng call in png_quantize_inner
    // and these two encodes are byte-for-byte equal, so the `<` assertion fails.
    let png = multicolor_rgba_png(96, 96);
    let opts = PngQuantOptions {
      max_quality: Some(75),
      min_quality: Some(0),
      ..Default::default()
    };

    let bare = quantize_bare_lodepng(&png, &opts);
    let inner = png_quantize_inner(&png, &opts).expect("quantize");

    assert!(
      inner.len() < bare.len(),
      "oxipng recompression must strictly shrink: inner {} vs bare lodepng {}",
      inner.len(),
      bare.len()
    );

    // Valid PNG (signature + IHDR) and lossless: the recompressed PNG decodes to the
    // same FULL RGBA (all 4 channels, incl. alpha) as the bare encode.
    assert_valid_png(&inner);
    let (bare_px, bw, bh) = decode_rgba_pixels(&bare);
    let (inner_px, iw, ih) = decode_rgba_pixels(&inner);
    assert_eq!((bw, bh), (iw, ih), "dimensions preserved");
    assert_eq!(
      bare_px.len() % 4,
      0,
      "decoded buffer must be full RGBA (4 channels)"
    );
    assert_eq!(
      bare_px, inner_px,
      "recompressed full RGBA (incl. alpha) must equal the bare lodepng pixels (lossless)"
    );
  }

  #[test]
  fn recompress_is_deterministic() {
    // The oxipng pass must be deterministic: two png_quantize_inner calls on the
    // same input/options produce byte-identical output (locks the size win in CI).
    let png = multicolor_rgba_png(80, 64);
    let opts = PngQuantOptions {
      max_quality: Some(75),
      min_quality: Some(0),
      ..Default::default()
    };
    let run1 = png_quantize_inner(&png, &opts).expect("run1");
    let run2 = png_quantize_inner(&png, &opts).expect("run2");
    assert_eq!(run1, run2, "recompression must be deterministic");
  }

  #[test]
  fn recompress_preserves_partial_alpha() {
    // PARTIAL-ALPHA: oxipng's `StripChunks::Safe` keeps tRNS, so transparency must
    // survive recompression. Strengthened from a weak alpha-RANGE check to FULL
    // bare-vs-inner RGBA equality: every pixel's R,G,B AND A must be byte-identical
    // pre/post recompression. A dropped/flattened tRNS (alpha forced to 255) would
    // make the alpha channels differ and fail the equality assert.
    let w = 32u32;
    let h = 32u32;
    let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(w, h);
    for y in 0..h {
      for x in 0..w {
        let r = ((x * 255) / w) as u8;
        let g = ((y * 255) / h) as u8;
        // Alpha sweeps the full 0..=255 range across the image.
        let a = (((x + y) * 255) / (w + h - 2)) as u8;
        img.put_pixel(x, y, Rgba([r, g, 128, a]));
      }
    }
    let png = encode_png(&DynamicImage::ImageRgba8(img));
    let opts = PngQuantOptions {
      max_quality: Some(75),
      min_quality: Some(0),
      ..Default::default()
    };

    // Full RGBA (incl. alpha) byte-equality bare-vs-inner + valid PNG.
    assert_lossless_full_rgba(&png, &opts);

    // Also assert the decoded alpha is genuinely a gradient (not all-opaque): this
    // guards against the source/quantizer collapsing alpha, so the equality above is
    // a meaningful partial-alpha test rather than a trivially-opaque one.
    let out = png_quantize_inner(&png, &opts).expect("quantize partial-alpha");
    let (px, ow, oh) = decode_rgba_pixels(&out);
    assert_eq!((ow, oh), (w, h), "dimensions preserved");
    let alphas: Vec<u8> = px.chunks_exact(4).map(|p| p[3]).collect();
    let min_a = *alphas.iter().min().unwrap();
    let max_a = *alphas.iter().max().unwrap();
    assert!(
      min_a < 64,
      "transparent pixels must survive recompression (min alpha {min_a})"
    );
    assert!(
      max_a > 192,
      "opaque pixels must survive recompression (max alpha {max_a})"
    );
    assert!(
      min_a < max_a,
      "alpha channel must remain a gradient, not flattened ({min_a}..{max_a})"
    );
  }

  #[test]
  fn recompress_lossless_on_edge_cases() {
    // Empirically prove the lossless contract (identical decoded full RGBA + valid PNG)
    // across the shapes Codex worried about. Each case is the bare lodepng encode vs
    // png_quantize_inner of the same quantizer input; oxipng may losslessly pick a
    // smaller representation (e.g. fewer bits/channels) but decoded pixels must match.
    // This intentionally does NOT pin colortype==Indexed.
    let opts = PngQuantOptions {
      max_quality: Some(75),
      min_quality: Some(0),
      ..Default::default()
    };

    // 1x1 (tiny) and 1xN (thin strip) — degenerate dimensions.
    for (w, h) in [(1u32, 1u32), (1u32, 17u32), (17u32, 1u32)] {
      let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(w, h);
      for y in 0..h {
        for x in 0..w {
          let r = ((x.wrapping_mul(37) + y.wrapping_mul(11)) % 256) as u8;
          let g = ((x.wrapping_mul(13) + y.wrapping_mul(53)) % 256) as u8;
          let b = ((x.wrapping_mul(101) + y.wrapping_mul(7)) % 256) as u8;
          img.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
      }
      let png = encode_png(&DynamicImage::ImageRgba8(img));
      assert_lossless_full_rgba(&png, &opts);
    }

    // All-transparent (a == 0 everywhere) with VARIED RGB under the zero alpha: a
    // dropped tRNS or RGB clobber would corrupt the decoded pixels.
    {
      let (w, h) = (24u32, 24u32);
      let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(w, h);
      for y in 0..h {
        for x in 0..w {
          let r = ((x * 255) / w) as u8;
          let g = ((y * 255) / h) as u8;
          let b = (((x + y) * 255) / (w + h - 2)) as u8;
          img.put_pixel(x, y, Rgba([r, g, b, 0]));
        }
      }
      let png = encode_png(&DynamicImage::ImageRgba8(img));
      assert_lossless_full_rgba(&png, &opts);
    }

    // Near-grayscale image (R≈G≈B): exercises grayscale_reduction without forcing it.
    {
      let (w, h) = (40u32, 30u32);
      let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(w, h);
      for y in 0..h {
        for x in 0..w {
          let v = (((x + y) * 255) / (w + h - 2)) as u8;
          img.put_pixel(x, y, Rgba([v, v.wrapping_add(1), v, 255]));
        }
      }
      let png = encode_png(&DynamicImage::ImageRgba8(img));
      assert_lossless_full_rgba(&png, &opts);
    }

    // >256 distinct colors: forces the quantizer to actually reduce the palette.
    {
      let (w, h) = (48u32, 48u32); // 2304 pixels, many distinct gradient colors
      let png = multicolor_rgba_png(w, h);
      assert_lossless_full_rgba(&png, &opts);
    }
  }

  #[test]
  fn decode_16bit_rgba_is_downscaled_not_split() {
    // Regression (Codex F1): a 16-bit RGBA PNG is 8 bytes/pixel. The old path
    // passed those bytes straight to `as_rgba()`, splitting each pixel into two
    // garbage RGBA8 pixels (2x the pixel count, corrupted color). Normalizing via
    // `image::to_rgba8()` must yield exactly width*height pixels, 16->8 bit scaled.
    let mut img: ImageBuffer<Rgba<u16>, Vec<u16>> = ImageBuffer::new(2, 1);
    img.put_pixel(0, 0, Rgba([65535, 0, 0, 65535])); // opaque red
    img.put_pixel(1, 0, Rgba([0, 65535, 0, 32768])); // half-alpha green
    let png = encode_png(&DynamicImage::ImageRgba16(img));

    let (bytes, w, h) = decode_rgba8(&png).expect("decode");
    assert_eq!((w, h), (2, 1));
    assert_eq!(
      bytes.len(),
      (w * h * 4) as usize,
      "exactly width*height*4 bytes"
    );
    let px = bytes.as_rgba();
    assert_eq!(px.len(), 2, "exactly width*height pixels, not 2x");
    assert_eq!(px[0], rgb::RGBA8::new(255, 0, 0, 255));
    assert_eq!(px[1].g, 255);
    assert_eq!(px[1].a, 128, "16-bit 32768 scales to 8-bit 128");
  }

  #[test]
  fn decode_8bit_rgb_gets_opaque_alpha_not_dropped() {
    // Regression (Codex F1): an 8-bit RGB PNG is 3 bytes/pixel, so the old
    // `len >= width*height*4` guard failed and the image was returned unquantized.
    // Normalizing must expand RGB to RGBA8 with a fully-opaque alpha so the image
    // is actually quantized.
    let mut img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(3, 1);
    img.put_pixel(0, 0, Rgb([10, 20, 30]));
    img.put_pixel(1, 0, Rgb([100, 110, 120]));
    img.put_pixel(2, 0, Rgb([200, 210, 220]));
    let png = encode_png(&DynamicImage::ImageRgb8(img));

    let (bytes, w, h) = decode_rgba8(&png).expect("decode");
    assert_eq!((w, h), (3, 1));
    let px = bytes.as_rgba();
    assert_eq!(px.len(), 3);
    assert_eq!(px[0], rgb::RGBA8::new(10, 20, 30, 255));
    assert_eq!(px[2], rgb::RGBA8::new(200, 210, 220, 255));
  }

  #[test]
  fn decode_rejects_non_png_input() {
    // Regression (Codex P0): pngQuantize is a PNG optimizer. It must reject non-PNG
    // input (e.g., JPEG, BMP, WebP) rather than sniffing the format and re-encoding
    // to PNG, which breaks the format contract (callers rely on output MIME/extension
    // remaining PNG).
    let mut img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(2, 2);
    img.put_pixel(0, 0, Rgb([255, 0, 0]));
    img.put_pixel(1, 0, Rgb([0, 255, 0]));
    img.put_pixel(0, 1, Rgb([0, 0, 255]));
    img.put_pixel(1, 1, Rgb([255, 255, 0]));
    let mut bytes = Vec::new();
    DynamicImage::ImageRgb8(img)
      .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Jpeg)
      .expect("encode test jpeg");

    assert!(
      decode_rgba8(&bytes).is_err(),
      "non-PNG (JPEG) input must be rejected"
    );
  }
}
