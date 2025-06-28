/// https://github.com/image-rs/image/blob/v0.24.5/src/math/utils.rs
use std::cmp::max;

/// Calculates the width and height an image should be resized to.
/// This preserves aspect ratio, and based on the `fill` parameter
/// will either fill the dimensions to fit inside the smaller constraint
/// (will overflow the specified bounds on one axis to preserve
/// aspect ratio), or will shrink so that both dimensions are
/// completely contained within the given `width` and `height`,
/// with empty space on one axis.
pub(crate) fn resize_dimensions(
  width: u32,
  height: u32,
  nwidth: u32,
  nheight: u32,
  fill: bool,
) -> (u32, u32) {
  let wratio = nwidth as f64 / width as f64;
  let hratio = nheight as f64 / height as f64;

  let ratio = if fill {
    f64::max(wratio, hratio)
  } else {
    f64::min(wratio, hratio)
  };

  let nw = max((width as f64 * ratio).round() as u64, 1);
  let nh = max((height as f64 * ratio).round() as u64, 1);

  if nw > u64::from(u32::MAX) {
    let ratio = u32::MAX as f64 / width as f64;
    (u32::MAX, max((height as f64 * ratio).round() as u32, 1))
  } else if nh > u64::from(u32::MAX) {
    let ratio = u32::MAX as f64 / height as f64;
    (max((width as f64 * ratio).round() as u32, 1), u32::MAX)
  } else {
    (nw as u32, nh as u32)
  }
}
