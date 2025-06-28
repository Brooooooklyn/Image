use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use image::imageops::overlay;
use image::{
  imageops::FilterType, ColorType, DynamicImage, ImageBuffer, ImageEncoder, ImageFormat, RgbaImage,
};
use libavif::AvifData;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use resvg::{
  tiny_skia,
  usvg::{self, fontdb::Database, Options},
};

use crate::{
  avif::{encode_avif_inner, AvifConfig},
  fast_resize::{fast_resize, FastResizeOptions, ResizeFit},
  png::PngEncodeOptions,
};

static FONT_DB: once_cell::sync::OnceCell<Arc<Database>> = once_cell::sync::OnceCell::new();

pub enum EncodeOptions {
  Png(PngEncodeOptions),
  Jpeg(u32),
  Webp(u32),
  WebpLossless,
  Avif(Option<AvifConfig>),
  Bmp,
  Ico,
  Tiff,
  Pnm,
  Tga,
  Farbfeld,
  RawPixels,
}

#[napi]
#[repr(u16)]
#[derive(Default, Clone, Copy)]
pub enum Orientation {
  #[default]
  /// Normal
  Horizontal = 1,
  MirrorHorizontal,
  Rotate180,
  MirrorVertical,
  MirrorHorizontalAndRotate270Cw,
  Rotate90Cw,
  MirrorHorizontalAndRotate90Cw,
  Rotate270Cw,
}

impl From<Orientation> for u16 {
  fn from(orientation: Orientation) -> Self {
    orientation as u16
  }
}

impl TryFrom<u16> for Orientation {
  type Error = Error;

  fn try_from(value: u16) -> Result<Self> {
    match value {
      1 => Ok(Orientation::Horizontal),
      2 => Ok(Orientation::MirrorHorizontal),
      3 => Ok(Orientation::Rotate180),
      4 => Ok(Orientation::MirrorVertical),
      5 => Ok(Orientation::MirrorHorizontalAndRotate270Cw),
      6 => Ok(Orientation::Rotate90Cw),
      7 => Ok(Orientation::MirrorHorizontalAndRotate90Cw),
      8 => Ok(Orientation::Rotate270Cw),
      _ => Err(Error::new(
        Status::InvalidArg,
        format!("Invalid orientation {value}"),
      )),
    }
  }
}

#[napi]
#[derive(Default, Clone, Copy)]
/// Available Sampling Filters.
///
/// ## Examples
///
/// To test the different sampling filters on a real example, you can find two
/// examples called
/// [`scaledown`](https://github.com/image-rs/image/tree/master/examples/scaledown)
/// and
/// [`scaleup`](https://github.com/image-rs/image/tree/master/examples/scaleup)
/// in the `examples` directory of the crate source code.
///
/// Here is a 3.58 MiB
/// [test image](https://github.com/image-rs/image/blob/master/examples/scaledown/test.jpg)
/// that has been scaled down to 300x225 px:
///
/// <!-- NOTE: To test new test images locally, replace the GitHub path with `../../../docs/` -->
/// <div style="display: flex; flex-wrap: wrap; align-items: flex-start;">
///   <div style="margin: 0 8px 8px 0;">
///     <img src="https://raw.githubusercontent.com/image-rs/image/master/examples/scaledown/scaledown-test-near.png" title="Nearest"><br>
///     Nearest Neighbor
///   </div>
///   <div style="margin: 0 8px 8px 0;">
///     <img src="https://raw.githubusercontent.com/image-rs/image/master/examples/scaledown/scaledown-test-tri.png" title="Triangle"><br>
///     Linear: Triangle
///   </div>
///   <div style="margin: 0 8px 8px 0;">
///     <img src="https://raw.githubusercontent.com/image-rs/image/master/examples/scaledown/scaledown-test-cmr.png" title="CatmullRom"><br>
///     Cubic: Catmull-Rom
///   </div>
///   <div style="margin: 0 8px 8px 0;">
///     <img src="https://raw.githubusercontent.com/image-rs/image/master/examples/scaledown/scaledown-test-gauss.png" title="Gaussian"><br>
///     Gaussian
///   </div>
///   <div style="margin: 0 8px 8px 0;">
///     <img src="https://raw.githubusercontent.com/image-rs/image/master/examples/scaledown/scaledown-test-lcz2.png" title="Lanczos3"><br>
///     Lanczos with window 3
///   </div>
/// </div>
///
/// ## Speed
///
/// Time required to create each of the examples above, tested on an Intel
/// i7-4770 CPU with Rust 1.37 in release mode:
///
/// <table style="width: auto;">
///   <tr>
///     <th>Nearest</th>
///     <td>31 ms</td>
///   </tr>
///   <tr>
///     <th>Triangle</th>
///     <td>414 ms</td>
///   </tr>
///   <tr>
///     <th>CatmullRom</th>
///     <td>817 ms</td>
///   </tr>
///   <tr>
///     <th>Gaussian</th>
///     <td>1180 ms</td>
///   </tr>
///   <tr>
///     <th>Lanczos3</th>
///     <td>1170 ms</td>
///   </tr>
/// </table>
pub enum ResizeFilterType {
  /// Nearest Neighbor
  Nearest,
  /// Linear Filter
  Triangle,
  /// Cubic Filter
  CatmullRom,
  /// Gaussian Filter
  Gaussian,
  #[default]
  /// Lanczos with window 3
  Lanczos3,
}

