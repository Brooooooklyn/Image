#![deny(clippy::all)]

// The `#[napi]` addon modules are gated behind the default-on `binding` feature so
// the CodSpeed bench can build the quantizer core with `--no-default-features` and
// link ZERO `napi_*` symbols (those are undefined outside Node; the CodSpeed runner
// executes under Valgrind, which binds eagerly and would abort on them otherwise).
// `binding` is in the crate's default features, so the shipped addon is unchanged.
#[cfg(feature = "binding")]
pub mod avif;
#[cfg(feature = "binding")]
mod fast_resize;
#[cfg(feature = "binding")]
pub mod jpeg;
// P3: deterministic integer sRGB->CIELAB + CIE76 ΔE, wired into the quantizer's
// perceptual color ASSIGNMENT metric (`pdist` in `quantize.rs`).
mod lab;
#[cfg(feature = "binding")]
pub mod png;
mod quantize;
// Runtime-dispatched SIMD kernels for the quantizer's nearest-palette argmin. Not
// behind `binding` (same as `quantize`/`lab`) so the `--no-default-features` bench
// core links it. Pure integer math -> byte-identical to the scalar reference.
mod quantize_simd;
/// Quantizer core, re-exported solely for the `benches/` CodSpeed micro-benchmarks
/// (`benches/quantize.rs`). These names are not part of the addon's public API and
/// may change without notice; nothing in the shipped JS surface depends on them.
#[doc(hidden)]
pub use quantize::{QuantizeConfig, QuantizeOutput, quantize_rgba};
#[cfg(feature = "binding")]
pub mod transformer;
#[cfg(feature = "binding")]
mod utils;
#[cfg(feature = "binding")]
mod webp;
