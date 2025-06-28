use fast_image_resize as fr;
use fr::{images::Image, FilterType};
use image::DynamicImage;
use napi::bindgen_prelude::*;
use napi_derive::napi;

#[napi]
#[derive(Default, Clone, Copy)]
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
#[derive(Clone, Copy)]
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

pub fn fast_resize(img: &DynamicImage, options: FastResizeOptions) -> Result<Image> {
  let width = img.width();
  let height = img.height();
  let mut rgba8 = img.to_rgba8();
  let mut src_image = Image::from_slice_u8(width, height, rgba8.as_mut(), fr::PixelType::U8x4)
    .map_err(|err| Error::new(Status::GenericFailure, format!("{err}")))?;

  // Multiple RGB channels of source image by alpha channel
  // (not required for the Nearest algorithm)
  let alpha_mul_div = fr::MulDiv::default();
  alpha_mul_div
    .multiply_alpha_inplace(&mut src_image)
    .map_err(|err| Error::new(Status::GenericFailure, format!("{err}")))?;

  // Create container for data of destination image
  let mut dst_width = options.width;
  let mut dst_height = options
    .height
    .unwrap_or_else(|| (options.width as f32 / img.width() as f32 * img.height() as f32) as u32);

  let mut resize_options = fr::ResizeOptions {
    algorithm: fr::ResizeAlg::Convolution(options.filter.unwrap_or_default().into()),
    ..Default::default()
  };

  match options.fit.unwrap_or_default() {
    ResizeFit::Cover => {
      resize_options = resize_options
        .fit_into_destination(Some(((dst_width as f64) / 2.0, (dst_height as f64) / 2.0)));
    }
    ResizeFit::Fill => {}
    ResizeFit::Inside => {
      let (width, height) =
        crate::utils::resize_dimensions(width, height, dst_width, dst_height, false);
      dst_width = width;
      dst_height = height;
    }
  }
  let mut dst_image = Image::new(dst_width, dst_height, src_image.pixel_type());

  // Create Resizer instance and resize source image
  // into buffer of destination image
  let mut resizer = fr::Resizer::new();
  resizer
    .resize(&src_image, &mut dst_image, Some(&resize_options))
    .map_err(|err| Error::new(Status::GenericFailure, format!("{err}")))?;

  // Divide RGB channels of destination image by alpha
  alpha_mul_div
    .divide_alpha_inplace(&mut dst_image)
    .map_err(|err| Error::new(Status::GenericFailure, format!("{err}")))?;
  Ok(dst_image)
}
