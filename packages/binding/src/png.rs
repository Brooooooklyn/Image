use napi::bindgen_prelude::*;
use napi_derive::napi;
use rgb::FromSlice;

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
  pub filter: Option<Vec<u32>>,
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
  /// Whether to use heuristics to pick the best filter and compression
  pub use_heuristics: Option<bool>,
}

#[inline(always)]
fn to_oxipng_options(opt: &PNGLosslessOptions) -> oxipng::Options {
  oxipng::Options {
    fix_errors: opt.fix_errors.unwrap_or(false),
    force: opt.force.unwrap_or(false),
    filter: opt
      .filter
      .as_ref()
      .map(|v| v.into_iter().map(|i| *i as u8).collect())
      .unwrap_or_else(|| oxipng::IndexSet::from_iter(0..5)),
    bit_depth_reduction: opt.bit_depth_reduction.unwrap_or(true),
    color_type_reduction: opt.color_type_reduction.unwrap_or(true),
    palette_reduction: opt.palette_reduction.unwrap_or(true),
    grayscale_reduction: opt.grayscale_reduction.unwrap_or(true),
    idat_recoding: opt.idat_recoding.unwrap_or(true),
    strip: opt
      .strip
      .map(|s| {
        if s {
          oxipng::Headers::All
        } else {
          oxipng::Headers::None
        }
      })
      .unwrap_or(oxipng::Headers::All),
    use_heuristics: opt.use_heuristics.unwrap_or(true),
    #[cfg(target_arch = "arm")]
    deflate: oxipng::Deflaters::Libdeflater,
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
  .map_err(|err| Error::new(Status::InvalidArg, format!("Optimize failed {}", err)))?;
  Ok(output.into())
}

pub struct LosslessPngTask {
  input: Buffer,
  options: PNGLosslessOptions,
}

#[napi]
impl Task for LosslessPngTask {
  type Output = Vec<u8>;
  type JsValue = Buffer;

  fn compute(&mut self) -> Result<Self::Output> {
    oxipng::optimize_from_memory(self.input.as_ref(), &to_oxipng_options(&self.options))
      .map_err(|err| Error::new(Status::InvalidArg, format!("Optimize failed {}", err)))
  }

  fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
    Ok(output.into())
  }
}

#[napi]
pub fn lossless_compress_png(
  input: Buffer,
  options: Option<PNGLosslessOptions>,
  signal: Option<AbortSignal>,
) -> AsyncTask<LosslessPngTask> {
  AsyncTask::with_optional_signal(
    LosslessPngTask {
      input,
      options: options.unwrap_or_default(),
    },
    signal,
  )
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
pub fn png_quantize(input: Buffer, options: Option<PngQuantOptions>) -> Result<Buffer> {
  let decoder = png::Decoder::new(input.as_ref());
  let mut reader = decoder
    .read_info()
    .map_err(|err| Error::new(Status::InvalidArg, format!("Read png info failed {}", err)))?;
  let mut decoded_buf = vec![0; reader.output_buffer_size()];
  let output_info = reader
    .next_frame(&mut decoded_buf)
    .map_err(|err| Error::new(Status::InvalidArg, format!("Read png frame failed {}", err)))?;

  let options = options.unwrap_or_default();
  let width = output_info.width;
  let height = output_info.height;
  let mut liq = imagequant::new();
  liq
    .set_speed(options.speed.unwrap_or(5) as i32)
    .map_err(|err| Error::new(Status::GenericFailure, format!("{}", err)))?;
  liq
    .set_quality(
      options.min_quality.unwrap_or(70) as u8,
      options.max_quality.unwrap_or(99) as u8,
    )
    .map_err(|err| Error::new(Status::GenericFailure, format!("{}", err)))?;
  let mut img = liq
    .new_image(decoded_buf.as_rgba(), width as usize, height as usize, 0.0)
    .map_err(|err| {
      Error::new(
        Status::GenericFailure,
        format!("Create image failed {}", err),
      )
    })?;
  let mut quantization_result = liq
    .quantize(&mut img)
    .map_err(|err| Error::new(Status::GenericFailure, format!("quantize failed {}", err)))?;
  quantization_result
    .set_dithering_level(1.0)
    .map_err(|err| Error::new(Status::GenericFailure, format!("{}", err)))?;
  let (palette, pixels) = quantization_result
    .remapped(&mut img)
    .map_err(|err| Error::new(Status::GenericFailure, format!("remap failed {}", err)))?;
  let mut encoder = lodepng::Encoder::new();
  encoder.set_palette(palette.as_slice()).map_err(|err| {
    Error::new(
      Status::GenericFailure,
      format!("Set palette on png encoder {}", err),
    )
  })?;
  let output = encoder
    .encode(pixels.as_slice(), width as usize, height as usize)
    .map_err(|err| {
      Error::new(
        Status::GenericFailure,
        format!("Encode quantized png failed {}", err),
      )
    })?;
  Ok(output.into())
}
