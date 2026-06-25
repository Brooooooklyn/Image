use image::DynamicImage;
use napi::bindgen_prelude::*;
use napi_derive::napi;

/// Options for HEIC encoding via Apple's `CGImageDestination` (macOS only).
///
/// Ungated so the napi signature and generated `index.d.ts` stay identical on every platform;
/// only the encode *implementation* ([`encode_heic`]) is macOS-gated.
#[napi(object)]
#[derive(Default, Clone)]
pub struct HeicConfig {
  /// Lossy quality 0-100 (default 80, matches AVIF). Mapped to ImageIO's
  /// `kCGImageDestinationLossyCompressionQuality`, but the compression ceiling is CLAMPED to 0.9:
  /// HEIC/HEVC via ImageIO has no truly-lossless mode, and compression 1.0 engages a near-lossless
  /// path the OS software encoder rejects on hosts without a hardware media engine. So `quality`
  /// 90-100 all map to 0.9 (a ~1-3/255 residual remains regardless). See `encode_heic`.
  pub quality: Option<u32>,
  /// Output bit depth, 8 or 10. Default: follow the source (16-bit `DynamicImage` -> 10, else 8).
  pub bit_depth: Option<u8>,
}

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

  // AVIF shares the ISOBMFF container with HEIF. If `avif`/`avis` appears ANYWHERE in the brand
  // list (major brand or any compatible brand) the file must defer to the existing libavif path,
  // even when the major brand is a generic HEIF brand like `mif1`/`msf1`/`heif`. This AVIF veto is
  // checked BEFORE any HEIF-positive, so a single pass collects both a HEIF hit and an AVIF veto and
  // the veto wins.
  let major = &buf[8..12];

  // Box-size bound for the compatible-brand scan. The big-endian box size at [0..4] would normally
  // bound the scan, but a single `box_size.min(len)` bound breaks size-0 ("to end of file"): a
  // declared size of 0 collapses the bound to 0 and the major brand at [8..12] is never reached.
  // So: only trust a declared size that is at least a full minimal ftyp (>= 16); otherwise scan to
  // the end of the buffer. The major brand below is inspected unconditionally regardless of `end`.
  let declared = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
  let end = if declared >= 16 {
    declared.min(buf.len())
  } else {
    buf.len()
  };

  // Single pass: inspect the major brand, then every compatible brand from offset 16 (after
  // size + type + major + 4-byte minor version), stepping 4 bytes and ignoring a trailing partial
  // (<4 byte) chunk. Record whether any HEIF brand was seen; bail immediately on any AVIF brand.
  let is_avif = |brand: &[u8]| AVIF_BRANDS.iter().any(|b| brand == *b);
  let is_heif = |brand: &[u8]| HEIF_BRANDS.iter().any(|b| brand == *b);

  if is_avif(major) {
    return false;
  }
  let mut found_heif = is_heif(major);

  let mut offset = 16;
  while offset + 4 <= end {
    let brand = &buf[offset..offset + 4];
    if is_avif(brand) {
      return false;
    }
    if is_heif(brand) {
      found_heif = true;
    }
    offset += 4;
  }

  found_heif
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

/// Reinterpret a native-endian `u16` framebuffer as raw bytes with no copy, for feeding a 16-bit
/// CGImage in [`encode_heic`]. macOS is always little-endian, so the native bytes already match the
/// `Order16Little` byte order we declare. `align_to::<u8>()` is sound here (every `u16` bit pattern
/// is a valid pair of `u8`s) and never splits: `u8` has alignment 1 and `size_of::<u16>()` is a
/// multiple of `size_of::<u8>()`, so head/tail are always empty (asserted in debug).
#[cfg(target_os = "macos")]
#[inline]
fn u16_slice_as_native_bytes(slice: &[u16]) -> &[u8] {
  let (head, bytes, tail) = unsafe { slice.align_to::<u8>() };
  debug_assert!(head.is_empty() && tail.is_empty());
  bytes
}

