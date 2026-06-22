use image::DynamicImage;
use napi::bindgen_prelude::*;

/// Brands that mark an ISOBMFF/HEIF container as HEVC-coded image content (HEIC/HEIF).
const HEIF_BRANDS: [&[u8; 4]; 11] = [
  b"heic", b"heix", b"heim", b"heis", b"hevc", b"hevx", b"hevm", b"hevs", b"mif1", b"msf1", b"heif",
];

/// AVIF brands. They share the ISOBMFF container with HEIF but must keep the existing
/// libavif/`guess_format` path, so a major-brand AVIF file is never reported as HEIC.
const AVIF_BRANDS: [&[u8; 4]; 2] = [b"avif", b"avis"];

/// Detect an ISOBMFF/HEIF container holding HEVC-coded image(s), i.e. a `.heic`/`.heif` file.
/// Returns false for AVIF (which shares the container but must keep its existing libavif path)
/// and for every non-HEIF input.
pub fn is_heic(buf: &[u8]) -> bool {
  // An `ftyp` box needs at least the 4-byte size, 4-byte type, and a 4-byte major brand.
  if buf.len() < 12 {
    return false;
  }
  // Bytes [4..8] are the box type; only `ftyp` carries the brand list we sniff.
  if &buf[4..8] != b"ftyp" {
    return false;
  }

  let major = &buf[8..12];
  // AVIF shares the container; its major brand must defer to the existing AVIF path.
  if AVIF_BRANDS.iter().any(|brand| major == *brand) {
    return false;
  }
  if HEIF_BRANDS.iter().any(|brand| major == *brand) {
    return true;
  }

  // Compatible brands start at offset 16 (after size + type + major + 4-byte minor version).
  // The big-endian box size at [0..4] bounds the scan; clamp it to the actual buffer length
  // so an oversized/zero/garbage size can neither over-read nor underflow the loop.
  let box_size = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
  let end = box_size.min(buf.len());

  let mut offset = 16;
  // Step 4 bytes per brand; a trailing partial (<4 byte) chunk is ignored.
  while offset + 4 <= end {
    let brand = &buf[offset..offset + 4];
    if HEIF_BRANDS.iter().any(|b| brand == *b) {
      return true;
    }
    offset += 4;
  }

  false
}

/// Un-premultiply a single 8-bit color channel by its alpha, recovering straight alpha.
///
/// CoreGraphics bitmap contexts only render premultiplied alpha, but the rest of the pipeline
/// (and `image::Rgba`) expects straight/un-associated alpha. For a fully-opaque pixel
/// (`a == 255`) this is the identity, so opaque camera HEICs round-trip exactly. The
/// `+ a/2` term rounds to nearest instead of truncating; the result is clamped to the channel max.
#[inline]
fn unpremultiply_u8(c: u8, a: u8) -> u8 {
  if a == 0 {
    0
  } else {
    ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8
  }
}

/// Un-premultiply a single 16-bit color channel by its alpha, recovering straight alpha.
/// See [`unpremultiply_u8`]; identical logic scaled to the 16-bit channel max (65535).
#[inline]
fn unpremultiply_u16(c: u16, a: u16) -> u16 {
  if a == 0 {
    0
  } else {
    ((c as u32 * 65535 + a as u32 / 2) / a as u32).min(65535) as u16
  }
}

/// Decode a HEIC/HEIF image to a `DynamicImage` plus EXIF orientation (1..8) if present.
/// macOS-only (delegates to the OS ImageIO HEVC decoder); errors elsewhere.
#[cfg(not(target_os = "macos"))]
pub(crate) fn decode_heic(_buf: &[u8]) -> Result<(DynamicImage, Option<u16>)> {
  Err(Error::new(
    Status::InvalidArg,
    "HEIC decoding is only supported on macOS".to_owned(),
  ))
}

