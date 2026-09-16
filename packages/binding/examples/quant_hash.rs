//! Byte-identity + quality harness for the quantizer.
//!
//! Per config it prints:
//!   * an FNV-1a-64 hash of `quantize_rgba` output (palette + indices + quality)
//!     — identical hash across two checkouts == byte-identical output;
//!   * `quality` (the internal gate score — only comparable within one build);
//!   * `cover` — fixed-metric palette coverage: count-weighted mean over the
//!     distinct posterized colors of dist2(color, NEAREST palette entry) with
//!     dist2 = dr²+dg²+db²+3·da² (plain RGBA, no internal-metric dependence, so
//!     it IS comparable across commits that change the working color space);
//!   * `repro` — count-weighted mean dist2(posterized src, palette[indices[i]])
//!     over pixels (per-pixel reproduction error incl. dither; lower is better).
//!
//! Run: `cargo run -p napi_rs_image --example quant_hash --no-default-features --release`

use std::collections::HashMap;

use image::ImageFormat;
use napi_rs_image::{QuantizeConfig, quantize_rgba};
use rgb::{FromSlice, RGBA8};

fn decode() -> (Vec<RGBA8>, usize, usize) {
  static FIXTURE: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../un-optimized.png"));
  let img = image::load_from_memory_with_format(FIXTURE, ImageFormat::Png)
    .expect("decode")
    .to_rgba8();
  let (w, h) = (img.width() as usize, img.height() as usize);
  (img.into_raw().as_rgba().to_vec(), w, h)
}

fn fnv(data: &[u8]) -> u64 {
  let mut h = 0xcbf29ce484222325u64;
  for &b in data {
    h ^= b as u64;
    h = h.wrapping_mul(0x100000001b3);
  }
  h
}

/// Fixed comparison metric: plain RGBA squared distance with alpha ×3.
fn d2(p: RGBA8, q: RGBA8) -> i64 {
  let dr = p.r as i64 - q.r as i64;
  let dg = p.g as i64 - q.g as i64;
  let db = p.b as i64 - q.b as i64;
  let da = p.a as i64 - q.a as i64;
  dr * dr + dg * dg + db * db + 3 * da * da
}

fn run(name: &str, px: &[RGBA8], w: usize, h: usize, cfg: &QuantizeConfig) {
  let out = quantize_rgba(px, w, h, cfg);
  let mut buf = Vec::with_capacity(out.palette.len() * 4 + out.indices.len() + 1);
  for c in &out.palette {
    buf.extend_from_slice(&[c.r, c.g, c.b, c.a]);
  }
  buf.extend_from_slice(&out.indices);
  buf.push(out.quality);

  // Distinct-color histogram of the source (visible only) for the metrics.
  let mut hist: HashMap<RGBA8, u64> = HashMap::new();
  for &p in px {
    if p.a > 0 {
      *hist.entry(p).or_insert(0) += 1;
    }
  }
  // cover: each distinct color's distance to its NEAREST palette entry.
  let mut cover_num = 0f64;
  let mut repro_num = 0f64;
  let mut n = 0f64;
  for (&c, &cnt) in &hist {
    let mut best = i64::MAX;
    for &e in &out.palette {
      best = best.min(d2(c, e));
    }
    cover_num += cnt as f64 * best as f64;
    n += cnt as f64;
  }
  // repro: per-pixel distance to the chosen entry.
  for (i, &p) in px.iter().enumerate() {
    if p.a > 0 {
      repro_num += d2(p, out.palette[out.indices[i] as usize]) as f64;
    }
  }
  println!(
    "{name:<28} {:016x}  pal={:<3} q={:<3} cover={:<9.1} repro={:<9.1}",
    fnv(&buf),
    out.palette.len(),
    out.quality,
    cover_num / n.max(1.0),
    repro_num / n.max(1.0)
  );
}

fn main() {
  let (px, w, h) = decode();

  // Synthetic translucent variant: deterministic alpha ramp + a fully
  // transparent quadrant, exercising the general (non-all-opaque) path.
  let mut pxa = px.clone();
  for (i, p) in pxa.iter_mut().enumerate() {
    let x = i % w;
    p.a = if x < w / 3 { 0 } else { (64 + (x * 191 / w)) as u8 };
  }

  let cases: &[(&str, QuantizeConfig)] = &[
    ("default_251_dither", QuantizeConfig { max_colors: 251, min_quality: 70, kmeans_iters: 5, dither: true, posterization: 0 }),
    ("q75_145_dither", QuantizeConfig { max_colors: 145, min_quality: 70, kmeans_iters: 5, dither: true, posterization: 0 }),
    ("colors16_dither", QuantizeConfig { max_colors: 16, min_quality: 0, kmeans_iters: 5, dither: true, posterization: 0 }),
    ("colors64_dither", QuantizeConfig { max_colors: 64, min_quality: 0, kmeans_iters: 5, dither: true, posterization: 0 }),
    ("colors256_dither", QuantizeConfig { max_colors: 256, min_quality: 0, kmeans_iters: 5, dither: true, posterization: 0 }),
    ("colors256_nodither", QuantizeConfig { max_colors: 256, min_quality: 0, kmeans_iters: 5, dither: false, posterization: 0 }),
    ("colors256_iter0", QuantizeConfig { max_colors: 256, min_quality: 0, kmeans_iters: 0, dither: true, posterization: 0 }),
    ("colors128_posterize2", QuantizeConfig { max_colors: 128, min_quality: 0, kmeans_iters: 5, dither: true, posterization: 2 }),
    ("retry_gate_minq99", QuantizeConfig { max_colors: 64, min_quality: 99, kmeans_iters: 5, dither: true, posterization: 0 }),
  ];

  for (name, cfg) in cases {
    run(name, &px, w, h, cfg);
    run(&format!("{name}_alpha"), &pxa, w, h, cfg);
  }
}
