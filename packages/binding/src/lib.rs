#![deny(clippy::all)]

use std::iter::FromIterator;

use napi::{bindgen_prelude::*, JsBuffer};
use napi_derive::napi;

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
fn to_oxipng_options(options: Option<PNGLosslessOptions>) -> oxipng::Options {
  let opt = options.unwrap_or_default();
  oxipng::Options {
    fix_errors: opt.fix_errors.unwrap_or(false),
    force: opt.force.unwrap_or(false),
    filter: opt
      .filter
      .map(|v| v.into_iter().map(|i| i as u8).collect())
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
pub fn lossless_compress_png(input: Buffer, options: Option<PNGLosslessOptions>) -> Result<Buffer> {
  let output = oxipng::optimize_from_memory(input.as_ref(), &to_oxipng_options(options))
    .map_err(|err| Error::new(Status::InvalidArg, format!("Optimize failed {}", err)))?;
  Ok(output.into())
}

#[napi(object)]
#[derive(Default)]
pub struct JpegCompressOptions {
  /// Output quality, default is 100 (lossless)
  pub quality: Option<u32>,
  /// If true, it will use MozJPEG’s scan optimization. Makes progressive image files smaller.
  /// Default is `true`
  pub optimize_scans: Option<bool>,
}

#[napi]
pub unsafe fn compress_jpeg(
  env: Env,
  input: Buffer,
  options: Option<JpegCompressOptions>,
) -> Result<JsBuffer> {
  std::panic::catch_unwind(|| {
    let opts = options.unwrap_or_default();
    let mut de_c_info: mozjpeg_sys::jpeg_decompress_struct = std::mem::zeroed();
    let mut err_handler = create_error_handler();
    de_c_info.common.err = &mut err_handler;
    mozjpeg_sys::jpeg_create_decompress(&mut de_c_info);
    let input_buf = input.as_ref();
    #[cfg(any(target_os = "windows", target_arch = "arm"))]
    mozjpeg_sys::jpeg_mem_src(&mut de_c_info, input_buf.as_ptr(), input_buf.len() as u32);
    #[cfg(not(any(target_os = "windows", target_arch = "arm")))]
    mozjpeg_sys::jpeg_mem_src(&mut de_c_info, input_buf.as_ptr(), input_buf.len() as u64);
    let mut compress_c_info: mozjpeg_sys::jpeg_compress_struct = std::mem::zeroed();
    compress_c_info.optimize_coding = 1;
    compress_c_info.common.err = &mut err_handler;
    mozjpeg_sys::jpeg_create_compress(&mut compress_c_info);
    mozjpeg_sys::jpeg_read_header(&mut de_c_info, 1);
    let src_coef_arrays = mozjpeg_sys::jpeg_read_coefficients(&mut de_c_info);
    mozjpeg_sys::jpeg_copy_critical_parameters(&de_c_info, &mut compress_c_info);
    if let Some(quality) = opts.quality {
      mozjpeg_sys::jpeg_set_quality(&mut compress_c_info, quality as i32, 0);
    }
    if opts.optimize_scans.unwrap_or(true) {
      mozjpeg_sys::jpeg_c_set_bool_param(
        &mut compress_c_info,
        mozjpeg_sys::J_BOOLEAN_PARAM::JBOOLEAN_OPTIMIZE_SCANS,
        1,
      );
    }
    mozjpeg_sys::jpeg_c_set_int_param(
      &mut compress_c_info,
      mozjpeg_sys::J_INT_PARAM::JINT_DC_SCAN_OPT_MODE,
      0,
    );
    let mut buf = std::ptr::null_mut();
    let mut outsize = 0;
    mozjpeg_sys::jpeg_mem_dest(&mut compress_c_info, &mut buf, &mut outsize);
    mozjpeg_sys::jpeg_write_coefficients(&mut compress_c_info, src_coef_arrays);
    mozjpeg_sys::jpeg_finish_compress(&mut compress_c_info);
    mozjpeg_sys::jpeg_finish_decompress(&mut de_c_info);
    env
      .create_buffer_with_borrowed_data(
        buf,
        outsize as usize,
        (de_c_info, compress_c_info, buf),
        |(mut input, mut output, buf), _| {
          mozjpeg_sys::jpeg_destroy_decompress(&mut input);
          mozjpeg_sys::jpeg_destroy_compress(&mut output);
          libc::free(buf as *mut std::ffi::c_void);
        },
      )
      .map(|v| v.into_raw())
  })
  .map_err(|err| {
    Error::new(
      Status::GenericFailure,
      format!("Compress JPEG failed {:?}", err),
    )
  })
  .and_then(|v| v)
}

unsafe fn create_error_handler() -> mozjpeg_sys::jpeg_error_mgr {
  let mut err: mozjpeg_sys::jpeg_error_mgr = std::mem::zeroed();
  mozjpeg_sys::jpeg_std_error(&mut err);
  err.error_exit = Some(unwind_error_exit);
  err.emit_message = Some(silence_message);
  err
}

extern "C" fn unwind_error_exit(cinfo: &mut mozjpeg_sys::jpeg_common_struct) {
  let message = unsafe {
    let err = cinfo.err.as_ref().unwrap();
    match err.format_message {
      Some(fmt) => {
        let buffer = std::mem::zeroed();
        fmt(cinfo, &buffer);
        let len = buffer.iter().take_while(|&&c| c != 0).count();
        String::from_utf8_lossy(&buffer[..len]).into()
      }
      None => format!("libjpeg error: {}", err.msg_code),
    }
  };
  std::panic::resume_unwind(Box::new(message))
}

extern "C" fn silence_message(
  _cinfo: &mut mozjpeg_sys::jpeg_common_struct,
  _level: std::os::raw::c_int,
) {
}

#[napi(object)]
#[derive(Default)]
pub struct PngQuantOptions {
  // default is 70
  pub min_quality: Option<u32>,
  // default is 99
  pub max_quality: Option<u32>,
  // 1- 10
  // Faster speeds generate images of lower quality, but may be useful for real-time generation of images.
  // default: 5
  pub speed: Option<u32>,
  // Number of least significant bits to ignore.
  // Useful for generating palettes for VGA, 15-bit textures, or other retro platforms.
  pub posterization: Option<u32>,
}

#[napi]
pub fn png_quantize(input: Buffer, options: Option<PngQuantOptions>) -> Result<Buffer> {
  let bitmap = lodepng::decode32(input.as_ref()).map_err(|err| {
    Error::new(
      Status::InvalidArg,
      format!("Decode png from buffer failed{}", err),
    )
  })?;
  let options = options.unwrap_or_default();
  let width = bitmap.width;
  let height = bitmap.height;
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
    .new_image(
      bitmap.buffer.as_slice(),
      width as usize,
      height as usize,
      0.0,
    )
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
    .encode(pixels.as_slice(), width, height)
    .map_err(|err| {
      Error::new(
        Status::GenericFailure,
        format!("Encode quantized png failed {}", err),
      )
    })?;
  Ok(output.into())
}