/// Decode a HEIC/HEIF image to a `DynamicImage` plus EXIF orientation (1..8) if present.
/// macOS-only (delegates to the OS ImageIO HEVC decoder); errors elsewhere.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn decode_heic(_buf: &[u8]) -> Result<(DynamicImage, Option<u16>)> {
  Err(Error::new(
    Status::InvalidArg,
    "HEIC decoding is only supported on macOS and Windows".to_owned(),
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

/// Encode a `DynamicImage` to HEIC. macOS-only (delegates to Apple's `CGImageDestination` HEVC
/// encoder); errors elsewhere. We ship no HEVC codec — Apple's OS holds the patent license.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn encode_heic(_img: &DynamicImage, _opts: Option<HeicConfig>) -> Result<Vec<u8>> {
  Err(Error::new(
    Status::GenericFailure,
    "HEIC encoding is only available on macOS and Windows".to_owned(),
  ))
}

/// macOS HEIC encode via Apple's `CGImageDestination` (UTI `public.heic`). We ship no HEVC codec;
/// Apple's OS holds the patent license, so all encoding goes through OS API calls.
///
/// Pipeline: `DynamicImage` -> RGBA pixel bytes (8- or 16-bit, STRAIGHT alpha — `CGImage` accepts
/// non-premultiplied `kCGImageAlphaLast`, so no premultiply is needed unlike the decode-side bitmap
/// context) -> `CFData` -> CF-backed `CGDataProvider` (keeps the bytes alive for the image's
/// lifetime) -> `CGImage` -> `CGImageDestination("public.heic")` with a lossy-quality property ->
/// finalize into a `CFMutableData` -> copy out to `Vec<u8>`. Orientation is NOT tagged: the
/// pipeline already produced upright pixels, so tagging would double-rotate. Every null/false OS
/// result becomes a clean `napi::Error`; no `unwrap`/panic.
#[cfg(target_os = "macos")]
pub(crate) fn encode_heic(img: &DynamicImage, opts: Option<HeicConfig>) -> Result<Vec<u8>> {
  use std::borrow::Cow;

  use objc2::rc::autoreleasepool;
  use objc2_core_foundation::{
    CFData, CFDictionary, CFMutableData, CFNumber, CFRetained, CFString,
  };
  use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo, kCGColorSpaceSRGB,
  };
  use objc2_image_io::{CGImageDestination, kCGImageDestinationLossyCompressionQuality};

  let opts = opts.unwrap_or_default();
  // Compression quality 0.0-1.0. We CLAMP the maximum to 0.9: ImageIO HEIC has no truly-lossless
  // mode, and compression 1.0 engages a near-lossless path that the OS *software* HEVC encoder
  // (used on hosts without a hardware media engine, e.g. paravirtualized CI VMs) cannot satisfy, so
  // `CGImageDestinationFinalize` returns false there. 0.9 is visually indistinguishable from 1.0
  // (a ~1-3/255 residual remains either way) yet encodes on every host, so `quality: 90..=100` all
  // map to 0.9 deterministically — no runtime fallback, no host-dependent behavior to mask failures.
  const MAX_COMPRESSION_QUALITY: f64 = 0.9;
  let quality_value =
    ((opts.quality.unwrap_or(80).min(100) as f64) / 100.0).min(MAX_COMPRESSION_QUALITY);
  // Resolve output depth: explicit bit_depth wins; otherwise follow a 16-bit source.
  // 10-bit HEIC is produced purely by feeding a 16-bpc CGImage below: ImageIO infers HEVC Main10
  // from the source bit depth (we set no explicit depth property). The `heic.spec.mjs` 10-bit
  // round-trip re-decodes the output and asserts `Rgba16`, which proves the encoded file is really
  // >8-bit (our decoder yields Rgba16 only when the source depth > 8).
  let sixteen_bit = match opts.bit_depth {
    Some(8) => false,
    Some(10) => true,
    _ => matches!(
      img,
      DynamicImage::ImageRgba16(_)
        | DynamicImage::ImageRgb16(_)
        | DynamicImage::ImageLuma16(_)
        | DynamicImage::ImageLumaA16(_)
    ),
  };

  let width = img.width() as usize;
  let height = img.height() as usize;
  if width == 0 || height == 0 {
    return Err(Error::new(
      Status::InvalidArg,
      "HEIC: image has zero dimensions".to_owned(),
    ));
  }

  autoreleasepool(|_pool| {
    // sRGB colorspace (same as decode); 16-bit preserves precision, not gamut.
    let color_space: CFRetained<CGColorSpace> =
      CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })).ok_or_else(|| {
        Error::new(
          Status::GenericFailure,
          "HEIC: failed to create sRGB color space".to_owned(),
        )
      })?;

    // 1. Build STRAIGHT-alpha RGBA pixel bytes + the matching CGImage layout parameters.
    //    `pixel_bytes` is borrowed straight from the source `DynamicImage` when it is already the
    //    target RGBA layout (the common decode->encode round-trip), and only converted/owned when a
    //    pixel format change is genuinely needed. Either way it is copied into a CFData below, so it
    //    only needs to live until then.
    let (pixel_bytes, bits_per_component, bits_per_pixel, bytes_per_row, bitmap_info): (
      Cow<[u8]>,
      usize,
      usize,
      usize,
      CGBitmapInfo,
    ) = if sixteen_bit {
      let bytes_per_row = width
        .checked_mul(8)
        .ok_or_else(|| Error::new(Status::InvalidArg, "HEIC: bytesPerRow overflow".to_owned()))?;
      // 16-bit RGBA as native-endian u16s, reinterpreted as bytes with no copy (macOS is LE, so this
      // matches Order16Little). Borrow the source buffer directly when it is already Rgba16; convert
      // only otherwise. (`to_rgba16()` clones even when the variant already matches, so the borrow
      // avoids a full-framebuffer clone plus the old per-component byte-build loop.)
      let pixels: Cow<[u8]> = match img {
        DynamicImage::ImageRgba16(buf) => Cow::Borrowed(u16_slice_as_native_bytes(buf.as_raw())),
        other => Cow::Owned(u16_slice_as_native_bytes(other.to_rgba16().as_raw()).to_vec()),
      };
      let host_16: CGImageByteOrderInfo = if cfg!(target_endian = "little") {
        CGImageByteOrderInfo::Order16Little
      } else {
        CGImageByteOrderInfo::Order16Big
      };
      (
        pixels,
        16,
        64,
        bytes_per_row,
        CGBitmapInfo(CGImageAlphaInfo::Last.0 | host_16.0),
      )
    } else {
      let bytes_per_row = width
        .checked_mul(4)
        .ok_or_else(|| Error::new(Status::InvalidArg, "HEIC: bytesPerRow overflow".to_owned()))?;
      // 8-bit RGBA; Order32Big gives the memory layout R,G,B,A. Borrow the source buffer directly
      // when it is already Rgba8 (avoids `to_rgba8()`'s clone-when-already-Rgba8); convert otherwise.
      let pixels: Cow<[u8]> = match img {
        DynamicImage::ImageRgba8(buf) => Cow::Borrowed(buf.as_raw().as_slice()),
        other => Cow::Owned(other.to_rgba8().into_raw()),
      };
      (
        pixels,
        8,
        32,
        bytes_per_row,
        CGBitmapInfo(CGImageAlphaInfo::Last.0 | CGImageByteOrderInfo::Order32Big.0),
      )
    };

    // 2. Wrap the pixel bytes in a CFData (copy) and a CF-backed CGDataProvider so the bytes live
    //    as long as the provider/image (the borrowed/owned `pixel_bytes` may be freed after this).
    let data = CFData::from_bytes(&pixel_bytes);
    let provider = CGDataProvider::with_cf_data(Some(&data)).ok_or_else(|| {
      Error::new(
        Status::GenericFailure,
        "HEIC: failed to create CGDataProvider".to_owned(),
      )
    })?;

    // 3. CGImage from the provider. `decode` map = null, no interpolation flag, default intent.
    let image = unsafe {
      CGImage::new(
        width,
        height,
        bits_per_component,
        bits_per_pixel,
        bytes_per_row,
        Some(&color_space),
        bitmap_info,
        Some(&provider),
        std::ptr::null(),
        false,
        CGColorRenderingIntent::RenderingIntentDefault,
      )
    }
    .ok_or_else(|| {
      Error::new(
        Status::GenericFailure,
        "HEIC: failed to create CGImage".to_owned(),
      )
    })?;

    // 4. Empty growable CFMutableData to receive the encoded bytes.
    let out_data = CFMutableData::new(None, 0).ok_or_else(|| {
      Error::new(
        Status::GenericFailure,
        "HEIC: failed to allocate output buffer".to_owned(),
      )
    })?;

    // 5. CGImageDestination targeting UTI "public.heic" for a single image.
    let uti = CFString::from_static_str("public.heic");
    let dest =
      unsafe { CGImageDestination::with_data(&out_data, &uti, 1, None) }.ok_or_else(|| {
        Error::new(
          Status::GenericFailure,
          "HEIC: encoder unavailable (CGImageDestination)".to_owned(),
        )
      })?;

    // 6. Properties: lossy compression quality (0.0-1.0, already clamped to MAX_COMPRESSION_QUALITY).
    let quality_number = CFNumber::new_f64(quality_value);
    let quality_key: &CFString = unsafe { kCGImageDestinationLossyCompressionQuality };
    // `from_slices` yields a typed `CFDictionary<CFString, CFNumber>`; `add_image` wants the
    // type-erased `CFDictionary<Opaque, Opaque>`. The key/value are CFTypes, so the cast is sound.
    let typed_props: CFRetained<CFDictionary<CFString, CFNumber>> =
      CFDictionary::from_slices(&[quality_key], &[&*quality_number]);
    let props: CFRetained<CFDictionary> =
      unsafe { CFRetained::cast_unchecked::<CFDictionary>(typed_props) };

    // 7. Add the image with the quality properties and finalize. With the quality clamp above there is
    //    no near-lossless path to reject, so a `finalize` failure here is a genuine error (bad input,
    //    allocation failure, no HEVC encoder at all) — surface it rather than silently retrying.
    unsafe { CGImageDestination::add_image(&dest, &image, Some(&props)) };
    if !unsafe { CGImageDestination::finalize(&dest) } {
      return Err(Error::new(
        Status::GenericFailure,
        "HEIC: finalize failed".to_owned(),
      ));
    }

    // 8. Copy the encoded bytes out of the CF data (it is freed at scope end).
    let cf_data: &CFData = &out_data;
    Ok(unsafe { cf_data.as_bytes_unchecked() }.to_vec())
  })
}

