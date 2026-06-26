use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use image::imageops::overlay;
use image::{
  ColorType, DynamicImage, ImageBuffer, ImageEncoder, ImageFormat, Pixel, Rgba, Rgba32FImage,
  RgbaImage, imageops::FilterType,
};
use libavif::AvifData;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use resvg::{
  tiny_skia,
  usvg::{self, Options, fontdb::Database},
};

use crate::{
  avif::{AvifConfig, encode_avif_inner},
  fast_resize::{FastResizeOptions, ResizeFit, fast_resize},
  heic::HeicConfig,
  png::PngEncodeOptions,
};

static FONT_DB: once_cell::sync::OnceCell<Arc<Database>> = once_cell::sync::OnceCell::new();

pub enum EncodeOptions {
  Png(PngEncodeOptions),
  Jpeg(u32),
  Webp(u32),
  WebpLossless,
  Avif(Option<AvifConfig>),
  Heic(Option<HeicConfig>),
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
#[derive(Clone, Copy, Default, PartialEq, Eq)]
/// Compositing / blending operator for `Transformer.composite`. Mirrors sharp/libvips
/// blend modes: Porter-Duff operators plus the W3C separable blend modes.
pub enum BlendMode {
  /// Source-over (default) — the overlay is drawn on top of the base.
  #[default]
  Over,
  /// Clear — neither source nor destination is shown.
  Clear,
  /// Source — only the overlay is shown.
  Source,
  /// Source-in — the overlay clipped to the base's shape.
  In,
  /// Source-out — the overlay where the base is transparent.
  Out,
  /// Source-atop — the overlay drawn only where the base is opaque.
  Atop,
  /// Destination — only the base is shown (overlay ignored).
  Dest,
  /// Destination-over — the base drawn on top of the overlay.
  DestOver,
  /// Destination-in — the base clipped to the overlay's shape.
  DestIn,
  /// Destination-out — the base where the overlay is transparent.
  DestOut,
  /// Destination-atop — the base drawn only where the overlay is opaque.
  DestAtop,
  /// Exclusive-or of the two coverage regions.
  Xor,
  /// Additive blending.
  Add,
  /// Saturating additive blending.
  Saturate,
  /// Multiply the channels.
  Multiply,
  /// Screen (inverse-multiply) the channels.
  Screen,
  /// Overlay (multiply or screen depending on the backdrop).
  Overlay,
  /// Keep the darker of the two channels.
  Darken,
  /// Keep the lighter of the two channels.
  Lighten,
  /// Brighten the backdrop to reflect the source.
  ColorDodge,
  /// Darken the backdrop to reflect the source.
  ColorBurn,
  /// Hard light (overlay with swapped operands).
  HardLight,
  /// Soft light.
  SoftLight,
  /// Absolute difference of the channels.
  Difference,
  /// Like difference but with lower contrast.
  Exclusion,
}

#[napi]
#[derive(Clone, Copy, Default)]
/// Where to anchor the overlay relative to the base image when no explicit
/// `left`/`top` is given.
pub enum Gravity {
  /// Center of the base image.
  #[default]
  Center,
  /// Top edge, horizontally centered.
  North,
  /// Top-right corner.
  NorthEast,
  /// Right edge, vertically centered.
  East,
  /// Bottom-right corner.
  SouthEast,
  /// Bottom edge, horizontally centered.
  South,
  /// Bottom-left corner.
  SouthWest,
  /// Left edge, vertically centered.
  West,
  /// Top-left corner.
  NorthWest,
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

/// A decoded image's format. Wraps the `image` crate's `#[non_exhaustive]` `ImageFormat`
/// (which has no HEIC variant) so HEIC can be represented alongside the standard formats.
#[derive(Clone, Copy)]
pub enum DetectedFormat {
  Standard(image::ImageFormat),
  Heic,
  Svg,
}

impl DetectedFormat {
  /// The underlying `image` crate format, if any. `None` for HEIC (decoded outside the
  /// `image` crate, and not parseable by `rexif`).
  pub(crate) fn image_format(&self) -> Option<image::ImageFormat> {
    match self {
      DetectedFormat::Standard(f) => Some(*f),
      DetectedFormat::Heic => None,
      DetectedFormat::Svg => None,
    }
  }

  /// Lowercase format name for `Metadata.format` (must match the previous output for
  /// existing formats, e.g. `Standard(Png)` → "png"; `Heic` → "heic").
  pub(crate) fn as_str(&self) -> String {
    match self {
      DetectedFormat::Standard(f) => format!("{f:?}").to_lowercase(),
      DetectedFormat::Heic => "heic".to_owned(),
      DetectedFormat::Svg => "svg".to_owned(),
    }
  }
}

pub(crate) struct ImageMetaData {
  pub(crate) image: DynamicImage,
  pub(crate) color_type: ColorType,
  pub(crate) exif: HashMap<String, String>,
  pub(crate) orientation: Option<u16>,
  pub(crate) format: DetectedFormat,
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

