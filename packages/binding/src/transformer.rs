use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use image::{ColorType, DynamicImage, ImageFormat};
use libavif::AvifData;
use napi::{bindgen_prelude::*, JsBuffer};
use napi_derive::napi;

use crate::avif::{encode_avif_inner, AvifConfig};

pub enum EncodeOptions {
  Webp(u32),
  WebpLossless,
  Avif(Option<AvifConfig>),
  Bmp,
  Ico,
  Tiff,
  Pnm,
  Tga,
  Farbfeld,
}

struct ImageMetaData {
  image: DynamicImage,
  color_type: ColorType,
  exif: HashMap<String, String>,
  orientation: Option<u16>,
  format: ImageFormat,
  has_parsed_exif: bool,
}

/// `env` from `Node.js` can ensure the thread safe.
struct ThreadSafeDynamicImage {
  raw: Buffer,
  image: *mut Option<ImageMetaData>,
}

impl Drop for ThreadSafeDynamicImage {
  fn drop(&mut self) {
    unsafe {
      Box::from_raw(self.image);
    }
  }
}

impl ThreadSafeDynamicImage {
  fn new(input: Buffer) -> Self {
    ThreadSafeDynamicImage {
      image: Box::into_raw(Box::new(None)),
      raw: input,
    }
  }

  fn get(&self, with_exif: bool) -> Result<&mut ImageMetaData> {
    let image = Box::leak(unsafe { Box::from_raw(self.image) });
    let mut exif = HashMap::new();
    let mut orientation = None;
    if image.is_none() {
      let input_buf = self.raw.as_ref();
      let image_format = image::guess_format(input_buf).map_err(|err| {
        Error::new(
          Status::InvalidArg,
          format!("Guess format from input image failed {}", err),
        )
      })?;
      if with_exif {
        if let Some((_exif, _orientation)) = parse_exif(input_buf, &image_format) {
          exif = _exif;
          orientation = _orientation;
        }
      }
      let dynamic_image = image::load_from_memory_with_format(input_buf, image_format.clone())
        .map_err(|err| Error::new(Status::InvalidArg, format!("Decode image failed {}", err)))?;
      let color_type = dynamic_image.color();
      image.replace(ImageMetaData {
        image: dynamic_image,
        exif,
        orientation,
        format: image_format,
        has_parsed_exif: true,
        color_type,
      });
    }
    let mut res = image.as_mut().unwrap();
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

unsafe impl Send for ThreadSafeDynamicImage {}
unsafe impl Sync for ThreadSafeDynamicImage {}

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
  dynamic_image: Arc<ThreadSafeDynamicImage>,
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
      exif: (output.2.len() > 0).then(|| output.2),
      orientation: output.3.map(|o| o as u32),
      format: format!("{:?}", output.4).to_lowercase(),
      color_type: output.5.into(),
    })
  }
}

pub struct EncodeTask {
  image: Arc<ThreadSafeDynamicImage>,
  options: EncodeOptions,
  rotate: bool,
  resize: (Option<u32>, Option<u32>),
}

pub enum EncodeOutput {
  Raw(*mut u8, usize),
  Buffer(Vec<u8>),
  Avif(AvifData<'static>),
}

unsafe impl Send for EncodeOutput {}

#[napi]
impl Task for EncodeTask {
  type Output = EncodeOutput;
  type JsValue = JsBuffer;

  fn compute(&mut self) -> Result<Self::Output> {
    let meta = self.image.get(self.rotate)?;
    let orientation = meta.orientation;
    if self.rotate {
      if let Some(orientation) = orientation {
        match orientation {
          1 => {}
          2 => meta.image = meta.image.fliph(),
          3 => meta.image = meta.image.rotate180(),
          4 => meta.image = meta.image.flipv(),
          5 => meta.image = meta.image.fliph().rotate270(),
          6 => meta.image = meta.image.rotate270(),
          7 => meta.image = meta.image.flipv().rotate270(),
          8 => meta.image = meta.image.rotate90(),
          _ => {
            return Err(Error::new(
              Status::InvalidArg,
              format!("Unsupported orientation value {}", orientation),
            ))
          }
        }
      }
    }
    let raw_width = meta.image.width();
    let raw_height = meta.image.height();
    match self.resize {
      (Some(w), Some(h)) => {
        meta.image = meta
          .image
          .resize(w, h, image::imageops::FilterType::Lanczos3)
      }
      (Some(w), None) => {
        meta.image = meta.image.resize(
          w,
          ((w as f32 / raw_width as f32) * (raw_height as f32)) as u32,
          image::imageops::FilterType::Lanczos3,
        )
      }
      _ => {}
    }
    let dynamic_image = &mut meta.image;
    let color_type = &meta.color_type;
    let format = match self.options {
      EncodeOptions::Webp(quality_factor) => {
        let (output_buf, size) = unsafe {
          crate::webp::encode_webp_inner(
            dynamic_image.as_bytes(),
            quality_factor,
            dynamic_image.width(),
            dynamic_image.height(),
            color_type,
          )
        }?;
        return Ok(EncodeOutput::Raw(output_buf, size));
      }
      EncodeOptions::WebpLossless => {
        let (output_buf, size) = unsafe {
          crate::webp::lossless_encode_webp_inner(
            dynamic_image.as_bytes(),
            dynamic_image.width(),
            dynamic_image.height(),
            color_type,
          )
        }?;
        return Ok(EncodeOutput::Raw(output_buf, size));
      }
      EncodeOptions::Avif(ref options) => {
        let output = encode_avif_inner(options.clone(), dynamic_image)?;
        return Ok(EncodeOutput::Avif(output));
      }
      EncodeOptions::Bmp => ImageFormat::Bmp,
      EncodeOptions::Ico => ImageFormat::Ico,
      EncodeOptions::Tiff => ImageFormat::Tiff,
      EncodeOptions::Pnm => ImageFormat::Pnm,
      EncodeOptions::Tga => ImageFormat::Tga,
      EncodeOptions::Farbfeld => ImageFormat::Farbfeld,
    };
    let mut output: Cursor<Vec<u8>> = Cursor::new(Vec::with_capacity(
      (dynamic_image.width() * dynamic_image.height() * 4) as usize,
    ));
    dynamic_image
      .write_to(&mut output, format.clone())
      .map_err(|err| {
        Error::new(
          Status::InvalidArg,
          format!("Encode to [{:?}] error {}", &format, err),
        )
      })?;
    Ok(EncodeOutput::Buffer(output.into_inner()))
  }

