use image::ColorType;
use napi::{bindgen_prelude::*, JsBuffer};
use napi_derive::napi;

use crate::decode::decode_input_image;

#[napi]
/// # Safety
///
/// The output buffer is checked by V8 while converting it into Node.js Buffer.
pub unsafe fn lossless_encode_webp(env: Env, input: Buffer) -> Result<JsBuffer> {
  let (decoded_buf, width, height, alpha_channel) = decode_input_image(input.as_ref())?;
  let (out_buf, len) = lossless_encode_webp_inner(
    &decoded_buf,
    width,
    height,
    if alpha_channel {
      &ColorType::Rgba8
    } else {
      &ColorType::Rgb8
    },
  )?;
  env
    .create_buffer_with_borrowed_data(out_buf, len, out_buf, |raw, _env| {
      Vec::from_raw_parts(raw, len, len);
    })
    .map(|v| v.into_raw())
}

#[inline]
pub(crate) unsafe fn lossless_encode_webp_inner(
  input: &[u8],
  width: u32,
  height: u32,
  color_type: &image::ColorType,
) -> Result<(*mut u8, usize)> {
  let mut out_buf = std::ptr::null_mut();
  let len = match color_type {
    ColorType::Rgb8 => {
      let stride = width as i32 * 3;
      libwebp_sys::WebPEncodeLosslessRGB(
        input.as_ptr(),
        width as i32,
        height as i32,
        stride,
        &mut out_buf,
      )
    }
    ColorType::Rgba8 => {
      let stride = width as i32 * 4;
      libwebp_sys::WebPEncodeLosslessRGBA(
        input.as_ptr(),
        width as i32,
        height as i32,
        stride,
        &mut out_buf,
      )
    }
    _ => {
      return Err(Error::new(
        Status::InvalidArg,
        format!(
          "Unsupported encoding color type [{:?}] into webp",
          color_type
        ),
      ))
    }
  };
  Ok((out_buf, len))
}

#[napi]
/// The quality factor `quality_factor` ranges from 0 to 100 and controls the loss and quality during compression.
/// The value 0 corresponds to low quality and small output sizes, whereas 100 is the highest quality and largest output size.
/// https://developers.google.com/speed/webp/docs/api#simple_encoding_api
pub unsafe fn encode_webp(env: Env, input: Buffer, quality_factor: u32) -> Result<JsBuffer> {
  let (decoded_buf, width, height, alpha_channel) = decode_input_image(input.as_ref())?;
  let (out_buf, len) = encode_webp_inner(
    &decoded_buf,
    quality_factor,
    width,
    height,
    if alpha_channel {
      &ColorType::Rgba8
    } else {
      &ColorType::Rgb8
    },
  )?;
  env
    .create_buffer_with_borrowed_data(out_buf, len, out_buf, |raw, _env| {
      Vec::from_raw_parts(raw, len, len);
    })
    .map(|v| v.into_raw())
}

#[inline]
pub(crate) unsafe fn encode_webp_inner(
  input: &[u8],
  quality_factor: u32,
  width: u32,
  height: u32,
  color_type: &image::ColorType,
) -> Result<(*mut u8, usize)> {
  let mut out_buf = std::ptr::null_mut();
  let len = match color_type {
    ColorType::Rgb8 => {
      let stride = width as i32 * 3;
      libwebp_sys::WebPEncodeRGB(
        input.as_ptr(),
        width as i32,
        height as i32,
        stride,
        quality_factor as f32,
        &mut out_buf,
      )
    }
    ColorType::Rgba8 => {
      let stride = width as i32 * 4;
      libwebp_sys::WebPEncodeRGBA(
        input.as_ptr(),
        width as i32,
        height as i32,
        stride,
        quality_factor as f32,
        &mut out_buf,
      )
    }
    _ => {
      return Err(Error::new(
        Status::InvalidArg,
        format!(
          "Unsupported encoding color type [{:?}] into webp",
          color_type
        ),
      ))
    }
  };
  Ok((out_buf, len))
}