/// Thread-local COM init (MTA). napi runs encode/decode on libuv worker threads, so COM must be
/// initialized per-thread, not per-process. We never `CoUninitialize` (the worker pool is shared and
/// another addon may rely on COM). S_FALSE / RPC_E_CHANGED_MODE are both fine — WIC works regardless
/// of the thread's apartment, so we ignore the returned HRESULT.
#[cfg(target_os = "windows")]
fn ensure_com_initialized() {
  use std::cell::Cell;
  thread_local! { static INIT: Cell<bool> = const { Cell::new(false) }; }
  INIT.with(|done| {
    if !done.get() {
      unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(
          None,
          windows::Win32::System::Com::COINIT_MULTITHREADED,
        );
      }
      done.set(true);
    }
  });
}

/// Map a WIC/COM error to a `napi::Error`. When the OS HEVC/HEIF codec component is absent (the
/// common Windows Server / CI case) WIC returns `WINCODEC_ERR_COMPONENTNOTFOUND` — surface a clear,
/// actionable message instead of a raw HRESULT.
#[cfg(target_os = "windows")]
fn wic_error(context: &str, e: windows::core::Error) -> Error {
  use windows::Win32::Foundation::WINCODEC_ERR_COMPONENTNOTFOUND;
  if e.code() == WINCODEC_ERR_COMPONENTNOTFOUND {
    Error::new(
      Status::GenericFailure,
      "HEIC: the OS HEVC/HEIF codec is not installed. Install 'HEIF Image Extensions' and \
       'HEVC Video Extensions' from the Microsoft Store."
        .to_owned(),
    )
  } else {
    Error::new(
      Status::GenericFailure,
      format!("HEIC ({context}): {} (0x{:08X})", e.message(), e.code().0 as u32),
    )
  }
}

