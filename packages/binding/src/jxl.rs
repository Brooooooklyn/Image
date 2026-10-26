//! JPEG XL (`.jxl`) input via the pure-Rust `jxl` crate (libjxl/jxl-rs).
//!
//! Decode only: jxl-rs has no encoder, so there is no `.jxl()` output method. The decoded image
//! goes through the same `ThreadsafeDynamicImage::get` path as every other format, so
//! `metadata()`, every encoder, `overlay()` and `composite()` accept JXL input.
//!
//! What comes out of [`decode_jxl`]:
//! - Upright pixels: the decoder bakes the header orientation in (`adjust_orientation`), like
//!   browsers and `djxl`. The caller stores no orientation, so `.rotate()` never rotates twice.
//! - sRGB: any other output colour space (Display P3, linear Rec.2100, an embedded ICC, ...) is
//!   converted with `moxcms`. jxl-rs has no colour management of its own.
//! - HDR: PQ and HLG given as a colour enum (not as an ICC) are tone mapped to SDR sRGB by
//!   [`hdr`], a port of libjxl's own tone mapping (Rec. 2408 down to 250 nits for PQ, the HLG OOTF
//!   down to 80 nits, out-of-gamut colours desaturated in sRGB). They do not go through the ICC
//!   jxl-rs writes for them: see `hdr.rs` for why. An HDR file with an embedded ICC still goes
//!   through that ICC and `moxcms`.
//! - 8-bit for files with <= 8 bits per sample, 16-bit otherwise (9..16 bit and float). Never f32.
//!   A float file that needs conversion is decoded as f32 and only clipped to [0, 1] after the
//!   conversion to sRGB, like `djxl`. So are PQ/HLG float and XYB files, whose codes can be outside
//!   [0, 1]: they are clipped only after the tone mapping. Other PQ/HLG files are decoded as u16, and
//!   an 8-bit one is rounded back to 8 bits after the tone mapping.
//! - Straight (unpremultiplied) alpha, which is what the `image` crate assumes.
//! - The first visible frame of an animation (already composited by the decoder).

mod hdr;

use std::fmt::Display;

use image::{DynamicImage, ImageBuffer, Luma, LumaA, Rgb, Rgba};
use jxl::api::{
  AlphaMode, Endianness, Event, ExtraChannel, JxlBitDepth, JxlColorEncoding, JxlColorProfile,
  JxlColorType, JxlDataFormat, JxlDecoder, JxlDecoderOptions, JxlOutputBuffer, JxlPixelFormat,
  JxlPrimaries, JxlSignature, JxlTransferFunction, JxlWhitePoint, check_signature,
};
use moxcms::{ColorProfile, Layout, TransformExecutor, TransformOptions};
use napi::bindgen_prelude::*;

/// Largest decoded buffer we allocate: 512 MiB, the same cap as the `image` crate's
/// `Limits::default().max_alloc` that every other format gets through `load_from_memory`.
const MAX_OUTPUT_BYTES: usize = 512 * 1024 * 1024;

/// Cap on the decoder's own buffers (134_217_728 pixels at 3 samples per pixel). jxl-rs counts
/// `max(w, 16) * h * (3 + extra channels)` samples and rejects the header (`Image size too large`)
/// when the count is `>=` this, before it allocates anything. So this is the cap that applies for
/// 8-bit RGB at 134_217_728 pixels or more (384 MiB of output) and for any image with alpha or other
/// extra channels (about 100M pixels with alpha); [`MAX_OUTPUT_BYTES`] covers everything else.
const SAMPLE_LIMIT: usize = 3 * 134_217_728;

/// True for a bare JPEG XL codestream (`FF 0A`) or the ISOBMFF-style JPEG XL container
/// (`00 00 00 0C 'JXL ' 0D 0A 87 0A`).
pub fn is_jxl(buf: &[u8]) -> bool {
  matches!(
    check_signature(buf),
    JxlSignature::Codestream | JxlSignature::Container
  )
}

fn decode_error(reason: impl Display) -> Error {
  Error::new(
    Status::InvalidArg,
    format!("Decode jxl image failed: {reason}"),
  )
}

