use std::io::Cursor;

use napi::{bindgen_prelude::*, JsBuffer};
use napi_derive::napi;

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
pub fn compress_jpeg_sync(
  env: Env,
  input: Buffer,
  options: Option<JpegCompressOptions>,
) -> Result<JsBuffer> {
  let options = options.unwrap_or_default();
  let quality = options.quality.unwrap_or(100);
  if quality != 100 {
    let img = image::load_from_memory_with_format(input.as_ref(), image::ImageFormat::Jpeg)
      .map_err(|err| {
        Error::new(
          Status::InvalidArg,
          format!("Load input jpeg image failed {}", err),
        )
      })?;
    let mut dest = Cursor::new(Vec::with_capacity(input.len()));
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut dest, quality as u8);
    encoder.encode_image(&img).map_err(|err| {
      Error::new(
        Status::GenericFailure,
        format!("Encode image from input jpeg failed {}", err),
      )
    })?;
    return env
      .create_buffer_with_data(dest.into_inner())
      .map(|b| b.into_raw());
  }
  let (buf, outsize, de_c_info, compress_c_info) =
    unsafe { moz_jpeg_compress(input.as_ref(), &options) }?;
  unsafe {
    env.create_buffer_with_borrowed_data(
      buf,
      outsize,
      (de_c_info, compress_c_info, buf),
      |(mut input, mut output, buf), _| {
        mozjpeg_sys::jpeg_destroy_decompress(&mut input);
        mozjpeg_sys::jpeg_destroy_compress(&mut output);
        libc::free(buf as *mut std::ffi::c_void);
      },
    )
  }
  .map(|v| v.into_raw())
}

/// # Safety
///
/// The output buffer from `mozjpeg` is checked by V8 while converting it into Node.js Buffer.
unsafe fn moz_jpeg_compress(
  input: &[u8],
  opts: &JpegCompressOptions,
) -> Result<(
  *mut u8,
  usize,
  mozjpeg_sys::jpeg_decompress_struct,
  mozjpeg_sys::jpeg_compress_struct,
)> {
  std::panic::catch_unwind(|| {
    let mut de_c_info: mozjpeg_sys::jpeg_decompress_struct = std::mem::zeroed();
    let mut err_handler = create_error_handler();
    de_c_info.common.err = &mut err_handler;
    mozjpeg_sys::jpeg_create_decompress(&mut de_c_info);
    let input_buf = input;
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
    (buf, outsize as usize, de_c_info, compress_c_info)
  })
  .map_err(|err| {
    Error::new(
      Status::GenericFailure,
      format!("Compress JPEG failed {:?}", err),
    )
  })
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

pub struct ThreadsafeMozjpegCompressOutput {
  buf: *mut u8,
  len: usize,
  de_c_info: mozjpeg_sys::jpeg_decompress_struct,
  compress_c_info: mozjpeg_sys::jpeg_compress_struct,
}

unsafe impl Send for ThreadsafeMozjpegCompressOutput {}

pub struct CompressJpegTask {
  options: JpegCompressOptions,
  input: Buffer,
}

pub enum JpegOptimizeOutput {
  Lossless(Box<ThreadsafeMozjpegCompressOutput>),
  Lossy(Vec<u8>),
}

#[napi]
impl Task for CompressJpegTask {
  type Output = JpegOptimizeOutput;
  type JsValue = JsBuffer;

  fn compute(&mut self) -> Result<Self::Output> {
    let quality = self.options.quality.unwrap_or(100);
    if quality != 100 {
      let img = image::load_from_memory_with_format(self.input.as_ref(), image::ImageFormat::Jpeg)
        .map_err(|err| {
          Error::new(
            Status::InvalidArg,
            format!("Load input jpeg image failed {}", err),
          )
        })?;
      let mut dest = Cursor::new(Vec::with_capacity(self.input.len()));
      let mut encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut dest, quality as u8);
      encoder.encode_image(&img).map_err(|err| {
        Error::new(
          Status::GenericFailure,
          format!("Encode image from input jpeg failed {}", err),
        )
      })?;
      return Ok(JpegOptimizeOutput::Lossy(dest.into_inner()));
    }
    unsafe { moz_jpeg_compress(self.input.as_ref(), &self.options) }.map(
      |(buf, len, de_c_info, compress_c_info)| {
        JpegOptimizeOutput::Lossless(Box::new(ThreadsafeMozjpegCompressOutput {
          buf,
          len,
          de_c_info,
          compress_c_info,
        }))
      },
    )
  }

  fn resolve(&mut self, env: Env, output: Self::Output) -> Result<Self::JsValue> {
    match output {
      JpegOptimizeOutput::Lossless(moz_jpeg_output) => {
        let ThreadsafeMozjpegCompressOutput {
          buf,
          len,
          de_c_info,
          compress_c_info,
        } = *moz_jpeg_output;
        unsafe {
          env.create_buffer_with_borrowed_data(
            buf,
            len,
            (de_c_info, compress_c_info, buf),
            |(mut input, mut output, buf), _| {
              mozjpeg_sys::jpeg_destroy_decompress(&mut input);
              mozjpeg_sys::jpeg_destroy_compress(&mut output);
              libc::free(buf as *mut std::ffi::c_void);
            },
          )
        }
      }
      JpegOptimizeOutput::Lossy(buf) => env.create_buffer_with_data(buf),
    }
    .map(|v| v.into_raw())
  }
}

#[napi]
pub fn compress_jpeg(
  input: Buffer,
  options: Option<JpegCompressOptions>,
  signal: Option<AbortSignal>,
) -> AsyncTask<CompressJpegTask> {
  AsyncTask::with_optional_signal(
    CompressJpegTask {
      input,
      options: options.unwrap_or_default(),
    },
    signal,
  )
}
