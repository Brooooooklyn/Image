//! CodSpeed micro-benchmarks for the clean-room PNG quantizer core
//! ([`napi_rs_image::quantize_rgba`]).
//!
//! These measure ONLY the palette / remap / dither hot path — there is no PNG
//! decode and no oxipng recompress in the measured loop — so a regression
//! localizes to the quantizer itself rather than the surrounding I/O.
//!
//! Built and run by CodSpeed in CI via `cargo codspeed build --no-default-features`
//! / `cargo codspeed run`. `--no-default-features` drops the crate's `binding`
//! feature so the bench links ONLY the pure quantizer core -- no `napi_*` symbols
//! (the CodSpeed runner executes under Valgrind, which binds eagerly and cannot
//! resolve the addon's Node-supplied symbols). The same flag makes a plain local
//! run work on every platform with no linker tricks:
//!   `cargo bench -p napi_rs_image --bench quantize --no-default-features`
//! `codspeed-criterion-compat` (imported as `criterion`) falls back to stock
//! criterion outside the CodSpeed runner.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use image::ImageFormat;
use napi_rs_image::{QuantizeConfig, quantize_rgba};
use rgb::{FromSlice, RGBA8};

/// The repo's primary quantizer fixture (1024x681, 8-bit RGBA), baked into the
/// bench binary so the input is byte-identical on every runner and independent
/// of the working directory.
static FIXTURE_PNG: &[u8] = include_bytes!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/../../un-optimized.png"
));

/// Decode the fixture to a flat RGBA8 buffer once, outside the measured loop,
/// mirroring `png.rs`'s `decode_rgba8`.
fn decode_fixture() -> (Vec<RGBA8>, usize, usize) {
  let img = image::load_from_memory_with_format(FIXTURE_PNG, ImageFormat::Png)
    .expect("decode bench fixture")
    .to_rgba8();
  let (w, h) = (img.width() as usize, img.height() as usize);
  (img.into_raw().as_rgba().to_vec(), w, h)
}

/// A single-pass config with the retry gate disabled (`min_quality = 0`), so the
/// run is exactly one quantization at `max_colors` — isolating palette size as
/// the only variable across the sweep.
fn single_pass(max_colors: u16, dither: bool) -> QuantizeConfig {
  QuantizeConfig {
    max_colors,
    min_quality: 0,
    kmeans_iters: 5,
    dither,
    posterization: 0,
  }
}

fn bench_quantize(c: &mut Criterion) {
  let (pixels, w, h) = decode_fixture();
  let px = pixels.as_slice();

  let mut group = c.benchmark_group("quantize");

  // Real user-facing presets, mirroring `QuantizeConfig::from_options`:
  //   no-arg default  -> maxQuality 99 => 251 colors, dither on, min_quality 70
  //   maxQuality 75   ->                  145 colors, dither on, min_quality 70
  let default_cfg = QuantizeConfig {
    max_colors: 251,
    min_quality: 70,
    kmeans_iters: 5,
    dither: true,
    posterization: 0,
  };
  group.bench_function("default", |b| {
    b.iter(|| black_box(quantize_rgba(black_box(px), w, h, &default_cfg)))
  });

  let q75_cfg = QuantizeConfig {
    max_colors: 145,
    min_quality: 70,
    kmeans_iters: 5,
    dither: true,
    posterization: 0,
  };
  group.bench_function("max_quality_75", |b| {
    b.iter(|| black_box(quantize_rgba(black_box(px), w, h, &q75_cfg)))
  });

  // Palette-size sweep (single pass, dither on): median-cut + k-means + remap
  // scaling with the centroid count.
  for k in [16u16, 64, 256] {
    let cfg = single_pass(k, true);
    group.bench_function(format!("colors/{k}"), |b| {
      b.iter(|| black_box(quantize_rgba(black_box(px), w, h, &cfg)))
    });
  }

  // Same 256-color palette with dithering off — the delta versus `colors/256`
  // isolates the Floyd-Steinberg diffusion cost.
  let no_dither_cfg = single_pass(256, false);
  group.bench_function("no_dither/256", |b| {
    b.iter(|| black_box(quantize_rgba(black_box(px), w, h, &no_dither_cfg)))
  });

  group.finish();
}

criterion_group!(benches, bench_quantize);
criterion_main!(benches);