/// Create the WIC imaging factory (after ensuring COM is initialized on this thread).
#[cfg(target_os = "windows")]
fn wic_factory() -> Result<windows::Win32::Graphics::Imaging::IWICImagingFactory> {
  use windows::Win32::Graphics::Imaging::CLSID_WICImagingFactory;
  use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
  ensure_com_initialized();
  unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
    .map_err(|e| wic_error("factory", e))
}

/// Best-effort EXIF orientation (1..8) from a WIC frame; `None` if absent/unreadable. WIC does NOT
/// auto-rotate, so this is the stored tag and the pipeline applies it (same contract as macOS).
/// Untagged / orientation-1 images yield `None` (no rotation). NOTE: the WIC HEIF query path for a
/// *tagged* file is unvalidated (no tagged fixture was available); a wrong path degrades to `None`
/// (no rotation), never a double-rotation.
#[cfg(target_os = "windows")]
fn read_orientation(frame: &windows::Win32::Graphics::Imaging::IWICBitmapFrameDecode) -> Option<u16> {
  use windows::core::PCWSTR;
  use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
  use windows::Win32::System::Com::StructuredStorage::PropVariantToUInt16;
  unsafe {
    let reader = frame.GetMetadataQueryReader().ok()?;
    let name: Vec<u16> = "/ifd/{ushort=274}".encode_utf16().chain(std::iter::once(0)).collect();
    let mut value = PROPVARIANT::default();
    reader.GetMetadataByName(PCWSTR(name.as_ptr()), &mut value as *mut _).ok()?;
    let orientation = PropVariantToUInt16(&value).ok()?;
    (1..=8).contains(&orientation).then_some(orientation)
  }
}

