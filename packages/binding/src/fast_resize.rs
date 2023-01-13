use std::io::{BufWriter, Cursor};
use std::num::NonZeroU32;

use fast_image_resize as fr;
use fr::FilterType;
use image::codecs::png::PngEncoder;
use image::io::Reader as ImageReader;
use image::{ColorType, ImageEncoder};
use napi::{bindgen_prelude::*, JsBuffer};
use napi_derive::napi;

#[napi]
pub enum FastResizeFilter {
  /// Each pixel of source image contributes to one pixel of the
  /// destination image with identical weights. For upscaling is equivalent
  /// of `Nearest` resize algorithm.    
  Box,
  /// Bilinear filter calculate the output pixel value using linear
  /// interpolation on all pixels that may contribute to the output value.
  Bilinear,
  /// Hamming filter has the same performance as `Bilinear` filter while
  /// providing the image downscaling quality comparable to bicubic
  /// (`CatmulRom` or `Mitchell`). Produces a sharper image than `Bilinear`,
  /// doesn't have dislocations on local level like with `Box`.
  /// The filter don’t show good quality for the image upscaling.
  Hamming,
  /// Catmull-Rom bicubic filter calculate the output pixel value using
  /// cubic interpolation on all pixels that may contribute to the output
  /// value.
  CatmullRom,
  /// Mitchell–Netravali bicubic filter calculate the output pixel value
  /// using cubic interpolation on all pixels that may contribute to the
  /// output value.
  Mitchell,
  /// Lanczos3 filter calculate the output pixel value using a high-quality
  /// Lanczos filter (a truncated sinc) on all pixels that may contribute
  /// to the output value.
  Lanczos3,
}

impl Default for FastResizeFilter {
  fn default() -> Self {
    FastResizeFilter::Lanczos3
  }
}

impl From<FastResizeFilter> for FilterType {
  fn from(value: FastResizeFilter) -> Self {
    match value {
      FastResizeFilter::Box => FilterType::Box,
      FastResizeFilter::Bilinear => FilterType::Bilinear,
      FastResizeFilter::Hamming => FilterType::Hamming,
      FastResizeFilter::CatmullRom => FilterType::CatmullRom,
      FastResizeFilter::Mitchell => FilterType::Mitchell,
      FastResizeFilter::Lanczos3 => FilterType::Lanczos3,
    }
  }
}

#[napi(object)]
pub struct FastResizeOptions {
  pub width: u32,
  pub height: Option<u32>,
  pub filter: Option<FastResizeFilter>,
}

#[napi]
pub fn fast_resize(data: JsBuffer, options: FastResizeOptions) -> Result<Buffer> {
  // Read source image from file
  let input = data.into_value()?;
  let reader = Cursor::new(&*input);
  let img = ImageReader::new(reader)
    .with_guessed_format()
    .map_err(|err| Error::new(Status::GenericFailure, format!("{err}")))?
    .decode()
    .map_err(|e| Error::new(Status::GenericFailure, format!("{e}")))?;
  let width = NonZeroU32::new(img.width())
    .ok_or_else(|| Error::new(Status::InvalidArg, "Image width should not be 0".to_owned()))?;
  let height = NonZeroU32::new(img.height()).ok_or_else(|| {
    Error::new(
      Status::InvalidArg,
      "Image height should not be 0".to_owned(),
    )
  })?;
  let mut rgba8 = img.to_rgba8();
  let mut src_image = fr::Image::from_slice_u8(width, height, rgba8.as_mut(), fr::PixelType::U8x4)
    .map_err(|err| Error::new(Status::GenericFailure, format!("{err}")))?;

  // Multiple RGB channels of source image by alpha channel
  // (not required for the Nearest algorithm)
  let alpha_mul_div = fr::MulDiv::default();
  alpha_mul_div
    .multiply_alpha_inplace(&mut src_image.view_mut())
    .map_err(|err| Error::new(Status::GenericFailure, format!("{err}")))?;

  // Create container for data of destination image
  let dst_width = NonZeroU32::new(options.width).ok_or_else(|| {
    Error::new(
      Status::InvalidArg,
      "Resized width should not be 0".to_owned(),
    )
  })?;
  let dst_height = NonZeroU32::new(options.height.unwrap_or_else(|| {
    <NonZeroU32 as Into<u32>>::into(dst_width) / <NonZeroU32 as Into<u32>>::into(width)
      * <NonZeroU32 as Into<u32>>::into(height)
  }))
  .ok_or_else(|| {
    Error::new(
      Status::InvalidArg,
      "Resized height should not be 0".to_owned(),
    )
  })?;
  let mut dst_image = fr::Image::new(dst_width, dst_height, src_image.pixel_type());

  // Get mutable view of destination image data
  let mut dst_view = dst_image.view_mut();

  // Create Resizer instance and resize source image
  // into buffer of destination image
  let mut resizer = fr::Resizer::new(fr::ResizeAlg::Convolution(
    options.filter.unwrap_or_default().into(),
  ));
  resizer
    .resize(&src_image.view(), &mut dst_view)
    .map_err(|err| Error::new(Status::GenericFailure, format!("{err}")))?;

  // Divide RGB channels of destination image by alpha
  alpha_mul_div
    .divide_alpha_inplace(&mut dst_view)
    .map_err(|err| Error::new(Status::GenericFailure, format!("{err}")))?;

  // Write destination image as PNG-file
  let mut result_buf = BufWriter::new(Vec::new());
  PngEncoder::new(&mut result_buf)
    .write_image(
      dst_image.buffer(),
      dst_width.get(),
      dst_height.get(),
      ColorType::Rgba8,
    )
    .map_err(|err| Error::new(Status::GenericFailure, format!("{err}")))?;
  let output = result_buf
    .into_inner()
    .map_err(|err| Error::new(Status::GenericFailure, format!("{err}")))?;
  Ok(output.into())
}