  fn resolve(&mut self, env: Env, output: Self::Output) -> Result<Self::JsValue> {
    match output {
      EncodeOutput::Raw(buf, size) => unsafe {
        env
          .create_buffer_with_borrowed_data(buf, size, buf, move |buf_ptr, _env| {
            Vec::from_raw_parts(buf_ptr, size, size);
          })
          .map(|v| v.into_raw())
      },
      EncodeOutput::Buffer(buf) => env.create_buffer_with_data(buf).map(|b| b.into_raw()),
      EncodeOutput::Avif(avif_data) => {
        let len = avif_data.len();
        let data_ptr = avif_data.as_slice().as_ptr();
        unsafe {
          env.create_buffer_with_borrowed_data(data_ptr, len, avif_data, |data, _env| drop(data))
        }
        .map(|b| b.into_raw())
      }
    }
  }
}

#[napi]
pub struct Transformer {
  dynamic_image: Arc<ThreadSafeDynamicImage>,
  rotate: bool,
  resize: (Option<u32>, Option<u32>),
}

#[napi]
impl Transformer {
  #[napi(constructor)]
  pub fn new(input: Buffer) -> Result<Transformer> {
    Ok(Self {
      dynamic_image: Arc::new(ThreadSafeDynamicImage::new(input)),
      rotate: false,
      resize: (None, None),
    })
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
  /// Rotate with exif orientation
  pub fn rotate(&mut self) -> &Self {
    self.rotate = true;
    self
  }

  #[napi]
  pub fn resize(&mut self, width: Option<u32>, height: Option<u32>) -> &Self {
    self.resize = (width, height);
    self
  }

  #[napi]
  /// The quality factor `quality_factor` ranges from 0 to 100 and controls the loss and quality during compression.
  /// The value 0 corresponds to low quality and small output sizes, whereas 100 is the highest quality and largest output size.
  /// https://developers.google.com/speed/webp/docs/api#simple_encoding_api
  pub fn webp(
    &mut self,
    quality_factor: u32,
    signal: Option<AbortSignal>,
  ) -> AsyncTask<EncodeTask> {
    AsyncTask::with_optional_signal(
      EncodeTask {
        image: self.dynamic_image.clone(),
        options: EncodeOptions::Webp(quality_factor),
        rotate: self.rotate,
        resize: self.resize,
      },
      signal,
    )
  }

  #[napi]
  /// The quality factor `quality_factor` ranges from 0 to 100 and controls the loss and quality during compression.
  /// The value 0 corresponds to low quality and small output sizes, whereas 100 is the highest quality and largest output size.
  /// https://developers.google.com/speed/webp/docs/api#simple_encoding_api
  pub fn webp_sync(&mut self, env: Env, quality_factor: u32) -> Result<JsBuffer> {
    let mut encoder = EncodeTask {
      image: self.dynamic_image.clone(),
      options: EncodeOptions::Webp(quality_factor),
      rotate: self.rotate,
      resize: self.resize,
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
        rotate: self.rotate,
        resize: self.resize,
      },
      signal,
    )
  }

  #[napi]
  pub fn webp_lossless_sync(&mut self, env: Env) -> Result<JsBuffer> {
    let mut encoder = EncodeTask {
      image: self.dynamic_image.clone(),
      options: EncodeOptions::WebpLossless,
      rotate: self.rotate,
      resize: self.resize,
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
        rotate: self.rotate,
        resize: self.resize,
      },
      signal,
    )
  }

  #[napi]
  pub fn avif_sync(&mut self, env: Env, options: Option<AvifConfig>) -> Result<JsBuffer> {
    let mut encoder = EncodeTask {
      image: self.dynamic_image.clone(),
      options: EncodeOptions::Avif(options),
      rotate: self.rotate,
      resize: self.resize,
    };
    let output = encoder.compute()?;
    encoder.resolve(env, output)
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
            rexif::TagValue::U16(v) => v.get(0).map(|v| *v),
            _ => None,
          });
        return Some((exif, orientation));
      }
    }
    _ => {}
  }
  None
}
