use image::ColorType;
use napi::bindgen_prelude::*;

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
