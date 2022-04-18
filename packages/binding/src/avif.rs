use image::{buffer::ConvertBuffer, DynamicImage, GenericImageView, ImageBuffer, Rgb};
use libavif::{AvifData, AvifImage, RgbPixels, YuvFormat};
use napi::bindgen_prelude::*;
use napi_derive::napi;

#[napi(object)]
#[derive(Default, Clone)]
pub struct AvifConfig {
  /// 0-100 scale
  pub quality: Option<u32>,
  /// 0-100 scale
  pub alpha_quality: Option<u32>,
  /// rav1e preset 1 (slow) 10 (fast but crappy), default is 4
  pub speed: Option<u32>,
  /// How many threads should be used (0 = match core count)
  pub threads: Option<u32>,
  /// set to '4:2:0' to use chroma subsampling, default '4:4:4'
  pub chroma_subsampling: Option<ChromaSubsampling>,
}

#[napi]
pub enum ChromaSubsampling {
  Yuv444,
  Yuv422,
  Yuv420,
  Yuv400,
}

impl From<ChromaSubsampling> for YuvFormat {
  #[inline]
  fn from(value: ChromaSubsampling) -> YuvFormat {
    match value {
      ChromaSubsampling::Yuv444 => YuvFormat::Yuv444,
      ChromaSubsampling::Yuv422 => YuvFormat::Yuv422,
      ChromaSubsampling::Yuv420 => YuvFormat::Yuv420,
      ChromaSubsampling::Yuv400 => YuvFormat::Yuv400,
    }
  }
}

struct Config {
  quality: u8,
  alpha_quality: u8,
  speed: u8,
  threads: usize,
  chroma_subsampling: ChromaSubsampling,
}

impl From<AvifConfig> for Config {
  fn from(config: AvifConfig) -> Self {
    Config {
      // See also: https://github.com/kornelski/cavif-rs#usage
      quality: config.quality.unwrap_or(80) as u8,
      // Calculate alphaQuality, this is consistent with cavif.
      // https://github.com/kornelski/cavif-rs/blob/37847b95bb81d4cf90e36b7fab2c7fbbcf95abe2/src/main.rs#L97
      alpha_quality: config.alpha_quality.unwrap_or(90) as u8,
      // Encoding speed between 1 (best, but slowest) and 10 (fastest, but a blurry mess), the default value is 4.
      // Speeds 1 and 2 are unbelievably slow, but make files ~3-5% smaller.
      // Speeds 7 and above degrade compression significantly, and are not recommended.
      speed: config.speed.unwrap_or(5) as u8,
      threads: config
        .threads
        .map(|n| n as usize)
        .unwrap_or(num_cpus::get()),
      chroma_subsampling: config
        .chroma_subsampling
        .unwrap_or(ChromaSubsampling::Yuv444),
    }
  }
}

#[inline]
pub(crate) fn encode_avif_inner(
  config: Option<AvifConfig>,
  input_image: &DynamicImage,
) -> Result<AvifData<'static>> {
  let mut encoder = libavif::Encoder::new();
  let config: Config = config.unwrap_or_default().into();
  encoder.set_quantizer((63.0 * (1.0 - config.quality as f32 / 100.0)) as u8);
  encoder.set_quantizer_alpha((63.0 * (1.0 - config.alpha_quality as f32 / 100.0)) as u8);
  encoder.set_speed(config.speed);
  encoder.set_max_threads(config.threads);
  let (width, height) = input_image.dimensions();
  let image = match input_image {
    DynamicImage::ImageRgb8(img) => {
      let avif_image = img.as_flat_samples();
      encode_image(
        avif_image.as_slice(),
        width,
        height,
        config.chroma_subsampling.into(),
      )
    }
    DynamicImage::ImageRgba8(img) => {
      let image = img.as_flat_samples();
      encode_image(
        image.as_slice(),
        width,
        height,
        config.chroma_subsampling.into(),
      )
    }
    DynamicImage::ImageLuma8(img) => {
      let image: ImageBuffer<Rgb<u8>, _> = img.convert();
      let avif_image = image.as_flat_samples();
      encode_image(
        avif_image.as_slice(),
        width,
        height,
        config.chroma_subsampling.into(),
      )
    }
    DynamicImage::ImageLumaA8(img) => {
      let image: ImageBuffer<Rgb<u8>, _> = img.convert();
      let avif_image = image.as_flat_samples();
      encode_image(
        avif_image.as_slice(),
        width,
        height,
        config.chroma_subsampling.into(),
      )
    }
    DynamicImage::ImageLuma16(img) => {
      let image: ImageBuffer<Rgb<u8>, _> = img.convert();
      let avif_image = image.as_flat_samples();
      encode_image(
        avif_image.as_slice(),
        width,
        height,
        config.chroma_subsampling.into(),
      )
    }
    DynamicImage::ImageLumaA16(img) => {
      let image: ImageBuffer<Rgb<u8>, _> = img.convert();
      let avif_image = image.as_flat_samples();
      encode_image(
        avif_image.as_slice(),
        width,
        height,
        config.chroma_subsampling.into(),
      )
    }
    DynamicImage::ImageRgb16(img) => {
      let image: ImageBuffer<Rgb<u8>, _> = img.convert();
      let avif_image = image.as_flat_samples();
      encode_image(
        avif_image.as_slice(),
        width,
        height,
        config.chroma_subsampling.into(),
      )
    }
    DynamicImage::ImageRgba16(img) => {
      let image: ImageBuffer<Rgb<u8>, _> = img.convert();
      let avif_image = image.as_flat_samples();
      encode_image(
        avif_image.as_slice(),
        width,
        height,
        config.chroma_subsampling.into(),
      )
    }
    DynamicImage::ImageRgb32F(img) => {
      let image: ImageBuffer<Rgb<u8>, _> = img.convert();
      let avif_image = image.as_flat_samples();
      encode_image(
        avif_image.as_slice(),
        width,
        height,
        config.chroma_subsampling.into(),
      )
    }
    DynamicImage::ImageRgba32F(img) => {
      let image: ImageBuffer<Rgb<u8>, _> = img.convert();
      let avif_image = image.as_flat_samples();
      encode_image(
        avif_image.as_slice(),
        width,
        height,
        config.chroma_subsampling.into(),
      )
    }
    _ => {
      return Err(Error::new(
        Status::InvalidArg,
        "Unsupported image type".to_owned(),
      ))
    }
  }?;
  encoder
    .encode(&image)
    .map_err(|err| Error::new(Status::InvalidArg, err.to_string()))
}

fn encode_image(
  avif_image: &[u8],
  width: u32,
  height: u32,
  format: YuvFormat,
) -> Result<AvifImage> {
  let image = if (width * height) as usize == avif_image.len() {
    AvifImage::from_luma8(width, height, avif_image)
      .map_err(|err| Error::new(Status::InvalidArg, err.to_string()))?
  } else {
    let rgb = RgbPixels::new(width, height, avif_image)
      .map_err(|err| Error::new(Status::InvalidArg, err.to_string()))?;
    rgb.to_image(format)
  };
  Ok(image)
}
