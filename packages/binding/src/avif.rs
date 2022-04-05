use napi::bindgen_prelude::*;
use napi_derive::napi;
use rgb::FromSlice;

use crate::decode::decode_input_image;

#[napi(object)]
#[derive(Default, Clone)]
pub struct AvifConfig {
  /// 0-100 scale
  pub quality: Option<u32>,
  /// 0-100 scale
  pub alpha_quality: Option<u32>,
  /// rav1e preset 1 (slow) 10 (fast but crappy), default is 4
  pub speed: Option<u32>,
  /// True if RGBA input has already been premultiplied. It inserts appropriate metadata.
  pub premultiplied_alpha: Option<bool>,
  /// Which pixel format to use in AVIF file. RGB tends to give larger files.
  pub color_space: Option<ColorSpace>,
  /// How many threads should be used (0 = match core count)
  pub threads: Option<u32>,
}

#[napi]
pub enum ColorSpace {
  YCbCr,
  RGB,
}

impl From<AvifConfig> for ravif::Config {
  fn from(config: AvifConfig) -> Self {
    ravif::Config {
      // See also: https://github.com/kornelski/cavif-rs#usage
      quality: config.quality.unwrap_or(80) as f32,
      // Calculate alphaQuality, this is consistent with cavif.
      // https://github.com/kornelski/cavif-rs/blob/37847b95bb81d4cf90e36b7fab2c7fbbcf95abe2/src/main.rs#L97
      alpha_quality: config.alpha_quality.unwrap_or(90) as f32,
      // Encoding speed between 1 (best, but slowest) and 10 (fastest, but a blurry mess), the default value is 4.
      // Speeds 1 and 2 are unbelievably slow, but make files ~3-5% smaller.
      // Speeds 7 and above degrade compression significantly, and are not recommended.
      speed: config.speed.unwrap_or(5) as u8,
      premultiplied_alpha: config.premultiplied_alpha.unwrap_or(false),
      color_space: match config.color_space {
        Some(ColorSpace::YCbCr) => ravif::ColorSpace::YCbCr,
        Some(ColorSpace::RGB) => ravif::ColorSpace::RGB,
        None => ravif::ColorSpace::YCbCr,
      },
      threads: config.threads.unwrap_or(0) as usize,
    }
  }
}

#[napi]
pub fn encode_avif(input: Buffer, config: Option<AvifConfig>) -> Result<Buffer> {
  let (image, width, height, alpha_channel) = decode_input_image(input.as_ref())?;
  let output = encode_avif_inner(config, &image, width, height, alpha_channel)?;
  Ok(output.into())
}

#[inline]
pub(crate) fn encode_avif_inner(
  config: Option<AvifConfig>,
  image: &[u8],
  width: u32,
  height: u32,
  alpha_channel: bool,
) -> Result<Vec<u8>> {
  if alpha_channel {
    ravif::encode_rgba(
      ravif::Img::new(image.as_rgba(), width as usize, height as usize),
      &config.unwrap_or_default().into(),
    )
    .map(|(output, _, _)| output)
  } else {
    ravif::encode_rgb(
      ravif::Img::new(image.as_rgb(), width as usize, height as usize),
      &config.unwrap_or_default().into(),
    )
    .map(|(output, _)| output)
  }
  .map_err(|err| {
    Error::new(
      Status::GenericFailure,
      format!("Encode avif failed {}", err),
    )
  })
}
