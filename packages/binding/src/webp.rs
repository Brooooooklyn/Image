use image::{DynamicImage, Rgb, Rgba, buffer::ConvertBuffer};
use napi::bindgen_prelude::*;

#[inline]
pub(crate) unsafe fn lossless_encode_webp_inner(
  input: &DynamicImage,
  width: u32,
  height: u32,
) -> Result<(*mut u8, usize)> {
  unsafe {
    let mut out_buf = std::ptr::null_mut();
    let len = match input {
      DynamicImage::ImageRgb8(input) => {
        let stride = width as i32 * 3;
        libwebp_sys::WebPEncodeLosslessRGB(
          input.as_ptr(),
          width as i32,
          height as i32,
          stride,
          &mut out_buf,
        )
      }
      DynamicImage::ImageRgba8(input) => {
        let stride = width as i32 * 4;
        libwebp_sys::WebPEncodeLosslessRGBA(
          input.as_ptr(),
          width as i32,
          height as i32,
          stride,
          &mut out_buf,
        )
      }
      // WebP is 8-bit; normalize any other color type (16-bit Rgb16/Rgba16 from HEIC,
      // 16-bit PNG/TIFF, etc.) down to RGBA8.
      other => {
        let converted = other.to_rgba8();
        let stride = width as i32 * 4;
        libwebp_sys::WebPEncodeLosslessRGBA(
          converted.as_ptr(),
          width as i32,
          height as i32,
          stride,
          &mut out_buf,
        )
      }
    };
    Ok((out_buf, len))
  }
}

#[inline]
pub(crate) unsafe fn encode_webp_inner(
  input: &DynamicImage,
  quality_factor: u32,
  width: u32,
  height: u32,
) -> Result<(*mut u8, usize)> {
  unsafe {
    let mut out_buf = std::ptr::null_mut();
    let len = match input {
      DynamicImage::ImageRgb8(input) => {
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
      DynamicImage::ImageRgba8(input) => {
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
      DynamicImage::ImageLuma8(input) => {
        let stride = width as i32 * 3;
        let converted: image::ImageBuffer<Rgb<u8>, _> = input.convert();
        libwebp_sys::WebPEncodeRGB(
          converted.as_ptr(),
          width as i32,
          height as i32,
          stride,
          quality_factor as f32,
          &mut out_buf,
        )
      }
      DynamicImage::ImageLumaA8(input) => {
        let stride = width as i32 * 4;
        let converted: image::ImageBuffer<Rgba<u8>, _> = input.convert();
        libwebp_sys::WebPEncodeRGBA(
          converted.as_ptr(),
          width as i32,
          height as i32,
          stride,
          quality_factor as f32,
          &mut out_buf,
        )
      }
      // WebP is 8-bit; normalize any other color type (16-bit Rgb16/Rgba16 from HEIC,
      // 16-bit PNG/TIFF, etc.) down to RGBA8.
      _ => {
        let converted = input.to_rgba8();
        let stride = width as i32 * 4;
        libwebp_sys::WebPEncodeRGBA(
          converted.as_ptr(),
          width as i32,
          height as i32,
          stride,
          quality_factor as f32,
          &mut out_buf,
        )
      }
    };
    Ok((out_buf, len))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use image::{DynamicImage, ImageBuffer, Rgb, Rgba};

  /// A solid 4x4 16-bit RGBA image. WebP is 8-bit, so the encoder must normalize this down to
  /// RGBA8 instead of rejecting it (the pre-existing bug this task fixes).
  fn rgba16_4x4() -> DynamicImage {
    let buf = ImageBuffer::<Rgba<u16>, _>::from_raw(4, 4, vec![32768u16; 4 * 4 * 4])
      .expect("rgba16 buffer");
    DynamicImage::ImageRgba16(buf)
  }

  /// A solid 4x4 16-bit RGB image for the lossless path.
  fn rgb16_4x4() -> DynamicImage {
    let buf =
      ImageBuffer::<Rgb<u16>, _>::from_raw(4, 4, vec![32768u16; 4 * 4 * 3]).expect("rgb16 buffer");
    DynamicImage::ImageRgb16(buf)
  }

  /// A solid 4x4 8-bit RGBA image (existing fast-path regression guard).
  fn rgba8_4x4() -> DynamicImage {
    let buf =
      ImageBuffer::<Rgba<u8>, _>::from_raw(4, 4, vec![128u8; 4 * 4 * 4]).expect("rgba8 buffer");
    DynamicImage::ImageRgba8(buf)
  }

  #[test]
  fn encode_webp_inner_normalizes_rgba16() {
    let image = rgba16_4x4();
    let (ptr, len) =
      unsafe { encode_webp_inner(&image, 75, image.width(), image.height()) }.expect("encode ok");
    assert!(!ptr.is_null(), "output buffer must not be null");
    assert!(len > 0, "output length must be > 0");
    unsafe { libwebp_sys::WebPFree(ptr as *mut _) };
  }

  #[test]
  fn lossless_encode_webp_inner_normalizes_rgb16() {
    let image = rgb16_4x4();
    let (ptr, len) = unsafe { lossless_encode_webp_inner(&image, image.width(), image.height()) }
      .expect("encode ok");
    assert!(!ptr.is_null(), "output buffer must not be null");
    assert!(len > 0, "output length must be > 0");
    unsafe { libwebp_sys::WebPFree(ptr as *mut _) };
  }

  #[test]
  fn encode_webp_inner_rgba8_fast_path_unaffected() {
    let image = rgba8_4x4();
    let (ptr, len) =
      unsafe { encode_webp_inner(&image, 75, image.width(), image.height()) }.expect("encode ok");
    assert!(!ptr.is_null(), "output buffer must not be null");
    assert!(len > 0, "output length must be > 0");
    unsafe { libwebp_sys::WebPFree(ptr as *mut _) };
  }

  #[test]
  fn lossless_encode_webp_inner_rgba8_fast_path_unaffected() {
    let image = rgba8_4x4();
    let (ptr, len) = unsafe { lossless_encode_webp_inner(&image, image.width(), image.height()) }
      .expect("encode ok");
    assert!(!ptr.is_null(), "output buffer must not be null");
    assert!(len > 0, "output length must be > 0");
    unsafe { libwebp_sys::WebPFree(ptr as *mut _) };
  }
}