impl From<ResizeFilterType> for FilterType {
  fn from(filter: ResizeFilterType) -> Self {
    match filter {
      ResizeFilterType::Nearest => FilterType::Nearest,
      ResizeFilterType::Triangle => FilterType::Triangle,
      ResizeFilterType::CatmullRom => FilterType::CatmullRom,
      ResizeFilterType::Gaussian => FilterType::Gaussian,
      ResizeFilterType::Lanczos3 => FilterType::Lanczos3,
    }
  }
}

pub(crate) struct ImageMetaData {
  pub(crate) image: DynamicImage,
  pub(crate) color_type: ColorType,
  pub(crate) exif: HashMap<String, String>,
  pub(crate) orientation: Option<u16>,
  pub(crate) format: ImageFormat,
  pub(crate) has_parsed_exif: bool,
}

/// `env` from `Node.js` can ensure the thread safe.
pub(crate) struct ThreadsafeDynamicImage {
  raw: Arc<Uint8Array>,
  image: *mut Option<ImageMetaData>,
}

impl Drop for ThreadsafeDynamicImage {
  fn drop(&mut self) {
    unsafe {
      drop(Box::from_raw(self.image));
    }
  }
}

impl ThreadsafeDynamicImage {
  fn new(input: Arc<Uint8Array>) -> Self {
    ThreadsafeDynamicImage {
      image: Box::into_raw(Box::new(None)),
      raw: input,
    }
  }

  pub(crate) fn get(&self, with_exif: bool) -> Result<&mut ImageMetaData> {
    let image = Box::leak(unsafe { Box::from_raw(self.image) });
    let mut exif = HashMap::new();
    let mut orientation = None;
    match image {
      None => {
        let input_buf = self.raw.as_ref();
        let image_format = image::guess_format(input_buf).map_err(|err| {
          Error::new(
            Status::InvalidArg,
            format!("Guess format from input image failed {err}"),
          )
        })?;
        if with_exif {
          if let Some((_exif, _orientation)) = parse_exif(input_buf, &image_format) {
            exif = _exif;
            orientation = _orientation;
          }
        }
        let dynamic_image = if image_format == ImageFormat::Avif {
          let avif = libavif::decode_rgb(input_buf).map_err(|err| {
            Error::new(
              Status::InvalidArg,
              format!("Decode avif image failed {err}"),
            )
          })?;
          let decoded_rgb = avif.to_vec();
          let decoded_length = decoded_rgb.len();
          let width = avif.width();
          let height = avif.height();
          if (width * height * 3) as usize == decoded_length {
            ImageBuffer::from_raw(width, height, decoded_rgb)
              .map(DynamicImage::ImageRgb8)
              .ok_or_else(|| {
                Error::new(Status::InvalidArg, "Decode avif image failed".to_owned())
              })?
          } else if (width * height * 4) as usize == decoded_length {
            ImageBuffer::from_raw(width, height, decoded_rgb)
              .map(DynamicImage::ImageRgba8)
              .ok_or_else(|| {
                Error::new(Status::InvalidArg, "Decode avif image failed".to_owned())
              })?
          } else {
            ImageBuffer::from_raw(width, height, decoded_rgb)
              .map(DynamicImage::ImageLuma8)
              .ok_or_else(|| {
                Error::new(Status::InvalidArg, "Decode avif image failed".to_owned())
              })?
          }
        } else {
          image::load_from_memory_with_format(input_buf, image_format)
            .map_err(|err| Error::new(Status::InvalidArg, format!("Decode image failed {err}")))?
        };
        let color_type = dynamic_image.color();
        image.replace(ImageMetaData {
          image: dynamic_image,
          exif,
          orientation,
          format: image_format,
          has_parsed_exif: true,
          color_type,
        });
        Ok(image.as_mut().unwrap())
      }
      Some(res) => {
        if !res.has_parsed_exif && with_exif {
          if let Some((exif, orientation)) = parse_exif(self.raw.as_ref(), &res.format) {
            res.exif = exif;
            res.orientation = orientation;
          }
          res.has_parsed_exif = true;
        }
        Ok(res)
      }
    }
  }
}

unsafe impl Send for ThreadsafeDynamicImage {}
unsafe impl Sync for ThreadsafeDynamicImage {}

#[napi]
pub enum JsColorType {
  /// Pixel is 8-bit luminance
  L8,
  /// Pixel is 8-bit luminance with an alpha channel
  La8,
  /// Pixel contains 8-bit R, G and B channels
  Rgb8,
  /// Pixel is 8-bit RGB with an alpha channel
  Rgba8,

  /// Pixel is 16-bit luminance
  L16,
  /// Pixel is 16-bit luminance with an alpha channel
  La16,
  /// Pixel is 16-bit RGB
  Rgb16,
  /// Pixel is 16-bit RGBA
  Rgba16,

  /// Pixel is 32-bit float RGB
  Rgb32F,
  /// Pixel is 32-bit float RGBA
  Rgba32F,
}