/// Decode the first visible frame of a JPEG XL file into an upright, sRGB, straight-alpha image.
pub(crate) fn decode_jxl(buf: &[u8]) -> Result<DynamicImage> {
  // jxl-rs still has `unwrap`/`expect` sites in its decode paths, so turn a panic into a JS error
  // instead of letting it unwind into napi's `extern "C"` async-work callback (which aborts Node).
  // Same precedent as `jpeg.rs`. On wasm the target is `panic = "abort"`, so this cannot help there:
  // a decoder panic is a hard trap.
  std::panic::catch_unwind(|| decode_first_frame(buf))
    .unwrap_or_else(|_| Err(decode_error("the decoder panicked")))
}

fn decode_first_frame(buf: &[u8]) -> Result<DynamicImage> {
  let mut options = JxlDecoderOptions::default();
  options.adjust_orientation = true;
  options.alpha_mode = AlphaMode::Unpremultiplied;
  options.sample_limit = Some(SAMPLE_LIMIT);
  let mut decoder = JxlDecoder::new(options);
  // `&[u8]` is a `JxlBitstreamInput`; the decoder advances it in place.
  let mut input: &[u8] = buf;

  // The first event is always `BasicInfo` (image header + colour profile).
  match decoder
    .process(&mut input, None, None)
    .map_err(decode_error)?
  {
    Event::BasicInfo => {}
    // The whole file is already in `input`, so asking for more means it is cut short.
    Event::NeedMoreInput { .. } => return Err(decode_error("truncated input")),
    event => return Err(decode_error(format!("unexpected decoder event {event:?}"))),
  }
  let info = decoder
    .basic_info()
    .ok_or_else(|| decode_error("missing basic info"))?
    .clone();
  // A CMYK file keeps K in a Black extra channel. Decoding it as RGB would silently drop K.
  if info
    .extra_channels
    .iter()
    .any(|channel| channel.ec_type == ExtraChannel::Black)
  {
    return Err(decode_error("CMYK JPEG XL is not supported"));
  }
  // Already swapped for orientations 5..8, because the decoder applies the orientation.
  let (width, height) = info.size;
  let has_alpha = info
    .extra_channels
    .iter()
    .any(|channel| channel.ec_type == ExtraChannel::Alpha);
  // The decoder's own default pixel format (set at `BasicInfo`) is gray exactly when the
  // codestream colour space is gray; asking for gray output on a colour file is an error.
  let gray = decoder
    .current_pixel_format()
    .is_some_and(|format| format.color_type.is_grayscale());
  let float = matches!(info.bit_depth, JxlBitDepth::Float { .. });
  let sixteen_bit = match info.bit_depth {
    JxlBitDepth::Int { bits_per_sample } => bits_per_sample > 8,
    JxlBitDepth::Float { .. } => true,
  };
  let color_type = match (gray, has_alpha) {
    (true, false) => JxlColorType::Grayscale,
    (true, true) => JxlColorType::GrayscaleAlpha,
    (false, false) => JxlColorType::Rgb,
    (false, true) => JxlColorType::Rgba,
  };
  let data_format = if sixteen_bit {
    // A target `bit_depth` of 16 rescales any 9..16 bit (or float) source to the full u16 range.
    JxlDataFormat::U16 {
      endianness: Endianness::native(),
      bit_depth: 16,
    }
  } else {
    JxlDataFormat::U8 { bit_depth: 8 }
  };
  let extra_channels = info.extra_channels.len();
  let mut set_format = |data_format| -> Result<JxlColorProfile> {
    decoder
      .set_pixel_format(JxlPixelFormat {
        color_type,
        color_data_format: Some(data_format),
        // `None`: do not emit extra channels (depth, spot colours, ...) as separate buffers. Alpha
        // is interleaved through `color_type` instead.
        extra_channel_format: vec![None; extra_channels],
      })
      .map_err(decode_error)?;
    // The profile of the pixels the decoder will write (it depends on the pixel format just set).
    decoder
      .output_color_profile()
      .cloned()
      .ok_or_else(|| decode_error("missing output color profile"))
  };
  let mut profile = set_format(data_format)?;
  let xyb = !info.uses_original_profile;
  // PQ/HLG in a colour enum: tone mapped by `hdr` instead of the ICC path (`convert` stays false).
  let hdr = hdr::HdrToSrgb::new(&profile, info.tone_mapping.intensity_target, xyb);
  let convert = hdr.is_none() && !is_srgb(&profile);
  // jxl-rs clips u8/u16 output to [0, 1] in the file's colour space, but a float file may hold
  // values outside it (HDR highlights, wide-gamut colours). The primaries matrix mixes every channel
  // into the others, so clipping first shifts the hue (red 1, 2 and 4 all come out the same). So
  // decode such a file as f32, which jxl-rs leaves unclipped, and clip after the conversion, like
  // `djxl`. The format can still change here (no frame header yet).
  let float_convert = float && convert;
  if float_convert {
    // An XYB file with an ICC comes out as linear sRGB (not sRGB) for f32: still a conversion.
    profile = set_format(JxlDataFormat::F32 {
      endianness: Endianness::native(),
    })?;
  }
  // The same clipping problem for PQ/HLG: a float file can hold codes outside [0, 1] (HLG
  // super-whites, negative values), and so does an XYB file of any bit depth (ringing around edges).
  // libjxl tone maps those unclipped, so decode them as f32 for `hdr` (codes in [0, 1] come out
  // exactly as through u16). A lossless integer file only holds codes in [0, 1]. When the f32 buffer
  // would be over [`MAX_OUTPUT_BYTES`], stay with u16 (clipped codes) instead of failing.
  let decoded_channels = color_type.samples_per_pixel();
  let hdr_f32 = hdr.is_some()
    && (float || xyb)
    && checked_buffer_len(width, height, decoded_channels, std::mem::size_of::<f32>()).is_ok();
  if hdr_f32 {
    set_format(JxlDataFormat::F32 {
      endianness: Endianness::native(),
    })?;
  }
  // `hdr` works on 16-bit codes, so an 8-bit PQ/HLG file is decoded as u16 too (no banding from
  // tone mapping 8-bit codes). The profile of a colour enum does not depend on the sample type.
  let hdr_from_u8 = hdr.is_some() && !sixteen_bit;
  if hdr_from_u8 && !hdr_f32 {
    set_format(JxlDataFormat::U16 {
      endianness: Endianness::native(),
      bit_depth: 16,
    })?;
  }

  // Reject oversized images before any pixel buffer exists. A gray image that needs conversion
  // becomes RGB(A), so check the size of the final buffer.
  let output_channels = if gray && convert {
    decoded_channels + 2
  } else {
    decoded_channels
  };
  let bytes_per_sample = data_format.bytes_per_sample();
  checked_buffer_len(width, height, output_channels, bytes_per_sample)?;
  // The f32 decode buffer is twice the size of the u16 one, so check it at 4 bytes per sample, and
  // an 8-bit PQ/HLG file is decoded at 2 bytes per sample.
  let decoded_bytes_per_sample = if float_convert || hdr_f32 {
    std::mem::size_of::<f32>()
  } else if hdr_from_u8 {
    std::mem::size_of::<u16>()
  } else {
    bytes_per_sample
  };
  let decoded_len = checked_buffer_len(width, height, decoded_channels, decoded_bytes_per_sample)?;
  let row_samples = width * decoded_channels;

  let w = u32::try_from(width).map_err(|_| decode_error("image width too large"))?;
  let h = u32::try_from(height).map_err(|_| decode_error("image height too large"))?;
  let out_layout = match (gray && !convert, has_alpha) {
    (true, false) => Layout::Gray,
    (true, true) => Layout::GrayAlpha,
    (false, false) => Layout::Rgb,
    (false, true) => Layout::Rgba,
  };
  let src_layout = match color_type {
    JxlColorType::Grayscale => Layout::Gray,
    JxlColorType::GrayscaleAlpha => Layout::GrayAlpha,
    JxlColorType::Rgba => Layout::Rgba,
    _ => Layout::Rgb,
  };

  if let Some(hdr) = hdr {
    let pixels = if hdr_f32 {
      let codes = decode_pixels::<f32>(&mut decoder, &mut input, decoded_len, height, row_samples)?;
      drop(decoder);
      let mut pixels = try_alloc::<u16>(codes.len())?;
      hdr.apply_f32(&codes, &mut pixels, src_layout.channels());
      pixels
    } else {
      let mut pixels =
        decode_pixels::<u16>(&mut decoder, &mut input, decoded_len, height, row_samples)?;
      drop(decoder);
      hdr.apply_u16(&mut pixels, src_layout.channels());
      pixels
    };
    if hdr_from_u8 {
      let mut pixels8 = try_alloc::<u8>(pixels.len())?;
      for (out, &value) in pixels8.iter_mut().zip(&pixels) {
        *out = ((u32::from(value) * 255 + 32767) / 65535) as u8;
      }
      image_u8(w, h, out_layout, pixels8)
    } else {
      image_u16(w, h, out_layout, pixels)
    }
  } else if sixteen_bit {
    let pixels = if float_convert {
      let pixels =
        decode_pixels::<f32>(&mut decoder, &mut input, decoded_len, height, row_samples)?;
      drop(decoder);
      let mut source = srgb_source(&profile)?;
      // The extended-range path reads the `cicp` transfer even with `allow_use_cicp_transfer` off
      // (see `transform_options`), so drop the tag to make it follow the ICC curves.
      source.cicp = None;
      let transform = source
        .create_transform_f32(
          src_layout,
          &ColorProfile::new_srgb(),
          out_layout,
          TransformOptions {
            // No clipping inside the matrix-shaper transform. Profiles with an A2B LUT (LUT-based
            // ICCs, including embedded HDR ones) still take moxcms's LUT path, which clips.
            allow_extended_range_rgb_xyz: true,
            ..transform_options()
          },
        )
        .map_err(|err| decode_error(format!("cannot convert to sRGB: {err}")))?;
      float_to_srgb_u16(transform.as_ref(), &pixels, width, src_layout, out_layout)?
    } else {
      let pixels =
        decode_pixels::<u16>(&mut decoder, &mut input, decoded_len, height, row_samples)?;
      drop(decoder);
      if convert {
        let transform = srgb_source(&profile)?
          .create_transform_16bit(
            src_layout,
            &ColorProfile::new_srgb(),
            out_layout,
            transform_options(),
          )
          .map_err(|err| decode_error(format!("cannot convert to sRGB: {err}")))?;
        to_srgb(transform.as_ref(), pixels, width, src_layout, out_layout)?
      } else {
        pixels
      }
    };
    image_u16(w, h, out_layout, pixels)
  } else {
    let pixels = decode_pixels::<u8>(&mut decoder, &mut input, decoded_len, height, row_samples)?;
    drop(decoder);
    let pixels = if convert {
      let transform = srgb_source(&profile)?
        .create_transform_8bit(
          src_layout,
          &ColorProfile::new_srgb(),
          out_layout,
          transform_options(),
        )
        .map_err(|err| decode_error(format!("cannot convert to sRGB: {err}")))?;
      to_srgb(transform.as_ref(), pixels, width, src_layout, out_layout)?
    } else {
      pixels
    };
    image_u8(w, h, out_layout, pixels)
  }
  .ok_or_else(|| decode_error("decoded buffer size mismatch"))
}