/// Windows HEIC decode via WIC. We ship no HEVC codec; the OS/Store extension holds the patent
/// license, so all decoding goes through OS API calls. WIC normalizes HEIF to 8-bit, so the output
/// is always `Rgba8` (unlike macOS, which preserves 10-bit as `Rgba16`).
///
/// Pipeline: bytes -> IStream (copy via CreateStreamOnHGlobal) -> IWICBitmapDecoder -> frame 0 ->
/// IWICFormatConverter to straight 32bppRGBA -> CopyPixels -> `DynamicImage::ImageRgba8`. Orientation
/// is read best-effort and returned (not baked). Every null/false OS result becomes a clean `Error`.
#[cfg(target_os = "windows")]
pub(crate) fn decode_heic(buf: &[u8]) -> Result<(DynamicImage, Option<u16>)> {
  use windows::Win32::Foundation::HGLOBAL;
  use windows::Win32::Graphics::Imaging::{
    GUID_WICPixelFormat32bppRGBA, WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom,
    WICDecodeMetadataCacheOnDemand,
  };
  use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
  use windows::Win32::System::Com::STREAM_SEEK_SET;

  let factory = wic_factory()?;
  unsafe {
    // bytes -> growable HGLOBAL-backed IStream (copies; avoids any borrowed-slice lifetime trap).
    let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true.into()).map_err(|e| wic_error("stream", e))?;
    let buf_len = u32::try_from(buf.len())
      .map_err(|_| Error::new(Status::InvalidArg, "HEIC: input too large (exceeds 4 GiB)".to_owned()))?;
    let mut written = 0u32;
    stream
      .Write(buf.as_ptr() as *const _, buf_len, Some(&mut written))
      .ok()
      .map_err(|e| wic_error("stream write", e))?;
    stream.Seek(0, STREAM_SEEK_SET, None).map_err(|e| wic_error("stream seek", e))?;

    let decoder = factory
      .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
      .map_err(|e| wic_error("decode", e))?;
    let frame = decoder.GetFrame(0).map_err(|e| wic_error("frame", e))?;

    let mut width = 0u32;
    let mut height = 0u32;
    frame.GetSize(&mut width, &mut height).map_err(|e| wic_error("size", e))?;
    if width == 0 || height == 0 {
      return Err(Error::new(Status::InvalidArg, "HEIC: decoded image has zero dimensions".to_owned()));
    }

    let orientation = read_orientation(&frame);

    // WIC normalizes HEIF to 8-bit; convert to straight (non-premultiplied) 32bppRGBA.
    let converter = factory.CreateFormatConverter().map_err(|e| wic_error("converter", e))?;
    converter
      .Initialize(
        &frame,
        &GUID_WICPixelFormat32bppRGBA,
        WICBitmapDitherTypeNone,
        None,
        0.0,
        WICBitmapPaletteTypeCustom,
      )
      .map_err(|e| wic_error("convert", e))?;

    let stride = width
      .checked_mul(4)
      .ok_or_else(|| Error::new(Status::InvalidArg, "HEIC: stride overflow".to_owned()))?;
    let size = stride
      .checked_mul(height)
      .ok_or_else(|| Error::new(Status::InvalidArg, "HEIC: buffer size overflow".to_owned()))? as usize;
    // Fallible allocation: a malformed frame can report a `size` that overflows `Vec` capacity on
    // 32-bit targets (`isize::MAX`) or exhausts memory. Map either to a clean `Error` instead of
    // panicking the napi worker (`vec![0u8; size]` would panic on capacity overflow).
    let mut pixels: Vec<u8> = Vec::new();
    pixels
      .try_reserve_exact(size)
      .map_err(|_| Error::new(Status::GenericFailure, "HEIC: cannot allocate decode buffer".to_owned()))?;
    pixels.resize(size, 0);
    converter
      .CopyPixels(std::ptr::null(), stride, &mut pixels)
      .map_err(|e| wic_error("copy pixels", e))?;

    let img = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(width, height, pixels)
      .map(DynamicImage::ImageRgba8)
      .ok_or_else(|| Error::new(Status::GenericFailure, "HEIC: buffer size mismatch".to_owned()))?;
    Ok((img, orientation))
  }
}