impl From<ColorType> for JsColorType {
  fn from(value: ColorType) -> Self {
    match value {
      ColorType::L8 => JsColorType::L8,
      ColorType::La8 => JsColorType::La8,
      ColorType::Rgb8 => JsColorType::Rgb8,
      ColorType::Rgba8 => JsColorType::Rgba8,
      ColorType::L16 => JsColorType::L16,
      ColorType::La16 => JsColorType::La16,
      ColorType::Rgb16 => JsColorType::Rgb16,
      ColorType::Rgba16 => JsColorType::Rgba16,
      ColorType::Rgb32F => JsColorType::Rgb32F,
      ColorType::Rgba32F => JsColorType::Rgba32F,
      _ => panic!("Unsupported color type"),
    }
  }
}

#[napi(object)]
pub struct Metadata {
  pub width: u32,
  pub height: u32,
  pub exif: Option<HashMap<String, String>>,
  pub orientation: Option<u32>,
  pub format: String,
  pub color_type: JsColorType,
}

pub struct MetadataTask {
  dynamic_image: Arc<ThreadsafeDynamicImage>,
  with_exif: bool,
}

#[napi]
impl Task for MetadataTask {
  type Output = (
    u32,
    u32,
    HashMap<String, String>,
    Option<u16>,
    ImageFormat,
    ColorType,
  );
  type JsValue = Metadata;

  fn compute(&mut self) -> Result<Self::Output> {
    let ImageMetaData {
      image: dynamic_image,
      exif,
      orientation,
      format,
      color_type,
      ..
    } = self.dynamic_image.get(self.with_exif)?;
    Ok((
      dynamic_image.width(),
      dynamic_image.height(),
      exif.clone(),
      *orientation,
      *format,
      *color_type,
    ))
  }

  fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
    Ok(Metadata {
      width: output.0,
      height: output.1,
      exif: (!output.2.is_empty()).then_some(output.2),
      orientation: output.3.map(|o| o as u32),
      format: format!("{:?}", output.4).to_lowercase(),
      color_type: output.5.into(),
    })
  }
}

#[napi(object)]
#[derive(Clone, Copy)]
pub struct ResizeOptions {
  pub width: u32,
  pub height: Option<u32>,
  pub filter: Option<ResizeFilterType>,
  pub fit: Option<ResizeFit>,
}

#[derive(Default, Clone)]
struct ImageTransformArgs {
  grayscale: bool,
  invert: bool,
  rotate: bool,
  resize: Option<ResizeOptions>,
  fast_resize: Option<FastResizeOptions>,
  contrast: Option<f32>,
  blur: Option<f32>,
  unsharpen: Option<(f32, i32)>,
  filter3x3: Option<[f32; 9]>,
  brightness: Option<i32>,
  huerotate: Option<i32>,
  orientation: Option<Orientation>,
  crop: Option<(u32, u32, u32, u32)>,
  overlay: Vec<(Arc<Uint8Array>, i64, i64)>,
}

pub struct EncodeTask {
  image: Arc<ThreadsafeDynamicImage>,
  options: EncodeOptions,
  image_transform_args: ImageTransformArgs,
}

pub enum EncodeOutput {
  Raw(*mut u8, usize),
  Buffer(Vec<u8>),
  Avif(AvifData<'static>),
}

impl EncodeOutput {
  pub(crate) fn into_buffer_slice(self, env: &Env) -> Result<BufferSlice> {
    match self {
      EncodeOutput::Raw(ptr, len) => unsafe {
        BufferSlice::from_external(env, ptr, len, ptr, |_, pointer| {
          Vec::from_raw_parts(pointer, len, len);
        })
      },
      EncodeOutput::Buffer(buf) => Ok(BufferSlice::from_data(env, buf)?),
      EncodeOutput::Avif(avif_data) => {
        let len = avif_data.len();
        let data_ptr = avif_data.as_slice().as_ptr();
        unsafe {
          BufferSlice::from_external(env, data_ptr.cast_mut(), len, avif_data, |_, data| {
            drop(data);
          })
        }
      }
    }
  }
}

unsafe impl Send for EncodeOutput {}

#[napi]
impl Task for EncodeTask {
  type Output = EncodeOutput;
  type JsValue = Buffer;