fn image_u16(w: u32, h: u32, layout: Layout, pixels: Vec<u16>) -> Option<DynamicImage> {
  match layout {
    Layout::Gray => ImageBuffer::<Luma<u16>, _>::from_raw(w, h, pixels).map(DynamicImage::from),
    Layout::GrayAlpha => {
      ImageBuffer::<LumaA<u16>, _>::from_raw(w, h, pixels).map(DynamicImage::from)
    }
    Layout::Rgb => ImageBuffer::<Rgb<u16>, _>::from_raw(w, h, pixels).map(DynamicImage::from),
    _ => ImageBuffer::<Rgba<u16>, _>::from_raw(w, h, pixels).map(DynamicImage::from),
  }
}

fn image_u8(w: u32, h: u32, layout: Layout, pixels: Vec<u8>) -> Option<DynamicImage> {
  match layout {
    Layout::Gray => ImageBuffer::<Luma<u8>, _>::from_raw(w, h, pixels).map(DynamicImage::from),
    Layout::GrayAlpha => {
      ImageBuffer::<LumaA<u8>, _>::from_raw(w, h, pixels).map(DynamicImage::from)
    }
    Layout::Rgb => ImageBuffer::<Rgb<u8>, _>::from_raw(w, h, pixels).map(DynamicImage::from),
    _ => ImageBuffer::<Rgba<u8>, _>::from_raw(w, h, pixels).map(DynamicImage::from),
  }
}

