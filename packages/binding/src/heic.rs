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
    assert!(is_heic(&ftyp(b"abcd", b"\0\0\0\0", &[b"wxyz", b"heic"], None)));
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
    assert!(is_heic(&ftyp(b"abcd", b"\0\0\0\0", &[b"heic"], Some(u32::MAX))));
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
}