  fn compute(&mut self) -> Result<Self::Output> {
    let meta = self.image.get(self.image_transform_args.rotate)?;
    let orientation = self
      .image_transform_args
      .orientation
      .map(Ok)
      .or_else(|| meta.orientation.map(|o| o.try_into()));
    if self.image_transform_args.rotate || self.image_transform_args.orientation.is_some() {
      if let Some(orientation) = orientation {
        match orientation? {
          Orientation::Horizontal => {}
          Orientation::MirrorHorizontal => meta.image = meta.image.fliph(),
          Orientation::Rotate180 => meta.image = meta.image.rotate180(),
          Orientation::MirrorVertical => meta.image = meta.image.flipv(),
          Orientation::MirrorHorizontalAndRotate270Cw => {
            meta.image = meta.image.fliph().rotate270()
          }
          Orientation::Rotate90Cw => meta.image = meta.image.rotate270(),
          Orientation::MirrorHorizontalAndRotate90Cw => meta.image = meta.image.flipv().rotate270(),
          Orientation::Rotate270Cw => meta.image = meta.image.rotate90(),
        }
      }
    }
    let raw_width = meta.image.width();
    let raw_height = meta.image.height();
    if let Some(ResizeOptions {
      width,
      height,
      filter,
      fit,
    }) = self.image_transform_args.resize
    {
      match fit.unwrap_or_default() {
        // the `resize_to_fill` is behavior like cover
        ResizeFit::Cover => {
          meta.image = meta.image.resize_to_fill(
            width,
            height.unwrap_or(((width as f32 / raw_width as f32) * (raw_height as f32)) as u32),
            filter.unwrap_or_default().into(),
          )
        }
        ResizeFit::Fill => {
          meta.image = meta.image.resize_exact(
            width,
            height.unwrap_or(((width as f32 / raw_width as f32) * (raw_height as f32)) as u32),
            filter.unwrap_or_default().into(),
          )
        }
        ResizeFit::Inside => {
          meta.image = meta.image.resize(
            width,
            height.unwrap_or(((width as f32 / raw_width as f32) * (raw_height as f32)) as u32),
            filter.unwrap_or_default().into(),
          )
        }
      }
    }
    if let Some(options) = self.image_transform_args.fast_resize {
      let resized_image = fast_resize(&meta.image, options)?;
      meta.image = DynamicImage::ImageRgba8(
        RgbaImage::from_raw(
          resized_image.width(),
          resized_image.height(),
          resized_image.into_vec(),
        )
        .ok_or_else(|| {
          Error::new(
            Status::GenericFailure,
            "Resized image is not valid".to_owned(),
          )
        })?,
      );
    }

    if self.image_transform_args.grayscale {
      meta.image = meta.image.grayscale();
    }
    if self.image_transform_args.invert {
      meta.image.invert();
    }
    if let Some(contrast) = self.image_transform_args.contrast {
      meta.image = meta.image.adjust_contrast(contrast);
    }
    if let Some(blur) = self.image_transform_args.blur {
      meta.image = meta.image.blur(blur);
    }
    if let Some((sigma, threshold)) = self.image_transform_args.unsharpen {
      meta.image = meta.image.unsharpen(sigma, threshold);
    }
    if let Some(filter) = self.image_transform_args.filter3x3 {
      meta.image = meta.image.filter3x3(filter.as_ref());
    }
    if let Some(brighten) = self.image_transform_args.brightness {
      meta.image = meta.image.brighten(brighten);
    }
    if let Some(hue) = self.image_transform_args.huerotate {
      meta.image = meta.image.huerotate(hue);
    }
    if let Some((x, y, width, height)) = self.image_transform_args.crop {
      meta.image = meta.image.crop_imm(x, y, width, height);
    }
    for (buffer, x, y) in std::mem::take(&mut self.image_transform_args.overlay).into_iter() {
      let top = ThreadsafeDynamicImage::new(buffer.clone());
      let top_image_meta = top.get(true)?;
      overlay(&mut meta.image, &top_image_meta.image, x, y);
    }

    let dynamic_image = &mut meta.image;
    let color_type = &meta.color_type;
    let width = dynamic_image.width();
    let height = dynamic_image.height();
    let format = match self.options {
      EncodeOptions::Webp(quality_factor) => {
        let (output_buf, size) = unsafe {
          crate::webp::encode_webp_inner(dynamic_image, quality_factor, width, height, color_type)
        }?;
        return Ok(EncodeOutput::Raw(output_buf, size));
      }
      EncodeOptions::WebpLossless => {
        let (output_buf, size) = unsafe {
          crate::webp::lossless_encode_webp_inner(
            dynamic_image.as_bytes(),
            width,
            height,
            color_type,
          )
        }?;
        if output_buf.is_null() {
          return Err(Error::new(
            Status::GenericFailure,
            format!(
              "Encode lossless webp failed, {}",
              dynamic_image.as_bytes().len()
            ),
          ));
        }
        return Ok(EncodeOutput::Raw(output_buf, size));
      }
      EncodeOptions::Avif(ref options) => {
        let output = encode_avif_inner(options.clone(), dynamic_image)?;
        return Ok(EncodeOutput::Avif(output));
      }
      EncodeOptions::Png(ref options) => {
        let mut output: Cursor<Vec<u8>> = Cursor::new(Vec::with_capacity(
          (dynamic_image.width() * dynamic_image.height() * 4) as usize,
        ));
        let png_encoder = image::codecs::png::PngEncoder::new_with_quality(
          &mut output,
          options.compression_type.unwrap_or_default().into(),
          options.filter_type.unwrap_or_default().into(),
        );
        png_encoder
          .write_image(
            dynamic_image.as_bytes(),
            dynamic_image.width(),
            dynamic_image.height(),
            dynamic_image.color().into(),
          )
          .map_err(|err| {
            Error::new(
              Status::GenericFailure,
              format!("Encode output png failed {err}"),
            )
          })?;
        return Ok(EncodeOutput::Buffer(output.into_inner()));
      }
      EncodeOptions::Jpeg(quality) => {
        let mut output: Cursor<Vec<u8>> = Cursor::new(Vec::with_capacity(
          (dynamic_image.width() * dynamic_image.height() * 4) as usize,
        ));
        let mut encoder =
          image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, quality as u8);
        encoder.encode_image(dynamic_image).map_err(|err| {
          Error::new(
            Status::GenericFailure,
            format!("Encode output jpeg failed {err}"),
          )
        })?;
        return Ok(EncodeOutput::Buffer(output.into_inner()));
      }
      EncodeOptions::Bmp => ImageFormat::Bmp,
      EncodeOptions::Ico => ImageFormat::Ico,
      EncodeOptions::Tiff => ImageFormat::Tiff,
      EncodeOptions::Pnm => ImageFormat::Pnm,
      EncodeOptions::Tga => ImageFormat::Tga,
      EncodeOptions::Farbfeld => ImageFormat::Farbfeld,
      EncodeOptions::RawPixels => {
        return Ok(EncodeOutput::Buffer(dynamic_image.as_bytes().to_vec()));
      }
    };
    let mut output: Cursor<Vec<u8>> = Cursor::new(Vec::with_capacity(
      (dynamic_image.width() * dynamic_image.height() * 4) as usize,
    ));
    dynamic_image.write_to(&mut output, format).map_err(|err| {
      Error::new(
        Status::InvalidArg,
        format!("Encode to [{:?}] error {}", &format, err),
      )
    })?;
    Ok(EncodeOutput::Buffer(output.into_inner()))
  }