/// macOS HEIC decode via Apple's ImageIO + CoreGraphics. We ship no HEVC codec; Apple's OS holds
/// the patent license, so all decoding goes through OS API calls.
///
/// Pipeline: input bytes -> `CFData` -> `CGImageSource` -> primary-image properties (bit depth and
/// EXIF orientation) -> `CGImage` -> render into our own sRGB premultiplied `CGBitmapContext`
/// (8-bit RGBA8 or 16-bit RGBA16) -> read the buffer back -> un-premultiply to straight alpha ->
/// `DynamicImage`. Orientation is returned, not baked into pixels (the pipeline rotates later,
/// same as JPEG). Every null/false OS result becomes a clean `napi::Error`; no `unwrap`/panic.
#[cfg(target_os = "macos")]
pub(crate) fn decode_heic(buf: &[u8]) -> Result<(DynamicImage, Option<u16>)> {
  use objc2::rc::autoreleasepool;
  use objc2_core_foundation::{
    CFData, CFDictionary, CFNumber, CFRetained, CGPoint, CGRect, CGSize,
  };
  use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo, kCGColorSpaceSRGB,
  };
  use objc2_image_io::{CGImageSource, kCGImagePropertyDepth, kCGImagePropertyOrientation};

  /// Look up a CFNumber-valued key in an ImageIO property dictionary and read it as `i32`.
  /// Returns `None` when the key is absent or not a number. The borrowed key/dict outlive the call.
  fn dict_i32(dict: &CFDictionary, key: &objc2_core_foundation::CFString) -> Option<i32> {
    // The properties dictionary is opaque (`CFType` values); fetch the raw value pointer and
    // downcast it to a concrete `CFNumber` before reading. The pointer is only borrowed for the
    // duration of this function, while `dict` (and thus the value) is alive.
    let value_ptr = unsafe { dict.value(key as *const _ as *const core::ffi::c_void) };
    if value_ptr.is_null() {
      return None;
    }
    let cf_type = unsafe { &*(value_ptr as *const objc2_core_foundation::CFType) };
    let number = cf_type.downcast_ref::<CFNumber>()?;
    number.as_i32()
  }

  autoreleasepool(|_pool| {
    // 1. bytes -> CFData (copies the input; the CFData owns its own buffer).
    let data = CFData::from_bytes(buf);

    // 2. CFData -> CGImageSource.
    let source = unsafe { CGImageSource::with_data(&data, None) }.ok_or_else(|| {
      Error::new(
        Status::InvalidArg,
        "HEIC: not a decodable HEIC (CGImageSource::with_data returned null)".to_owned(),
      )
    })?;

    // 3. Primary-image properties: bit depth (8 vs >8) and EXIF orientation (1..8).
    let mut depth: i32 = 8;
    let mut orientation: Option<u16> = None;
    if let Some(props) = unsafe { source.properties_at_index(0, None) } {
      if let Some(d) = dict_i32(&props, unsafe { kCGImagePropertyDepth }) {
        depth = d;
      }
      if let Some(o) = dict_i32(&props, unsafe { kCGImagePropertyOrientation })
        && (1..=8).contains(&o)
      {
        orientation = Some(o as u16);
      }
    }
    let sixteen_bit = depth > 8;

    // 4. CGImageSource -> primary CGImage.
    let image = unsafe { source.image_at_index(0, None) }.ok_or_else(|| {
      Error::new(
        Status::InvalidArg,
        "HEIC: failed to create image (CGImageSource::image_at_index returned null)".to_owned(),
      )
    })?;
    let width = CGImage::width(Some(&image));
    let height = CGImage::height(Some(&image));
    if width == 0 || height == 0 {
      return Err(Error::new(
        Status::InvalidArg,
        "HEIC: decoded image has zero dimensions".to_owned(),
      ));
    }

    // 5. sRGB colorspace for both depths. CG color-matches wide-gamut (e.g. Display-P3) down to
    //    sRGB when drawing; 16-bit preserves precision, not gamut. Matches v1 documented behavior.
    let color_space: CFRetained<CGColorSpace> =
      CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })).ok_or_else(|| {
        Error::new(
          Status::GenericFailure,
          "HEIC: failed to create sRGB color space".to_owned(),
        )
      })?;

    // Guard against width*height*channels overflow on absurd dimensions.
    let pixels = width
      .checked_mul(height)
      .and_then(|p| p.checked_mul(4))
      .ok_or_else(|| {
        Error::new(
          Status::InvalidArg,
          "HEIC: image dimensions overflow".to_owned(),
        )
      })?;

    let rect = CGRect::new(
      CGPoint::new(0.0, 0.0),
      CGSize::new(width as f64, height as f64),
    );

    // 6 + 7. Render premultiplied into our own buffer, then un-premultiply to straight alpha and
    //        build the DynamicImage. CGBitmapContext is premultiplied-only.
    if sixteen_bit {
      // 16-bit RGBA, native-endian (ByteOrder16Host) so the buffer reads back as `u16` directly.
      let mut pixel_buf: Vec<u16> = vec![0u16; pixels];
      let bytes_per_row = width
        .checked_mul(8)
        .ok_or_else(|| Error::new(Status::InvalidArg, "HEIC: bytesPerRow overflow".to_owned()))?;
      // "Host" byte order: pick the variant matching the target endianness so the rendered buffer
      // reads back directly as native-endian `u16` (every Apple platform is little-endian today).
      let host_16: CGImageByteOrderInfo = if cfg!(target_endian = "little") {
        CGImageByteOrderInfo::Order16Little
      } else {
        CGImageByteOrderInfo::Order16Big
      };
      let bitmap_info: u32 = CGImageAlphaInfo::PremultipliedLast.0 | host_16.0;
      {
        let context = unsafe {
          CGBitmapContextCreate(
            pixel_buf.as_mut_ptr() as *mut core::ffi::c_void,
            width,
            height,
            16,
            bytes_per_row,
            Some(&color_space),
            bitmap_info,
          )
        }
        .ok_or_else(|| {
          Error::new(
            Status::GenericFailure,
            "HEIC: failed to create 16-bit bitmap context".to_owned(),
          )
        })?;
        // `image` and `pixel_buf` both outlive this draw; the context is dropped before we move
        // `pixel_buf`, flushing all pixels into the buffer.
        CGContext::draw_image(Some(&context), rect, Some(&image));
      }
      // Un-premultiply R,G,B by A in place; leave A untouched.
      for px in pixel_buf.chunks_exact_mut(4) {
        let a = px[3];
        px[0] = unpremultiply_u16(px[0], a);
        px[1] = unpremultiply_u16(px[1], a);
        px[2] = unpremultiply_u16(px[2], a);
      }
      let img =
        image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(width as u32, height as u32, pixel_buf)
          .map(DynamicImage::ImageRgba16)
          .ok_or_else(|| {
            Error::new(
              Status::GenericFailure,
              "HEIC: buffer size mismatch (16-bit)".to_owned(),
            )
          })?;
      Ok((img, orientation))
    } else {
      // 8-bit RGBA, ByteOrder32Big so the memory layout is R,G,B,A.
      let mut pixel_buf: Vec<u8> = vec![0u8; pixels];
      let bytes_per_row = width
        .checked_mul(4)
        .ok_or_else(|| Error::new(Status::InvalidArg, "HEIC: bytesPerRow overflow".to_owned()))?;
      let bitmap_info: u32 =
        CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
      {
        let context = unsafe {
          CGBitmapContextCreate(
            pixel_buf.as_mut_ptr() as *mut core::ffi::c_void,
            width,
            height,
            8,
            bytes_per_row,
            Some(&color_space),
            bitmap_info,
          )
        }
        .ok_or_else(|| {
          Error::new(
            Status::GenericFailure,
            "HEIC: failed to create 8-bit bitmap context".to_owned(),
          )
        })?;
        CGContext::draw_image(Some(&context), rect, Some(&image));
      }
      for px in pixel_buf.chunks_exact_mut(4) {
        let a = px[3];
        px[0] = unpremultiply_u8(px[0], a);
        px[1] = unpremultiply_u8(px[1], a);
        px[2] = unpremultiply_u8(px[2], a);
      }
      let img =
        image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(width as u32, height as u32, pixel_buf)
          .map(DynamicImage::ImageRgba8)
          .ok_or_else(|| {
            Error::new(
              Status::GenericFailure,
              "HEIC: buffer size mismatch (8-bit)".to_owned(),
            )
          })?;
      Ok((img, orientation))
    }
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Build a minimal ISOBMFF `ftyp` box: `[size][b"ftyp"][major][minor][compatible...]`.
  /// `size` is the big-endian box size written into bytes `[0..4]`; when `None` it is set to
  /// the actual byte length so the compatible-brand scan covers every brand.
  fn ftyp(major: &[u8; 4], minor: &[u8; 4], compatible: &[&[u8; 4]], size: Option<u32>) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&[0, 0, 0, 0]); // size placeholder
    buf.extend_from_slice(b"ftyp");
    buf.extend_from_slice(major);
    buf.extend_from_slice(minor);
    for brand in compatible {
      buf.extend_from_slice(*brand);
    }
    let len = size.unwrap_or(buf.len() as u32);
    buf[0..4].copy_from_slice(&len.to_be_bytes());
    buf
  }

  // --- Positive: HEIF/HEVC major brands ---

  #[test]
  fn major_brand_heic_is_heic() {
    assert!(is_heic(&ftyp(b"heic", b"\0\0\0\0", &[], None)));
  }

  #[test]
  fn major_brand_heix_is_heic() {
    assert!(is_heic(&ftyp(b"heix", b"\0\0\0\0", &[], None)));
  }

  #[test]
  fn major_brand_mif1_is_heic() {
    assert!(is_heic(&ftyp(b"mif1", b"\0\0\0\0", &[], None)));
  }

  #[test]
  fn all_heif_major_brands_are_heic() {
    for brand in [
      b"heic", b"heix", b"heim", b"heis", b"hevc", b"hevx", b"hevm", b"hevs", b"mif1", b"msf1",
      b"heif",
    ] {
      assert!(is_heic(&ftyp(brand, b"\0\0\0\0", &[], None)), "{brand:?}");
    }
  }

  // --- Positive: HEIF brand only present as a compatible brand ---

  #[test]
  fn compatible_brand_heic_is_heic() {
    // Major brand is non-HEIF filler; `heic` appears only in the compatible-brand list.
    assert!(is_heic(&ftyp(b"abcd", b"\0\0\0\0", &[b"heic"], None)));
  }

  #[test]
  fn compatible_brand_after_filler_is_heic() {
    // `heic` is the second compatible brand, exercising the bounded scan loop.
    assert!(is_heic(&ftyp(
      b"abcd",
      b"\0\0\0\0",
      &[b"wxyz", b"heic"],
      None
    )));
  }

  // --- Negative: AVIF must not be treated as HEIC ---

  #[test]
  fn major_brand_avif_is_not_heic() {
    assert!(!is_heic(&ftyp(b"avif", b"\0\0\0\0", &[], None)));
  }

  #[test]
  fn major_brand_avis_is_not_heic() {
    assert!(!is_heic(&ftyp(b"avis", b"\0\0\0\0", &[], None)));
  }

  #[test]
  fn avif_major_with_heif_compatible_is_not_heic() {
    // The major-brand AVIF guard must win even when `mif1` is listed as compatible.
    assert!(!is_heic(&ftyp(b"avif", b"\0\0\0\0", &[b"mif1"], None)));
  }

  // --- Negative: non-ISOBMFF / malformed inputs ---

  #[test]
  fn png_signature_is_not_heic() {
    assert!(!is_heic(b"\x89PNG\r\n\x1a\n\0\0\0\0"));
  }

  #[test]
  fn jpeg_signature_is_not_heic() {
    assert!(!is_heic(b"\xFF\xD8\xFF\xE0\0\x10JFIF\0\x01"));
  }

  #[test]
  fn ftyp_at_wrong_offset_is_not_heic() {
    // `ftyp` present but not at bytes [4..8].
    assert!(!is_heic(b"\0\0ftyp\0\0heic"));
  }

  #[test]
  fn empty_buffer_is_not_heic() {
    assert!(!is_heic(&[]));
  }

  #[test]
  fn truncated_eight_byte_buffer_is_not_heic() {
    assert!(!is_heic(b"\0\0\0\x18ftyp"));
  }

  // --- Robustness: no panics on adversarial box sizes / odd lengths ---

  #[test]
  fn huge_box_size_does_not_panic() {
    // Box size claims far more bytes than the buffer holds; scan must clamp to buf.len().
    assert!(is_heic(&ftyp(
      b"abcd",
      b"\0\0\0\0",
      &[b"heic"],
      Some(u32::MAX)
    )));
  }

  #[test]
  fn zero_box_size_does_not_panic() {
    // A zero/short box size must still allow the major brand to be inspected.
    assert!(is_heic(&ftyp(b"heic", b"\0\0\0\0", &[], Some(0))));
  }

  #[test]
  fn trailing_partial_brand_is_ignored() {
    // 12-byte ftyp header + major `heic`, then 2 stray bytes (a partial compatible brand).
    let mut buf = ftyp(b"abcd", b"\0\0\0\0", &[], None);
    buf.extend_from_slice(b"he"); // partial chunk, must be ignored without panic
    assert!(!is_heic(&buf));
  }

  // --- decode_heic: non-macOS stub still errors (macOS now decodes real fixtures) ---

  #[cfg(not(target_os = "macos"))]
  #[test]
  fn decode_heic_stub_errors_off_macos() {
    // Off macOS the stub returns an error (no OS HEVC decoder to delegate to).
    let buf = ftyp(b"heic", b"\0\0\0\0", &[], None);
    assert!(super::decode_heic(&buf).is_err());
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn decode_heic_rejects_garbage_on_macos() {
    // A bare ftyp box with no HEVC payload is not a decodable image; the OS returns null and we
    // surface a clean error rather than panicking.
    let buf = ftyp(b"heic", b"\0\0\0\0", &[], None);
    assert!(super::decode_heic(&buf).is_err());
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn decode_heic_8bit_fixture_is_rgba8() {
    // Real ImageIO decode of the committed 8-bit fixture: dimensions match and the 8-bit depth
    // branch yields an RGBA8 DynamicImage. Fixtures live at the repo root (two dirs up).
    let bytes = std::fs::read(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/../../un-optimized.heic"
    ))
    .expect("read 8-bit heic fixture");
    let (img, _orientation) = super::decode_heic(&bytes).expect("decode 8-bit heic");
    assert_eq!(img.width(), 1024);
    assert_eq!(img.height(), 681);
    assert!(matches!(img, DynamicImage::ImageRgba8(_)));
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn decode_heic_10bit_fixture_is_rgba16() {
    // Real ImageIO decode of the committed genuine 10-bit fixture: `kCGImagePropertyDepth` reports
    // > 8, so the 16-bit branch yields an RGBA16 DynamicImage.
    let bytes = std::fs::read(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/../../un-optimized-10bit.heic"
    ))
    .expect("read 10-bit heic fixture");
    let (img, _orientation) = super::decode_heic(&bytes).expect("decode 10-bit heic");
    assert_eq!(img.width(), 256);
    assert_eq!(img.height(), 256);
    assert!(matches!(img, DynamicImage::ImageRgba16(_)));
  }

  // --- Un-premultiply helpers (pure, the trickiest logic) ---

  #[test]
  fn unpremultiply_u8_opaque_is_noop() {
    // Fully opaque (a == 255) must be the identity so opaque camera HEICs are exact.
    assert_eq!(super::unpremultiply_u8(200, 255), 200);
    assert_eq!(super::unpremultiply_u8(0, 255), 0);
    assert_eq!(super::unpremultiply_u8(255, 255), 255);
  }

  #[test]
  fn unpremultiply_u8_half_alpha_doubles() {
    // A premultiplied 50 at ~half alpha (128) recovers to ~100 (rounded).
    assert_eq!(super::unpremultiply_u8(50, 128), 100);
  }

  #[test]
  fn unpremultiply_u8_zero_alpha_is_zero() {
    assert_eq!(super::unpremultiply_u8(0, 0), 0);
    assert_eq!(super::unpremultiply_u8(200, 0), 0);
  }

  #[test]
  fn unpremultiply_u8_clamps_at_max() {
    // A nonsensical premultiplied value larger than alpha would overflow 255; it must clamp.
    assert_eq!(super::unpremultiply_u8(200, 100), 255);
  }

  #[test]
  fn unpremultiply_u16_opaque_is_noop() {
    assert_eq!(super::unpremultiply_u16(50000, 65535), 50000);
    assert_eq!(super::unpremultiply_u16(0, 65535), 0);
    assert_eq!(super::unpremultiply_u16(65535, 65535), 65535);
  }

  #[test]
  fn unpremultiply_u16_half_alpha_doubles() {
    // ~half alpha (32768) doubles the premultiplied channel (rounded to nearest).
    assert_eq!(super::unpremultiply_u16(10000, 32768), 20000);
  }

  #[test]
  fn unpremultiply_u16_zero_alpha_is_zero() {
    assert_eq!(super::unpremultiply_u16(0, 0), 0);
    assert_eq!(super::unpremultiply_u16(40000, 0), 0);
  }

  #[test]
  fn unpremultiply_u16_clamps_at_max() {
    assert_eq!(super::unpremultiply_u16(40000, 20000), 65535);
  }
}
