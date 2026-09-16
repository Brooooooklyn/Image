//! Byte-identity harness for the quantizer: prints an FNV-1a-64 hash of
//! `quantize_rgba`'s output (palette + indices + quality) for the repo fixture
//! across a fixed matrix of configs, plus a synthetic translucent variant that
//! exercises the non-opaque `nearest_lab` path.
//!
//! Run: `cargo run -p napi_rs_image --example quant_hash --no-default-features --release`
//! Identical output across two checkouts == byte-identical quantizer output.

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

fn run(name: &str, px: &[RGBA8], w: usize, h: usize, cfg: &QuantizeConfig) {
  let out = quantize_rgba(px, w, h, cfg);
  let mut buf = Vec::with_capacity(out.palette.len() * 4 + out.indices.len() + 1);
  for c in &out.palette {
    buf.extend_from_slice(&[c.r, c.g, c.b, c.a]);
  }
  buf.extend_from_slice(&out.indices);
  buf.push(out.quality);
  println!("{name:<28} {:016x}  (palette={}, quality={})", fnv(&buf), out.palette.len(), out.quality);
}

fn main() {
  let (px, w, h) = decode();

  // Synthetic translucent variant: deterministic alpha ramp + a fully
  // transparent quadrant, exercising the general (non-all-opaque) path.
  let mut pxa = px.clone();
  for (i, p) in pxa.iter_mut().enumerate() {
    let x = i % w;
    let y = i / w;
    p.a = if x < w / 3 { 0 } else { (64 + (x * 191 / w)) as u8 };
    let _ = y;
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