  fn resolve(&mut self, env: Env, output: Self::Output) -> Result<Self::JsValue> {
    output
      .into_buffer_slice(&env)
      .and_then(|slice| slice.into_buffer(&env))
  }
}

#[napi]
pub struct Transformer {
  pub(crate) dynamic_image: Arc<ThreadsafeDynamicImage>,
  image_transform_args: ImageTransformArgs,
}

#[napi]
impl Transformer {
  #[napi(constructor)]
  pub fn new(input: Uint8Array) -> Transformer {
    Self {
      dynamic_image: Arc::new(ThreadsafeDynamicImage::new(Arc::new(input))),
      image_transform_args: ImageTransformArgs::default(),
    }
  }

  #[napi]
  /// Support CSS3 color, e.g. rgba(255, 255, 255, .8)
  pub fn from_svg(input: Either<String, &[u8]>, background: Option<String>) -> Result<Transformer> {
    let font_db = FONT_DB
      .get_or_init(|| {
        let mut fontdb = Database::new();
        fontdb.load_system_fonts();
        Arc::new(fontdb)
      })
      .clone();
    let options = Options::<'_> {
      fontdb: font_db,
      ..Default::default()
    };
    let tree = match input {
      Either::A(a) => usvg::Tree::from_str(a.as_str(), &options),
      Either::B(b) => usvg::Tree::from_data(b, &options),
    }
    .map_err(|err| Error::from_reason(format!("{err}")))?;
    let mut size = tree.size().to_int_size();
    let min_svg_size = 1000;
    while size.width() < min_svg_size || size.height() < min_svg_size {
      size = resvg::tiny_skia::IntSize::from_wh(size.width() * 2, size.height() * 2).unwrap();
    }
    let mut pix_map = tiny_skia::Pixmap::new(size.width(), size.height()).unwrap();

    // Inspired by [resvg-js/src/options.rs/fn create_pixmap](https://github.com/yisibl/resvg-js/blob/475ed45c091ef101f62f274b8a30883440bdfd89/src/options.rs#L185)
    let background = background
      .map(|bg| bg.parse::<svgtypes::Color>())
      .transpose()
      .map_err(|err| Error::from_reason(format!("{err}")))?;
    if let Some(bg) = background {
      let color = tiny_skia::Color::from_rgba8(bg.red, bg.green, bg.blue, bg.alpha);
      pix_map.fill(color);
    }
    resvg::render(
      &tree,
      tiny_skia::Transform::identity(),
      &mut pix_map.as_mut(),
    );

    let width = pix_map.width();
    let height = pix_map.height();
    let data = pix_map.take();