/// `width * height * channels * bytes_per_sample` as a sample count, or an error when it overflows
/// `usize` (32-bit on i686/armv7/wasm) or exceeds [`MAX_OUTPUT_BYTES`].
fn checked_buffer_len(
  width: usize,
  height: usize,
  channels: usize,
  bytes_per_sample: usize,
) -> Result<usize> {
  let samples = width
    .checked_mul(height)
    .and_then(|pixels| pixels.checked_mul(channels));
  match samples.and_then(|samples| samples.checked_mul(bytes_per_sample)) {
    Some(bytes) if bytes <= MAX_OUTPUT_BYTES => Ok(samples.unwrap_or_default()),
    _ => Err(decode_error(format!(
      "image too large: {width}x{height} needs more than {} MiB",
      MAX_OUTPUT_BYTES >> 20
    ))),
  }
}

/// Allocate a zeroed buffer without aborting the process when the allocator says no.
fn try_alloc<T: Copy + Default>(len: usize) -> Result<Vec<T>> {
  let mut buffer = Vec::new();
  buffer
    .try_reserve_exact(len)
    .map_err(|_| decode_error("cannot allocate the pixel buffer"))?;
  buffer.resize(len, T::default());
  Ok(buffer)
}

/// Run the decoder until the first visible frame is complete, writing interleaved samples into a
/// `Vec<T>`. Backing the buffer with `T` (not `u8`) keeps every u16 row 2-byte aligned and every
/// f32 row 4-byte aligned, which jxl-rs requires ("else the library may panic").
fn decode_pixels<T: bytemuck::Pod + Default>(
  decoder: &mut JxlDecoder,
  input: &mut &[u8],
  len: usize,
  height: usize,
  row_samples: usize,
) -> Result<Vec<T>> {
  let mut pixels = try_alloc::<T>(len)?;
  let row_bytes = row_samples * std::mem::size_of::<T>();
  let mut buffers = [JxlOutputBuffer::new(
    bytemuck::cast_slice_mut(&mut pixels),
    height,
    row_bytes,
  )];
  loop {
    match decoder
      .process(input, Some(&mut buffers), None)
      .map_err(decode_error)?
    {
      Event::FrameHeader => {}
      // Animations: stop after the first visible frame. Later frames are never decoded.
      Event::FrameComplete { .. } => break,
      Event::NeedMoreInput { .. } => return Err(decode_error("truncated input")),
      event => return Err(decode_error(format!("unexpected decoder event {event:?}"))),
    }
  }
  Ok(pixels)
}