  #[allow(clippy::mut_from_ref)]
  pub(crate) fn get(&self, with_exif: bool) -> Result<&mut ImageMetaData> {
    let image = Box::leak(unsafe { Box::from_raw(self.image) });
    let mut exif = HashMap::new();
    let mut orientation = None;
    match image {
      None => {
        let input_buf = self.raw.as_ref();
        // Sniff HEIC first: it shares the ISOBMFF container with AVIF but `image`'s
        // `guess_format` can't tell them apart and has no HEIC variant. A HEIC input is
        // routed to the OS decoder (`decode_heic`); everything else keeps the existing path.
        let (dynamic_image, detected_format, heic_orientation) = if crate::heic::is_heic(input_buf)
        {
          let (img, orient) = crate::heic::decode_heic(input_buf)?;
          (img, DetectedFormat::Heic, orient)
        } else {
          let image_format = image::guess_format(input_buf).map_err(|err| {
            Error::new(
              Status::InvalidArg,
              format!("Guess format from input image failed {err}"),
            )
          })?;
          let img = if image_format == ImageFormat::Avif {
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
          (img, DetectedFormat::Standard(image_format), None)
        };

        // HEIC orientation comes from ImageIO (Task 4); store it unconditionally so a later
        // `get(true)`/`.rotate()` keeps it even if the first `get()` had `with_exif = false`.
        if let Some(o) = heic_orientation {
          orientation = Some(o);
        }
        // rexif EXIF only applies to image-crate formats (Jpeg/Tiff); skip for HEIC, whose
        // `image_format()` is `None`.
        if with_exif
          && let Some(fmt) = detected_format.image_format()
          && let Some((_exif, _orientation)) = parse_exif(input_buf, &fmt)
        {
          exif = _exif;
          orientation = _orientation;
        }
        let color_type = dynamic_image.color();
        image.replace(ImageMetaData {
          image: dynamic_image,
          exif,
          orientation,
          format: detected_format,
          // Only mark EXIF as parsed when we actually attempted it. Otherwise a later
          // `get(true)` (e.g. from `.rotate()`) would see `has_parsed_exif == true`,
          // skip parsing, and silently drop the orientation. See issue #199.
          has_parsed_exif: with_exif,
          color_type,
        });
        Ok(image.as_mut().unwrap())
      }
      Some(res) => {
        // `parse_exif` only applies to image-crate formats; HEIC has no underlying
        // `ImageFormat`, so skip it (and don't flip `has_parsed_exif`).
        if !res.has_parsed_exif
          && with_exif
          && let Some(fmt) = res.format.image_format()
        {
          if let Some((exif, orientation)) = parse_exif(self.raw.as_ref(), &fmt) {
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
  image_transform_args: ImageTransformArgs,
}

#[napi]
impl Task for MetadataTask {
  type Output = (
    u32,
    u32,
    HashMap<String, String>,
    Option<u16>,
    DetectedFormat,
    ColorType,
  );
  type JsValue = Metadata;

  fn compute(&mut self) -> Result<Self::Output> {
    // Parse EXIF when explicitly requested OR when a rotate is pending (so the
    // orientation-aware dimensions below are correct), mirroring `EncodeTask`.
    let meta = self
      .dynamic_image
      .get(self.with_exif || self.image_transform_args.rotate)?;
    let (width, height, color_type) = if self.image_transform_args.changes_dimensions_or_color() {
      // Compute on a CLONE so the shared, cached `DynamicImage` is never mutated;
      // a later encode of the same `Transformer` must still apply transforms once.
      let mut image = meta.image.clone();
      apply_transforms(
        &mut image,
        &self.image_transform_args,
        meta.orientation,
        false,
      )?;
      (image.width(), image.height(), image.color())
    } else {
      (meta.image.width(), meta.image.height(), meta.color_type)
    };
    // Decide the RETURNED EXIF/orientation (#158). Two concerns: (1) a pending
    // rotate forced us to parse EXIF above for swapped dims, but a
    // `with_exif=false` caller never requested it, so never leak it; (2) when a
    // rotate is staged it bakes the orientation into the previewed pixels, so the
    // returned orientation must be normalized (see the rotation branch below).
    let rotation_applied =
      self.image_transform_args.rotate || self.image_transform_args.orientation.is_some();
    let (exif, orientation) = if rotation_applied {
      // A staged rotate bakes EXIF orientation into the pixels (the previewed dims
      // are already upright), so the encoded output carries no orientation —
      // report it normalized and drop the now-stale `Orientation` key, matching
      // sharp autoOrient / ImageMagick -auto-orient / Pillow exif_transpose. Keep
      // the rest of the source EXIF only when the caller asked for it.
      let exif = if self.with_exif {
        let mut e = meta.exif.clone();
        e.remove("Orientation");
        e
      } else {
        HashMap::new()
      };
      (exif, None)
    } else if self.with_exif {
      (meta.exif.clone(), meta.orientation)
    } else {
      // with_exif=false, no rotate: suppress rexif EXIF/orientation; HEIC
      // orientation comes from the decoder (not rexif) and was always surfaced on
      // main — preserve it.
      let orientation = match meta.format {
        DetectedFormat::Heic => meta.orientation,
        _ => None,
      };
      (HashMap::new(), orientation)
    };
    Ok((width, height, exif, orientation, meta.format, color_type))
  }

  fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
    Ok(Metadata {
      width: output.0,
      height: output.1,
      exif: (!output.2.is_empty()).then_some(output.2),
      orientation: output.3.map(|o| o as u32),
      format: output.4.as_str(),
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

#[napi(object)]
#[derive(Clone, Default)]
pub struct CompositeOptions {
  /// Pixel offset from the top edge. Provide both `top` and `left` together;
  /// supplying only one is an error. When set, takes precedence over `gravity`.
  pub top: Option<i64>,
  /// Pixel offset from the left edge. Provide both `left` and `top` together;
  /// supplying only one is an error. When set, takes precedence over `gravity`.
  pub left: Option<i64>,
  /// Anchor position. Defaults to `Center`; used only when neither `left` nor
  /// `top` is set.
  pub gravity: Option<Gravity>,
  /// Blend / compositing operator. Defaults to `Over` (source-over).
  pub blend: Option<BlendMode>,
  /// Repeat the overlay to tile across the whole base. Ignores `left`/`top`/`gravity`.
  pub tile: Option<bool>,
  /// Multiply the overlay's alpha by this factor (0.0..=1.0). Fades the OVERLAY
  /// (distinct from `Transformer.opacity`, which fades the base).
  pub opacity: Option<f64>,
}

/// A single staged composite/overlay operation. `overlay()` and `composite()` both push one of
/// these; the legacy `overlay()` produces the source-over, no-tile, full-opacity shape that
/// `compute()` routes through the unchanged fast 8-bit path.
#[derive(Clone)]
struct CompositeItem {
  buffer: Arc<Uint8Array>,
  left: i64,
  top: i64,
  has_offset: bool, // both left & top were given (or legacy overlay())
  gravity: Gravity, // used only when !has_offset; defaults to Center
  blend: BlendMode,
  tile: bool,
  opacity: f32, // 1.0 == no-op
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
  overlay: Vec<CompositeItem>,
  /// Multiply the alpha channel by this factor (0.0..=1.0). Promotes the image to RGBA8.
  opacity: Option<f32>,
}

impl ImageTransformArgs {
  /// Whether any staged transform changes the encoded image's dimensions or
  /// color type. Pure value-filters (invert/contrast/blur/unsharpen/filter3x3/
  /// brighten/huerotate) and the in-place overlay do not, so `metadata()` can
  /// skip cloning + applying them. `opacity` is included: it promotes the image
  /// to RGBA8, so `metadata().colorType` must reflect it (like `grayscale`).
  fn changes_dimensions_or_color(&self) -> bool {
    self.rotate
      || self.orientation.is_some()
      || self.resize.is_some()
      || self.fast_resize.is_some()
      || self.grayscale
      || self.crop.is_some()
      || self.opacity.is_some()
  }

  /// No staged transform — the encode pipeline only reads the image, so it can borrow the
  /// cached decode instead of cloning it.
  fn is_noop(&self) -> bool {
    !self.rotate
      && self.orientation.is_none()
      && self.resize.is_none()
      && self.fast_resize.is_none()
      && !self.grayscale
      && !self.invert
      && self.contrast.is_none()
      && self.blur.is_none()
      && self.unsharpen.is_none()
      && self.filter3x3.is_none()
      && self.brightness.is_none()
      && self.huerotate.is_none()
      && self.crop.is_none()
      && self.overlay.is_empty()
      && self.opacity.is_none()
  }
}

/// Apply the staged pipeline to `image` in pipeline order:
/// rotate/orientation -> resize -> fast_resize -> grayscale ->
/// [value filters, encode-only] -> crop. The overlay step is NOT handled here
/// (it needs the surrounding task's lifetime/cache); callers apply it after.
///
/// `for_encode == false` (metadata) skips the pure value-filters, which never
/// change dimensions or color type.
fn apply_transforms(
  image: &mut DynamicImage,
  args: &ImageTransformArgs,
  base_orientation: Option<u16>,
  for_encode: bool,
) -> Result<()> {
  let orientation = args
    .orientation
    .map(Ok)
    .or_else(|| base_orientation.map(|o| o.try_into()));
  if (args.rotate || args.orientation.is_some())
    && let Some(orientation) = orientation
  {
    match orientation? {
      Orientation::Horizontal => {}
      Orientation::MirrorHorizontal => *image = image.fliph(),
      Orientation::Rotate180 => *image = image.rotate180(),
      Orientation::MirrorVertical => *image = image.flipv(),
      Orientation::MirrorHorizontalAndRotate270Cw => *image = image.fliph().rotate270(),
      Orientation::Rotate90Cw => *image = image.rotate90(),
      Orientation::MirrorHorizontalAndRotate90Cw => *image = image.flipv().rotate270(),
      Orientation::Rotate270Cw => *image = image.rotate270(),
    }
  }
  let raw_width = image.width();
  let raw_height = image.height();
  if let Some(ResizeOptions {
    width,
    height,
    filter,
    fit,
  }) = args.resize
  {
    match fit.unwrap_or_default() {
      // the `resize_to_fill` is behavior like cover
      ResizeFit::Cover => {
        *image = image.resize_to_fill(
          width,
          height.unwrap_or(((width as f32 / raw_width as f32) * (raw_height as f32)) as u32),
          filter.unwrap_or_default().into(),
        )
      }
      ResizeFit::Fill => {
        *image = image.resize_exact(
          width,
          height.unwrap_or(((width as f32 / raw_width as f32) * (raw_height as f32)) as u32),
          filter.unwrap_or_default().into(),
        )
      }
      ResizeFit::Inside => {
        *image = image.resize(
          width,
          height.unwrap_or(((width as f32 / raw_width as f32) * (raw_height as f32)) as u32),
          filter.unwrap_or_default().into(),
        )
      }
    }
  }
  if let Some(options) = args.fast_resize {
    let resized_image = fast_resize(&*image, options)?;
    *image = DynamicImage::ImageRgba8(
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

  if args.grayscale {
    *image = image.grayscale();
  }
  if for_encode {
    // Alpha invariant (#42): the value/color filters must leave alpha untouched so opacity
    // scales the real transparency, while the SPATIAL filters (`blur`/`unsharpen`/
    // `filter3x3`) feather alpha on purpose. `invert` (passes `rgba[3]` through) and
    // `brighten` (`map_with_alpha(|a| a)`) already do; `apply_contrast` and
    // `apply_huerotate` are depth-aware, alpha-preserving re-implementations of the crate
    // filters (which otherwise scale/crush alpha as a side effect). Nothing here needs an
    // alpha snapshot.
    if args.invert {
      image.invert();
    }
    if let Some(contrast) = args.contrast {
      apply_contrast(image, contrast);
    }
    if let Some(blur) = args.blur {
      *image = image.blur(blur);
    }
    if let Some((sigma, threshold)) = args.unsharpen {
      *image = image.unsharpen(sigma, threshold);
    }
    if let Some(filter) = args.filter3x3 {
      *image = image.filter3x3(filter.as_ref());
    }
    if let Some(brighten) = args.brightness {
      *image = image.brighten(brighten);
    }
    if let Some(hue) = args.huerotate {
      apply_huerotate(image, hue);
    }
  }
  // Multiply the alpha channel (issue #42), applied AFTER the value filters. The value
  // filters leave alpha untouched and the spatial filters feather it on purpose, so the
  // CURRENT alpha is exactly what opacity should scale. Unconditional (not gated on
  // `for_encode`) so a pending opacity is reflected in `metadata().colorType`.
  if let Some(factor) = args.opacity {
    apply_opacity(image, factor);
  }
  if let Some((x, y, width, height)) = args.crop {
    *image = image.crop_imm(x, y, width, height);
  }
  Ok(())
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
  pub(crate) fn into_buffer_slice<'env>(self, env: &'env Env) -> Result<BufferSlice<'env>> {
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
    // Only clone when the pipeline will mutate the pixels. A plain encode with nothing staged
    // borrows the cached decode read-only — no memory doubling (PR #218). When transforms/overlay
    // ARE staged we clone so the shared cache stays pristine and reuse stays idempotent (#158, Task 4).
    let owned;
    let dynamic_image: &DynamicImage = if self.image_transform_args.is_noop() {
      &meta.image
    } else {
      let mut img = meta.image.clone();
      apply_transforms(&mut img, &self.image_transform_args, meta.orientation, true)?;
      for item in std::mem::take(&mut self.image_transform_args.overlay).into_iter() {
        let top = ThreadsafeDynamicImage::new(item.buffer.clone());
        let top_image_meta = top.get(true)?;
        let (x, y) = resolve_position(
          item.has_offset,
          item.left,
          item.top,
          item.gravity,
          img.width(),
          img.height(),
          top_image_meta.image.width(),
          top_image_meta.image.height(),
        );
        // Source-over with no tiling / full opacity keeps the fast 8-bit path (unchanged
        // behaviour, byte-identical to `overlay`). Everything else uses the precise compositor.
        if item.blend == BlendMode::Over && !item.tile && item.opacity == 1.0 {
          overlay(&mut img, &top_image_meta.image, x, y);
        } else {
          apply_composite(
            &mut img,
            &top_image_meta.image,
            x,
            y,
            item.blend,
            item.opacity,
            item.tile,
          );
        }
      }
      owned = img;
      &owned
    };
    let width = dynamic_image.width();
    let height = dynamic_image.height();
    let format = match self.options {
      EncodeOptions::Webp(quality_factor) => {
        let (output_buf, size) =
          unsafe { crate::webp::encode_webp_inner(dynamic_image, quality_factor, width, height) }?;
        return Ok(EncodeOutput::Raw(output_buf, size));
      }
      EncodeOptions::WebpLossless => {
        let (output_buf, size) =
          unsafe { crate::webp::lossless_encode_webp_inner(dynamic_image, width, height) }?;
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
      EncodeOptions::Heic(ref options) => {
        let buf = crate::heic::encode_heic(dynamic_image, options.clone())?;
        return Ok(EncodeOutput::Buffer(buf));
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

fn transformer_from_rgba8(image: RgbaImage, format: DetectedFormat) -> Transformer {
  let image_meta = Box::new(Some(ImageMetaData {
    color_type: ColorType::Rgba8,
    orientation: None,
    image: DynamicImage::ImageRgba8(image),
    exif: HashMap::new(),
    format,
    has_parsed_exif: true,
  }));
  Transformer {
    dynamic_image: Arc::new(ThreadsafeDynamicImage {
      raw: Arc::new(vec![0].into()),
      image: Box::into_raw(image_meta),
    }),
    image_transform_args: Default::default(),
  }
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
    let svg_width = tree.size().width();
    let svg_height = tree.size().height();
    // (usvg's `Size` is `NonZeroPositiveF32`, so both are > 0 and finite here.)
    const MIN_SVG_SIZE: f32 = 1000.0;
    // Upper bound on the rasterized SVG area (~1 GiB of RGBA). Bounds memory for adversarial or
    // degenerate intrinsic SVG sizes; tune if you need larger native-size SVG rasters.
    const MAX_SVG_PIXELS: u64 = 1 << 28; // 268_435_456 px
    // Smallest uniform power-of-two scale whose ROUNDED longer axis reaches `MIN_SVG_SIZE`, for
    // resize quality. Thresholding on the rounded dimension (not the raw float) avoids needlessly
    // doubling a value like 999.6 -> 1000. Targeting only the longer axis keeps a thin, high-aspect
    // SVG (e.g. 1x2000) from exploding into a huge raster. `scale` stays f32 (the render transform
    // is f32); for an intrinsic size so tiny that `scale` would overflow f32, the loop exits at +inf
    // and the size is rejected below rather than rendered with a broken transform.
    let mut scale = 1.0_f32;
    while scale.is_finite()
      && (svg_width * scale).round() < MIN_SVG_SIZE
      && (svg_height * scale).round() < MIN_SVG_SIZE
    {
      scale *= 2.0;
    }
    // Final integer dimensions, computed in f64 so an f32-overflowed scale or an astronomically large
    // axis is caught here rather than silently saturating an `as u32` cast.
    let target_width = (svg_width as f64 * scale as f64).round();
    let target_height = (svg_height as f64 * scale as f64).round();
    // Reject degenerate sizes instead of corrupting output: a non-finite product, a sub-pixel axis
    // that rounds to 0 (rendering it would be an invisible blank), a dimension past u32, or an area
    // over the budget (tiny-skia's `Pixmap::new` bounds only row width, not total pixels, on 64-bit,
    // so a huge size aborts the process). NB: 0.5 rounds to 1 and still renders; 0.1 rounds to 0 and
    // is rejected.
    if !(target_width.is_finite() && target_height.is_finite())
      || target_width < 1.0
      || target_height < 1.0
      || target_width > u32::MAX as f64
      || target_height > u32::MAX as f64
      || (target_width as u64).saturating_mul(target_height as u64) > MAX_SVG_PIXELS
    {
      return Err(Error::from_reason(format!(
        "SVG raster size out of range: source {svg_width}x{svg_height} scaled by {scale}"
      )));
    }
    let target_width = target_width as u32;
    let target_height = target_height as u32;
    let mut pix_map = tiny_skia::Pixmap::new(target_width, target_height).ok_or_else(|| {
      Error::from_reason(format!(
        "Failed to rasterize SVG at {target_width}x{target_height}"
      ))
    })?;

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
      tiny_skia::Transform::from_scale(scale, scale),
      &mut pix_map.as_mut(),
    );

    let width = pix_map.width();
    let height = pix_map.height();
    // tiny_skia stores premultiplied RGBA; demultiply to straight RGBA before treating the buffer as
    // an `RgbaImage`, otherwise semi-transparent pixels (rgba backgrounds and antialiased edges) are
    // darkened. `take_demultiplied` still returns the owned buffer, so the handoff stays copy-free.
    let data = pix_map.take_demultiplied();
    let image = RgbaImage::from_vec(width, height, data).ok_or_else(|| {
      Error::new(
        Status::InvalidArg,
        "Rendered SVG pixel buffer does not match its dimensions".to_owned(),
      )
    })?;
    Ok(transformer_from_rgba8(image, DetectedFormat::Svg))
  }

  #[napi]
  pub fn from_rgba_pixels(
    input: Either<&[u8], Uint8ClampedSlice>,
    width: u32,
    height: u32,
  ) -> Result<Transformer> {
    if let Some(image) = RgbaImage::from_vec(
      width,
      height,
      match input {
        Either::A(a) => a.to_vec(),
        Either::B(b) => b.to_vec(),
      },
    ) {
      Ok(transformer_from_rgba8(
        image,
        DetectedFormat::Standard(ImageFormat::Png),
      ))
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
        image_transform_args: self.image_transform_args.clone(),
      },
      signal,
    )
  }

  #[napi]
  pub fn metadata_sync(&mut self, env: Env, with_exif: Option<bool>) -> Result<Metadata> {
    let mut task = MetadataTask {
      dynamic_image: self.dynamic_image.clone(),
      with_exif: with_exif.unwrap_or(false),
      image_transform_args: self.image_transform_args.clone(),
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
  /// Multiply the image's alpha channel by `factor` (clamped to `0.0..=1.0`),
  /// like CSS `opacity`. `1.0` leaves the image unchanged; `0.0` makes it fully
  /// transparent. Existing transparency is preserved (`new = old * factor`), and
  /// the image is promoted to an alpha-capable type while keeping its bit depth
  /// (RGBA8 / RGBA16 / RGBA32F). Out-of-range float (HDR) alpha is normalized into
  /// `0.0..=1.0` before the factor is applied, so a requested fade is always effective.
  ///
  /// Like every other filter, this applies to *this* image's content; a later
  /// `overlay` is composited on top afterward. To fade an image you are dropping
  /// onto another, call `opacity` on that (top) image first, then pass it to the
  /// bottom image's `overlay`. Handy for fade-out animation frames.
  pub fn opacity(&mut self, factor: f64) -> &Self {
    self.image_transform_args.opacity = Some(factor as f32);
    self
  }

  #[napi]
  /// Crop a cut-out of this image delimited by the bounding rectangle.
  pub fn crop(&mut self, x: u32, y: u32, width: u32, height: u32) -> &Self {
    self.image_transform_args.crop = Some((x, y, width, height));
    self
  }

  #[napi]
  /// Overlay an image at a given coordinate (x, y) using source-over blending.
  pub fn overlay(&mut self, on_top: Uint8Array, x: i64, y: i64) -> Result<&Self> {
    self.image_transform_args.overlay.push(CompositeItem {
      buffer: Arc::new(on_top),
      left: x,
      top: y,
      has_offset: true,         // legacy overlay() is always an explicit offset
      gravity: Gravity::Center, // unused when has_offset
      blend: BlendMode::Over,
      tile: false,
      opacity: 1.0,
    });
    Ok(self)
  }

  #[napi]
  /// Composite `on_top` onto this image with a sharp-style blend mode, gravity-based
  /// positioning, tiling, and per-overlay opacity. See `CompositeOptions`.
  ///
  /// Placement (sharp parity): when neither `left` nor `top` is given the overlay is
  /// anchored by `gravity`, which defaults to the CENTRE of the base image. `left` and
  /// `top` must be provided together — supplying only one is an error.
  ///
  /// Source-over (`blend: Over`, no tiling, full opacity) composites at 8-bit, identical
  /// to `overlay`. Other blend modes / tiling / opacity < 1 run at the base image's native
  /// channel depth (8/16-bit, or 32-bit float), then the result is converted back to the
  /// base's original color type — so an opaque base never gains an alpha channel. On an
  /// opaque base, coverage-reducing modes (Clear/Out/DestOut/Xor) flatten the overlapped
  /// region toward black, since the removed alpha can't be stored without an alpha channel.
  pub fn composite(
    &mut self,
    on_top: Uint8Array,
    options: Option<CompositeOptions>,
  ) -> Result<&Self> {
    let o = options.unwrap_or_default();
    let has_offset = match (o.left, o.top) {
      (Some(_), Some(_)) => true,
      (None, None) => false,
      _ => {
        return Err(Error::new(
          Status::InvalidArg,
          "composite: `left` and `top` must be provided together".to_owned(),
        ));
      }
    };
    let opacity = o.opacity.unwrap_or(1.0) as f32;
    let opacity = if opacity.is_finite() {
      opacity.clamp(0.0, 1.0)
    } else {
      1.0
    };
    self.image_transform_args.overlay.push(CompositeItem {
      buffer: Arc::new(on_top),
      left: o.left.unwrap_or(0),
      top: o.top.unwrap_or(0),
      has_offset,
      gravity: o.gravity.unwrap_or_default(),
      blend: o.blend.unwrap_or_default(),
      tile: o.tile.unwrap_or(false),
      opacity,
    });
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
  pub fn raw_pixels_sync(&self, env: Env) -> Result<Buffer> {
    // Route through `EncodeTask` so staged transforms apply, exactly like every
    // other `*_sync` encoder (e.g. `webp_sync`/`png_sync`) and async
    // `raw_pixels` (#158, finding F2). `env` is injected by napi; the JS
    // `rawPixelsSync(): Buffer` signature is unchanged.
    let mut encoder = EncodeTask {
      image: self.dynamic_image.clone(),
      options: EncodeOptions::RawPixels,
      image_transform_args: self.image_transform_args.clone(),
    };
    let output = encoder.compute()?;
    encoder.resolve(env, output)
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
  /// Encode to HEIC via the OS-native HEVC encoder — Apple ImageIO on macOS, the Windows Imaging
  /// Component (WIC) on Windows. Ships no HEVC codec (the OS holds the patent license). Rejects on
  /// other platforms, and on Windows hosts lacking the OS HEVC/HEIF Store extension. See `HeicConfig`
  /// for the per-platform quality, bit-depth, and alpha behavior.
  pub fn heic(
    &mut self,
    options: Option<HeicConfig>,
    signal: Option<AbortSignal>,
  ) -> AsyncTask<EncodeTask> {
    AsyncTask::with_optional_signal(
      EncodeTask {
        image: self.dynamic_image.clone(),
        options: EncodeOptions::Heic(options),
        image_transform_args: self.image_transform_args.clone(),
      },
      signal,
    )
  }

  #[napi]
  /// Encode to HEIC via the OS-native HEVC encoder — Apple ImageIO on macOS, the Windows Imaging
  /// Component (WIC) on Windows. Ships no HEVC codec (the OS holds the patent license). Rejects on
  /// other platforms, and on Windows hosts lacking the OS HEVC/HEIF Store extension. See `HeicConfig`
  /// for the per-platform quality, bit-depth, and alpha behavior.
  pub fn heic_sync(&mut self, env: Env, options: Option<HeicConfig>) -> Result<Buffer> {
    let mut encoder = EncodeTask {
      image: self.dynamic_image.clone(),
      options: EncodeOptions::Heic(options),
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
/// Adjust the contrast of `image`'s color/luma channels by `contrast`, preserving bit
/// depth and the alpha channel.
///
/// `image`'s `adjust_contrast` maps EVERY channel with `pixel.map`, including alpha — so
/// it scales transparency as an unwanted side effect (contrast is a luma/color op). This
/// applies the identical per-channel curve `clamp(((c/max - 0.5) * percent + 0.5) * max)`
/// using the real per-depth `max` (so color output is byte-identical to the crate) but
/// via `map_with_alpha`, leaving the alpha channel untouched. Operates in place per native
/// type — no full-image copy. See issue #42.
fn apply_contrast(image: &mut DynamicImage, contrast: f32) {
  let percent = ((100.0 + contrast) / 100.0).powi(2);
  // The crate truncates (`NumCast`/`as`) after clamping, so we cast (not round) to match.
  let curve = move |c: f32, max: f32| (((c / max - 0.5) * percent + 0.5) * max).clamp(0.0, max);
  macro_rules! contrast_int {
    ($buf:expr, $ty:ty, $max:expr) => {{
      for pixel in $buf.pixels_mut() {
        *pixel = pixel.map_with_alpha(|c| curve(c as f32, $max) as $ty, |a| a);
      }
    }};
  }
  match image {
    DynamicImage::ImageLuma8(buf) => contrast_int!(buf, u8, 255.0),
    DynamicImage::ImageLumaA8(buf) => contrast_int!(buf, u8, 255.0),
    DynamicImage::ImageRgb8(buf) => contrast_int!(buf, u8, 255.0),
    DynamicImage::ImageRgba8(buf) => contrast_int!(buf, u8, 255.0),
    DynamicImage::ImageLuma16(buf) => contrast_int!(buf, u16, 65535.0),
    DynamicImage::ImageLumaA16(buf) => contrast_int!(buf, u16, 65535.0),
    DynamicImage::ImageRgb16(buf) => contrast_int!(buf, u16, 65535.0),
    DynamicImage::ImageRgba16(buf) => contrast_int!(buf, u16, 65535.0),
    DynamicImage::ImageRgb32F(buf) => {
      for pixel in buf.pixels_mut() {
        *pixel = pixel.map_with_alpha(|c| curve(c, 1.0), |a| a);
      }
    }
    DynamicImage::ImageRgba32F(buf) => {
      for pixel in buf.pixels_mut() {
        *pixel = pixel.map_with_alpha(|c| curve(c, 1.0), |a| a);
      }
    }
    _ => {}
  }
}

/// Hue-rotate `image` by `degrees`, preserving bit depth and alpha.
///
/// `image` 0.25's `DynamicImage::huerotate` clamps EVERY output channel against a
/// hardcoded `255`, which crushes >8-bit color to 8-bit and clamps alpha to 255 (it
/// also treats grayscale luma/alpha as RGB, emitting garbage). This re-implements the
/// same standard hue-rotation matrix (so 8-bit RGB/RGBA output stays byte-identical to
/// the crate) but clamps each color channel to its real per-depth max — 255 (`u8`),
/// 65535 (`u16`), 1.0 (`f32`) — and copies the alpha channel through untouched.
/// Grayscale has no hue, so rotating it leaves luma (and alpha) unchanged.
fn apply_huerotate(image: &mut DynamicImage, degrees: i32) {
  // 0 and 360 degrees are documented no-ops. Short-circuit so the identity rotation is
  // EXACTLY the input for every type (no float rounding, and NaN / HDR samples pass
  // through untouched instead of being run through the matrix).
  if degrees.rem_euclid(360) == 0 {
    return;
  }
  let angle = (degrees as f64).to_radians();
  let (cosv, sinv) = (angle.cos(), angle.sin());
  // Same coefficients as `image`'s `imageops::huerotate` / CSS `hue-rotate()`; each row
  // sums to 1.0, so a neutral gray maps to itself.
  let m = [
    0.213 + cosv * 0.787 - sinv * 0.213,
    0.715 - cosv * 0.715 - sinv * 0.715,
    0.072 - cosv * 0.072 + sinv * 0.928,
    0.213 - cosv * 0.213 + sinv * 0.143,
    0.715 + cosv * 0.285 + sinv * 0.140,
    0.072 - cosv * 0.072 - sinv * 0.283,
    0.213 - cosv * 0.213 - sinv * 0.787,
    0.715 - cosv * 0.715 + sinv * 0.715,
    0.072 + cosv * 0.928 + sinv * 0.072,
  ];
  let rotate = |r: f64, g: f64, b: f64| {
    (
      m[0] * r + m[1] * g + m[2] * b,
      m[3] * r + m[4] * g + m[5] * b,
      m[6] * r + m[7] * g + m[8] * b,
    )
  };
  // Lower-clamp a float channel to 0 (no negative light) while PRESERVING NaN: `NaN < 0.0`
  // is false, so NaN passes through instead of collapsing to 0 like `f32::max` would.
  // HDR magnitudes above 1.0 are kept.
  let clamp_lo = |v: f64| -> f32 {
    let v = v as f32;
    if v < 0.0 { 0.0 } else { v }
  };
  match image {
    // Integer RGB(A): truncate toward zero after clamping to the real max, matching the
    // crate's `NumCast::from` (`as` cast) so 8-bit output is identical.
    DynamicImage::ImageRgb8(buf) => {
      for p in buf.pixels_mut() {
        let (r, g, b) = rotate(p[0] as f64, p[1] as f64, p[2] as f64);
        p[0] = r.clamp(0.0, 255.0) as u8;
        p[1] = g.clamp(0.0, 255.0) as u8;
        p[2] = b.clamp(0.0, 255.0) as u8;
      }
    }
    DynamicImage::ImageRgba8(buf) => {
      for p in buf.pixels_mut() {
        let (r, g, b) = rotate(p[0] as f64, p[1] as f64, p[2] as f64);
        p[0] = r.clamp(0.0, 255.0) as u8;
        p[1] = g.clamp(0.0, 255.0) as u8;
        p[2] = b.clamp(0.0, 255.0) as u8;
        // p[3] (alpha) left untouched.
      }
    }
    DynamicImage::ImageRgb16(buf) => {
      for p in buf.pixels_mut() {
        let (r, g, b) = rotate(p[0] as f64, p[1] as f64, p[2] as f64);
        p[0] = r.clamp(0.0, 65535.0) as u16;
        p[1] = g.clamp(0.0, 65535.0) as u16;
        p[2] = b.clamp(0.0, 65535.0) as u16;
      }
    }
    DynamicImage::ImageRgba16(buf) => {
      for p in buf.pixels_mut() {
        let (r, g, b) = rotate(p[0] as f64, p[1] as f64, p[2] as f64);
        p[0] = r.clamp(0.0, 65535.0) as u16;
        p[1] = g.clamp(0.0, 65535.0) as u16;
        p[2] = b.clamp(0.0, 65535.0) as u16;
        // p[3] (alpha) left untouched.
      }
    }
    // Float channels are HDR: clamp only the lower bound to 0 (no negative light) and
    // preserve finite magnitudes above 1.0 instead of clipping them to SDR. `max(0.0)`
    // also sanitizes NaN to 0.
    DynamicImage::ImageRgb32F(buf) => {
      for p in buf.pixels_mut() {
        let (r, g, b) = rotate(p[0] as f64, p[1] as f64, p[2] as f64);
        p[0] = clamp_lo(r);
        p[1] = clamp_lo(g);
        p[2] = clamp_lo(b);
      }
    }
    DynamicImage::ImageRgba32F(buf) => {
      for p in buf.pixels_mut() {
        let (r, g, b) = rotate(p[0] as f64, p[1] as f64, p[2] as f64);
        p[0] = clamp_lo(r);
        p[1] = clamp_lo(g);
        p[2] = clamp_lo(b);
        // p[3] (alpha) left untouched.
      }
    }
    // Grayscale (Luma / LumaA, any depth): no hue to rotate — leave luma and alpha as-is.
    _ => {}
  }
}

/// Multiply the alpha channel by `factor`, preserving the source bit depth.
///
/// `factor` is clamped to `0.0..=1.0`; a non-finite `factor` (NaN / ∞) is treated
/// as `1.0` (identity) so bad input never silently zeroes the image. The image is
/// promoted to the alpha-capable type *of its own depth* (RGBA8 / RGBA16 / RGBA32F)
/// instead of always dropping to 8-bit, so 16-bit (e.g. 10-bit HEIC) and HDR float
/// sources keep their precision. See issue #42 and the bit-depth regression tests.
///
/// Scales the image's CURRENT alpha. The value filters (`apply_contrast`,
/// `apply_huerotate`, `invert`, `brighten`) leave alpha untouched and the spatial filters
/// (`blur`/`unsharpen`/`filter3x3`) feather alpha on purpose, so the current alpha is
/// always exactly what opacity should scale.
fn apply_opacity(image: &mut DynamicImage, factor: f32) {
  let factor = if factor.is_finite() {
    factor.clamp(0.0, 1.0)
  } else {
    1.0
  };
  match image.color() {
    ColorType::L16 | ColorType::La16 | ColorType::Rgb16 | ColorType::Rgba16 => {
      let mut buf = image.to_rgba16();
      for pixel in buf.pixels_mut() {
        pixel[3] = (pixel[3] as f32 * factor).round().clamp(0.0, 65535.0) as u16;
      }
      *image = DynamicImage::ImageRgba16(buf);
    }
    ColorType::Rgb32F | ColorType::Rgba32F => {
      let mut buf = image.to_rgba32f();
      for pixel in buf.pixels_mut() {
        // Normalize the SOURCE alpha into the unit range BEFORE applying the factor, so a
        // requested fade is always effective. Clamping the *product* instead would let an
        // out-of-range alpha swallow the fade (`4.0 * 0.5 = 2.0` -> clamp -> `1.0`, still
        // fully opaque). Alpha is normalized `[0,1]` at every depth — the 8/16-bit paths
        // can't exceed their max and the image crate's f32 `DEFAULT_MAX_VALUE` is 1.0. For
        // external Rgba32F data `clamp` floors `-∞`/negatives to 0.0 and saturates `+∞`/
        // values > 1 to 1.0; NaN is undefined under `clamp`, so sanitize it to 0.0. Both
        // the normalized alpha and the factor are in `[0,1]`, so the product needs no
        // further clamp.
        let a = pixel[3];
        let normalized = if a.is_nan() { 0.0 } else { a.clamp(0.0, 1.0) };
        pixel[3] = normalized * factor;
      }
      *image = DynamicImage::ImageRgba32F(buf);
    }
    _ => {
      let mut buf = image.to_rgba8();
      for pixel in buf.pixels_mut() {
        pixel[3] = (pixel[3] as f32 * factor).round().clamp(0.0, 255.0) as u8;
      }
      *image = DynamicImage::ImageRgba8(buf);
    }
  }
}

/// Resolve the top-left placement of an overlay (sharp semantics). When an explicit offset was
/// given (`has_offset`), `left`/`top` are used verbatim; otherwise the overlay is anchored by
/// `gravity` (default Center). Computed in i64; negative results are valid (the overlay is
/// clipped by the compositor).
fn resolve_position(
  has_offset: bool,
  left: i64,
  top: i64,
  gravity: Gravity,
  bw: u32,
  bh: u32,
  tw: u32,
  th: u32,
) -> (i64, i64) {
  if has_offset {
    return (left, top);
  }
  let (bw, bh, tw, th) = (bw as i64, bh as i64, tw as i64, th as i64);
  let cx = (bw - tw) / 2;
  let cy = (bh - th) / 2;
  let right = bw - tw;
  let bottom = bh - th;
  match gravity {
    Gravity::NorthWest => (0, 0),
    Gravity::North => (cx, 0),
    Gravity::NorthEast => (right, 0),
    Gravity::West => (0, cy),
    Gravity::Center => (cx, cy),
    Gravity::East => (right, cy),
    Gravity::SouthWest => (0, bottom),
    Gravity::South => (cx, bottom),
    Gravity::SouthEast => (right, bottom),
  }
}

// --- Blend math core (W3C "Compositing and Blending Level 1"). All channels are straight-alpha
// f32 in [0,1]; `cb` is the backdrop, `cs` the source. ---

fn color_dodge(cb: f32, cs: f32) -> f32 {
  if cb == 0.0 {
    0.0
  } else if cs == 1.0 {
    1.0
  } else {
    (cb / (1.0 - cs)).min(1.0)
  }
}

fn color_burn(cb: f32, cs: f32) -> f32 {
  if cb == 1.0 {
    1.0
  } else if cs == 0.0 {
    0.0
  } else {
    1.0 - ((1.0 - cb) / cs).min(1.0)
  }
}

fn hard_light(cb: f32, cs: f32) -> f32 {
  if cs <= 0.5 {
    2.0 * cb * cs
  } else {
    let d = 2.0 * cs - 1.0;
    cb + d - cb * d
  }
}

fn soft_light(cb: f32, cs: f32) -> f32 {
  let d = if cb <= 0.25 {
    ((16.0 * cb - 12.0) * cb + 4.0) * cb
  } else {
    cb.sqrt()
  };
  if cs <= 0.5 {
    cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
  } else {
    cb + (2.0 * cs - 1.0) * (d - cb)
  }
}

/// Separable blend `B(cb, cs)`. Returns `None` for Porter-Duff-only modes (identity = `cs`).
fn separable_blend(mode: BlendMode, cb: f32, cs: f32) -> Option<f32> {
  Some(match mode {
    BlendMode::Multiply => cb * cs,
    BlendMode::Screen => cb + cs - cb * cs,
    BlendMode::Overlay => hard_light(cs, cb), // NOTE the swapped args
    BlendMode::Darken => cb.min(cs),
    BlendMode::Lighten => cb.max(cs),
    BlendMode::ColorDodge => color_dodge(cb, cs),
    BlendMode::ColorBurn => color_burn(cb, cs),
    BlendMode::HardLight => hard_light(cb, cs),
    BlendMode::SoftLight => soft_light(cb, cs),
    BlendMode::Difference => (cb - cs).abs(),
    BlendMode::Exclusion => cb + cs - 2.0 * cb * cs,
    _ => return None,
  })
}

/// Porter-Duff coefficients `(Fa, Fb)`. Separable modes (and `Over`) use source-over.
fn pd_coeffs(mode: BlendMode, a_s: f32, ab: f32) -> (f32, f32) {
  match mode {
    BlendMode::Clear => (0.0, 0.0),
    BlendMode::Source => (1.0, 0.0),
    BlendMode::Dest => (0.0, 1.0),
    BlendMode::DestOver => (1.0 - ab, 1.0),
    BlendMode::In => (ab, 0.0),
    BlendMode::DestIn => (0.0, a_s),
    BlendMode::Out => (1.0 - ab, 0.0),
    BlendMode::DestOut => (0.0, 1.0 - a_s),
    BlendMode::Atop => (ab, 1.0 - a_s),
    BlendMode::DestAtop => (1.0 - ab, a_s),
    BlendMode::Xor => (1.0 - ab, 1.0 - a_s),
    BlendMode::Add => (1.0, 1.0),
    BlendMode::Saturate => (
      if a_s > 0.0 {
        (1.0 - ab).min(a_s) / a_s
      } else {
        0.0
      },
      1.0,
    ),
    // Over + all separable modes composite with source-over coefficients.
    _ => (1.0, 1.0 - a_s),
  }
}

/// Blend one source pixel over one backdrop pixel. All channels straight-alpha `[0,1]`.
fn blend_rgba_f32(cb: [f32; 4], cs: [f32; 4], mode: BlendMode, opacity: f32) -> [f32; 4] {
  let ab = cb[3];
  let a_s = (cs[3] * opacity).clamp(0.0, 1.0);
  let (fa, fb) = pd_coeffs(mode, a_s, ab);
  let ao = (a_s * fa + ab * fb).clamp(0.0, 1.0);
  let mut out = [0.0f32; 4];
  for c in 0..3 {
    let b = separable_blend(mode, cb[c], cs[c]).unwrap_or(cs[c]);
    let cs_blended = (1.0 - ab) * cs[c] + ab * b;
    let co = a_s * fa * cs_blended + ab * fb * cb[c]; // premultiplied
    out[c] = if ao > 0.0 {
      (co / ao).clamp(0.0, 1.0)
    } else {
      0.0
    };
  }
  out[3] = ao;
  out
}

/// Clamp/normalize an f32 channel into `[0,1]`, sanitizing NaN to 0.0 (mirrors `apply_opacity`'s
/// NaN handling). HDR values outside the unit range are clamped for blending.
#[inline]
fn norm_f32(v: f32) -> f32 {
  if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) }
}

/// True for modes whose output, when the source is fully transparent (`a_s == 0`), is NOT the
/// untouched backdrop (they clear / replace the overlapped region). For all other modes `a_s == 0`
/// is a destination-preserving no-op, so the pixel can be skipped to avoid clamping HDR / NaN
/// destinations in the f32 path.
fn clears_dest_when_source_transparent(mode: BlendMode) -> bool {
  matches!(
    mode,
    BlendMode::Clear
      | BlendMode::Source
      | BlendMode::In
      | BlendMode::Out
      | BlendMode::DestIn
      | BlendMode::DestAtop
  )
}

/// Overlap rectangle of `top` placed at `(x, y)` over a `bw x bh` base, clamped exactly like the
/// `image` crate's `overlay_bounds_ext` (handles negative offsets and tops larger than the base).
/// Returns `(origin_bottom_x, origin_bottom_y, origin_top_x, origin_top_y, x_range, y_range)`.
fn overlap_bounds(
  bw: u32,
  bh: u32,
  tw: u32,
  th: u32,
  x: i64,
  y: i64,
) -> (u32, u32, u32, u32, u32, u32) {
  if x > i64::from(bw)
    || y > i64::from(bh)
    || x.saturating_add(i64::from(tw)) <= 0
    || y.saturating_add(i64::from(th)) <= 0
  {
    return (0, 0, 0, 0, 0, 0);
  }
  let max_x = x.saturating_add(i64::from(tw));
  let max_y = y.saturating_add(i64::from(th));
  let max_inbounds_x = max_x.clamp(0, i64::from(bw)) as u32;
  let max_inbounds_y = max_y.clamp(0, i64::from(bh)) as u32;
  let origin_bottom_x = x.clamp(0, i64::from(bw)) as u32;
  let origin_bottom_y = y.clamp(0, i64::from(bh)) as u32;
  let x_range = max_inbounds_x - origin_bottom_x;
  let y_range = max_inbounds_y - origin_bottom_y;
  let origin_top_x = x.saturating_mul(-1).clamp(0, i64::from(tw)) as u32;
  let origin_top_y = y.saturating_mul(-1).clamp(0, i64::from(th)) as u32;
  (
    origin_bottom_x,
    origin_bottom_y,
    origin_top_x,
    origin_top_y,
    x_range,
    y_range,
  )
}

/// Invoke `f(ox, oy)` for each placement: once at `(x, y)` normally; for tiling, at every
/// top-left origin stepping by the overlay size across the whole base from `(0,0)` (ignoring x/y).
/// Streams the origins instead of materializing a `Vec` — a 1x1 tile over a large base would
/// otherwise allocate one tuple per pixel.
fn for_each_placement(
  tile: bool,
  bw: u32,
  bh: u32,
  tw: u32,
  th: u32,
  x: i64,
  y: i64,
  mut f: impl FnMut(i64, i64),
) {
  if !tile {
    f(x, y);
    return;
  }
  // Caller guards `tw > 0 && th > 0`; keep a defensive check so the loop always terminates.
  if tw == 0 || th == 0 {
    return;
  }
  let mut oy = 0u32;
  while oy < bh {
    let mut ox = 0u32;
    while ox < bw {
      f(ox as i64, oy as i64);
      ox += tw;
    }
    oy += th;
  }
}

/// Composite `top` onto an 8-bit RGBA `base` in place. Channels normalize `/255`, denormalize
/// `(v*255).round().clamp(0,255)` (the same rounding idiom as `apply_opacity`).
fn composite_into_u8(
  base: &mut RgbaImage,
  top: &RgbaImage,
  x: i64,
  y: i64,
  mode: BlendMode,
  opacity: f32,
  tile: bool,
) {
  let (bw, bh) = (base.width(), base.height());
  let (tw, th) = (top.width(), top.height());
  if tw == 0 || th == 0 {
    return;
  }
  for_each_placement(tile, bw, bh, tw, th, x, y, |ox, oy| {
    let (obx, oby, otx, oty, rw, rh) = overlap_bounds(bw, bh, tw, th, ox, oy);
    for ry in 0..rh {
      for rx in 0..rw {
        let s = top.get_pixel(otx + rx, oty + ry).0;
        let cs = [
          s[0] as f32 / 255.0,
          s[1] as f32 / 255.0,
          s[2] as f32 / 255.0,
          s[3] as f32 / 255.0,
        ];
        let d = base.get_pixel(obx + rx, oby + ry).0;
        let cb = [
          d[0] as f32 / 255.0,
          d[1] as f32 / 255.0,
          d[2] as f32 / 255.0,
          d[3] as f32 / 255.0,
        ];
        let out = blend_rgba_f32(cb, cs, mode, opacity);
        base.put_pixel(
          obx + rx,
          oby + ry,
          Rgba([
            (out[0] * 255.0).round().clamp(0.0, 255.0) as u8,
            (out[1] * 255.0).round().clamp(0.0, 255.0) as u8,
            (out[2] * 255.0).round().clamp(0.0, 255.0) as u8,
            (out[3] * 255.0).round().clamp(0.0, 255.0) as u8,
          ]),
        );
      }
    }
  });
}

/// Composite `top` onto a 16-bit RGBA `base` in place. Channels normalize `/65535`, denormalize
/// `(v*65535).round().clamp(0,65535)`.
fn composite_into_u16(
  base: &mut ImageBuffer<Rgba<u16>, Vec<u16>>,
  top: &ImageBuffer<Rgba<u16>, Vec<u16>>,
  x: i64,
  y: i64,
  mode: BlendMode,
  opacity: f32,
  tile: bool,
) {
  let (bw, bh) = (base.width(), base.height());
  let (tw, th) = (top.width(), top.height());
  if tw == 0 || th == 0 {
    return;
  }
  for_each_placement(tile, bw, bh, tw, th, x, y, |ox, oy| {
    let (obx, oby, otx, oty, rw, rh) = overlap_bounds(bw, bh, tw, th, ox, oy);
    for ry in 0..rh {
      for rx in 0..rw {
        let s = top.get_pixel(otx + rx, oty + ry).0;
        let cs = [
          s[0] as f32 / 65535.0,
          s[1] as f32 / 65535.0,
          s[2] as f32 / 65535.0,
          s[3] as f32 / 65535.0,
        ];
        let d = base.get_pixel(obx + rx, oby + ry).0;
        let cb = [
          d[0] as f32 / 65535.0,
          d[1] as f32 / 65535.0,
          d[2] as f32 / 65535.0,
          d[3] as f32 / 65535.0,
        ];
        let out = blend_rgba_f32(cb, cs, mode, opacity);
        base.put_pixel(
          obx + rx,
          oby + ry,
          Rgba([
            (out[0] * 65535.0).round().clamp(0.0, 65535.0) as u16,
            (out[1] * 65535.0).round().clamp(0.0, 65535.0) as u16,
            (out[2] * 65535.0).round().clamp(0.0, 65535.0) as u16,
            (out[3] * 65535.0).round().clamp(0.0, 65535.0) as u16,
          ]),
        );
      }
    }
  });
}

/// Composite `top` onto a 32-bit float RGBA `base` in place. Channels are clamped to `[0,1]` for
/// blending (HDR values outside the unit range are clamped, NaN sanitized to 0.0); the blended
/// result is already clamped to `[0,1]`, so it is written directly.
fn composite_into_f32(
  base: &mut Rgba32FImage,
  top: &Rgba32FImage,
  x: i64,
  y: i64,
  mode: BlendMode,
  opacity: f32,
  tile: bool,
) {
  let (bw, bh) = (base.width(), base.height());
  let (tw, th) = (top.width(), top.height());
  if tw == 0 || th == 0 {
    return;
  }
  for_each_placement(tile, bw, bh, tw, th, x, y, |ox, oy| {
    let (obx, oby, otx, oty, rw, rh) = overlap_bounds(bw, bh, tw, th, ox, oy);
    for ry in 0..rh {
      for rx in 0..rw {
        let s = top.get_pixel(otx + rx, oty + ry).0;
        // Compute the effective source alpha first. When it is 0 and the mode does not clear
        // based on the source, the output equals the backdrop — skip the pixel so `norm_f32`
        // does NOT clamp / sanitize the (possibly HDR > 1 or NaN) destination for a true no-op
        // overlay. Only the f32 path needs this; u8/u16 normalize losslessly.
        let a_s = (norm_f32(s[3]) * opacity).clamp(0.0, 1.0);
        if a_s == 0.0 && !clears_dest_when_source_transparent(mode) {
          continue;
        }
        let cs = [
          norm_f32(s[0]),
          norm_f32(s[1]),
          norm_f32(s[2]),
          norm_f32(s[3]),
        ];
        let d = base.get_pixel(obx + rx, oby + ry).0;
        let cb = [
          norm_f32(d[0]),
          norm_f32(d[1]),
          norm_f32(d[2]),
          norm_f32(d[3]),
        ];
        let out = blend_rgba_f32(cb, cs, mode, opacity);
        base.put_pixel(obx + rx, oby + ry, Rgba(out));
      }
    }
  });
}

/// Convert the composited RGBA `DynamicImage` back to the base's `original` color family, so an
/// opaque base never gains an alpha channel and `metadata().colorType` stays accurate. Falls back
/// to the RGBA image (of the working depth) for any color type not separately handled.
fn restore_color_type(img: DynamicImage, original: ColorType) -> DynamicImage {
  match original {
    ColorType::L8 => DynamicImage::ImageLuma8(img.to_luma8()),
    ColorType::La8 => DynamicImage::ImageLumaA8(img.to_luma_alpha8()),
    ColorType::Rgb8 => DynamicImage::ImageRgb8(img.to_rgb8()),
    ColorType::Rgba8 => img,
    ColorType::L16 => DynamicImage::ImageLuma16(img.to_luma16()),
    ColorType::La16 => DynamicImage::ImageLumaA16(img.to_luma_alpha16()),
    ColorType::Rgb16 => DynamicImage::ImageRgb16(img.to_rgb16()),
    ColorType::Rgba16 => img,
    ColorType::Rgb32F => DynamicImage::ImageRgb32F(img.to_rgb32f()),
    ColorType::Rgba32F => img,
    _ => img,
  }
}

/// Flatten an RGBA8 working buffer onto black: premultiply RGB by the result alpha, then force the
/// alpha opaque. Used before dropping the alpha channel for an opaque base so a coverage-reducing
/// result (e.g. `DestOut`) is actually visible instead of keeping the untouched straight RGB.
fn flatten_on_black_u8(img: &mut RgbaImage) {
  for p in img.pixels_mut() {
    let a = p.0[3] as f32 / 255.0;
    for c in 0..3 {
      p.0[c] = (p.0[c] as f32 * a).round().clamp(0.0, 255.0) as u8;
    }
    p.0[3] = 255;
  }
}

/// 16-bit counterpart of `flatten_on_black_u8`.
fn flatten_on_black_u16(img: &mut ImageBuffer<Rgba<u16>, Vec<u16>>) {
  for p in img.pixels_mut() {
    let a = p.0[3] as f32 / 65535.0;
    for c in 0..3 {
      p.0[c] = (p.0[c] as f32 * a).round().clamp(0.0, 65535.0) as u16;
    }
    p.0[3] = 65535;
  }
}

/// 32-bit float counterpart of `flatten_on_black_u8` (alpha already normalized to `[0,1]`).
fn flatten_on_black_f32(img: &mut Rgba32FImage) {
  for p in img.pixels_mut() {
    let a = p.0[3];
    for c in 0..3 {
      p.0[c] *= a;
    }
    p.0[3] = 1.0;
  }
}

/// Composite `top` onto `base` at `(x, y)` (or tiled) with `mode`/`opacity`. Works at the base's
/// native channel depth (8/16-bit or 32-bit float), then restores the base's ORIGINAL color type.
fn apply_composite(
  base: &mut DynamicImage,
  top: &DynamicImage,
  x: i64,
  y: i64,
  mode: BlendMode,
  opacity: f32,
  tile: bool,
) {
  let original = base.color();
  // Choose working depth from the base. 16-bit -> Rgba16; 32-bit float -> Rgba32F; else Rgba8.
  match original {
    ColorType::Rgba16 | ColorType::Rgb16 | ColorType::La16 | ColorType::L16 => {
      let mut work = base.to_rgba16();
      composite_into_u16(&mut work, &top.to_rgba16(), x, y, mode, opacity, tile);
      if !original.has_alpha() {
        flatten_on_black_u16(&mut work);
      }
      *base = restore_color_type(DynamicImage::ImageRgba16(work), original);
    }
    ColorType::Rgb32F | ColorType::Rgba32F => {
      let mut work = base.to_rgba32f();
      composite_into_f32(&mut work, &top.to_rgba32f(), x, y, mode, opacity, tile);
      if !original.has_alpha() {
        flatten_on_black_f32(&mut work);
      }
      *base = restore_color_type(DynamicImage::ImageRgba32F(work), original);
    }
    _ => {
      let mut work = base.to_rgba8();
      composite_into_u8(&mut work, &top.to_rgba8(), x, y, mode, opacity, tile);
      if !original.has_alpha() {
        flatten_on_black_u8(&mut work);
      }
      *base = restore_color_type(DynamicImage::ImageRgba8(work), original);
    }
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

#[cfg(test)]
mod tests {
  use super::DetectedFormat;
  use image::ImageFormat;

  #[test]
  fn standard_as_str_matches_lowercase_debug() {
    assert_eq!(DetectedFormat::Standard(ImageFormat::Png).as_str(), "png");
    assert_eq!(DetectedFormat::Standard(ImageFormat::Jpeg).as_str(), "jpeg");
    assert_eq!(DetectedFormat::Standard(ImageFormat::WebP).as_str(), "webp");
    assert_eq!(DetectedFormat::Standard(ImageFormat::Avif).as_str(), "avif");
  }

  #[test]
  fn heic_as_str_is_heic() {
    assert_eq!(DetectedFormat::Heic.as_str(), "heic");
  }

  #[test]
  fn svg_as_str_is_svg() {
    assert_eq!(DetectedFormat::Svg.as_str(), "svg");
    // Standard(Png) must still report "png" (raw rgba pixel path).
    assert_eq!(DetectedFormat::Standard(ImageFormat::Png).as_str(), "png");
  }

  #[test]
  fn svg_image_format_is_none() {
    assert_eq!(DetectedFormat::Svg.image_format(), None);
  }

  #[test]
  fn standard_image_format_is_some() {
    assert_eq!(
      DetectedFormat::Standard(ImageFormat::Png).image_format(),
      Some(ImageFormat::Png)
    );
    assert_eq!(
      DetectedFormat::Standard(ImageFormat::Jpeg).image_format(),
      Some(ImageFormat::Jpeg)
    );
  }

  #[test]
  fn heic_image_format_is_none() {
    assert_eq!(DetectedFormat::Heic.image_format(), None);
  }

  use super::{
    BlendMode, Gravity, ImageTransformArgs, apply_composite, apply_contrast, apply_huerotate,
    apply_opacity, apply_transforms, resolve_position,
  };
  use image::{ColorType, DynamicImage, ImageBuffer, RgbImage, RgbaImage};

  #[test]
  fn huerotate_preserves_16bit_color_through_pipeline() {
    // `image` 0.25's `DynamicImage::huerotate` clamps every channel to a hardcoded 255,
    // crushing 16-bit color to 8-bit. The pipeline must use a depth-aware hue rotation:
    // `huerotate(0)` is the identity rotation, so a 16-bit color must return unchanged,
    // never crushed to 255. Guards the Codex adversarial review on #42.
    let mut img =
      DynamicImage::ImageRgb16(ImageBuffer::from_raw(1, 1, vec![40000u16, 20000, 10000]).unwrap());
    let args = ImageTransformArgs {
      huerotate: Some(0),
      ..Default::default()
    };
    apply_transforms(&mut img, &args, None, true).unwrap();
    assert_eq!(img.color(), ColorType::Rgb16, "must stay 16-bit");
    assert_eq!(
      img.to_rgb16().get_pixel(0, 0).0,
      [40000, 20000, 10000],
      "16-bit color must survive huerotate, not crush to 255"
    );
  }

  #[test]
  fn huerotate_8bit_matches_image_crate() {
    // The custom hue rotation must be byte-identical to the crate for 8-bit RGB/RGBA
    // (where the crate's 255 max is already correct), so existing 8-bit output never
    // changes. Alpha must pass through untouched.
    let base = DynamicImage::ImageRgba8(
      RgbaImage::from_raw(2, 1, vec![200, 50, 100, 255, 10, 220, 30, 128]).unwrap(),
    );
    // Non-zero angles must match the crate byte-for-byte. (0 / 360 are the identity
    // fast-path, which is intentionally exact and may differ from the crate's matrix.)
    for angle in [45, 90, 180, 270, -90] {
      let mut mine = base.clone();
      apply_huerotate(&mut mine, angle);
      let theirs = base.huerotate(angle);
      assert_eq!(
        mine.to_rgba8(),
        theirs.to_rgba8(),
        "8-bit huerotate must match the image crate at {angle} deg"
      );
    }
  }

  #[test]
  fn huerotate_identity_preserves_nan_and_hdr_for_float() {
    // 0 / 360 are documented no-ops: the identity fast-path must return the input bit for
    // bit, including NaN and HDR (>1.0) float samples. The matrix path would contaminate
    // finite neighbors of a NaN channel and zero them. Codex adversarial review on #42.
    let base = DynamicImage::ImageRgba32F(
      ImageBuffer::from_raw(1, 1, vec![f32::NAN, 0.3, 2.5, 1.0]).unwrap(),
    );
    for angle in [0, 360, -360, 720] {
      let mut img = base.clone();
      apply_huerotate(&mut img, angle);
      let p = img.to_rgba32f().get_pixel(0, 0).0;
      assert!(p[0].is_nan(), "NaN preserved at {angle} deg");
      assert_eq!(p[1], 0.3, "finite neighbor preserved at {angle} deg");
      assert_eq!(p[2], 2.5, "HDR neighbor preserved at {angle} deg");
      assert_eq!(p[3], 1.0, "alpha preserved at {angle} deg");
    }
  }

  #[test]
  fn huerotate_preserves_16bit_alpha_directly() {
    // The depth-aware hue rotation keeps 16-bit alpha (the crate clamped it to 255) and
    // keeps 16-bit color out of the 8-bit range.
    let mut img = DynamicImage::ImageRgba16(
      ImageBuffer::from_raw(1, 1, vec![40000u16, 20000, 10000, 50000]).unwrap(),
    );
    apply_huerotate(&mut img, 90);
    let px = img.to_rgba16().get_pixel(0, 0).0;
    assert_eq!(
      px[3], 50000,
      "16-bit alpha must pass through, not clamp to 255"
    );
    assert!(
      px[0] as u32 + px[1] as u32 + px[2] as u32 > 1000,
      "16-bit color must not be crushed to ~255; got {px:?}"
    );
  }

  #[test]
  fn opacity_noop_contrast_invisible_for_hdr_float_alpha() {
    // For an Rgba32F image with alpha > 1.0, a no-op adjustContrast(0) must not change
    // opacity output. `apply_contrast` never touches alpha, so the alpha that reaches
    // `apply_opacity` is identical whether or not a no-op contrast is staged; opacity then
    // normalizes it the same way in both paths. Codex adversarial review on #42.
    let base =
      DynamicImage::ImageRgba32F(ImageBuffer::from_raw(1, 1, vec![0.2f32, 0.3, 0.4, 4.0]).unwrap());
    let alpha_of = |contrast: Option<f32>| {
      let mut img = base.clone();
      let args = ImageTransformArgs {
        contrast,
        opacity: Some(0.5),
        ..Default::default()
      };
      apply_transforms(&mut img, &args, None, true).unwrap();
      img.to_rgba32f().get_pixel(0, 0).0[3]
    };
    let without = alpha_of(None);
    let with_noop = alpha_of(Some(0.0));
    assert!(
      (without - with_noop).abs() < 1e-6,
      "no-op contrast(0) changed HDR-alpha opacity output: {without} vs {with_noop}"
    );
  }

  #[test]
  fn huerotate_nonzero_preserves_nan_for_float() {
    // A NaN sample must survive a real (non-zero) hue rotation rather than silently
    // collapse to 0 (black). The rotation matrix contaminates the other color channels
    // with NaN too — mathematically honest — but we must not turn NaN into valid black.
    // Codex adversarial review on #42.
    let mut img = DynamicImage::ImageRgba32F(
      ImageBuffer::from_raw(1, 1, vec![f32::NAN, 0.3, 0.4, 0.9]).unwrap(),
    );
    apply_huerotate(&mut img, 90);
    let p = img.to_rgba32f().get_pixel(0, 0).0;
    assert!(
      p[0].is_nan(),
      "NaN must survive a non-zero hue rotation, not become 0; got {}",
      p[0]
    );
    assert_eq!(p[3], 0.9, "alpha untouched");
  }

  #[test]
  fn huerotate_preserves_hdr_float_above_one() {
    // Float images can hold HDR values above 1.0. `huerotate(0)` is the identity rotation,
    // so an HDR channel must survive — never clipped to 1.0. The crate clamped float to
    // 255, so it preserved these; our depth-aware impl must not regress. Guards the Codex
    // adversarial review on #42.
    let mut img =
      DynamicImage::ImageRgba32F(ImageBuffer::from_raw(1, 1, vec![2.5f32, 0.3, 1.8, 4.0]).unwrap());
    apply_huerotate(&mut img, 0);
    if let DynamicImage::ImageRgba32F(buf) = &img {
      let p = buf.get_pixel(0, 0).0;
      assert!(
        (p[0] - 2.5).abs() < 1e-4,
        "HDR R must survive identity hue rotation; got {}",
        p[0]
      );
      assert!((p[1] - 0.3).abs() < 1e-4, "G survives; got {}", p[1]);
      assert!(
        (p[2] - 1.8).abs() < 1e-4,
        "HDR B must survive; got {}",
        p[2]
      );
      assert!((p[3] - 4.0).abs() < 1e-6, "alpha untouched; got {}", p[3]);
    } else {
      panic!("expected Rgba32F");
    }
  }

  #[test]
  fn huerotate_leaves_grayscale_luma_unchanged() {
    // Grayscale has no hue, so rotating it is a no-op on luma (the crate emitted garbage
    // by treating luma/alpha as RGB). Alpha is preserved too.
    let mut img = DynamicImage::ImageLumaA8(ImageBuffer::from_raw(1, 1, vec![120u8, 200]).unwrap());
    apply_huerotate(&mut img, 90);
    assert_eq!(img.color(), ColorType::La8, "grayscale stays grayscale");
    if let DynamicImage::ImageLumaA8(buf) = &img {
      assert_eq!(
        buf.get_pixel(0, 0).0,
        [120, 200],
        "luma and alpha unchanged"
      );
    } else {
      panic!("expected LumaA8");
    }
  }

  #[test]
  fn contrast_alpha_is_independent_of_staged_opacity() {
    // adjust_contrast must never change the alpha channel (it is a luma/color op), so
    // staging a documented no-op opacity(1) must not change the result. Alpha
    // preservation around contrast is UNCONDITIONAL, not coupled to opacity being staged.
    // Codex adversarial review on #42.
    let base =
      DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![100, 120, 140, 200]).unwrap());
    let alpha = |opacity: Option<f32>| {
      let mut img = base.clone();
      let args = ImageTransformArgs {
        contrast: Some(50.0),
        opacity,
        ..Default::default()
      };
      apply_transforms(&mut img, &args, None, true).unwrap();
      img.to_rgba8().get_pixel(0, 0).0[3]
    };
    assert_eq!(
      alpha(None),
      200,
      "contrast alone must leave alpha untouched"
    );
    assert_eq!(alpha(Some(1.0)), 200, "opacity(1) must be a true no-op");
    assert_eq!(
      alpha(None),
      alpha(Some(1.0)),
      "a staged no-op opacity must not change contrast output"
    );
  }

  #[test]
  fn opacity_preserves_16bit_alpha_through_contrast_and_huerotate() {
    // `huerotate` in image 0.25 clamps the 4th channel to 255, destroying 16-bit alpha,
    // and it runs AFTER contrast in the pipeline. Value filters (contrast, huerotate)
    // must leave the alpha that opacity scales untouched: a 16-bit alpha of 40000 scaled
    // by 0.5 must land near 20000, never ~128 (255 * 0.5). Guards the Codex adversarial
    // review on #42 (a regression of the contrast-only restore).
    let mut img = DynamicImage::ImageRgba16(
      ImageBuffer::from_raw(1, 1, vec![40000u16, 20000, 10000, 40000]).unwrap(),
    );
    let args = ImageTransformArgs {
      contrast: Some(100.0),
      huerotate: Some(90),
      opacity: Some(0.5),
      ..Default::default()
    };
    apply_transforms(&mut img, &args, None, true).unwrap();
    assert_eq!(img.color(), ColorType::Rgba16, "must stay 16-bit");
    let alpha = img.to_rgba16().get_pixel(0, 0).0[3];
    assert!(
      (19000..=21000).contains(&alpha),
      "16-bit alpha 40000 * 0.5 ~= 20000 must survive contrast+huerotate; got {alpha}"
    );
  }

  #[test]
  fn opacity_rgba8_multiplies_alpha_and_keeps_8bit() {
    let mut img =
      DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![10, 20, 30, 200]).unwrap());
    apply_opacity(&mut img, 0.5);
    assert_eq!(img.color(), ColorType::Rgba8);
    let px = img.to_rgba8();
    // round(200 * 0.5) = 100; color channels untouched.
    assert_eq!(px.get_pixel(0, 0).0, [10, 20, 30, 100]);
  }

  #[test]
  fn opacity_preserves_16bit_depth() {
    // The bug this guards (Codex review): a 16-bit source must NOT be silently
    // down-converted to 8-bit. Even the identity factor must keep Rgba16.
    let mut img = DynamicImage::ImageRgba16(
      ImageBuffer::from_raw(1, 1, vec![1000u16, 2000, 3000, 40000]).unwrap(),
    );
    apply_opacity(&mut img, 1.0);
    assert_eq!(
      img.color(),
      ColorType::Rgba16,
      "identity opacity must not drop to 8-bit"
    );

    let mut img = DynamicImage::ImageRgba16(
      ImageBuffer::from_raw(1, 1, vec![1000u16, 2000, 3000, 40000]).unwrap(),
    );
    apply_opacity(&mut img, 0.5);
    assert_eq!(img.color(), ColorType::Rgba16);
    if let DynamicImage::ImageRgba16(buf) = &img {
      assert_eq!(buf.get_pixel(0, 0).0, [1000, 2000, 3000, 20000]);
    } else {
      panic!("expected Rgba16");
    }
  }

  #[test]
  fn opacity_promotes_rgb16_to_rgba16_not_rgba8() {
    // RGB16 (no alpha) gains an alpha channel but stays 16-bit.
    let mut img =
      DynamicImage::ImageRgb16(ImageBuffer::from_raw(1, 1, vec![1000u16, 2000, 3000]).unwrap());
    apply_opacity(&mut img, 0.5);
    assert_eq!(img.color(), ColorType::Rgba16);
    if let DynamicImage::ImageRgba16(buf) = &img {
      // full alpha (65535) scaled by 0.5 -> 32768 (32767.5 rounds up).
      assert_eq!(buf.get_pixel(0, 0).0[3], 32768);
    } else {
      panic!("expected Rgba16");
    }
  }

  #[test]
  fn opacity_preserves_32f_depth() {
    let mut img =
      DynamicImage::ImageRgba32F(ImageBuffer::from_raw(1, 1, vec![0.1f32, 0.2, 0.3, 0.8]).unwrap());
    apply_opacity(&mut img, 0.5);
    assert_eq!(img.color(), ColorType::Rgba32F);
    if let DynamicImage::ImageRgba32F(buf) = &img {
      assert!((buf.get_pixel(0, 0).0[3] - 0.4).abs() < 1e-6);
    } else {
      panic!("expected Rgba32F");
    }
  }

  #[test]
  fn opacity_normalizes_out_of_range_float_alpha() {
    // Alpha is opacity, normalized `0.0..=1.0` at EVERY depth (the 8/16-bit paths can't
    // exceed their max; the image crate's f32 `DEFAULT_MAX_VALUE` is 1.0). The SOURCE alpha
    // is normalized into the unit range BEFORE the user's factor is applied, so a requested
    // fade is always effective — clamping the *product* instead would let an out-of-range
    // alpha swallow the fade and stay fully opaque. A valid in-range alpha follows
    // `new = old * factor` exactly. Resolves the Cursor/Codex review on #42 in favor of
    // normalize-source-then-multiply.

    // Out-of-range source normalizes to full opacity under identity opacity.
    let mut img =
      DynamicImage::ImageRgba32F(ImageBuffer::from_raw(1, 1, vec![0.2f32, 0.3, 0.4, 4.0]).unwrap());
    apply_opacity(&mut img, 1.0);
    let identity = img.to_rgba32f().get_pixel(0, 0).0[3];
    assert_eq!(
      identity, 1.0,
      "4.0 normalizes to full opacity; got {identity}"
    );

    // ...and a real fade still bites: normalize 4.0 -> 1.0, then * 0.5 -> 0.5 (NOT 1.0).
    let mut img =
      DynamicImage::ImageRgba32F(ImageBuffer::from_raw(1, 1, vec![0.2f32, 0.3, 0.4, 4.0]).unwrap());
    apply_opacity(&mut img, 0.5);
    let faded = img.to_rgba32f().get_pixel(0, 0).0[3];
    assert_eq!(
      faded, 0.5,
      "opacity(0.5) must still fade out-of-range alpha; got {faded}"
    );

    // In-range source: new = old * factor, unchanged by the normalization.
    let mut img =
      DynamicImage::ImageRgba32F(ImageBuffer::from_raw(1, 1, vec![0.2f32, 0.3, 0.4, 0.8]).unwrap());
    apply_opacity(&mut img, 0.5);
    let in_range = img.to_rgba32f().get_pixel(0, 0).0[3];
    assert!(
      (in_range - 0.4).abs() < 1e-6,
      "in-range alpha follows new = old * factor; got {in_range}"
    );
  }

  #[test]
  fn opacity_saturates_positive_infinite_float_alpha() {
    // `+∞` is the extreme of "alpha greater than the unit range", so the source normalizes
    // to 1.0 (full opacity) exactly like a finite alpha > 1.0 does — NOT 0.0 (transparent).
    // Grouping `+∞` with NaN/negative made opacity flip an "infinitely opaque" pixel to
    // fully transparent, contradicting the normalized-alpha model. Under identity opacity it
    // must stay fully opaque. Codex adversarial review on #42.
    let mut img = DynamicImage::ImageRgba32F(
      ImageBuffer::from_raw(1, 1, vec![0.2f32, 0.3, 0.4, f32::INFINITY]).unwrap(),
    );
    apply_opacity(&mut img, 1.0);
    let a = img.to_rgba32f().get_pixel(0, 0).0[3];
    assert_eq!(a, 1.0, "+inf alpha must normalize to full opacity; got {a}");
  }

  #[test]
  fn opacity_sanitizes_malformed_float_alpha() {
    // Decoded Rgba32F is external data: a negative, negative-infinite, or NaN source alpha
    // normalizes to the transparent bound 0.0 (`clamp` floors `-∞`/negatives; NaN is
    // sanitized), so after multiplying by the factor it stays 0.0 rather than leaking into
    // raw/composite output. Positive out-of-range alpha normalizes to full opacity instead
    // (see `opacity_saturates_positive_infinite_float_alpha` and
    // `opacity_normalizes_out_of_range_float_alpha`). Cursor + Codex review on #42.
    for bad in [-0.5f32, f32::NEG_INFINITY, f32::NAN] {
      let mut img = DynamicImage::ImageRgba32F(
        ImageBuffer::from_raw(1, 1, vec![0.2f32, 0.3, 0.4, bad]).unwrap(),
      );
      apply_opacity(&mut img, 0.5);
      let a = img.to_rgba32f().get_pixel(0, 0).0[3];
      assert_eq!(a, 0.0, "malformed alpha {bad} must sanitize to 0; got {a}");
    }
  }

  #[test]
  fn opacity_clamps_above_one_and_ignores_non_finite() {
    let mut img =
      DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![10, 20, 30, 200]).unwrap());
    apply_opacity(&mut img, 2.0);
    assert_eq!(
      img.to_rgba8().get_pixel(0, 0).0[3],
      200,
      "factor > 1 clamps to identity"
    );

    let mut img =
      DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![10, 20, 30, 200]).unwrap());
    apply_opacity(&mut img, f32::NAN);
    assert_eq!(
      img.to_rgba8().get_pixel(0, 0).0[3],
      200,
      "NaN must be a no-op, never silently zero the alpha"
    );
  }

  #[test]
  fn contrast_color_matches_image_crate() {
    // `apply_contrast` must produce the SAME color as the crate's `adjust_contrast` (it
    // differs only by preserving alpha). On Rgb8 (no alpha) the two must be byte-identical.
    let base = DynamicImage::ImageRgb8(
      image::RgbImage::from_raw(2, 1, vec![200, 50, 100, 10, 220, 30]).unwrap(),
    );
    for c in [-50.0f32, -10.0, 0.0, 25.0, 100.0] {
      let mut mine = base.clone();
      apply_contrast(&mut mine, c);
      let theirs = base.adjust_contrast(c);
      assert_eq!(
        mine.to_rgb8(),
        theirs.to_rgb8(),
        "contrast color must match the image crate at {c}"
      );
    }
  }

  #[test]
  fn contrast_preserves_alpha_and_matches_crate_color_rgba() {
    // On RGBA, color must match the crate while alpha is preserved (the crate mangles it).
    let base =
      DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![200, 50, 100, 200]).unwrap());
    let mine = {
      let mut img = base.clone();
      apply_contrast(&mut img, 60.0);
      img.to_rgba8().get_pixel(0, 0).0
    };
    let crate_px = base.adjust_contrast(60.0).to_rgba8().get_pixel(0, 0).0;
    assert_eq!(
      mine[0..3],
      crate_px[0..3],
      "color channels must match the crate"
    );
    assert_eq!(mine[3], 200, "alpha must be preserved");
    assert_ne!(
      crate_px[3], 200,
      "sanity: the crate's adjust_contrast does mangle this alpha"
    );
  }

  #[test]
  fn contrast_preserves_16bit_alpha_and_depth() {
    let mut img = DynamicImage::ImageRgba16(
      ImageBuffer::from_raw(1, 1, vec![40000u16, 20000, 10000, 50000]).unwrap(),
    );
    apply_contrast(&mut img, 50.0);
    assert_eq!(img.color(), ColorType::Rgba16, "must stay 16-bit");
    assert_eq!(
      img.to_rgba16().get_pixel(0, 0).0[3],
      50000,
      "16-bit alpha must be preserved, not scaled/crushed"
    );
  }

  #[test]
  fn contrast_noop_is_invisible_for_lumaa_pipeline() {
    // A no-op `adjustContrast(0)` must not change downstream output. `apply_contrast` keeps
    // the LumaA color model (does not promote to RGBA), so a staged contrast never makes
    // `huerotate` see a different pixel model. Guards the Codex adversarial review on #42.
    let base = DynamicImage::ImageLumaA8(ImageBuffer::from_raw(1, 1, vec![120u8, 200]).unwrap());

    let without = {
      let mut img = base.clone();
      let args = ImageTransformArgs {
        huerotate: Some(90),
        opacity: Some(1.0),
        ..Default::default()
      };
      apply_transforms(&mut img, &args, None, true).unwrap();
      img.to_rgba8().get_pixel(0, 0).0
    };
    let with_noop_contrast = {
      let mut img = base.clone();
      let args = ImageTransformArgs {
        contrast: Some(0.0),
        huerotate: Some(90),
        opacity: Some(1.0),
        ..Default::default()
      };
      apply_transforms(&mut img, &args, None, true).unwrap();
      img.to_rgba8().get_pixel(0, 0).0
    };
    assert_eq!(
      without, with_noop_contrast,
      "a no-op contrast(0) must not change huerotate output (LumaA must not be promoted early)"
    );
  }

  #[test]
  fn composite_multiply_halves_gray() {
    // round(0.502^2 * 255) = 64. Multiply of two mid-grays.
    let mut base =
      DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![128, 128, 128, 255]).unwrap());
    let top =
      DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![128, 128, 128, 255]).unwrap());
    apply_composite(&mut base, &top, 0, 0, BlendMode::Multiply, 1.0, false);
    assert_eq!(base.color(), ColorType::Rgba8);
    assert_eq!(base.to_rgba8().get_pixel(0, 0).0, [64, 64, 64, 255]);
  }

  #[test]
  fn composite_color_dodge_saturates_without_nan() {
    // cs == 1.0 hits the ColorDodge guard: channels saturate to 255, no NaN/panic.
    let mut base =
      DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![128, 128, 128, 255]).unwrap());
    let top =
      DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![255, 255, 255, 255]).unwrap());
    apply_composite(&mut base, &top, 0, 0, BlendMode::ColorDodge, 1.0, false);
    assert_eq!(base.to_rgba8().get_pixel(0, 0).0, [255, 255, 255, 255]);
  }