/// Windows HEIC encode via WIC (`GUID_ContainerFormatHeif`, HEVC by default). We ship no HEVC codec;
/// the OS/Store extension holds the patent license. Output is 8-bit and OPAQUE: the WIC HEVC encoder
/// negotiates alpha away (we flatten to opaque by design) and does not emit 10-bit (`bit_depth: 10`
/// is rejected). Quality maps directly to `ImageQuality` with NO 0.9 clamp (Windows encodes 1.0 fine).
///
/// Pipeline: `DynamicImage` -> straight RGBA8 -> IWICBitmap (CreateBitmapFromMemory) -> HEIF encoder
/// frame with `ImageQuality` (VT_R4) -> WriteSource -> two-phase Commit -> read bytes from the
/// growable HGLOBAL. Orientation is NOT tagged (pixels are already upright, same as macOS).
#[cfg(target_os = "windows")]
pub(crate) fn encode_heic(img: &DynamicImage, opts: Option<HeicConfig>) -> Result<Vec<u8>> {
  use windows::core::PWSTR;
  use windows::Win32::Foundation::HGLOBAL;
  use windows::Win32::Graphics::Imaging::{
    GUID_ContainerFormatHeif, GUID_WICPixelFormat32bppRGBA, IWICBitmapFrameEncode,
    WICBitmapEncoderNoCache,
  };
  use windows::Win32::System::Com::StructuredStorage::{
    CreateStreamOnHGlobal, GetHGlobalFromStream, IPropertyBag2, PROPBAG2,
  };
  use windows::Win32::System::Com::{STATFLAG_NONAME, STATSTG};
  use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
  use windows::Win32::System::Variant::VARIANT;

  let opts = opts.unwrap_or_default();
  // 10-bit is not supported by the WIC HEVC encoder (it emits 8-bit); reject rather than silently
  // downgrade, so callers are not misled about the output depth.
  if opts.bit_depth == Some(10) {
    return Err(Error::new(
      Status::InvalidArg,
      "HEIC: 10-bit output is not supported on Windows (the OS WIC HEVC encoder emits 8-bit only)"
        .to_owned(),
    ));
  }

  let width = img.width();
  let height = img.height();
  if width == 0 || height == 0 {
    return Err(Error::new(Status::InvalidArg, "HEIC: image has zero dimensions".to_owned()));
  }

  // Straight RGBA8. Alpha is flattened to opaque by the WIC encoder (chosen behavior).
  let rgba = img.to_rgba8();
  let stride = width
    .checked_mul(4)
    .ok_or_else(|| Error::new(Status::InvalidArg, "HEIC: stride overflow".to_owned()))?;
  // Bound the total RGBA byte count: `CreateBitmapFromMemory`'s windows-rs wrapper converts the
  // buffer length to `u32` via `try_into().unwrap()`, which would PANIC (crashing the napi worker
  // thread) for a buffer exceeding `u32::MAX`. Reject oversized images with a clean error instead.
  // (`stride * height` == `width * height * 4` == `rgba.as_raw().len()`; mirrors the decode guard.)
  let _buffer_size = stride
    .checked_mul(height)
    .ok_or_else(|| Error::new(Status::InvalidArg, "HEIC: buffer size overflow".to_owned()))?;
  // quality 0..=100 -> 0.0..=1.0 (no 0.9 clamp; Windows encodes 1.0 fine, unlike macOS ImageIO).
  let quality = (opts.quality.unwrap_or(80).min(100) as f32) / 100.0;

  let factory = wic_factory()?;
  unsafe {
    let out = CreateStreamOnHGlobal(HGLOBAL::default(), true.into()).map_err(|e| wic_error("out stream", e))?;
    let encoder = factory
      .CreateEncoder(&GUID_ContainerFormatHeif, std::ptr::null())
      .map_err(|e| wic_error("encoder", e))?;
    encoder.Initialize(&out, WICBitmapEncoderNoCache).map_err(|e| wic_error("encoder init", e))?;

    let mut frame_opt: Option<IWICBitmapFrameEncode> = None;
    let mut bag_opt: Option<IPropertyBag2> = None;
    encoder
      .CreateNewFrame(&mut frame_opt, &mut bag_opt)
      .map_err(|e| wic_error("new frame", e))?;
    let frame = frame_opt.ok_or_else(|| Error::new(Status::GenericFailure, "HEIC: null frame encoder".to_owned()))?;
    let bag = bag_opt.ok_or_else(|| Error::new(Status::GenericFailure, "HEIC: null options bag".to_owned()))?;

    // Set ImageQuality (VT_R4). The property bag carries only the name; the VARIANT carries the type.
    let mut prop_name: Vec<u16> = "ImageQuality".encode_utf16().chain(std::iter::once(0)).collect();
    let mut prop = PROPBAG2::default();
    prop.pstrName = PWSTR(prop_name.as_mut_ptr());
    bag
      .Write(1, &prop, &VARIANT::from(quality))
      .map_err(|e| wic_error("set quality", e))?;

    frame.Initialize(&bag).map_err(|e| wic_error("frame init", e))?;
    frame.SetSize(width, height).map_err(|e| wic_error("set size", e))?;
    // The encoder negotiates 32bppRGBA -> its supported opaque format; we accept whatever it picks.
    let mut pixel_format = GUID_WICPixelFormat32bppRGBA;
    frame.SetPixelFormat(&mut pixel_format).map_err(|e| wic_error("set pixel format", e))?;

    let source = factory
      .CreateBitmapFromMemory(width, height, &GUID_WICPixelFormat32bppRGBA, stride, rgba.as_raw())
      .map_err(|e| wic_error("source bitmap", e))?;
    frame.WriteSource(&source, std::ptr::null()).map_err(|e| wic_error("write source", e))?;
    frame.Commit().map_err(|e| wic_error("frame commit", e))?;
    encoder.Commit().map_err(|e| wic_error("encoder commit", e))?;

    // Copy the encoded bytes out of the growable HGLOBAL before the stream is dropped. Use the
    // stream's LOGICAL length (`Stat().cbSize`), not `GlobalSize` (the HGLOBAL allocation capacity),
    // so the returned buffer can never carry trailing allocation slack.
    let hglobal = GetHGlobalFromStream(&out).map_err(|e| wic_error("get hglobal", e))?;
    let mut stat = STATSTG::default();
    out.Stat(&mut stat, STATFLAG_NONAME).map_err(|e| wic_error("stat", e))?;
    let size = usize::try_from(stat.cbSize)
      .map_err(|_| Error::new(Status::GenericFailure, "HEIC: encoded size exceeds usize".to_owned()))?;
    let ptr = GlobalLock(hglobal) as *const u8;
    if ptr.is_null() {
      return Err(Error::new(Status::GenericFailure, "HEIC: failed to lock output buffer".to_owned()));
    }
    let bytes = std::slice::from_raw_parts(ptr, size).to_vec();
    let _ = GlobalUnlock(hglobal);
    Ok(bytes)
  }
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

  #[test]
  fn mif1_major_with_avif_compatible_is_not_heic() {
    // THE FIX: a generic HEIF major brand (`mif1`) with `avif` ONLY in the compatible list must
    // defer to the libavif path. Pre-fix this returned true (HEIF-positive on the major brand).
    assert!(!is_heic(&ftyp(b"mif1", b"\0\0\0\0", &[b"avif"], None)));
  }

  #[test]
  fn msf1_major_with_avis_compatible_is_not_heic() {
    // THE FIX: `msf1` major with `avis` (animated AVIF) only as a compatible brand is AVIF, not HEIC.
    assert!(!is_heic(&ftyp(b"msf1", b"\0\0\0\0", &[b"avis"], None)));
  }

  #[test]
  fn mif1_major_with_heic_compatible_is_heic() {
    // A generic HEIF major brand with a real HEIF compatible brand and no AVIF brand is HEIC.
    assert!(is_heic(&ftyp(b"mif1", b"\0\0\0\0", &[b"heic"], None)));
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

  #[cfg(not(any(target_os = "macos", target_os = "windows")))]
  #[test]
  fn decode_heic_stub_errors_off_macos_windows() {
    // Off macOS/Windows the stub returns an error (no OS HEVC decoder to delegate to).
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
