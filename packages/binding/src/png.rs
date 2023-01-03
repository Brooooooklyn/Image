use napi::bindgen_prelude::*;
use napi_derive::napi;
use rgb::FromSlice;

#[napi]
pub enum CompressionType {
  /// Default compression level
  Default,
  /// Fast, minimal compression
  Fast,
  /// High compression level
  Best,
}

impl Default for CompressionType {
  fn default() -> Self {
    CompressionType::Default
  }
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
pub enum FilterType {
  /// No processing done, best used for low bit depth greyscale or data with a
  /// low color count
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

impl Default for FilterType {
  fn default() -> Self {
    FilterType::NoFilter
  }
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
  // Standard filter types
  None,
  Sub,
  Up,
  Average,
  Paeth,
  // Heuristic strategies
  MinSum,
  Entropy,
  Bigrams,
  BigEnt,
  Brute,
}

impl From<&PngRowFilter> for oxipng::RowFilter {
  fn from(value: &PngRowFilter) -> Self {
    match value {
      PngRowFilter::None => oxipng::RowFilter::None,
      PngRowFilter::Sub => oxipng::RowFilter::Sub,
      PngRowFilter::Up => oxipng::RowFilter::Up,
      PngRowFilter::Average => oxipng::RowFilter::Average,
      PngRowFilter::Paeth => oxipng::RowFilter::Paeth,
      PngRowFilter::MinSum => oxipng::RowFilter::MinSum,
      PngRowFilter::Entropy => oxipng::RowFilter::Entropy,
      PngRowFilter::Bigrams => oxipng::RowFilter::Bigrams,
      PngRowFilter::BigEnt => oxipng::RowFilter::BigEnt,
      PngRowFilter::Brute => oxipng::RowFilter::Brute,
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
    filter: opt
      .filter
      .as_ref()
      .map(|v| v.iter().map(|i| i.into()).collect())
      .unwrap_or_else(|| {
        oxipng::IndexSet::from_iter([
          oxipng::RowFilter::None,
          oxipng::RowFilter::Sub,
          oxipng::RowFilter::Up,
          oxipng::RowFilter::Average,
          oxipng::RowFilter::Paeth,
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
          oxipng::Headers::All
        } else {
          oxipng::Headers::None
        }
      })
      .unwrap_or(oxipng::Headers::All),
    #[cfg(target_arch = "arm")]
    deflate: oxipng::Deflaters::Libdeflater { compression: 12 },
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
pub fn png_quantize_sync(input: Buffer, options: Option<PngQuantOptions>) -> Result<Buffer> {
  let output = png_quantize_inner(input.as_ref(), &options.unwrap_or_default())?;
  Ok(output.into())
}

#[inline(never)]
fn png_quantize_inner(input: &[u8], options: &PngQuantOptions) -> Result<Vec<u8>> {
  let decoder = png::Decoder::new(input);
  let mut reader = decoder
    .read_info()
    .map_err(|err| Error::new(Status::InvalidArg, format!("Read png info failed {}", err)))?;
  let mut decoded_buf = vec![0; reader.output_buffer_size()];
  let output_info = reader
    .next_frame(&mut decoded_buf)
    .map_err(|err| Error::new(Status::InvalidArg, format!("Read png frame failed {}", err)))?;
  let width = output_info.width;
  let height = output_info.height;
  // The input png quality is too low
  if decoded_buf.len() < (width * height * 4) as usize {
    return Ok(input.to_vec());
  }
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
  Ok(output)
}

pub struct PngQuantTask {
  input: Buffer,
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
  input: Buffer,
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