  #[test]
  fn composite_dest_over_keeps_opaque_backdrop() {
    // DestOver onto an opaque backdrop: Fa = 1 - ab = 0, so the base shows through unchanged.
    let mut base =
      DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![255, 0, 0, 255]).unwrap());
    let top = DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![0, 0, 255, 255]).unwrap());
    apply_composite(&mut base, &top, 0, 0, BlendMode::DestOver, 1.0, false);
    assert_eq!(base.to_rgba8().get_pixel(0, 0).0, [255, 0, 0, 255]);
  }

  #[test]
  fn composite_over_matches_legacy_overlay_within_one() {
    // The custom `Over` path must match `image::imageops::overlay` within ±1 per channel
    // (rounding vs truncation).
    let base = DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![0, 0, 255, 255]).unwrap());
    let top = DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![255, 0, 0, 128]).unwrap());

    let legacy = {
      let mut b = base.clone();
      image::imageops::overlay(&mut b, &top, 0, 0);
      b.to_rgba8().get_pixel(0, 0).0
    };
    let custom = {
      let mut b = base.clone();
      apply_composite(&mut b, &top, 0, 0, BlendMode::Over, 1.0, false);
      b.to_rgba8().get_pixel(0, 0).0
    };
    for c in 0..4 {
      let diff = (legacy[c] as i32 - custom[c] as i32).abs();
      assert!(
        diff <= 1,
        "channel {c}: legacy {} vs custom {} differ by {diff} (> 1)",
        legacy[c],
        custom[c]
      );
    }
  }

  #[test]
  fn composite_tile_covers_whole_base() {
    // A 2x2 opaque overlay tiled over a 4x4 base must paint every pixel.
    let mut base = DynamicImage::ImageRgba8(
      RgbaImage::from_raw(4, 4, vec![50, 60, 70, 255].repeat(16)).unwrap(),
    );
    let top =
      DynamicImage::ImageRgba8(RgbaImage::from_raw(2, 2, vec![10, 20, 30, 255].repeat(4)).unwrap());
    apply_composite(&mut base, &top, 0, 0, BlendMode::Over, 1.0, true);
    let out = base.to_rgba8();
    for y in 0..4 {
      for x in 0..4 {
        assert_eq!(
          out.get_pixel(x, y).0,
          [10, 20, 30, 255],
          "tile must cover pixel ({x}, {y})"
        );
      }
    }
  }

  #[test]
  fn composite_preserves_opaque_color_type() {
    // An opaque RGB8 base must NOT gain an alpha channel after compositing.
    let mut base = DynamicImage::ImageRgb8(RgbImage::from_raw(1, 1, vec![100, 150, 200]).unwrap());
    let top =
      DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![128, 128, 128, 255]).unwrap());
    apply_composite(&mut base, &top, 0, 0, BlendMode::Multiply, 1.0, false);
    assert_eq!(
      base.color(),
      ColorType::Rgb8,
      "opaque base must stay Rgb8 (no alpha channel added)"
    );
  }

  #[test]
  fn composite_over_applies_per_overlay_opacity() {
    // A faded overlay (opacity 0.5) Over an opaque base must blend half-and-half. With
    // a_s = top_alpha * opacity = 0.5 and ab = 1, Over gives Fa = 1, Fb = 1 - a_s = 0.5,
    // so ao = 0.5*1 + 1*0.5 = 1.0. Red fades to ~0.5 (128), the base blue keeps ~0.5
    // (128), green stays 0, and the opaque base keeps full alpha + its Rgba8 color type.
    let mut base =
      DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![0, 0, 255, 255]).unwrap());
    let top = DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![255, 0, 0, 255]).unwrap());
    apply_composite(&mut base, &top, 0, 0, BlendMode::Over, 0.5, false);
    assert_eq!(base.color(), ColorType::Rgba8);
    let px = base.to_rgba8().get_pixel(0, 0).0;
    assert!(
      (127..=129).contains(&px[0]),
      "R ~= 128 (half-faded red); got {}",
      px[0]
    );
    assert_eq!(px[1], 0, "green stays 0");
    assert!(
      (127..=129).contains(&px[2]),
      "B ~= 128 (half-kept base blue); got {}",
      px[2]
    );
    assert_eq!(px[3], 255, "opaque base keeps full alpha");
  }

  #[test]
  fn composite_multiply_runs_at_16bit_depth() {
    // The 16-bit branch (`composite_into_u16`) must blend at full depth. Multiply of two
    // mid-grays: 32768/65535 ~= 0.50000763, squared ~= 0.25000763, * 65535 ~= 16384.25,
    // rounding to ~16384 — the result keeps 16-bit precision and the base stays Rgba16.
    let mut base = DynamicImage::ImageRgba16(
      image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(1, 1, vec![32768, 32768, 32768, 65535])
        .unwrap(),
    );
    let top = DynamicImage::ImageRgba16(
      image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(1, 1, vec![32768, 32768, 32768, 65535])
        .unwrap(),
    );
    apply_composite(&mut base, &top, 0, 0, BlendMode::Multiply, 1.0, false);
    assert_eq!(base.color(), ColorType::Rgba16, "must stay 16-bit");
    let px = base.to_rgba16().get_pixel(0, 0).0;
    for c in 0..3 {
      assert!(
        (16380..=16388).contains(&px[c]),
        "channel {c}: 16-bit Multiply ~= 16384; got {}",
        px[c]
      );
    }
    assert_eq!(px[3], 65535, "opaque alpha preserved at 16-bit");
  }

  #[test]
  fn resolve_position_defaults_to_center() {
    // No explicit offset + default Center gravity: a 2x2 top on a 4x4 base lands at (1, 1).
    assert_eq!(
      resolve_position(false, 0, 0, Gravity::Center, 4, 4, 2, 2),
      (1, 1)
    );
  }

  #[test]
  fn resolve_position_southeast_gravity() {
    // SouthEast anchors the 2x2 top at the bottom-right corner of a 4x4 base: (2, 2).
    assert_eq!(
      resolve_position(false, 0, 0, Gravity::SouthEast, 4, 4, 2, 2),
      (2, 2)
    );
  }

  #[test]
  fn resolve_position_offset_overrides_gravity() {
    // has_offset = true: the explicit (left, top) wins, gravity is ignored (negatives allowed).
    assert_eq!(
      resolve_position(true, 3, -2, Gravity::SouthEast, 4, 4, 2, 2),
      (3, -2)
    );
  }

  #[test]
  fn composite_tile_streams_1x1_over_large_base() {
    // A 1x1 opaque top tiled over an 8x8 base must paint every one of the 64 pixels. Proves the
    // streaming `for_each_placement` loop is correct AND bounded (no per-pixel Vec allocation).
    let mut base =
      DynamicImage::ImageRgba8(RgbaImage::from_raw(8, 8, vec![1, 2, 3, 255].repeat(64)).unwrap());
    let top = DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![7, 8, 9, 255]).unwrap());
    apply_composite(&mut base, &top, 0, 0, BlendMode::Over, 1.0, true);
    let out = base.to_rgba8();
    for y in 0..8 {
      for x in 0..8 {
        assert_eq!(
          out.get_pixel(x, y).0,
          [7, 8, 9, 255],
          "tile must cover pixel ({x}, {y})"
        );
      }
    }
  }

  #[test]
  fn composite_f32_transparent_overlay_is_identity() {
    // A fully transparent top over an HDR Rgba32F base (channel > 1.0) must leave the destination
    // byte-for-byte untouched — the f32 no-op short-circuit must NOT clamp the highlight to 1.0.
    let mut base = DynamicImage::ImageRgba32F(
      image::ImageBuffer::<image::Rgba<f32>, _>::from_raw(1, 1, vec![4.0, 2.0, 0.5, 1.0]).unwrap(),
    );
    let top = DynamicImage::ImageRgba32F(
      image::ImageBuffer::<image::Rgba<f32>, _>::from_raw(1, 1, vec![0.0, 0.0, 0.0, 0.0]).unwrap(),
    );
    apply_composite(&mut base, &top, 0, 0, BlendMode::Over, 1.0, false);
    assert_eq!(base.color(), ColorType::Rgba32F, "must stay Rgba32F");
    let px = base.to_rgba32f().get_pixel(0, 0).0;
    assert_eq!(
      px[0], 4.0,
      "HDR highlight must survive a no-op overlay (not clamped to 1.0)"
    );
    assert_eq!(px[1], 2.0);
    assert_eq!(px[2], 0.5);
    assert_eq!(px[3], 1.0);
  }

  #[test]
  fn composite_dest_out_flattens_opaque_rgb_to_black() {
    // DestOut with an opaque top fully clears coverage (ao -> 0). On an Rgb8 (no-alpha) base the
    // overlapped region must flatten to black rather than keep the original RGB, and stay Rgb8.
    let mut base = DynamicImage::ImageRgb8(RgbImage::from_raw(1, 1, vec![200, 200, 200]).unwrap());
    let top = DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![123, 45, 67, 255]).unwrap());
    apply_composite(&mut base, &top, 0, 0, BlendMode::DestOut, 1.0, false);
    assert_eq!(base.color(), ColorType::Rgb8, "opaque base stays Rgb8");
    assert_eq!(
      base.to_rgb8().get_pixel(0, 0).0,
      [0, 0, 0],
      "fully cleared coverage must flatten to black"
    );
  }

  #[test]
  fn composite_dest_out_half_flattens_opaque_rgb() {
    // A 50%-opaque DestOut top halves coverage (ao ~= 0.5). Flatten-on-black scales the kept RGB
    // by that alpha: 200 * 0.5 ~= 100. Stays Rgb8.
    let mut base = DynamicImage::ImageRgb8(RgbImage::from_raw(1, 1, vec![200, 200, 200]).unwrap());
    let top = DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![10, 20, 30, 128]).unwrap());
    apply_composite(&mut base, &top, 0, 0, BlendMode::DestOut, 1.0, false);
    assert_eq!(base.color(), ColorType::Rgb8);
    let px = base.to_rgb8().get_pixel(0, 0).0;
    for c in 0..3 {
      assert!(
        (98..=102).contains(&px[c]),
        "channel {c}: ~100 after half-flatten; got {}",
        px[c]
      );
    }
  }
}