    Transformer::from_rgba_pixels(Either::A(data.as_slice()), width, height)
  }

  #[napi]
  pub fn from_rgba_pixels(
    input: Either<&[u8], Uint8ClampedSlice>,
    width: u32,
    height: u32,
  ) -> Result<Transformer> {
    if let Some(image) = image::RgbaImage::from_vec(
      width,
      height,
      match input {
        Either::A(a) => a.to_vec(),
        Either::B(b) => b.to_vec(),
      },
    ) {
      let image_meta = Box::new(Some(ImageMetaData {
        color_type: ColorType::Rgba8,
        orientation: None,
        image: DynamicImage::ImageRgba8(image),
        exif: HashMap::new(),
        format: ImageFormat::Png,
        has_parsed_exif: true,
      }));
      Ok(Self {
        dynamic_image: Arc::new(ThreadsafeDynamicImage {
          raw: Arc::new(vec![0].into()),
          image: Box::into_raw(image_meta),
        }),
        image_transform_args: Default::default(),
      })
    } else {
      Err(Error::new(
        Status::InvalidArg,
        "Input buffer is not matched the width and height".to_string(),
      ))
    }
  }

  #[napi]
  pub fn metadata(
    &mut self,
    with_exif: Option<bool>,
    signal: Option<AbortSignal>,
  ) -> AsyncTask<MetadataTask> {
    AsyncTask::with_optional_signal(
      MetadataTask {
        dynamic_image: self.dynamic_image.clone(),
        with_exif: with_exif.unwrap_or(false),
      },
      signal,
    )
  }

  #[napi]
  pub fn metadata_sync(&mut self, env: Env, with_exif: Option<bool>) -> Result<Metadata> {
    let mut task = MetadataTask {
        dynamic_image: self.dynamic_image.clone(),
        with_exif: with_exif.unwrap_or(false),
    };
    let output = task.compute()?;
    task.resolve(env, output)
  }

  #[napi]
  /// Rotate with exif orientation
  /// If the orientation param is not null,
  /// the new orientation value will override the exif orientation value
  pub fn rotate(&mut self, orientation: Option<Orientation>) -> &Self {
    self.image_transform_args.rotate = true;
    self.image_transform_args.orientation = orientation;
    self
  }

  #[napi]
  /// Return a grayscale version of this image.
  /// Returns `Luma` images in most cases. However, for `f32` images,
  /// this will return a greyscale `Rgb/Rgba` image instead.
  pub fn grayscale(&mut self) -> &Self {
    self.image_transform_args.grayscale = true;
    self
  }

  #[napi]
  /// Invert the colors of this image.
  pub fn invert(&mut self) -> &Self {
    self.image_transform_args.invert = true;
    self
  }

  #[napi]
  /// Resize this image using the specified filter algorithm.
  /// The image is scaled to the maximum possible size that fits
  /// within the bounds specified by `width` and `height`.
  pub fn resize(
    &mut self,
    width_or_options: Either<u32, ResizeOptions>,
    height: Option<u32>,
    filter: Option<ResizeFilterType>,
    fit: Option<ResizeFit>,
  ) -> &Self {
    match width_or_options {
      Either::A(width) => {
        self.image_transform_args.resize = Some(ResizeOptions {
          width,
          height,
          filter,
          fit,
        });
      }
      Either::B(options) => self.image_transform_args.resize = Some(options),
    }
    self
  }

  #[napi]
  /// Resize this image using the specified filter algorithm.
  /// The image is scaled to the maximum possible size that fits
  /// within the bounds specified by `width` and `height`.
  ///
  /// This is using faster SIMD based resize implementation
  /// the resize filter is different from `resize` method
  pub fn fast_resize(&mut self, options: FastResizeOptions) -> &Self {
    self.image_transform_args.fast_resize = Some(options);
    self
  }

  #[napi]
  /// Performs a Gaussian blur on this image.
  /// `sigma` is a measure of how much to blur by.
  pub fn blur(&mut self, sigma: f64) -> &Self {
    self.image_transform_args.blur = Some(sigma as f32);
    self
  }

  #[napi]
  /// Performs an unsharpen mask on this image.
  /// `sigma` is the amount to blur the image by.
  /// `threshold` is a control of how much to sharpen.
  ///
  /// See <https://en.wikipedia.org/wiki/Unsharp_masking#Digital_unsharp_masking>
  pub fn unsharpen(&mut self, sigma: f64, threshold: i32) -> &Self {
    self.image_transform_args.unsharpen = Some((sigma as f32, threshold));
    self
  }

  #[napi(js_name = "filter3x3")]
  /// Filters this image with the specified 3x3 kernel.
  pub fn filter3x3(&mut self, kernel: Vec<f64>) -> Result<&Self> {
    if kernel.len() != 9 {
      return Err(Error::new(
        Status::InvalidArg,
        "filter must be 3 x 3".to_owned(),
      ));
    }
    self.image_transform_args.filter3x3 = Some([
      kernel[0] as f32,
      kernel[1] as f32,
      kernel[2] as f32,
      kernel[3] as f32,
      kernel[4] as f32,
      kernel[5] as f32,
      kernel[6] as f32,
      kernel[7] as f32,
      kernel[8] as f32,
    ]);
    Ok(self)
  }

  #[napi]
  /// Adjust the contrast of this image.
  /// `contrast` is the amount to adjust the contrast by.
  /// Negative values decrease the contrast and positive values increase the contrast.
  pub fn adjust_contrast(&mut self, contrast: f64) -> &Self {
    self.image_transform_args.contrast = Some(contrast as f32);
    self
  }

  #[napi]
  /// Brighten the pixels of this image.
  /// `value` is the amount to brighten each pixel by.
  /// Negative values decrease the brightness and positive values increase it.
  pub fn brighten(&mut self, brightness: i32) -> &Self {
    self.image_transform_args.brightness = Some(brightness);
    self
  }

  #[napi]
  /// Hue rotate the supplied image.
  /// `value` is the degrees to rotate each pixel by.
  /// 0 and 360 do nothing, the rest rotates by the given degree value.
  /// just like the css webkit filter hue-rotate(180)
  pub fn huerotate(&mut self, hue: i32) -> &Self {
    self.image_transform_args.huerotate = Some(hue);
    self
  }

  #[napi]
  /// Crop a cut-out of this image delimited by the bounding rectangle.
  pub fn crop(&mut self, x: u32, y: u32, width: u32, height: u32) -> &Self {
    self.image_transform_args.crop = Some((x, y, width, height));
    self
  }

  #[napi]
  /// Overlay an image at a given coordinate (x, y)
  pub fn overlay(&mut self, on_top: Uint8Array, x: i64, y: i64) -> Result<&Self> {
    self
      .image_transform_args
      .overlay
      .push((Arc::new(on_top), x, y));
    Ok(self)
  }

  #[napi]
  /// Return this image's pixels as a native endian byte slice.
  pub fn raw_pixels(&self, signal: Option<AbortSignal>) -> AsyncTask<EncodeTask> {
    AsyncTask::with_optional_signal(
      EncodeTask {
        image: self.dynamic_image.clone(),
        options: EncodeOptions::RawPixels,
        image_transform_args: self.image_transform_args.clone(),
      },
      signal,
    )
  }

  #[napi]
  /// Return this image's pixels as a native endian byte slice.
  pub fn raw_pixels_sync(&self) -> Result<Buffer> {
    let meta = self.dynamic_image.get(false)?;
    Ok(meta.image.as_bytes().to_vec().into())
  }

  #[napi]
  /// The quality factor `quality_factor` ranges from 0 to 100 and controls the loss and quality during compression.
  /// The value 0 corresponds to low quality and small output sizes, whereas 100 is the highest quality and largest output size.
  /// https://developers.google.com/speed/webp/docs/api#simple_encoding_api
  pub fn webp(
    &mut self,
    quality_factor: Option<u32>,
    signal: Option<AbortSignal>,
  ) -> AsyncTask<EncodeTask> {
    AsyncTask::with_optional_signal(
      EncodeTask {
        image: self.dynamic_image.clone(),
        options: EncodeOptions::Webp(quality_factor.unwrap_or(90)),
        image_transform_args: self.image_transform_args.clone(),
      },
      signal,
    )
  }

  #[napi]
  /// The quality factor `quality_factor` ranges from 0 to 100 and controls the loss and quality during compression.
  /// The value 0 corresponds to low quality and small output sizes, whereas 100 is the highest quality and largest output size.
  /// https://developers.google.com/speed/webp/docs/api#simple_encoding_api
  pub fn webp_sync(&mut self, env: Env, quality_factor: Option<u32>) -> Result<Buffer> {
    let mut encoder = EncodeTask {
      image: self.dynamic_image.clone(),
      options: EncodeOptions::Webp(quality_factor.unwrap_or(90)),
      image_transform_args: self.image_transform_args.clone(),
    };
    let output = encoder.compute()?;
    encoder.resolve(env, output)
  }

  #[napi]
  pub fn webp_lossless(&mut self, signal: Option<AbortSignal>) -> AsyncTask<EncodeTask> {
    AsyncTask::with_optional_signal(
      EncodeTask {
        image: self.dynamic_image.clone(),
        options: EncodeOptions::WebpLossless,
        image_transform_args: self.image_transform_args.clone(),
      },
      signal,
    )
  }

  #[napi]
  pub fn webp_lossless_sync(&mut self, env: Env) -> Result<Buffer> {
    let mut encoder = EncodeTask {
      image: self.dynamic_image.clone(),
      options: EncodeOptions::WebpLossless,
      image_transform_args: self.image_transform_args.clone(),
    };
    let output = encoder.compute()?;
    encoder.resolve(env, output)
  }

  #[napi]
  pub fn avif(
    &mut self,
    options: Option<AvifConfig>,
    signal: Option<AbortSignal>,
  ) -> AsyncTask<EncodeTask> {
    AsyncTask::with_optional_signal(
      EncodeTask {
        image: self.dynamic_image.clone(),
        options: EncodeOptions::Avif(options),
        image_transform_args: self.image_transform_args.clone(),
      },
      signal,
    )
  }

  #[napi]
  pub fn avif_sync(&mut self, env: Env, options: Option<AvifConfig>) -> Result<Buffer> {
    let mut encoder = EncodeTask {
      image: self.dynamic_image.clone(),
      options: EncodeOptions::Avif(options),
      image_transform_args: self.image_transform_args.clone(),
    };
    let output = encoder.compute()?;
    encoder.resolve(env, output)
  }

  #[napi]
  pub fn png(
    &mut self,
    options: Option<PngEncodeOptions>,
    signal: Option<AbortSignal>,
  ) -> AsyncTask<EncodeTask> {
    AsyncTask::with_optional_signal(
      EncodeTask {
        image: self.dynamic_image.clone(),
        options: EncodeOptions::Png(options.unwrap_or_default()),
        image_transform_args: self.image_transform_args.clone(),
      },
      signal,
    )
  }

  #[napi]
  pub fn png_sync(&mut self, env: Env, options: Option<PngEncodeOptions>) -> Result<Buffer> {
    let mut encoder = EncodeTask {
      image: self.dynamic_image.clone(),
      options: EncodeOptions::Png(options.unwrap_or_default()),
      image_transform_args: self.image_transform_args.clone(),
    };
    let output = encoder.compute()?;
    encoder.resolve(env, output)
  }

  #[napi]
  /// default `quality` is 90
  pub fn jpeg(
    &mut self,
    quality: Option<u32>,
    signal: Option<AbortSignal>,
  ) -> AsyncTask<EncodeTask> {
    AsyncTask::with_optional_signal(
      EncodeTask {
        image: self.dynamic_image.clone(),
        options: EncodeOptions::Jpeg(quality.unwrap_or(90)),
        image_transform_args: self.image_transform_args.clone(),
      },
      signal,
    )
  }

  #[napi]
  /// default `quality` is 90
  pub fn jpeg_sync(&mut self, env: Env, quality: Option<u32>) -> Result<Buffer> {
    let mut encoder = EncodeTask {
      image: self.dynamic_image.clone(),
      options: EncodeOptions::Jpeg(quality.unwrap_or(90)),
      image_transform_args: self.image_transform_args.clone(),
    };
    let output = encoder.compute()?;
    encoder.resolve(env, output)
  }

  #[napi]
  pub fn bmp(&mut self, signal: Option<AbortSignal>) -> AsyncTask<EncodeTask> {
    AsyncTask::with_optional_signal(
      EncodeTask {
        image: self.dynamic_image.clone(),
        options: EncodeOptions::Bmp,
        image_transform_args: self.image_transform_args.clone(),
      },
      signal,
    )
  }

  #[napi]
  pub fn bmp_sync(&mut self, env: Env) -> Result<Buffer> {
    let mut encoder = EncodeTask {
      image: self.dynamic_image.clone(),
      options: EncodeOptions::Bmp,
      image_transform_args: self.image_transform_args.clone(),
    };
    let output = encoder.compute()?;
    encoder.resolve(env, output)
  }

  #[napi]
  pub fn ico(&mut self, signal: Option<AbortSignal>) -> AsyncTask<EncodeTask> {
    AsyncTask::with_optional_signal(
      EncodeTask {
        image: self.dynamic_image.clone(),
        options: EncodeOptions::Ico,
        image_transform_args: self.image_transform_args.clone(),
      },
      signal,
    )
  }

  #[napi]
  pub fn ico_sync(&mut self, env: Env) -> Result<Buffer> {
    let mut encoder = EncodeTask {
      image: self.dynamic_image.clone(),
      options: EncodeOptions::Ico,
      image_transform_args: self.image_transform_args.clone(),
    };
    let output = encoder.compute()?;
    encoder.resolve(env, output)
  }

  #[napi]
  pub fn tiff(&mut self, signal: Option<AbortSignal>) -> AsyncTask<EncodeTask> {
    AsyncTask::with_optional_signal(
      EncodeTask {
        image: self.dynamic_image.clone(),
        options: EncodeOptions::Tiff,
        image_transform_args: self.image_transform_args.clone(),
      },
      signal,
    )
  }

  #[napi]
  pub fn tiff_sync<'scope>(&'scope mut self, env: &'scope Env) -> Result<BufferSlice<'scope>> {
    let mut encoder = EncodeTask {
      image: self.dynamic_image.clone(),
      options: EncodeOptions::Tiff,
      image_transform_args: self.image_transform_args.clone(),
    };
    let output = encoder.compute()?;
    output.into_buffer_slice(env)
  }

  #[napi]
  pub fn pnm(&mut self, signal: Option<AbortSignal>) -> AsyncTask<EncodeTask> {
    AsyncTask::with_optional_signal(
      EncodeTask {
        image: self.dynamic_image.clone(),
        options: EncodeOptions::Pnm,
        image_transform_args: self.image_transform_args.clone(),
      },
      signal,
    )
  }

  #[napi]
  pub fn pnm_sync<'scope>(&'scope mut self, env: &'scope Env) -> Result<BufferSlice<'scope>> {
    let mut encoder = EncodeTask {
      image: self.dynamic_image.clone(),
      options: EncodeOptions::Pnm,
      image_transform_args: self.image_transform_args.clone(),
    };
    let output = encoder.compute()?;
    output.into_buffer_slice(env)
  }

  #[napi]
  pub fn tga(&mut self, signal: Option<AbortSignal>) -> AsyncTask<EncodeTask> {
    AsyncTask::with_optional_signal(
      EncodeTask {
        image: self.dynamic_image.clone(),
        options: EncodeOptions::Tga,
        image_transform_args: self.image_transform_args.clone(),
      },
      signal,
    )
  }

  #[napi]
  pub fn tga_sync<'scope>(&'scope mut self, env: &'scope Env) -> Result<BufferSlice<'scope>> {
    let mut encoder = EncodeTask {
      image: self.dynamic_image.clone(),
      options: EncodeOptions::Tga,
      image_transform_args: self.image_transform_args.clone(),
    };
    let output = encoder.compute()?;
    output.into_buffer_slice(env)
  }

  #[napi]
  pub fn farbfeld(&mut self, signal: Option<AbortSignal>) -> AsyncTask<EncodeTask> {
    AsyncTask::with_optional_signal(
      EncodeTask {
        image: self.dynamic_image.clone(),
        options: EncodeOptions::Farbfeld,
        image_transform_args: self.image_transform_args.clone(),
      },
      signal,
    )
  }

  #[napi]
  pub fn farbfeld_sync<'scope>(&'scope mut self, env: &'scope Env) -> Result<BufferSlice<'scope>> {
    let mut encoder = EncodeTask {
      image: self.dynamic_image.clone(),
      options: EncodeOptions::Farbfeld,
      image_transform_args: self.image_transform_args.clone(),
    };
    let output = encoder.compute()?;
    output.into_buffer_slice(env)
  }
}
#[inline]
fn parse_exif(
  buf: &[u8],
  image_format: &ImageFormat,
) -> Option<(HashMap<String, String>, Option<u16>)> {
  match image_format {
    image::ImageFormat::Jpeg | image::ImageFormat::Tiff => {
      if let Ok(exif_data) = rexif::parse_buffer(buf) {
        let exif = exif_data
          .entries
          .iter()
          .filter(|t| t.tag != rexif::ExifTag::UnknownToMe)
          .map(|t| (t.tag.to_string(), t.value_more_readable.to_string()))
          .collect::<HashMap<String, String>>();
        let orientation = exif_data
          .entries
          .iter()
          .find(|t| t.tag == rexif::ExifTag::Orientation)
          .and_then(|exif| match &exif.value {
            rexif::TagValue::U16(v) => v.first().copied(),
            _ => None,
          });
        return Some((exif, orientation));
      }
    }
    _ => {}
  }
  None
}