/// sRGB (or gray with the sRGB curve) needs no conversion. Everything else, including any ICC
/// profile, goes through `moxcms`, except PQ/HLG enums (see [`hdr`]).
fn is_srgb(profile: &JxlColorProfile) -> bool {
  matches!(
    profile,
    JxlColorProfile::Simple(JxlColorEncoding::RgbColorSpace {
      white_point: JxlWhitePoint::D65,
      primaries: JxlPrimaries::SRGB,
      transfer_function: JxlTransferFunction::SRGB,
      ..
    }) | JxlColorProfile::Simple(JxlColorEncoding::GrayscaleColorSpace {
      transfer_function: JxlTransferFunction::SRGB,
      ..
    })
  )
}

/// The `moxcms` profile of the decoded pixels. `try_as_icc` also builds an ICC for enum colour
/// spaces (P3, Rec.2100, custom primaries, ...); `as_icc` would panic where that fails.
fn srgb_source(profile: &JxlColorProfile) -> Result<ColorProfile> {
  let icc = profile
    .try_as_icc()
    .ok_or_else(|| decode_error("cannot build an ICC profile for the image color space"))?;
  ColorProfile::new_from_slice(&icc)
    .map_err(|err| decode_error(format!("cannot read the image color profile: {err}")))
}

fn transform_options() -> TransformOptions {
  TransformOptions {
    // Follow the TRC/A2B curves in the ICC that jxl-rs writes, not moxcms's own curve for the
    // `cicp` tag next to them. For the DCI transfer jxl-rs writes a pure 2.6 gamma TRC (like libjxl)
    // but cicp 17 (SMPTE ST 428), which moxcms decodes as `v^2.6 / 0.9166`: about 9% too bright.
    allow_use_cicp_transfer: false,
    // PQ/HLG enums never get here (see [`hdr`]). An HDR file with its own ICC follows that ICC.
    ..TransformOptions::default()
  }
}

/// Convert row by row. When the layout keeps its channel count (RGB(A) -> RGB(A)) one scratch row is
/// enough; gray -> RGB(A) needs the bigger output buffer anyway (its size was checked up front).
fn to_srgb<T: Copy + Default>(
  transform: &(dyn TransformExecutor<T> + Send + Sync),
  mut pixels: Vec<T>,
  width: usize,
  src_layout: Layout,
  dst_layout: Layout,
) -> Result<Vec<T>> {
  let src_row = width * src_layout.channels();
  let dst_row = width * dst_layout.channels();
  let convert_error =
    |err: moxcms::CmsError| decode_error(format!("cannot convert to sRGB: {err}"));
  if src_row == dst_row {
    let mut scratch = try_alloc::<T>(dst_row)?;
    for row in pixels.chunks_exact_mut(src_row) {
      transform
        .transform(row, &mut scratch)
        .map_err(convert_error)?;
      row.copy_from_slice(&scratch);
    }
    Ok(pixels)
  } else {
    let mut output = try_alloc::<T>(pixels.len() / src_row * dst_row)?;
    for (src, dst) in pixels
      .chunks_exact(src_row)
      .zip(output.chunks_exact_mut(dst_row))
    {
      transform.transform(src, dst).map_err(convert_error)?;
    }
    Ok(output)
  }
}

