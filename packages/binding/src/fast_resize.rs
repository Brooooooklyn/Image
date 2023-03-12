use std::num::NonZeroU32;

use fast_image_resize as fr;
use fr::FilterType;
use image::DynamicImage;
use napi::bindgen_prelude::*;
use napi_derive::napi;

#[napi]
#[derive(Default)]
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
  #[default]
  Lanczos3,
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

#[napi]
pub enum ResizeFit {
  /// (default) Preserving aspect ratio
  /// ensure the image covers both provided dimensions by cropping/clipping to fit.
  Cover,
  /// Ignore the aspect ratio of the input and stretch to both provided dimensions.
  Fill,
  /// Preserving aspect ratio
  /// resize the image to be as large as possible while ensuring its dimensions are less than or equal to both those specified.
  Inside,
}

impl Default for ResizeFit {
  fn default() -> Self {
    Self::Cover
  }
}

#[napi(object)]
#[derive(Clone, Copy)]
pub struct FastResizeOptions {
  pub width: u32,
  pub height: Option<u32>,
  pub filter: Option<FastResizeFilter>,
  pub fit: Option<ResizeFit>,
}

pub fn fast_resize(img: &DynamicImage, options: FastResizeOptions) -> Result<fr::Image> {
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
  let mut dst_width = NonZeroU32::new(options.width).ok_or_else(|| {
    Error::new(
      Status::InvalidArg,
      "Resized width should not be 0".to_owned(),
    )
  })?;
  let mut dst_height = NonZeroU32::new(
    options
      .height
      .unwrap_or_else(|| (options.width as f32 / img.width() as f32 * img.height() as f32) as u32),
  )
  .ok_or_else(|| {
    Error::new(
      Status::InvalidArg,
      "Resized height should not be 0".to_owned(),
    )
  })?;

  match options.fit.unwrap_or_default() {
    ResizeFit::Cover => {
      src_image
        .view()
        .set_crop_box_to_fit_dst_size(dst_width, dst_height, None);
    }
    ResizeFit::Fill => {}
    ResizeFit::Inside => {
      let (width, height) = crate::utils::resize_dimensions(
        width.get(),
        height.get(),
        dst_width.get(),
        dst_height.get(),
        false,
      );
      dst_width = NonZeroU32::new(width).unwrap();
      dst_height = NonZeroU32::new(height).unwrap();
    }
  }
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
  Ok(dst_image)
}
