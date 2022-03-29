use napi::{bindgen_prelude::*, JsBuffer};
use napi_derive::napi;

use crate::decode::decode_input_image;

#[napi]
/// # Safety
///
/// The output buffer is checked by V8 while converting it into Node.js Buffer.
pub unsafe fn lossless_encode_webp(env: Env, input: Buffer) -> Result<JsBuffer> {
  let (decoded_buf, width, height, alpha_channel) = decode_input_image(input.as_ref())?;
  let mut out_buf = std::ptr::null_mut();
  let len = if alpha_channel {
    let stride = width as i32 * 4;
    libwebp_sys::WebPEncodeLosslessRGBA(
      decoded_buf.as_ptr(),
      width as i32,
      height as i32,
      stride,
      &mut out_buf,
    )
  } else {
    let stride = width as i32 * 3;
    libwebp_sys::WebPEncodeLosslessRGB(
      decoded_buf.as_ptr(),
      width as i32,
      height as i32,
      stride,
      &mut out_buf,
    )
  };
  env
    .create_buffer_with_borrowed_data(out_buf, len, out_buf, |raw, _env| {
      Vec::from_raw_parts(raw, len, len);
    })
    .map(|v| v.into_raw())
}

#[napi]
/// The quality factor `quality_factor` ranges from 0 to 100 and controls the loss and quality during compression.
/// The value 0 corresponds to low quality and small output sizes, whereas 100 is the highest quality and largest output size.
/// https://developers.google.com/speed/webp/docs/api#simple_encoding_api
pub unsafe fn encode_webp(env: Env, input: Buffer, quality_factor: u32) -> Result<JsBuffer> {
  let (decoded_buf, width, height, alpha_channel) = decode_input_image(input.as_ref())?;
  let mut out_buf = std::ptr::null_mut();
  let len = if alpha_channel {
    let stride = width as i32 * 4;
    libwebp_sys::WebPEncodeRGBA(
      decoded_buf.as_ptr(),
      width as i32,
      height as i32,
      stride,
      quality_factor as f32,
      &mut out_buf,
    )
  } else {
    let stride = width as i32 * 3;
    libwebp_sys::WebPEncodeRGB(
      decoded_buf.as_ptr(),
      width as i32,
      height as i32,
      stride,
      quality_factor as f32,
      &mut out_buf,
    )
  };
  env
    .create_buffer_with_borrowed_data(out_buf, len, out_buf, |raw, _env| {
      Vec::from_raw_parts(raw, len, len);
    })
    .map(|v| v.into_raw())
}