/// Convert f32 rows to sRGB through one scratch row, then clip each sample (alpha too) to [0, 1]
/// and quantize it to u16 the way jxl-rs does for u16 output (`round(v * 65535)`).
fn float_to_srgb_u16(
  transform: &(dyn TransformExecutor<f32> + Send + Sync),
  pixels: &[f32],
  width: usize,
  src_layout: Layout,
  dst_layout: Layout,
) -> Result<Vec<u16>> {
  let src_row = width * src_layout.channels();
  let dst_row = width * dst_layout.channels();
  let mut scratch = try_alloc::<f32>(dst_row)?;
  let mut output = try_alloc::<u16>(pixels.len() / src_row * dst_row)?;
  for (src, dst) in pixels
    .chunks_exact(src_row)
    .zip(output.chunks_exact_mut(dst_row))
  {
    transform
      .transform(src, &mut scratch)
      .map_err(|err| decode_error(format!("cannot convert to sRGB: {err}")))?;
    for (out, value) in dst.iter_mut().zip(&scratch) {
      // `as` maps NaN to 0.
      *out = (value.clamp(0.0, 1.0) * 65535.0).round() as u16;
    }
  }
  Ok(output)
}

#[cfg(test)]
mod tests {
  use super::{MAX_OUTPUT_BYTES, checked_buffer_len, is_jxl};

  #[test]
  fn detects_bare_codestream() {
    assert!(is_jxl(&[0xff, 0x0a]));
    assert!(is_jxl(&[0xff, 0x0a, 0xfa, 0x7f]));
  }

  #[test]
  fn detects_container() {
    let container = [
      0, 0, 0, 0x0c, b'J', b'X', b'L', b' ', 0x0d, 0x0a, 0x87, 0x0a, 0, 0, 0, 0x14,
    ];
    assert!(is_jxl(&container));
    // A cut-off container signature is not a JPEG XL file yet.
    assert!(!is_jxl(&container[..8]));
  }

  #[test]
  fn rejects_other_inputs() {
    // PNG
    assert!(!is_jxl(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]));
    // HEIC `ftyp`
    assert!(!is_jxl(&[
      0, 0, 0, 0x18, b'f', b't', b'y', b'p', b'h', b'e', b'i', b'c'
    ]));
    // JPEG
    assert!(!is_jxl(&[0xff, 0xd8, 0xff, 0xe0]));
    assert!(!is_jxl(&[]));
    assert!(!is_jxl(&[0xff]));
  }

  #[test]
  fn output_limit_is_512_mib() {
    assert_eq!(MAX_OUTPUT_BYTES, 536_870_912);
    // Only this function's arithmetic is checked here. A real 16384x8192 RGBA8 file is rejected
    // earlier by the decoder's `SAMPLE_LIMIT` (4 samples per pixel).
    // 16384x8192 RGBA8 is exactly 512 MiB: within this cap.
    assert_eq!(
      checked_buffer_len(16384, 8192, 4, 1).unwrap(),
      16384 * 8192 * 4
    );
    // One more row is over the cap.
    assert!(checked_buffer_len(16384, 8193, 4, 1).is_err());
    // The same pixels at 16 bit are twice the bytes.
    assert!(checked_buffer_len(16384, 8192, 4, 2).is_err());
    assert_eq!(
      checked_buffer_len(16384, 4096, 4, 2).unwrap(),
      16384 * 4096 * 4
    );
    // The 387-byte 12000x12000 bomb, RGBA8: 576 MB.
    assert!(checked_buffer_len(12000, 12000, 4, 1).is_err());
    // Overflow is an error, not a wrap-around.
    assert!(checked_buffer_len(usize::MAX, 2, 1, 1).is_err());
    assert!(checked_buffer_len(1 << 20, 1 << 20, 4, 2).is_err());
  }
}
